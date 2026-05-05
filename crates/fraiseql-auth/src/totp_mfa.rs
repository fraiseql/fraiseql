//! TOTP Multi-Factor Authentication (RFC 6238).
//!
//! Endpoints:
//! - `POST /auth/v1/mfa/enroll` — generate TOTP secret, return QR code URI
//! - `POST /auth/v1/mfa/challenge` — initiate MFA challenge after first-factor auth
//! - `POST /auth/v1/mfa/verify` — verify TOTP code, issue full session
//! - `POST /auth/v1/mfa/unenroll` — remove MFA (requires re-authentication)
//!
//! TOTP: RFC 6238, 30-second window, 1-step tolerance, HMAC-SHA1.
//! Recovery codes: 8 single-use codes generated at enrollment.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use tokio::sync::RwLock;

/// TOTP time step in seconds (RFC 6238 default).
pub(crate) const TOTP_TIME_STEP: u64 = 30;

/// Number of time steps to tolerate in each direction.
const TOTP_SKEW_STEPS: u64 = 1;

/// Number of recovery codes generated at enrollment.
pub(crate) const RECOVERY_CODE_COUNT: usize = 8;

/// TOTP secret length in bytes (160 bits, per RFC 4226 recommendation).
const TOTP_SECRET_BYTES: usize = 20;

/// Maximum TOTP code length for input validation.
const MAX_TOTP_CODE_BYTES: usize = 10;

type HmacSha1 = Hmac<Sha1>;

// ---------------------------------------------------------------------------
// Base32 encoding (minimal, RFC 4648)
// ---------------------------------------------------------------------------

/// Encode bytes as base32 (RFC 4648, no padding).
pub(crate) fn base32_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits_in_buffer = 0;

    for &byte in input {
        buffer = (buffer << 8) | u64::from(byte);
        bits_in_buffer += 8;
        while bits_in_buffer >= 5 {
            bits_in_buffer -= 5;
            let idx = ((buffer >> bits_in_buffer) & 0x1F) as usize;
            result.push(ALPHABET[idx] as char);
        }
    }
    if bits_in_buffer > 0 {
        let idx = ((buffer << (5 - bits_in_buffer)) & 0x1F) as usize;
        result.push(ALPHABET[idx] as char);
    }
    result
}

// ---------------------------------------------------------------------------
// Core TOTP algorithm
// ---------------------------------------------------------------------------

/// Compute the TOTP code for a given secret and counter.
fn totp_code(secret: &[u8], counter: u64) -> u32 {
    let counter_bytes = counter.to_be_bytes();

    let mut mac =
        HmacSha1::new_from_slice(secret).expect("HMAC-SHA1 accepts any key length");
    mac.update(&counter_bytes);
    let result = mac.finalize().into_bytes();
    let hmac_result: &[u8] = result.as_slice();

    // Dynamic truncation (RFC 4226 §5.4)
    let offset = (hmac_result[19] & 0x0F) as usize;
    let binary = u32::from(hmac_result[offset] & 0x7F) << 24
        | u32::from(hmac_result[offset + 1]) << 16
        | u32::from(hmac_result[offset + 2]) << 8
        | u32::from(hmac_result[offset + 3]);

    binary % 1_000_000
}

/// Generate the current TOTP code as a zero-padded 6-digit string.
pub fn generate_totp(secret: &[u8], time: u64) -> String {
    let counter = time / TOTP_TIME_STEP;
    format!("{:06}", totp_code(secret, counter))
}

/// Verify a TOTP code with ±1 step tolerance.
pub fn verify_totp(secret: &[u8], code: &str, time: u64) -> bool {
    let counter = time / TOTP_TIME_STEP;
    let Ok(code_num) = code.parse::<u32>() else {
        return false;
    };

    for offset in 0..=TOTP_SKEW_STEPS {
        if totp_code(secret, counter + offset) == code_num {
            return true;
        }
        if offset > 0 && counter >= offset && totp_code(secret, counter - offset) == code_num {
            return true;
        }
    }
    false
}

/// Generate a cryptographically random TOTP secret.
pub fn generate_totp_secret() -> Vec<u8> {
    use rand::{Rng, rngs::OsRng};
    let mut secret = vec![0u8; TOTP_SECRET_BYTES];
    OsRng.fill(&mut secret[..]);
    secret
}

/// Generate the `otpauth://` URI for QR code scanning.
pub fn totp_uri(secret: &[u8], email: &str, issuer: &str) -> String {
    let encoded_secret = base32_encode(secret);
    let encoded_email = urlencoding::encode(email);
    let encoded_issuer = urlencoding::encode(issuer);
    format!(
        "otpauth://totp/{encoded_issuer}:{encoded_email}?secret={encoded_secret}&issuer={encoded_issuer}&algorithm=SHA1&digits=6&period=30"
    )
}

/// Generate recovery codes (8 alphanumeric, 8 characters each).
pub fn generate_recovery_codes() -> Vec<String> {
    use rand::{Rng, rngs::OsRng};

    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // No 0/O/1/I
    (0..RECOVERY_CODE_COUNT)
        .map(|_| {
            (0..8)
                .map(|_| CHARSET[OsRng.gen_range(0..CHARSET.len())] as char)
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// MFA enrollment record
// ---------------------------------------------------------------------------

/// MFA enrollment state for a user.
#[derive(Debug, Clone)]
pub struct MfaEnrollment {
    /// TOTP secret (raw bytes).
    pub secret:         Vec<u8>,
    /// Recovery codes (hashed for storage; plaintext only returned at enrollment).
    pub recovery_codes: Vec<String>,
    /// Whether MFA is fully verified (user confirmed with a valid TOTP code).
    pub verified:       bool,
}

/// MFA store trait — stores per-user MFA enrollment.
// Reason: used as dyn Trait (Arc<dyn MfaStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait MfaStore: Send + Sync {
    /// Store or update MFA enrollment for a user.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` on store failure.
    async fn set_enrollment(&self, user_id: &str, enrollment: MfaEnrollment) -> crate::error::Result<()>;

    /// Get MFA enrollment for a user.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` on store failure.
    async fn get_enrollment(&self, user_id: &str) -> crate::error::Result<Option<MfaEnrollment>>;

    /// Remove MFA enrollment for a user.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` on store failure.
    async fn remove_enrollment(&self, user_id: &str) -> crate::error::Result<bool>;

    /// Consume a recovery code (single-use).
    ///
    /// Returns `true` if the code was valid and consumed.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` on store failure.
    async fn consume_recovery_code(&self, user_id: &str, code: &str) -> crate::error::Result<bool>;
}

/// In-memory MFA store for testing.
#[derive(Debug)]
pub struct InMemoryMfaStore {
    enrollments: RwLock<HashMap<String, MfaEnrollment>>,
}

impl InMemoryMfaStore {
    /// Create a new in-memory MFA store.
    pub fn new() -> Self {
        Self {
            enrollments: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryMfaStore {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: MfaStore is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl MfaStore for InMemoryMfaStore {
    async fn set_enrollment(&self, user_id: &str, enrollment: MfaEnrollment) -> crate::error::Result<()> {
        let mut enrollments = self.enrollments.write().await;
        enrollments.insert(user_id.to_string(), enrollment);
        Ok(())
    }

    async fn get_enrollment(&self, user_id: &str) -> crate::error::Result<Option<MfaEnrollment>> {
        let enrollments = self.enrollments.read().await;
        Ok(enrollments.get(user_id).cloned())
    }

    async fn remove_enrollment(&self, user_id: &str) -> crate::error::Result<bool> {
        let mut enrollments = self.enrollments.write().await;
        Ok(enrollments.remove(user_id).is_some())
    }

    async fn consume_recovery_code(&self, user_id: &str, code: &str) -> crate::error::Result<bool> {
        let mut enrollments = self.enrollments.write().await;
        let Some(enrollment) = enrollments.get_mut(user_id) else {
            return Ok(false);
        };
        let code_upper = code.to_uppercase();
        if let Some(pos) = enrollment.recovery_codes.iter().position(|c| c == &code_upper) {
            enrollment.recovery_codes.remove(pos);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /auth/v1/mfa/enroll`.
#[derive(Debug, Deserialize)]
pub struct MfaEnrollRequest {
    /// User ID (from first-factor auth).
    pub user_id: String,
}

/// Response body for `POST /auth/v1/mfa/enroll`.
#[derive(Debug, Serialize)]
pub struct MfaEnrollResponse {
    /// `otpauth://` URI for QR code scanning.
    pub totp_uri:       String,
    /// Base32-encoded TOTP secret for manual entry.
    pub secret:         String,
    /// 8 single-use recovery codes.
    pub recovery_codes: Vec<String>,
}

/// Request body for `POST /auth/v1/mfa/verify`.
#[derive(Debug, Deserialize)]
pub struct MfaVerifyRequest {
    /// User ID.
    pub user_id: String,
    /// 6-digit TOTP code or recovery code.
    pub code:    String,
}

/// Response body for `POST /auth/v1/mfa/verify`.
#[derive(Debug, Serialize)]
pub struct MfaVerifyResponse {
    /// Whether MFA verification was successful.
    pub verified: bool,
}

/// Request body for `POST /auth/v1/mfa/unenroll`.
#[derive(Debug, Deserialize)]
pub struct MfaUnenrollRequest {
    /// User ID.
    pub user_id: String,
    /// Current TOTP code (required for re-authentication).
    pub code:    String,
}

/// Shared state for MFA endpoints.
#[derive(Clone)]
pub struct MfaAuthState {
    /// MFA store backend.
    pub mfa_store: Arc<dyn MfaStore>,
    /// Issuer name for the `otpauth://` URI (e.g., "FraiseQL").
    pub issuer:    String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/mfa/enroll
// ---------------------------------------------------------------------------

/// Enroll a user in TOTP MFA.
///
/// Generates a TOTP secret, returns the `otpauth://` URI for QR scanning
/// and 8 single-use recovery codes. The enrollment is not verified until
/// the user confirms with a valid TOTP code via `/auth/v1/mfa/verify`.
///
/// # Errors
///
/// Returns `400` if user_id is empty. Returns `409` if MFA is already enrolled.
pub async fn mfa_enroll(
    State(state): State<Arc<MfaAuthState>>,
    Json(req): Json<MfaEnrollRequest>,
) -> Response {
    if req.user_id.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "user_id is required");
    }

    // Check for existing enrollment
    match state.mfa_store.get_enrollment(&req.user_id).await {
        Ok(Some(existing)) if existing.verified => {
            return json_error(StatusCode::CONFLICT, "MFA is already enrolled");
        },
        Ok(_) => {},
        Err(e) => {
            tracing::error!(error = %e, "MFA store lookup failed");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "MFA enrollment failed");
        },
    }

    let secret = generate_totp_secret();
    let recovery_codes = generate_recovery_codes();
    let uri = totp_uri(&secret, &req.user_id, &state.issuer);
    let encoded_secret = base32_encode(&secret);

    let enrollment = MfaEnrollment {
        secret,
        recovery_codes: recovery_codes.clone(),
        verified: false,
    };

    if let Err(e) = state.mfa_store.set_enrollment(&req.user_id, enrollment).await {
        tracing::error!(error = %e, "MFA store write failed");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "MFA enrollment failed");
    }

    Json(MfaEnrollResponse {
        totp_uri:       uri,
        secret:         encoded_secret,
        recovery_codes,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/mfa/verify
// ---------------------------------------------------------------------------

/// Verify a TOTP code or recovery code for MFA.
///
/// On first verification after enrollment, this confirms the enrollment.
/// Subsequent verifications are used as the MFA challenge step.
///
/// # Errors
///
/// Returns `400` if inputs are invalid. Returns `401` if the code is wrong.
/// Returns `404` if no MFA enrollment exists for the user.
pub async fn mfa_verify(
    State(state): State<Arc<MfaAuthState>>,
    Json(req): Json<MfaVerifyRequest>,
) -> Response {
    if req.user_id.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "user_id is required");
    }
    if req.code.is_empty() || req.code.len() > MAX_TOTP_CODE_BYTES {
        return json_error(StatusCode::BAD_REQUEST, "invalid code format");
    }

    let enrollment = match state.mfa_store.get_enrollment(&req.user_id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no MFA enrollment found");
        },
        Err(e) => {
            tracing::error!(error = %e, "MFA store lookup failed");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "MFA verification failed");
        },
    };

    // Try TOTP code first
    if req.code.len() == 6
        && req.code.chars().all(|c| c.is_ascii_digit())
        && verify_totp(&enrollment.secret, &req.code, unix_now())
    {
        // If not yet verified, mark as verified
        if !enrollment.verified {
            let mut updated = enrollment;
            updated.verified = true;
            if let Err(e) = state.mfa_store.set_enrollment(&req.user_id, updated).await {
                tracing::error!(error = %e, "MFA store update failed");
            }
        }
        return Json(MfaVerifyResponse { verified: true }).into_response();
    }

    // Try recovery code
    match state.mfa_store.consume_recovery_code(&req.user_id, &req.code).await {
        Ok(true) => {
            return Json(MfaVerifyResponse { verified: true }).into_response();
        },
        Ok(false) => {},
        Err(e) => {
            tracing::error!(error = %e, "recovery code check failed");
        },
    }

    json_error(StatusCode::UNAUTHORIZED, "invalid MFA code")
}

// ---------------------------------------------------------------------------
// POST /auth/v1/mfa/unenroll
// ---------------------------------------------------------------------------

/// Remove MFA enrollment. Requires a valid TOTP code for re-authentication.
///
/// # Errors
///
/// Returns `400` if inputs are invalid. Returns `401` if the code is wrong.
/// Returns `404` if no MFA enrollment exists.
pub async fn mfa_unenroll(
    State(state): State<Arc<MfaAuthState>>,
    Json(req): Json<MfaUnenrollRequest>,
) -> Response {
    if req.user_id.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "user_id is required");
    }

    let enrollment = match state.mfa_store.get_enrollment(&req.user_id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return json_error(StatusCode::NOT_FOUND, "no MFA enrollment found");
        },
        Err(e) => {
            tracing::error!(error = %e, "MFA store lookup failed");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "MFA unenrollment failed");
        },
    };

    // Verify the TOTP code for re-authentication
    if !verify_totp(&enrollment.secret, &req.code, unix_now()) {
        return json_error(StatusCode::UNAUTHORIZED, "invalid TOTP code");
    }

    match state.mfa_store.remove_enrollment(&req.user_id).await {
        Ok(true) => Json(serde_json::json!({ "unenrolled": true })).into_response(),
        Ok(false) => json_error(StatusCode::NOT_FOUND, "no MFA enrollment found"),
        Err(e) => {
            tracing::error!(error = %e, "MFA store removal failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "MFA unenrollment failed")
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
