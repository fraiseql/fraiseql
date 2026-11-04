//! One-time password (`OTP`) authentication — email magic links and 6-digit codes.
//!
//! Provides:
//! - [`OtpStore`] — stores and validates `OTP` codes.
//! - [`InMemoryOtpStore`] — thread-safe in-memory backend for single-node and testing.
//! - [`EmailDelivery`] — delivers `OTP` emails.
//! - [`NoopEmailDelivery`] — logs only (no real email sent); for testing and dev.
//! - Axum handlers: `POST /auth/v1/otp` and `POST /auth/v1/verify`.
//!
//! # Security
//!
//! - Codes are 6 random decimal digits (10⁶ space, ~20 bits).
//! - Each code is **single-use**: consumed on first successful verify.
//! - `TTL` is 10 minutes.
//! - Verification attempts are limited to **3 per 15 minutes** per email. The budget belongs to the
//!   address, not the code: a resend issues a new code but no new guesses, and the budget returns
//!   only when the 15-minute send window does.
//! - Anyone who knows an address can spend its guesses and its sends, and so keep its owner from
//!   signing in by `OTP` for up to 15 minutes. The send cap always allowed that; the per-IP rules
//!   on `/auth/v1/otp` and `/auth/v1/verify` bound how many addresses one client can do it to.
//! - Codes are compared with constant-time equality to prevent timing attacks.
//! - Each code is mailed with a **link token**: 256 random bits that redeem the same sign-in
//!   without the address, so a magic link works on any device. The token and the code are one
//!   credential: redeeming either spends both, and a resend replaces both. Only `POST
//!   /auth/v1/verify` redeems it; a `GET` is refused, so a link scanner spends nothing.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
    session::{SessionStore, unix_now},
};

// ─── Constants ────────────────────────────────────────────────────────────────

/// `OTP` validity window in seconds (10 minutes).
pub const OTP_TTL_SECS: u64 = 600;

/// Maximum verification attempts per code before it is invalidated.
pub(super) const MAX_VERIFY_ATTEMPTS: u32 = 3;

/// Rate-limit window for `OTP` send requests (15 minutes).
pub(super) const OTP_RATE_WINDOW_SECS: u64 = 900;

/// Maximum `OTP` send requests in the rate-limit window.
pub(super) const OTP_RATE_MAX: u32 = 3;

/// Account-store provider key for email-`OTP` identities (#367).
///
/// Distinct from a social provider: no third party asserted the address —
/// receiving the code *is* the proof of control.
pub const OTP_PROVIDER: &str = "otp";

mod allow_list;
pub mod postgres;

pub use allow_list::{OTP_DELIVERY_QUEUE_CAPACITY, OtpAllowGate, OtpAllowList, PgOtpAllowList};

// ─── OTP record ───────────────────────────────────────────────────────────────

/// Internal record for a pending `OTP` code.
#[derive(Debug, Clone)]
struct OtpRecord {
    /// The 6-digit code.
    code:      String,
    /// SHA-256 of the link token mailed with the code.
    link_hash: Vec<u8>,
    /// Unix timestamp when this code expires.
    expires:   u64,
    /// How many verification attempts have been made against this code.
    attempts:  u32,
}

/// A code and the link token mailed with it: one credential, redeemed once by either.
///
/// Its `Debug` prints neither half: both are live credentials.
pub struct OtpIssue {
    /// The 6-digit code, typed by hand.
    pub code:       String,
    /// The link token: 256 random bits, base64url. It redeems the sign-in without the address.
    pub link_token: String,
}

impl std::fmt::Debug for OtpIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OtpIssue").finish_non_exhaustive()
    }
}

impl OtpIssue {
    /// A fresh code and link token.
    #[must_use]
    pub fn generate() -> Self {
        use base64::Engine as _;
        // SECURITY: rand::rng() uses OS-level entropy; random_range is unbiased.
        let code = format!("{:06}", rand::rng().random_range(0u32..1_000_000));
        let bytes: [u8; 32] = rand::rng().random();
        Self {
            code,
            link_token: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
        }
    }
}

/// SHA-256 of a link token: what the stores keep, so a read of them cannot replay a link.
pub(crate) fn link_token_hash(token: &str) -> Vec<u8> {
    use sha2::Digest as _;
    sha2::Sha256::digest(token.as_bytes()).to_vec()
}

impl OtpRecord {
    fn is_expired(&self) -> bool {
        // Fail-closed: a clock failure treats the code as expired, never valid.
        Self::is_expired_at(self.expires, unix_now().ok())
    }

    /// Pure expiry decision against an optional clock reading. `None` (clock failure)
    /// fails closed → expired.
    fn is_expired_at(expires: u64, now: Option<u64>) -> bool {
        now.is_none_or(|now| now >= expires)
    }
}

// ─── Rate-limit record ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RateRecord {
    /// Number of `OTP` sends in the current window.
    count:        u32,
    /// Unix timestamp when the window started.
    window_start: u64,
    /// Verification attempts against any of this address's codes in the window.
    guesses:      u32,
}

// ─── OtpStore trait ───────────────────────────────────────────────────────────

/// `OTP` storage and validation backend.
// Reason: used as dyn Trait (Arc<dyn OtpStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait OtpStore: Send + Sync {
    /// Delete expired codes and stale send-budget rows. Returns the number removed.
    ///
    /// Required rather than defaulted, for the reason [`crate::MfaStore::sweep_expired`]
    /// gives. `tb_otp_send_budget` is the sharper case: it holds one row per address that
    /// has ever requested a code and nothing removed them (#950).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the store fails.
    async fn sweep_expired(&self) -> Result<u64>;

    /// Generate and store a new code and link token for the given email, replacing any it
    /// held.
    ///
    /// # Returns
    ///
    /// The code and link token, to be delivered to the user.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::RateLimited`] if the per-email send rate is exceeded.
    /// Returns [`AuthError::DatabaseError`] if the backing store fails.
    async fn create_otp(&self, email: &str) -> Result<OtpIssue>;

    /// Verify a code for the given email and consume it on success.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the code is correct, not expired, and within attempt limits.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the code is wrong or expired.
    /// Returns [`AuthError::RateLimited`] if the attempt count is exceeded.
    async fn verify_otp(&self, email: &str, code: &str) -> Result<()>;

    /// Redeem a link token, consuming it and the code mailed with it.
    ///
    /// # Returns
    ///
    /// The address the token was issued to.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the token is unknown, spent, replaced or expired.
    /// Returns [`AuthError::DatabaseError`] if the backing store fails.
    async fn redeem_link(&self, link_token: &str) -> Result<String>;
}

// ─── In-memory OTP store ──────────────────────────────────────────────────────

/// Thread-safe in-memory `OTP` store.
///
/// Suitable for single-node deployments and testing.  For distributed deployments
/// use a Redis-backed store (not provided here).
pub struct InMemoryOtpStore {
    /// email → pending OTP record
    codes:       DashMap<String, OtpRecord>,
    /// email → rate-limit record
    rate_limits: DashMap<String, RateRecord>,
}

impl InMemoryOtpStore {
    /// Create a new empty `OTP` store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            codes:       DashMap::new(),
            rate_limits: DashMap::new(),
        }
    }

    /// Charge one guess to `email`'s window and return the window's count.
    ///
    /// The code's own count restarts with every send; this one restarts only with the
    /// send window, so a resend issues a new code without issuing new guesses.
    fn charge_guess(&self, email: &str, now: u64) -> u32 {
        let mut entry = self.rate_limits.entry(email.to_string()).or_insert(RateRecord {
            count:        0,
            window_start: now,
            guesses:      0,
        });
        if now >= entry.window_start + OTP_RATE_WINDOW_SECS {
            entry.count = 0;
            entry.window_start = now;
            entry.guesses = 0;
        }
        entry.guesses += 1;
        entry.guesses
    }

    /// Return the number of pending `OTP` codes (useful for tests).
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.codes.len()
    }
}

impl Default for InMemoryOtpStore {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl OtpStore for InMemoryOtpStore {
    async fn sweep_expired(&self) -> Result<u64> {
        let now = unix_now()?;
        let before = self.codes.len() + self.rate_limits.len();
        self.codes.retain(|_, r| r.expires > now);
        // A budget row whose window has closed is dead weight: the next send for that
        // address opens a fresh window regardless of what this row says.
        self.rate_limits.retain(|_, r| now < r.window_start + OTP_RATE_WINDOW_SECS);
        Ok((before - (self.codes.len() + self.rate_limits.len())) as u64)
    }

    async fn create_otp(&self, email: &str) -> Result<OtpIssue> {
        let now = unix_now()?;

        // Per-email rate limiting: max OTP_RATE_MAX sends per OTP_RATE_WINDOW_SECS.
        {
            let mut entry = self.rate_limits.entry(email.to_string()).or_insert(RateRecord {
                count:        0,
                window_start: now,
                guesses:      0,
            });
            // Reset window if it has expired. Only a new window returns the guesses.
            if now >= entry.window_start + OTP_RATE_WINDOW_SECS {
                entry.count = 0;
                entry.window_start = now;
                entry.guesses = 0;
            }
            if entry.count >= OTP_RATE_MAX {
                return Err(AuthError::RateLimited {
                    retry_after_secs: (entry.window_start + OTP_RATE_WINDOW_SECS)
                        .saturating_sub(now),
                });
            }
            entry.count += 1;
        }

        let issue = OtpIssue::generate();
        self.codes.insert(
            email.to_string(),
            OtpRecord {
                code:      issue.code.clone(),
                link_hash: link_token_hash(&issue.link_token),
                expires:   now + OTP_TTL_SECS,
                attempts:  0,
            },
        );

        Ok(issue)
    }

    async fn verify_otp(&self, email: &str, code: &str) -> Result<()> {
        let mut entry = self.codes.get_mut(email).ok_or_else(|| AuthError::InvalidToken {
            reason: "no pending OTP for email".into(),
        })?;

        if entry.is_expired() {
            drop(entry);
            self.codes.remove(email);
            return Err(AuthError::InvalidToken {
                reason: "OTP has expired".into(),
            });
        }

        entry.attempts += 1;
        let guesses = self.charge_guess(email, unix_now()?);
        if entry.attempts > MAX_VERIFY_ATTEMPTS || guesses > MAX_VERIFY_ATTEMPTS {
            drop(entry);
            self.codes.remove(email);
            return Err(AuthError::RateLimited {
                retry_after_secs: OTP_RATE_WINDOW_SECS,
            });
        }

        // Constant-time comparison (#788): a data-dependent `!=` on the code leaks
        // how many leading digits matched via its return time. The 3-attempt cap is
        // the primary mitigation, but honouring the module's own "constant-time"
        // doc claim costs nothing here.
        if !crate::constant_time::ConstantTimeOps::compare_str(&entry.code, code) {
            return Err(AuthError::InvalidToken {
                reason: "invalid OTP code".into(),
            });
        }

        // Code is correct — consume it (single-use).
        drop(entry);
        self.codes.remove(email);
        Ok(())
    }

    async fn redeem_link(&self, link_token: &str) -> Result<String> {
        let presented = link_token_hash(link_token);
        let holder = self
            .codes
            .iter()
            .find(|entry| entry.value().link_hash == presented)
            .map(|entry| entry.key().clone());
        // Removed only if it still holds this token: a concurrent resend replaced it.
        let (email, record) = holder
            .and_then(|email| {
                self.codes.remove_if(&email, |_, record| record.link_hash == presented)
            })
            .ok_or_else(|| AuthError::InvalidToken {
                reason: "no pending link for token".into(),
            })?;
        if record.is_expired() {
            return Err(AuthError::InvalidToken {
                reason: "link has expired".into(),
            });
        }
        Ok(email)
    }
}

// ─── Email delivery ───────────────────────────────────────────────────────────

/// Abstract email delivery backend.
// Reason: used as dyn Trait (Arc<dyn EmailDelivery>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait EmailDelivery: Send + Sync {
    /// Send an `OTP` code, and the link token that redeems it without the address, to the
    /// given email address. A sender with a magic-link template puts the token in the link.
    ///
    /// Returns a message identifier that can be returned to the caller for
    /// tracking / idempotency (may be empty for noop implementations).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if delivery fails.
    async fn send_otp(&self, email: &str, issue: &OtpIssue) -> Result<String>;
}

/// No-op email delivery — logs the `OTP` via `tracing` instead of sending real email.
///
/// Suitable for development and testing.
pub struct NoopEmailDelivery;

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl EmailDelivery for NoopEmailDelivery {
    async fn send_otp(&self, email: &str, issue: &OtpIssue) -> Result<String> {
        let code = &issue.code;
        // The link token is not logged: unlike the code it redeems without the address.
        tracing::info!(email, code, "NoopEmailDelivery: OTP code (NOT sent via real email)");
        // Return a deterministic fake message_id for test assertions.
        Ok(format!("noop-{email}-{code}"))
    }
}

// ─── Route state ─────────────────────────────────────────────────────────────

/// Axum state for `OTP` routes.
#[derive(Clone)]
pub struct OtpRouteState {
    /// `OTP` code store.
    pub otp_store:       Arc<dyn OtpStore>,
    /// Email delivery backend.
    pub email_delivery:  Arc<dyn EmailDelivery>,
    /// Session store (to create sessions after successful verify).
    pub session_store:   Arc<dyn SessionStore>,
    /// Account store resolving the verified email to a stable local user (#367).
    ///
    /// `None` mints the legacy `otp:<email>` pseudo-identity, which is **not a
    /// real account**: it never links to the same person's social or
    /// local-password sign-in, so the same human ends up with as many
    /// identities as sign-in methods. The server always supplies one; the
    /// option exists for library callers with their own identity model.
    pub account_store:   Option<Arc<dyn crate::account_linking::AccountStore>>,
    /// Who may sign in (`[auth.local] otp_allowed_query`, #1852). `None` admits every address
    /// and delivers on the request.
    pub allow_gate:      Option<OtpAllowGate>,
    /// Cookie transport (`[auth.local] session_transport = "cookie"`): the session goes in
    /// `HttpOnly` cookies and no body carries a token. `None` answers with the tokens as
    /// `JSON`.
    pub session_cookies: Option<crate::SessionCookies>,
}

// ─── Request / Response types ─────────────────────────────────────────────────

/// Request body for `POST /auth/v1/otp`.
#[derive(Debug, Deserialize)]
pub struct OtpRequest {
    /// Destination email address.
    pub email: String,
}

/// Response body for `POST /auth/v1/otp`.
#[derive(Debug, Serialize)]
pub struct OtpResponse {
    /// Delivery message identifier (opaque; useful for debugging / idempotency).
    pub message_id: String,
}

/// Request body for `POST /auth/v1/verify`: a link token, or an address and its code.
///
/// No `Debug`: it carries a live credential.
#[derive(Deserialize)]
pub struct VerifyRequest {
    /// Email address the `OTP` was sent to, with `code`.
    #[serde(default)]
    pub email: Option<String>,
    /// The 6-digit code, with `email`.
    #[serde(default)]
    pub code:  Option<String>,
    /// The link token from a magic link, alone.
    #[serde(default)]
    pub token: Option<String>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /auth/v1/otp`
///
/// Generates a 6-digit `OTP`, stores it, and delivers it via the configured
/// email backend. Returns 200 with a `message_id` on success.
///
/// Returns 429 if the per-email send rate limit is exceeded.
///
/// Under an [`OtpAllowGate`] an address the list refuses still gets a stored code, and so the
/// same budgets and the same answers as one it admits; only the mail is withheld. The mail
/// then leaves off the request, so neither its latency nor its failure answers who is approved.
///
/// # Errors
///
/// Returns 422 Unprocessable Entity if the email is blank.
/// Returns 429 Too Many Requests if the rate limit is exceeded.
/// Returns 500 Internal Server Error if delivery fails.
pub async fn otp_send(
    State(state): State<Arc<OtpRouteState>>,
    Json(req): Json<OtpRequest>,
) -> Response {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "invalid_email",
                "message": "email must not be blank"
            })),
        )
            .into_response();
    }

    let issue = match state.otp_store.create_otp(&email).await {
        Ok(issue) => issue,
        Err(AuthError::RateLimited { retry_after_secs }) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error":             "rate_limited",
                    "retry_after_secs": retry_after_secs
                })),
            )
                .into_response();
        },
        Err(e) => {
            tracing::error!(error = %e, "OTP store error");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        },
    };

    let message_id = if let Some(gate) = &state.allow_gate {
        gate.deliver_if_admitted(&email, issue).await;
        String::new()
    } else {
        match state.email_delivery.send_otp(&email, &issue).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(error = %e, "Email delivery failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, "delivery failed").into_response();
            },
        }
    };

    let logger = get_audit_logger();
    logger.log_success(
        AuditEventType::OauthStart,
        SecretType::CsrfToken,
        None,
        &format!("otp_send:{email}"),
    );

    (StatusCode::OK, Json(OtpResponse { message_id })).into_response()
}

/// The answer to a wrong, expired or refused code: one body, so they cannot be told apart.
fn invalid_otp() -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "error":   "invalid_otp",
            "message": "invalid or expired OTP code"
        })),
    )
        .into_response()
}

/// The answer to a request that is neither a link token nor an address and its code.
fn invalid_request() -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "error":   "invalid_request",
            "message": "send either `token`, or `email` and `code`"
        })),
    )
        .into_response()
}

/// `POST /auth/v1/verify`
///
/// Redeems a magic link's `token`, or an `email` and its 6-digit `code`, and issues a session
/// token on success. Only a `POST` redeems: a link scanner's `GET` is refused by the router.
///
/// # Errors
///
/// Returns 422 Unprocessable Entity if the token or code is invalid, spent or expired (one
/// answer for all of them), or if the request is neither shape.
/// Returns 429 Too Many Requests if the attempt rate limit is exceeded.
pub async fn otp_verify(
    State(state): State<Arc<OtpRouteState>>,
    Json(req): Json<VerifyRequest>,
) -> Response {
    let logger = get_audit_logger();

    let redeemed = match (req.token, req.email, req.code) {
        (Some(token), None, None) => state.otp_store.redeem_link(&token).await,
        (None, Some(email), Some(code)) => {
            let email = email.trim().to_lowercase();
            state.otp_store.verify_otp(&email, &code).await.map(|()| email)
        },
        _ => return invalid_request(),
    };
    let email = match redeemed {
        Ok(email) => email,
        Err(AuthError::RateLimited { retry_after_secs }) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error":             "rate_limited",
                    "retry_after_secs": retry_after_secs
                })),
            )
                .into_response();
        },
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::CsrfToken,
                None,
                "otp_verify",
                &e.to_string(),
            );
            return invalid_otp();
        },
    };

    // Checked again here, not only at send: removing an address invalidates a code it already
    // holds, and a code the list refused at send was never mailed. Same answer as a wrong code.
    if let Some(gate) = &state.allow_gate {
        if !gate.admits(&email).await {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::CsrfToken,
                None,
                "otp_verify",
                "address refused by otp_allowed_query",
            );
            return invalid_otp();
        }
    }

    // OTP verified — resolve the local account (#367).
    //
    // Completing the flow proves control of the mailbox, so the email is
    // verified by construction: no provider asserted it, FraiseQL observed it.
    // That is what makes email-keyed linking correct here — the same person
    // signing in later via Google or a local password lands on this account
    // instead of a parallel one.
    let user_id = match &state.account_store {
        Some(accounts) => {
            match accounts
                .link_or_create_user(None, Some(&email), true, OTP_PROVIDER, &email)
                .await
            {
                Ok(result) => result.user_id,
                Err(e) => {
                    tracing::error!(error = %e, "OTP account resolution failed");
                    return (StatusCode::INTERNAL_SERVER_ERROR, "user resolution failed")
                        .into_response();
                },
            }
        },
        None => format!("otp:{email}"),
    };
    let expires_at = match unix_now() {
        Ok(now) => now + 3_600, // 1-hour session
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
    };

    let tokens = match state.session_store.create_session(&user_id, expires_at).await {
        Ok(t) => t,
        Err(e) => {
            if let Some(resp) = crate::totp_mfa::second_factor_response(&e) {
                return resp;
            }
            tracing::error!(error = %e, "Session creation failed after OTP verify");
            return (StatusCode::INTERNAL_SERVER_ERROR, "session creation failed").into_response();
        },
    };

    logger.log_success(
        AuditEventType::AuthSuccess,
        SecretType::SessionToken,
        Some(user_id),
        "otp_verify",
    );

    crate::session_cookies::session_response(state.session_cookies.as_ref(), tokens)
}

#[cfg(test)]
mod tests;
