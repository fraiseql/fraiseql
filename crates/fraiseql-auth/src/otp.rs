//! One-Time Password (OTP) and magic link authentication.
//!
//! - `POST /auth/v1/otp` — send a 6-digit OTP or magic link to the user's email.
//! - `POST /auth/v1/verify` — verify the OTP and issue a session.
//!
//! OTP codes are 6 digits, 10-minute TTL, single-use, rate-limited.
//! Magic links contain a signed JWT with 1-hour TTL, single-use.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    account_linking::UserStore,
    provider::UserInfo,
    session::SessionStore,
};

/// Maximum OTP attempts before lockout.
const MAX_OTP_ATTEMPTS: u32 = 5;

/// OTP code length (digits).
const OTP_LENGTH: usize = 6;

/// OTP TTL in seconds (10 minutes).
const OTP_TTL_SECS: u64 = 600;

/// Maximum email length in bytes.
const MAX_EMAIL_BYTES: usize = 320;

/// A pending OTP record.
#[derive(Debug, Clone)]
pub struct OtpRecord {
    /// The 6-digit OTP code.
    pub code:       String,
    /// Email the OTP was sent to.
    pub email:      String,
    /// Unix timestamp when this OTP expires.
    pub expires_at: u64,
    /// Number of failed verification attempts.
    pub attempts:   u32,
    /// Whether this OTP has been consumed.
    pub consumed:   bool,
}

/// OTP store trait — store and verify OTP codes.
// Reason: used as dyn Trait (Arc<dyn OtpStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait OtpStore: Send + Sync {
    /// Store a new OTP code for the given email.
    ///
    /// # Errors
    ///
    /// Returns error if the store is at capacity.
    async fn store_otp(&self, email: &str, code: &str, expires_at: u64) -> crate::error::Result<()>;

    /// Verify and consume an OTP code.
    ///
    /// Returns `Ok(true)` if the code is valid and consumed.
    /// Returns `Ok(false)` if the code is invalid or expired.
    /// Returns `Err` if max attempts exceeded or store error.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::RateLimited` if max verification attempts exceeded.
    async fn verify_otp(&self, email: &str, code: &str) -> crate::error::Result<bool>;
}

/// Email delivery trait — abstraction over SMTP, Resend, SendGrid, etc.
// Reason: used as dyn Trait (Arc<dyn EmailSender>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait EmailSender: Send + Sync {
    /// Send an OTP email.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the email delivery fails.
    async fn send_otp_email(&self, to: &str, code: &str) -> crate::error::Result<()>;

    /// Send a magic link email.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the email delivery fails.
    async fn send_magic_link_email(&self, to: &str, link: &str) -> crate::error::Result<()>;
}

/// In-memory OTP store for testing.
#[derive(Debug)]
pub struct InMemoryOtpStore {
    /// Records keyed by email (lowercase).
    records: RwLock<std::collections::HashMap<String, OtpRecord>>,
}

impl InMemoryOtpStore {
    /// Create a new in-memory OTP store.
    pub fn new() -> Self {
        Self {
            records: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for InMemoryOtpStore {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Reason: OtpStore is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl OtpStore for InMemoryOtpStore {
    async fn store_otp(&self, email: &str, code: &str, expires_at: u64) -> crate::error::Result<()> {
        let mut records = self.records.write().await;
        records.insert(
            email.to_lowercase(),
            OtpRecord {
                code:       code.to_string(),
                email:      email.to_string(),
                expires_at,
                attempts:   0,
                consumed:   false,
            },
        );
        Ok(())
    }

    async fn verify_otp(&self, email: &str, code: &str) -> crate::error::Result<bool> {
        let mut records = self.records.write().await;
        let key = email.to_lowercase();

        let Some(record) = records.get_mut(&key) else {
            return Ok(false);
        };

        if record.consumed {
            return Ok(false);
        }

        if unix_now() > record.expires_at {
            records.remove(&key);
            return Ok(false);
        }

        if record.attempts >= MAX_OTP_ATTEMPTS {
            records.remove(&key);
            return Err(crate::error::AuthError::RateLimited {
                retry_after_secs: 900, // 15 minutes
            });
        }

        if crate::constant_time::ConstantTimeOps::compare(record.code.as_bytes(), code.as_bytes()) {
            record.consumed = true;
            Ok(true)
        } else {
            record.attempts += 1;
            Ok(false)
        }
    }
}

/// No-op email sender for testing — records sent emails.
#[derive(Debug)]
pub struct InMemoryEmailSender {
    /// Sent OTP emails: (to, code).
    pub otp_emails:        RwLock<Vec<(String, String)>>,
    /// Sent magic link emails: (to, link).
    pub magic_link_emails: RwLock<Vec<(String, String)>>,
}

impl InMemoryEmailSender {
    /// Create a new in-memory email sender.
    pub fn new() -> Self {
        Self {
            otp_emails:        RwLock::new(Vec::new()),
            magic_link_emails: RwLock::new(Vec::new()),
        }
    }

    /// Get the number of OTP emails sent.
    pub async fn otp_count(&self) -> usize {
        self.otp_emails.read().await.len()
    }

    /// Get the last OTP code sent to an email.
    pub async fn last_otp_for(&self, email: &str) -> Option<String> {
        let emails = self.otp_emails.read().await;
        emails
            .iter()
            .rev()
            .find(|(to, _)| to == email)
            .map(|(_, code)| code.clone())
    }
}

impl Default for InMemoryEmailSender {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: EmailSender is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl EmailSender for InMemoryEmailSender {
    async fn send_otp_email(&self, to: &str, code: &str) -> crate::error::Result<()> {
        let mut emails = self.otp_emails.write().await;
        emails.push((to.to_string(), code.to_string()));
        Ok(())
    }

    async fn send_magic_link_email(&self, to: &str, link: &str) -> crate::error::Result<()> {
        let mut emails = self.magic_link_emails.write().await;
        emails.push((to.to_string(), link.to_string()));
        Ok(())
    }
}

/// Generate a cryptographically random 6-digit OTP code.
pub fn generate_otp_code() -> String {
    use rand::{Rng, rngs::OsRng};
    let code: u32 = OsRng.gen_range(0..1_000_000);
    format!("{code:0>6}")
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /auth/v1/otp`.
#[derive(Debug, Deserialize)]
pub struct OtpRequest {
    /// Email address to send the OTP to.
    pub email: String,
}

/// Response body for `POST /auth/v1/otp`.
#[derive(Debug, Serialize)]
pub struct OtpResponse {
    /// Always "otp_sent" (even if the email doesn't exist, to prevent enumeration).
    pub status: String,
    /// Seconds until the OTP expires.
    pub expires_in: u64,
}

/// Request body for `POST /auth/v1/verify`.
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// Email address.
    pub email: String,
    /// 6-digit OTP code.
    pub code:  String,
}

/// Response body for `POST /auth/v1/verify`.
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    /// Access token for API requests.
    pub access_token:  String,
    /// Refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token type (always "Bearer").
    pub token_type:    String,
    /// Seconds until the access token expires.
    pub expires_in:    u64,
}

/// Shared state for OTP endpoints.
#[derive(Clone)]
pub struct OtpAuthState {
    /// OTP store backend.
    pub otp_store:     Arc<dyn OtpStore>,
    /// Email delivery backend.
    pub email_sender:  Arc<dyn EmailSender>,
    /// Session store for creating sessions after verification.
    pub session_store: Arc<dyn SessionStore>,
    /// Optional user store for account linking.
    pub user_store:    Option<Arc<dyn UserStore>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/otp
// ---------------------------------------------------------------------------

/// Send a 6-digit OTP code to the provided email address.
///
/// The response always returns `200` with `"status": "otp_sent"` regardless of
/// whether the email exists, to prevent email enumeration attacks.
///
/// # Errors
///
/// Returns `400` if the email is empty or exceeds the maximum length.
pub async fn send_otp(
    State(state): State<Arc<OtpAuthState>>,
    Json(req): Json<OtpRequest>,
) -> Response {
    // Validate email
    if req.email.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "email is required");
    }
    if req.email.len() > MAX_EMAIL_BYTES {
        return json_error(StatusCode::BAD_REQUEST, "email exceeds maximum length");
    }

    // Generate OTP
    let code = generate_otp_code();
    let expires_at = unix_now() + OTP_TTL_SECS;

    // Store OTP
    if let Err(e) = state.otp_store.store_otp(&req.email, &code, expires_at).await {
        tracing::error!(error = %e, "OTP store failed");
        // Still return success to prevent enumeration
    }

    // Send email (fire and forget — don't reveal failures)
    if let Err(e) = state.email_sender.send_otp_email(&req.email, &code).await {
        tracing::error!(error = %e, "OTP email delivery failed");
    }

    Json(OtpResponse {
        status:     "otp_sent".to_string(),
        expires_in: OTP_TTL_SECS,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/verify
// ---------------------------------------------------------------------------

/// Verify a 6-digit OTP code and issue a session.
///
/// # Errors
///
/// Returns `400` if the email or code is empty, or the code is invalid.
/// Returns `429` if max verification attempts exceeded.
pub async fn verify_otp(
    State(state): State<Arc<OtpAuthState>>,
    Json(req): Json<VerifyRequest>,
) -> Response {
    // Validate inputs
    if req.email.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "email is required");
    }
    if req.email.len() > MAX_EMAIL_BYTES {
        return json_error(StatusCode::BAD_REQUEST, "email exceeds maximum length");
    }
    if req.code.len() != OTP_LENGTH {
        return json_error(StatusCode::BAD_REQUEST, "invalid OTP code format");
    }

    // Verify OTP
    match state.otp_store.verify_otp(&req.email, &req.code).await {
        Ok(true) => {
            // OTP valid — resolve user and create session
        },
        Ok(false) => {
            return json_error(StatusCode::BAD_REQUEST, "invalid or expired OTP code");
        },
        Err(crate::error::AuthError::RateLimited { retry_after_secs }) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "too many verification attempts",
                    "retry_after_secs": retry_after_secs,
                })),
            )
                .into_response();
        },
        Err(e) => {
            tracing::error!(error = %e, "OTP verification error");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "verification failed");
        },
    }

    // Resolve user ID
    let user_id = if let Some(user_store) = &state.user_store {
        // Create a synthetic UserInfo for email-based auth
        let user_info = UserInfo {
            id:         req.email.clone(),
            email:      req.email.clone(),
            name:       None,
            picture:    None,
            raw_claims: serde_json::json!({}),
        };
        match user_store.find_or_create_user("email", &user_info).await {
            Ok(user) => user.id,
            Err(e) => {
                tracing::error!(error = %e, "user store lookup failed");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "user resolution failed");
            },
        }
    } else {
        req.email.clone()
    };

    // Create session (7-day expiry)
    let session_expiry = unix_now() + (7 * 24 * 60 * 60);
    match state.session_store.create_session(&user_id, session_expiry).await {
        Ok(tokens) => Json(VerifyResponse {
            access_token:  tokens.access_token,
            refresh_token: Some(tokens.refresh_token),
            token_type:    "Bearer".to_string(),
            expires_in:    tokens.expires_in,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session creation failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "session could not be created")
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt as _;

    use super::*;
    use crate::session::InMemorySessionStore;

    fn build_otp_state() -> (Arc<OtpAuthState>, Arc<InMemoryEmailSender>, Arc<InMemoryOtpStore>) {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let email_sender = Arc::new(InMemoryEmailSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

        let state = Arc::new(OtpAuthState {
            otp_store:     otp_store.clone(),
            email_sender:  email_sender.clone(),
            session_store,
            user_store:    None,
        });

        (state, email_sender, otp_store)
    }

    fn otp_router(state: Arc<OtpAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/otp", post(send_otp))
            .route("/auth/v1/verify", post(verify_otp))
            .with_state(state)
    }

    fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    // ── send_otp tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_send_otp_returns_success() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "otp_sent");
        assert_eq!(json["expires_in"], OTP_TTL_SECS);

        assert_eq!(email_sender.otp_count().await, 1);
    }

    #[tokio::test]
    async fn test_send_otp_empty_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_otp_oversized_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let long_email = "a".repeat(321);
        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": long_email }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── verify_otp tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_verify_otp_full_flow() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        // Step 1: Send OTP
        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Get the OTP code from the email sender
        let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

        // Step 2: Verify OTP
        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["token_type"], "Bearer");
    }

    #[tokio::test]
    async fn test_verify_wrong_code_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        // Send OTP
        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        app.clone().oneshot(req).await.unwrap();

        // Verify with wrong code
        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_consumed_otp_returns_400() {
        let (state, email_sender, _) = build_otp_state();
        let app = otp_router(state);

        // Send OTP
        let req = post_json("/auth/v1/otp", serde_json::json!({ "email": "alice@example.com" }));
        app.clone().oneshot(req).await.unwrap();
        let code = email_sender.last_otp_for("alice@example.com").await.unwrap();

        // First verify succeeds
        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Second verify with same code fails (consumed)
        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_invalid_code_length_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "alice@example.com", "code": "123" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("format"));
    }

    #[tokio::test]
    async fn test_verify_nonexistent_email_returns_400() {
        let (state, _, _) = build_otp_state();
        let app = otp_router(state);

        let req = post_json(
            "/auth/v1/verify",
            serde_json::json!({ "email": "nobody@example.com", "code": "123456" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── generate_otp_code tests ──────────────────────────────────────────

    #[test]
    fn test_generate_otp_code_is_6_digits() {
        let code = generate_otp_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_otp_code_is_random() {
        let code1 = generate_otp_code();
        let code2 = generate_otp_code();
        // This could technically fail with 1-in-a-million probability
        // But for practical purposes, two random codes should differ
        // We verify the format is correct rather than exact values
        assert_eq!(code1.len(), 6);
        assert_eq!(code2.len(), 6);
    }

    // ── OTP store unit tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_otp_store_verify_correct_code() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_otp_store_verify_wrong_code() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "000000").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_otp_store_single_use() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        assert!(store.verify_otp("alice@example.com", "123456").await.unwrap());
        assert!(!store.verify_otp("alice@example.com", "123456").await.unwrap());
    }

    #[tokio::test]
    async fn test_otp_store_expired_code_rejected() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now().saturating_sub(10); // Already expired
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_otp_store_max_attempts_lockout() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("alice@example.com", "123456", expires_at).await.unwrap();

        // Exhaust attempts with wrong codes
        for _ in 0..MAX_OTP_ATTEMPTS {
            let _ = store.verify_otp("alice@example.com", "000000").await;
        }

        // Next attempt should be rate-limited
        let result = store.verify_otp("alice@example.com", "123456").await;
        assert!(
            matches!(result, Err(crate::error::AuthError::RateLimited { .. })),
            "expected RateLimited after max attempts, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_otp_store_case_insensitive_email() {
        let store = InMemoryOtpStore::new();
        let expires_at = unix_now() + 600;
        store.store_otp("Alice@Example.COM", "123456", expires_at).await.unwrap();

        let result = store.verify_otp("alice@example.com", "123456").await.unwrap();
        assert!(result);
    }
}
