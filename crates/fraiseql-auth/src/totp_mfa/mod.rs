//! `TOTP` `MFA` (RFC 6238) — enroll, challenge, verify, unenroll.
//!
//! Provides a full `TOTP`-based multi-factor authentication flow:
//!
//! 1. **Enroll** (`POST /auth/v1/mfa/enroll`) — generates a `TOTP` secret and 8 single-use recovery
//!    codes for the signed-in account. Returns an `otpauth://` `URI` for the authenticator app.
//!    Replacing a confirmed enrollment needs a current code from it.
//! 2. **Challenge** — a first factor (password, `OTP`, a social or `SAML` sign-in) for a user with
//!    a confirmed enrollment mints no session. It answers `401` with `"error": "mfa_required"` and
//!    a short-lived `challenge_token` bound to that user, issued where every path mints its session
//!    (`PostgresSessionStore::create_session`), so no path can skip it.
//! 3. **Verify** (`POST /auth/v1/mfa/verify`) — redeems the challenge with a `TOTP` code or
//!    recovery code and issues a full session token pair.
//! 4. **Unenroll** (`POST /auth/v1/mfa/unenroll`) — removes `MFA` from the signed-in account
//!    (requires the current `TOTP` code or a recovery code for re-authentication).
//!
//! # Security
//!
//! - Enroll, confirm and unenroll act on the account the caller's session was minted for, the
//!   [`SessionSubject`] the server's authentication layer resolves. They never take an account from
//!   the request: a handler that did would let anyone act on anyone's factor.
//! - `TOTP` uses `SHA-1`, 6-digit codes, and a 30-second window with `±1` step tolerance (RFC 6238
//!   §5.2).
//! - Recovery codes are 16 random hex characters (64 bits of entropy) and are `bcrypt`-hashed at
//!   rest.
//! - Challenge tokens are 32-byte random values with a 5-minute `TTL`.
//! - Verification is attempt-limited: a challenge is consumed after 5 wrong codes, and a user who
//!   accumulates 10 failures inside a 15-minute sliding window can neither mint new challenges nor
//!   unenroll until the window elapses. Without both limits a 6-digit code is brute-forceable
//!   inside the challenge `TTL` — the per-challenge cap alone does not bind, because challenges can
//!   be re-minted freely.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
    session::{SessionStore, unix_now},
};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Number of recovery codes generated at enrollment.
pub(super) const RECOVERY_CODE_COUNT: usize = 8;

/// Length of each recovery code (16 hex chars = 64-bit entropy).
const RECOVERY_CODE_HEX_LEN: usize = 16;

/// Challenge token `TTL` in seconds (5 minutes).
pub(super) const CHALLENGE_TTL_SECS: u64 = 300;

/// `TOTP` tolerance: ±1 step around the current 30-second window (RFC 6238 §5.2).
const TOTP_STEP_TOLERANCE: u8 = 1;

/// Wrong codes tolerated against a single challenge before the challenge is consumed.
///
/// With `±1` step tolerance a 6-digit `TOTP` has 3 valid codes out of 10^6, so an
/// unbounded challenge is brute-forceable inside its 5-minute `TTL`.
pub(super) const MAX_CHALLENGE_ATTEMPTS: u32 = 5;

/// Failed verifications tolerated per user inside [`FAILURE_WINDOW_SECS`] before
/// further challenges are refused.
///
/// The per-challenge cap alone is not enough: challenges can be re-minted, so the
/// budget has to be tracked against the user, not the token.
pub(super) const MAX_USER_FAILURES: u32 = 10;

/// Sliding-window length for the per-user failure budget (15 minutes).
pub(super) const FAILURE_WINDOW_SECS: u64 = 900;

/// `bcrypt` cost factor.
///
/// 12 is the recommended production minimum; lowered to 4 in tests to keep the
/// suite fast.  This is the only deviation from the prod constant.
#[cfg(not(test))]
pub(super) const BCRYPT_COST: u32 = 12;
#[cfg(test)]
pub(super) const BCRYPT_COST: u32 = 4;

// ─── Domain types ─────────────────────────────────────────────────────────────

/// `TOTP` enrollment record for a single user.
#[derive(Debug, Clone)]
pub struct TotpEnrollment {
    /// Base32-encoded `TOTP` secret.
    pub secret_base32:        String,
    /// `bcrypt` hashes of the 8 recovery codes.
    pub recovery_code_hashes: Vec<String>,
    /// Whether enrollment has been confirmed (first `TOTP` code verified).
    pub confirmed:            bool,
}

/// A challenge a first factor earned: the caller redeems it with a current code at
/// `/auth/v1/mfa/verify`. Printed by neither `Debug` nor `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct ChallengeToken(String);

impl ChallengeToken {
    /// The token, for the answer to the caller who passed the first factor.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ChallengeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChallengeToken(..)")
    }
}

/// Proof that a user has just passed the second factor.
///
/// Only a redeemed challenge makes one, and
/// [`SessionStore::create_session_after_second_factor`] takes nothing else, so a session for
/// a user enrolled in `MFA` cannot be minted around it.
#[derive(Debug)]
pub struct SecondFactorPassed {
    user_id: String,
}

impl SecondFactorPassed {
    /// The user who passed it.
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
}

/// Pending `MFA` challenge record.
#[derive(Debug, Clone)]
struct ChallengeRecord {
    /// Which user the challenge was issued for.
    user_id:  String,
    /// Unix timestamp when the challenge expires.
    expires:  u64,
    /// Wrong codes submitted against this challenge so far.
    attempts: u32,
}

/// Per-user failed-verification budget over a sliding window.
#[derive(Debug, Clone)]
struct FailureRecord {
    /// Failures recorded since `window_start`.
    count:        u32,
    /// Unix timestamp the current window opened at.
    window_start: u64,
}

// ─── MfaStore trait ───────────────────────────────────────────────────────────

/// Storage backend for `TOTP` `MFA` state.
// Reason: used as dyn Trait (Arc<dyn MfaStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait MfaStore: Send + Sync {
    /// Begin enrollment: generate and store a `TOTP` secret + recovery codes.
    ///
    /// Returns `(secret_base32, otpauth_uri, recovery_codes_plaintext)`.
    /// The plaintext recovery codes are returned **once** and never stored; only
    /// their `bcrypt` hashes are persisted.
    ///
    /// A confirmed enrollment is replaced only on `current_code`, a `TOTP` or recovery
    /// code of that enrollment: a session alone does not prove control of the factor it
    /// would remove. A wrong code is charged to the user's failure budget.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] when a confirmed enrollment exists and
    /// `current_code` is absent or wrong, [`AuthError::RateLimited`] while the user's
    /// failure budget is spent, and [`AuthError::DatabaseError`] if the store fails.
    async fn begin_enrollment(
        &self,
        user_id: &str,
        issuer: &str,
        account_name: &str,
        current_code: Option<&str>,
    ) -> Result<EnrollmentResponse>;

    /// Complete enrollment by verifying the first `TOTP` code.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if no pending enrollment exists or
    /// if the code is wrong.
    async fn confirm_enrollment(&self, user_id: &str, totp_code: &str) -> Result<()>;

    /// Delete challenge rows whose expiry has passed. Returns the number removed.
    ///
    /// Required rather than defaulted: a store that cannot sweep must say so in its own
    /// body, because a default no-op would let a new backend inherit unbounded growth
    /// silently (#950). Every read already checks expiry, so this is about table size,
    /// not correctness — and it must never touch an unexpired row.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the store fails.
    async fn sweep_expired(&self) -> Result<u64>;

    /// Issue a `MFA` challenge token for the given user after first-factor auth.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the store fails.
    async fn create_challenge(&self, user_id: &str) -> Result<String>;

    /// Verify a challenge token + `TOTP`/recovery code and consume the challenge.
    ///
    /// Returns the `user_id` on success.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the challenge or code is wrong/expired.
    async fn verify_challenge(&self, challenge_token: &str, code: &str) -> Result<String>;

    /// Remove `MFA` enrollment for a user (requires valid `TOTP` or recovery code).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if the code is wrong or `MFA` is not enrolled.
    async fn unenroll(&self, user_id: &str, code: &str) -> Result<()>;

    /// Return `true` if the user has an active (confirmed) `MFA` enrollment.
    ///
    /// An error reads as `false`, so this answers a display question only: it must not
    /// decide whether a sign-in owes the second factor.
    async fn is_enrolled(&self, user_id: &str) -> bool;
}

/// Response from [`MfaStore::begin_enrollment`].
#[derive(Debug)]
pub struct EnrollmentResponse {
    /// Base32-encoded `TOTP` secret (to display in QR code).
    pub secret_base32:  String,
    /// `otpauth://` `URI` for authenticator apps.
    pub otpauth_uri:    String,
    /// Plaintext recovery codes — show to the user **once**, never stored.
    pub recovery_codes: Vec<String>,
}

pub mod postgres;

// ─── In-memory MFA store ──────────────────────────────────────────────────────

/// Thread-safe in-memory `MFA` store.
pub struct InMemoryMfaStore {
    /// user_id → TotpEnrollment
    enrollments: DashMap<String, TotpEnrollment>,
    /// challenge_token → ChallengeRecord
    challenges:  DashMap<String, ChallengeRecord>,
    /// user_id → FailureRecord (brute-force budget)
    failures:    DashMap<String, FailureRecord>,
}

impl InMemoryMfaStore {
    /// Create a new empty `MFA` store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enrollments: DashMap::new(),
            challenges:  DashMap::new(),
            failures:    DashMap::new(),
        }
    }

    /// Return whether the user has a pending (unconfirmed) enrollment.
    #[must_use]
    pub fn has_pending_enrollment(&self, user_id: &str) -> bool {
        self.enrollments.get(user_id).is_some_and(|e| !e.confirmed)
    }

    /// Seconds left in the current lockout, or `None` if the user is not locked out.
    fn lockout_remaining(&self, user_id: &str, now: u64) -> Option<u64> {
        let record = self.failures.get(user_id)?;
        let elapsed = now.saturating_sub(record.window_start);
        (record.count >= MAX_USER_FAILURES && elapsed < FAILURE_WINDOW_SECS)
            .then(|| FAILURE_WINDOW_SECS - elapsed)
    }

    /// Charge one failed verification against `user_id`, opening a fresh window
    /// if the previous one has elapsed.
    fn record_failure(&self, user_id: &str, now: u64) {
        let mut record = self.failures.entry(user_id.to_string()).or_insert(FailureRecord {
            count:        0,
            window_start: now,
        });
        if now.saturating_sub(record.window_start) >= FAILURE_WINDOW_SECS {
            record.count = 0;
            record.window_start = now;
        }
        record.count = record.count.saturating_add(1);
    }
}

impl Default for InMemoryMfaStore {
    fn default() -> Self {
        Self::new()
    }
}

// ─── TOTP helpers ─────────────────────────────────────────────────────────────

/// Build a [`TOTP`] instance from a base32 secret string.
///
/// Uses `SHA-1`, 6 digits, 30-second step (RFC 6238 defaults).
/// `issuer` and `account_name` are embedded in the `otpauth://` URI; for
/// verification-only callers pass `None` and `""`.
pub(super) fn build_totp(
    secret_base32: &str,
    issuer: Option<&str>,
    account_name: &str,
) -> Result<TOTP> {
    let secret_bytes =
        Secret::Encoded(secret_base32.to_string())
            .to_bytes()
            .map_err(|e| AuthError::Internal {
                message: format!("bad TOTP secret: {e}"),
            })?;
    TOTP::new(
        Algorithm::SHA1,
        6, // digits
        TOTP_STEP_TOLERANCE,
        30, // period (seconds)
        secret_bytes,
        issuer.map(str::to_string),
        account_name.to_string(),
    )
    .map_err(|e| AuthError::Internal {
        message: format!("TOTP init error: {e}"),
    })
}

/// Verify a `TOTP` code with `±1` step tolerance.
pub(super) fn verify_totp_code(secret_base32: &str, code: &str) -> Result<bool> {
    let totp = build_totp(secret_base32, None, "")?;
    Ok(totp.check_current(code).unwrap_or(false))
}

/// Generate a random recovery code (`RECOVERY_CODE_HEX_LEN` lowercase hex chars).
pub(super) fn generate_recovery_code() -> String {
    // SECURITY: rand::rng() uses OS-level entropy for recovery codes.
    // Each byte encodes as 2 hex chars, so RECOVERY_CODE_HEX_LEN / 2 bytes.
    let byte_count = RECOVERY_CODE_HEX_LEN / 2;
    let mut bytes = vec![0u8; byte_count];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Generate a 32-byte random challenge token (URL-safe base64).
pub(super) fn generate_challenge_token() -> String {
    use base64::Engine as _;
    // SECURITY: rand::rng() uses OS-level entropy for MFA challenge tokens.
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Check a candidate code against a list of `bcrypt` hashes.
///
/// Returns the index of the matching hash if found.
pub(super) fn check_recovery_code(candidate: &str, hashes: &[String]) -> Option<usize> {
    for (i, hash) in hashes.iter().enumerate() {
        if bcrypt::verify(candidate, hash).unwrap_or(false) {
            return Some(i);
        }
    }
    None
}

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl MfaStore for InMemoryMfaStore {
    async fn begin_enrollment(
        &self,
        user_id: &str,
        issuer: &str,
        account_name: &str,
        current_code: Option<&str>,
    ) -> Result<EnrollmentResponse> {
        let now = unix_now()?;
        let confirmed = self.enrollments.get(user_id).filter(|e| e.confirmed).map(|e| e.clone());
        if let Some(enrollment) = confirmed {
            if let Some(retry_after_secs) = self.lockout_remaining(user_id, now) {
                return Err(AuthError::RateLimited { retry_after_secs });
            }
            let proven = match current_code {
                Some(code) => {
                    verify_totp_code(&enrollment.secret_base32, code)?
                        || check_recovery_code(code, &enrollment.recovery_code_hashes).is_some()
                },
                None => false,
            };
            if !proven {
                if current_code.is_some() {
                    self.record_failure(user_id, now);
                }
                return Err(AuthError::InvalidToken {
                    reason: "replacing a confirmed MFA enrollment needs a current code".into(),
                });
            }
        }

        // Generate a new TOTP secret.
        let secret = Secret::generate_secret();
        let secret_base32 = secret.to_encoded().to_string();

        // Build the otpauth:// URI (issuer + account_name are embedded in the URI).
        let totp = build_totp(&secret_base32, Some(issuer), account_name)?;
        let otpauth_uri = totp.get_url();

        // Generate 8 recovery codes and bcrypt-hash them.
        let mut recovery_codes_plain = Vec::with_capacity(RECOVERY_CODE_COUNT);
        let mut recovery_code_hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);
        for _ in 0..RECOVERY_CODE_COUNT {
            let code = generate_recovery_code();
            let hash = bcrypt::hash(&code, BCRYPT_COST).map_err(|e| AuthError::Internal {
                message: format!("bcrypt error: {e}"),
            })?;
            recovery_codes_plain.push(code);
            recovery_code_hashes.push(hash);
        }

        self.enrollments.insert(
            user_id.to_string(),
            TotpEnrollment {
                secret_base32: secret_base32.clone(),
                recovery_code_hashes,
                confirmed: false,
            },
        );

        Ok(EnrollmentResponse {
            secret_base32,
            otpauth_uri,
            recovery_codes: recovery_codes_plain,
        })
    }

    async fn sweep_expired(&self) -> Result<u64> {
        let now = unix_now()?;
        let before = self.challenges.len();
        self.challenges.retain(|_, c| c.expires > now);
        Ok((before - self.challenges.len()) as u64)
    }

    async fn confirm_enrollment(&self, user_id: &str, totp_code: &str) -> Result<()> {
        let mut record =
            self.enrollments.get_mut(user_id).ok_or_else(|| AuthError::InvalidToken {
                reason: "no pending MFA enrollment for user".into(),
            })?;

        if !verify_totp_code(&record.secret_base32, totp_code)? {
            return Err(AuthError::InvalidToken {
                reason: "invalid TOTP code".into(),
            });
        }
        record.confirmed = true;
        Ok(())
    }

    async fn create_challenge(&self, user_id: &str) -> Result<String> {
        let now = unix_now()?;

        // Refuse to mint a fresh challenge for a user who has burned through the
        // failure budget — otherwise the per-challenge cap is trivially reset.
        if let Some(retry_after_secs) = self.lockout_remaining(user_id, now) {
            return Err(AuthError::RateLimited { retry_after_secs });
        }

        let expires = now + CHALLENGE_TTL_SECS;
        let token = generate_challenge_token();
        self.challenges.insert(
            token.clone(),
            ChallengeRecord {
                user_id: user_id.to_string(),
                expires,
                attempts: 0,
            },
        );
        Ok(token)
    }

    async fn verify_challenge(&self, challenge_token: &str, code: &str) -> Result<String> {
        let now = unix_now()?;

        let record =
            self.challenges.get(challenge_token).ok_or_else(|| AuthError::InvalidToken {
                reason: "unknown challenge token".into(),
            })?;

        if now >= record.expires {
            drop(record);
            self.challenges.remove(challenge_token);
            return Err(AuthError::InvalidToken {
                reason: "challenge token expired".into(),
            });
        }

        let user_id = record.user_id.clone();
        drop(record);

        if let Some(retry_after_secs) = self.lockout_remaining(&user_id, now) {
            self.challenges.remove(challenge_token);
            return Err(AuthError::RateLimited { retry_after_secs });
        }

        // Look up the user's TOTP enrollment.
        let mut enrollment =
            self.enrollments.get_mut(&user_id).ok_or_else(|| AuthError::InvalidToken {
                reason: "user has no MFA enrollment".into(),
            })?;

        if !enrollment.confirmed {
            return Err(AuthError::InvalidToken {
                reason: "MFA enrollment not confirmed".into(),
            });
        }

        // Try TOTP first, then recovery codes.
        if verify_totp_code(&enrollment.secret_base32, code)? {
            drop(enrollment);
            self.challenges.remove(challenge_token);
            self.failures.remove(&user_id);
            return Ok(user_id);
        }

        // Try recovery codes (bcrypt, slow — intentional).
        let idx = check_recovery_code(code, &enrollment.recovery_code_hashes);
        if let Some(i) = idx {
            // Consume (remove) the used recovery code.
            enrollment.recovery_code_hashes.remove(i);
            drop(enrollment);
            self.challenges.remove(challenge_token);
            self.failures.remove(&user_id);
            return Ok(user_id);
        }
        drop(enrollment);

        // Wrong code: charge the per-user budget and burn one of the challenge's
        // attempts, consuming the challenge once they are exhausted.
        self.record_failure(&user_id, now);
        // The `get_mut` guard must be released before `remove`, or the two DashMap
        // operations deadlock on the same shard — hence the separate binding.
        let exhausted = if let Some(mut challenge) = self.challenges.get_mut(challenge_token) {
            challenge.attempts = challenge.attempts.saturating_add(1);
            challenge.attempts >= MAX_CHALLENGE_ATTEMPTS
        } else {
            true
        };
        if exhausted {
            self.challenges.remove(challenge_token);
        }

        Err(AuthError::InvalidToken {
            reason: "invalid TOTP or recovery code".into(),
        })
    }

    async fn unenroll(&self, user_id: &str, code: &str) -> Result<()> {
        let now = unix_now()?;

        // unenroll accepts the same secrets as verify_challenge, so it is the same
        // brute-force surface and shares the same budget.
        if let Some(retry_after_secs) = self.lockout_remaining(user_id, now) {
            return Err(AuthError::RateLimited { retry_after_secs });
        }

        let enrollment = self.enrollments.get(user_id).ok_or_else(|| AuthError::InvalidToken {
            reason: "user has no MFA enrollment".into(),
        })?;

        if !enrollment.confirmed {
            return Err(AuthError::InvalidToken {
                reason: "MFA enrollment not confirmed".into(),
            });
        }

        // Re-authenticate: accept TOTP or a recovery code.
        let totp_ok = verify_totp_code(&enrollment.secret_base32, code)?;
        let recovery_ok =
            !totp_ok && check_recovery_code(code, &enrollment.recovery_code_hashes).is_some();

        if !totp_ok && !recovery_ok {
            drop(enrollment);
            self.record_failure(user_id, now);
            return Err(AuthError::InvalidToken {
                reason: "re-authentication failed — invalid TOTP or recovery code".into(),
            });
        }

        drop(enrollment);
        self.enrollments.remove(user_id);
        self.failures.remove(user_id);
        Ok(())
    }

    async fn is_enrolled(&self, user_id: &str) -> bool {
        self.enrollments.get(user_id).map_or(false, |e| e.confirmed)
    }
}

// ─── Route state ─────────────────────────────────────────────────────────────

/// Axum route state for `MFA` endpoints.
#[derive(Clone)]
pub struct MfaRouteState {
    /// `MFA` storage backend.
    pub mfa_store:      Arc<dyn MfaStore>,
    /// Session store (to issue full sessions after `MFA` verification).
    pub session_store:  Arc<dyn SessionStore>,
    /// Service / issuer name shown in authenticator apps.
    pub issuer:         String,
    /// Per-IP budget for `/verify`, keyed on the transport peer. The per-user failure budget
    /// stops one account's code being guessed; this stops one client guessing across many.
    pub verify_limiter: Arc<crate::rate_limiting::KeyedRateLimiter>,
}

// ─── Request / Response types ─────────────────────────────────────────────────

/// The account an authenticated session was minted for, which the server's
/// authentication layer puts on the request.
///
/// Enroll, confirm and unenroll act on this account and on no other: an account named in
/// a request body is a parameter, not an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSubject(pub String);

/// Request for `POST /auth/v1/mfa/enroll`. The account is the caller's own.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaEnrollRequest {
    /// Display name shown in the authenticator app.
    pub account_name: String,
    /// A current `TOTP` or recovery code of the confirmed enrollment this replaces.
    #[serde(default)]
    pub code:         Option<String>,
}

/// Response for `POST /auth/v1/mfa/enroll`.
#[derive(Debug, Serialize)]
pub struct MfaEnrollResponse {
    /// `otpauth://` `URI` — encode as a `QR` code for the authenticator app.
    pub otpauth_uri:    String,
    /// 8 single-use recovery codes (shown **once**, store securely).
    pub recovery_codes: Vec<String>,
}

/// Request for `POST /auth/v1/mfa/verify`.
#[derive(Debug, Deserialize)]
pub struct MfaVerifyRequest {
    /// Challenge token from the first factor's `mfa_required` answer.
    pub challenge_token: String,
    /// 6-digit `TOTP` code or one of the 8-digit recovery codes.
    pub code:            String,
}

/// Request for `POST /auth/v1/mfa/confirm`. The enrollment is the caller's own.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaConfirmRequest {
    /// The first `TOTP` code from the authenticator, proving the secret was stored.
    pub code: String,
}

/// Request for `POST /auth/v1/mfa/unenroll`. The enrollment is the caller's own.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MfaUnenrollRequest {
    /// Current `TOTP` code or a recovery code (re-authentication).
    pub code: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// The `401` for a request the authentication layer resolved to no one. A route mounted
/// without that layer refuses every request this way rather than acting on an account it
/// was never told.
fn authentication_required() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error":   "authentication_required",
            "message": "sign in to manage MFA"
        })),
    )
        .into_response()
}

/// Render a lockout as `429 Too Many Requests` with a `Retry-After` header.
///
/// Returns `None` for every other error so callers keep their own generic
/// response — only the retry window is safe to disclose.
fn rate_limited_response(err: &AuthError) -> Option<Response> {
    let AuthError::RateLimited { retry_after_secs } = err else {
        return None;
    };
    Some(
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
            Json(serde_json::json!({
                "error":   "too_many_attempts",
                "message": "too many failed MFA attempts — try again later"
            })),
        )
            .into_response(),
    )
}

/// The answer to a sign-in that passed its first factor and owes the second.
///
/// `401` with the challenge to redeem at `/auth/v1/mfa/verify`, or the `429` of a user
/// whose failure budget is spent. `None` for every other error.
///
/// Every route that mints a session renders its session-store error through this, so a
/// user enrolled in `MFA` is answered the same way whichever first factor they used.
#[must_use]
pub fn second_factor_response(err: &AuthError) -> Option<Response> {
    if let AuthError::SecondFactorRequired { challenge } = err {
        return Some(challenge_response(challenge));
    }
    rate_limited_response(err)
}

/// `401` with the challenge to redeem at `/auth/v1/mfa/verify`.
pub(crate) fn challenge_response(challenge: &ChallengeToken) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error":           "mfa_required",
            "message":         "a second factor is required: redeem challenge_token at /auth/v1/mfa/verify",
            "challenge_token": challenge.expose(),
        })),
    )
        .into_response()
}

/// `POST /auth/v1/mfa/enroll`
///
/// Generates a `TOTP` secret and recovery codes for the signed-in account.
///
/// # Errors
///
/// Returns 401 without a session, 422 when replacing a confirmed enrollment without a
/// current code of it, 429 while the failure budget is spent, and 500 if the `MFA` store
/// fails.
pub async fn mfa_enroll(
    State(state): State<Arc<MfaRouteState>>,
    subject: Option<axum::Extension<SessionSubject>>,
    Json(req): Json<MfaEnrollRequest>,
) -> Response {
    let Some(axum::Extension(SessionSubject(user_id))) = subject else {
        return authentication_required();
    };
    match state
        .mfa_store
        .begin_enrollment(&user_id, &state.issuer, &req.account_name, req.code.as_deref())
        .await
    {
        Ok(resp) => (
            StatusCode::OK,
            Json(MfaEnrollResponse {
                otpauth_uri:    resp.otpauth_uri,
                recovery_codes: resp.recovery_codes,
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(resp) = rate_limited_response(&e) {
                return resp;
            }
            if let AuthError::InvalidToken { .. } = e {
                get_audit_logger().log_failure(
                    AuditEventType::AuthFailure,
                    SecretType::SessionToken,
                    Some(user_id),
                    "mfa_enroll",
                    &e.to_string(),
                );
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "error":   "invalid_code",
                        "message": "replacing an MFA enrollment needs a current code from it"
                    })),
                )
                    .into_response();
            }
            tracing::error!(error = %e, "MFA enroll error");
            (StatusCode::INTERNAL_SERVER_ERROR, "enrollment failed").into_response()
        },
    }
}

/// `POST /auth/v1/mfa/verify`
///
/// Verifies the `TOTP` code or recovery code and issues a full session token pair.
///
/// # Errors
///
/// Returns 422 if the code is wrong or the challenge is expired.
pub async fn mfa_verify(
    State(state): State<Arc<MfaRouteState>>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<MfaVerifyRequest>,
) -> Response {
    let logger = get_audit_logger();
    if let Err(e) = state.verify_limiter.check(&peer.ip().to_string()) {
        logger.log_failure(
            AuditEventType::AuthFailure,
            SecretType::SessionToken,
            None,
            "mfa_verify",
            "rate limited",
        );
        if let Some(resp) = rate_limited_response(&e) {
            return resp;
        }
        return (StatusCode::TOO_MANY_REQUESTS, "rate limited").into_response();
    }

    let user_id = match state.mfa_store.verify_challenge(&req.challenge_token, &req.code).await {
        Ok(uid) => uid,
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "mfa_verify",
                &e.to_string(),
            );
            if let Some(resp) = rate_limited_response(&e) {
                return resp;
            }
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":   "invalid_mfa",
                    "message": "invalid or expired MFA code"
                })),
            )
                .into_response();
        },
    };

    let expires_at = match unix_now() {
        Ok(now) => now + 3_600, // 1-hour session
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
    };

    let passed = SecondFactorPassed {
        user_id: user_id.clone(),
    };
    let tokens =
        match state.session_store.create_session_after_second_factor(passed, expires_at).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "Session creation failed after MFA verify");
                return (StatusCode::INTERNAL_SERVER_ERROR, "session creation failed")
                    .into_response();
            },
        };

    logger.log_success(
        AuditEventType::AuthSuccess,
        SecretType::SessionToken,
        Some(user_id),
        "mfa_verify",
    );

    (StatusCode::OK, Json(tokens)).into_response()
}

/// `POST /auth/v1/mfa/confirm`
///
/// Completes an enrollment begun by `/enroll` by verifying the first `TOTP` code.
/// Until this succeeds the enrollment stays `confirmed = false` and `create_challenge`
/// refuses to issue against it, so without this route `MFA` could be enrolled through
/// the API and never used (#950).
///
/// # Errors
///
/// Returns 401 without a session, and 422 if there is no pending enrollment or the code
/// is wrong.
pub async fn mfa_confirm(
    State(state): State<Arc<MfaRouteState>>,
    subject: Option<axum::Extension<SessionSubject>>,
    Json(req): Json<MfaConfirmRequest>,
) -> Response {
    let Some(axum::Extension(SessionSubject(user_id))) = subject else {
        return authentication_required();
    };
    let logger = get_audit_logger();

    match state.mfa_store.confirm_enrollment(&user_id, &req.code).await {
        Ok(()) => {
            logger.log_success(
                AuditEventType::AuthSuccess,
                SecretType::SessionToken,
                Some(user_id),
                "mfa_confirm",
            );
            StatusCode::OK.into_response()
        },
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.clone()),
                "mfa_confirm",
                &e.to_string(),
            );
            if let Some(resp) = rate_limited_response(&e) {
                return resp;
            }
            // One answer for "no pending enrollment" and "wrong code" alike.
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":   "invalid_code",
                    "message": "enrollment confirmation failed"
                })),
            )
                .into_response()
        },
    }
}

/// `POST /auth/v1/mfa/unenroll`
///
/// Removes `MFA` from the signed-in account after re-authentication.
///
/// # Errors
///
/// Returns 401 without a session, and 422 if re-authentication fails.
pub async fn mfa_unenroll(
    State(state): State<Arc<MfaRouteState>>,
    subject: Option<axum::Extension<SessionSubject>>,
    Json(req): Json<MfaUnenrollRequest>,
) -> Response {
    let Some(axum::Extension(SessionSubject(user_id))) = subject else {
        return authentication_required();
    };
    let logger = get_audit_logger();

    match state.mfa_store.unenroll(&user_id, &req.code).await {
        Ok(()) => {
            logger.log_success(
                AuditEventType::SessionTokenRevoked,
                SecretType::SessionToken,
                Some(user_id),
                "mfa_unenroll",
            );
            StatusCode::OK.into_response()
        },
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(user_id.clone()),
                "mfa_unenroll",
                &e.to_string(),
            );
            if let Some(resp) = rate_limited_response(&e) {
                return resp;
            }
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":   "invalid_code",
                    "message": "re-authentication failed"
                })),
            )
                .into_response()
        },
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
