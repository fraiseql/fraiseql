//! `TOTP` `MFA` (RFC 6238) — enroll, challenge, verify, unenroll.
//!
//! Provides a full `TOTP`-based multi-factor authentication flow:
//!
//! 1. **Enroll** (`POST /auth/v1/mfa/enroll`) — generates a `TOTP` secret and 8 single-use recovery
//!    codes. Returns an `otpauth://` `URI` for the authenticator app.
//! 2. **Challenge** (`POST /auth/v1/mfa/challenge`) — creates a short-lived challenge token after
//!    the first authentication factor is verified.
//! 3. **Verify** (`POST /auth/v1/mfa/verify`) — verifies a `TOTP` code or recovery code and issues
//!    a full session token pair.
//! 4. **Unenroll** (`POST /auth/v1/mfa/unenroll`) — removes `MFA` from an account (requires the
//!    current `TOTP` code or a recovery code for re-authentication).
//!
//! # Security
//!
//! - `TOTP` uses `SHA-1`, 6-digit codes, and a 30-second window with `±1` step tolerance (RFC 6238
//!   §5.2).
//! - Recovery codes are 16 random hex characters (64 bits of entropy) and are `bcrypt`-hashed at
//!   rest.
//! - Challenge tokens are 32-byte random values with a 5-minute `TTL`.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
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
const RECOVERY_CODE_COUNT: usize = 8;

/// Length of each recovery code (16 hex chars = 64-bit entropy).
const RECOVERY_CODE_HEX_LEN: usize = 16;

/// Challenge token `TTL` in seconds (5 minutes).
const CHALLENGE_TTL_SECS: u64 = 300;

/// `TOTP` tolerance: ±1 step around the current 30-second window (RFC 6238 §5.2).
const TOTP_STEP_TOLERANCE: u8 = 1;

/// `bcrypt` cost factor.
///
/// 12 is the recommended production minimum; lowered to 4 in tests to keep the
/// suite fast.  This is the only deviation from the prod constant.
#[cfg(not(test))]
const BCRYPT_COST: u32 = 12;
#[cfg(test)]
const BCRYPT_COST: u32 = 4;

// ─── Domain types ─────────────────────────────────────────────────────────────

/// `TOTP` enrollment record for a single user.
#[derive(Debug, Clone)]
pub struct TotpEnrollment {
    /// Base32-encoded `TOTP` secret.
    pub secret_base32: String,
    /// `bcrypt` hashes of the 8 recovery codes.
    pub recovery_code_hashes: Vec<String>,
    /// Whether enrollment has been confirmed (first `TOTP` code verified).
    pub confirmed: bool,
}

/// Pending `MFA` challenge record.
#[derive(Debug, Clone)]
struct ChallengeRecord {
    /// Which user the challenge was issued for.
    user_id: String,
    /// Unix timestamp when the challenge expires.
    expires: u64,
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
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the store fails.
    async fn begin_enrollment(
        &self,
        user_id: &str,
        issuer: &str,
        account_name: &str,
    ) -> Result<EnrollmentResponse>;

    /// Complete enrollment by verifying the first `TOTP` code.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::InvalidToken`] if no pending enrollment exists or
    /// if the code is wrong.
    async fn confirm_enrollment(&self, user_id: &str, totp_code: &str) -> Result<()>;

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
    async fn is_enrolled(&self, user_id: &str) -> bool;
}

/// Response from [`MfaStore::begin_enrollment`].
#[derive(Debug)]
pub struct EnrollmentResponse {
    /// Base32-encoded `TOTP` secret (to display in QR code).
    pub secret_base32: String,
    /// `otpauth://` `URI` for authenticator apps.
    pub otpauth_uri: String,
    /// Plaintext recovery codes — show to the user **once**, never stored.
    pub recovery_codes: Vec<String>,
}

// ─── In-memory MFA store ──────────────────────────────────────────────────────

/// Thread-safe in-memory `MFA` store.
pub struct InMemoryMfaStore {
    /// user_id → TotpEnrollment
    enrollments: DashMap<String, TotpEnrollment>,
    /// challenge_token → ChallengeRecord
    challenges: DashMap<String, ChallengeRecord>,
}

impl InMemoryMfaStore {
    /// Create a new empty `MFA` store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enrollments: DashMap::new(),
            challenges: DashMap::new(),
        }
    }

    /// Return whether the user has a pending (unconfirmed) enrollment.
    #[must_use]
    pub fn has_pending_enrollment(&self, user_id: &str) -> bool {
        self.enrollments.get(user_id).is_some_and(|e| !e.confirmed)
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
fn build_totp(secret_base32: &str, issuer: Option<&str>, account_name: &str) -> Result<TOTP> {
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
fn verify_totp_code(secret_base32: &str, code: &str) -> Result<bool> {
    let totp = build_totp(secret_base32, None, "")?;
    Ok(totp.check_current(code).unwrap_or(false))
}

/// Generate a random recovery code (`RECOVERY_CODE_HEX_LEN` lowercase hex chars).
fn generate_recovery_code() -> String {
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
fn generate_challenge_token() -> String {
    use base64::Engine as _;
    // SECURITY: rand::rng() uses OS-level entropy for MFA challenge tokens.
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Check a candidate code against a list of `bcrypt` hashes.
///
/// Returns the index of the matching hash if found.
fn check_recovery_code(candidate: &str, hashes: &[String]) -> Option<usize> {
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
    ) -> Result<EnrollmentResponse> {
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
        let expires = unix_now()? + CHALLENGE_TTL_SECS;
        let token = generate_challenge_token();
        self.challenges.insert(
            token.clone(),
            ChallengeRecord {
                user_id: user_id.to_string(),
                expires,
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
            return Ok(user_id);
        }

        // Try recovery codes (bcrypt, slow — intentional).
        let idx = check_recovery_code(code, &enrollment.recovery_code_hashes);
        if let Some(i) = idx {
            // Consume (remove) the used recovery code.
            enrollment.recovery_code_hashes.remove(i);
            drop(enrollment);
            self.challenges.remove(challenge_token);
            return Ok(user_id);
        }

        Err(AuthError::InvalidToken {
            reason: "invalid TOTP or recovery code".into(),
        })
    }

    async fn unenroll(&self, user_id: &str, code: &str) -> Result<()> {
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
            return Err(AuthError::InvalidToken {
                reason: "re-authentication failed — invalid TOTP or recovery code".into(),
            });
        }

        drop(enrollment);
        self.enrollments.remove(user_id);
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
    pub mfa_store: Arc<dyn MfaStore>,
    /// Session store (to issue full sessions after `MFA` verification).
    pub session_store: Arc<dyn SessionStore>,
    /// Service / issuer name shown in authenticator apps.
    pub issuer: String,
}

// ─── Request / Response types ─────────────────────────────────────────────────

/// Request for `POST /auth/v1/mfa/enroll`.
#[derive(Debug, Deserialize)]
pub struct MfaEnrollRequest {
    /// Authenticated user identifier.
    pub user_id: String,
    /// Display name shown in the authenticator app.
    pub account_name: String,
}

/// Response for `POST /auth/v1/mfa/enroll`.
#[derive(Debug, Serialize)]
pub struct MfaEnrollResponse {
    /// `otpauth://` `URI` — encode as a `QR` code for the authenticator app.
    pub otpauth_uri: String,
    /// 8 single-use recovery codes (shown **once**, store securely).
    pub recovery_codes: Vec<String>,
}

/// Request for `POST /auth/v1/mfa/challenge`.
#[derive(Debug, Deserialize)]
pub struct MfaChallengeRequest {
    /// User whose `MFA` challenge to initiate.
    pub user_id: String,
}

/// Response for `POST /auth/v1/mfa/challenge`.
#[derive(Debug, Serialize)]
pub struct MfaChallengeResponse {
    /// Short-lived challenge token (5 minutes).
    pub challenge_token: String,
}

/// Request for `POST /auth/v1/mfa/verify`.
#[derive(Debug, Deserialize)]
pub struct MfaVerifyRequest {
    /// Challenge token from the `/challenge` step.
    pub challenge_token: String,
    /// 6-digit `TOTP` code or one of the 8-digit recovery codes.
    pub code: String,
}

/// Request for `POST /auth/v1/mfa/unenroll`.
#[derive(Debug, Deserialize)]
pub struct MfaUnenrollRequest {
    /// User to unenroll.
    pub user_id: String,
    /// Current `TOTP` code or a recovery code (re-authentication).
    pub code: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /auth/v1/mfa/enroll`
///
/// Generates a `TOTP` secret and recovery codes for the given user.
///
/// # Errors
///
/// Returns 500 if the `MFA` store fails.
pub async fn mfa_enroll(
    State(state): State<Arc<MfaRouteState>>,
    Json(req): Json<MfaEnrollRequest>,
) -> Response {
    match state
        .mfa_store
        .begin_enrollment(&req.user_id, &state.issuer, &req.account_name)
        .await
    {
        Ok(resp) => (
            StatusCode::OK,
            Json(MfaEnrollResponse {
                otpauth_uri: resp.otpauth_uri,
                recovery_codes: resp.recovery_codes,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "MFA enroll error");
            (StatusCode::INTERNAL_SERVER_ERROR, "enrollment failed").into_response()
        },
    }
}

/// `POST /auth/v1/mfa/challenge`
///
/// Initiates a `MFA` challenge for the given user (called after first-factor auth).
///
/// # Errors
///
/// Returns 404 if the user has no confirmed `MFA` enrollment.
pub async fn mfa_challenge(
    State(state): State<Arc<MfaRouteState>>,
    Json(req): Json<MfaChallengeRequest>,
) -> Response {
    if !state.mfa_store.is_enrolled(&req.user_id).await {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_enrolled",
                "message": "user has no active MFA enrollment"
            })),
        )
            .into_response();
    }

    match state.mfa_store.create_challenge(&req.user_id).await {
        Ok(token) => (
            StatusCode::OK,
            Json(MfaChallengeResponse {
                challenge_token: token,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "MFA challenge error");
            (StatusCode::INTERNAL_SERVER_ERROR, "challenge creation failed").into_response()
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
    Json(req): Json<MfaVerifyRequest>,
) -> Response {
    let logger = get_audit_logger();

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

    let tokens = match state.session_store.create_session(&user_id, expires_at).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "Session creation failed after MFA verify");
            return (StatusCode::INTERNAL_SERVER_ERROR, "session creation failed").into_response();
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

/// `POST /auth/v1/mfa/unenroll`
///
/// Removes `MFA` from the account after re-authentication.
///
/// # Errors
///
/// Returns 422 if re-authentication fails.
pub async fn mfa_unenroll(
    State(state): State<Arc<MfaRouteState>>,
    Json(req): Json<MfaUnenrollRequest>,
) -> Response {
    let logger = get_audit_logger();

    match state.mfa_store.unenroll(&req.user_id, &req.code).await {
        Ok(()) => {
            logger.log_success(
                AuditEventType::SessionTokenRevoked,
                SecretType::SessionToken,
                Some(req.user_id),
                "mfa_unenroll",
            );
            StatusCode::OK.into_response()
        },
        Err(e) => {
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                Some(req.user_id.clone()),
                "mfa_unenroll",
                &e.to_string(),
            );
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
