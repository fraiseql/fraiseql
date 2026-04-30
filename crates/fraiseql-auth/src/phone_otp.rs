//! Phone-based OTP authentication (SMS).
//!
//! - `POST /auth/v1/otp/sms` — send a 6-digit OTP to a phone number via SMS.
//! - `POST /auth/v1/verify/sms` — verify the OTP and issue a session.
//!
//! Phone numbers are normalized to E.164 format before storage and lookup.
//! The same [`OtpStore`] backend is used for both email and phone OTPs —
//! keyed by `sms:<E.164 number>` to avoid collisions.

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
    otp::{OtpStore, generate_otp_code},
    provider::UserInfo,
    session::SessionStore,
};

/// OTP TTL in seconds (10 minutes).
const OTP_TTL_SECS: u64 = 600;

/// OTP code length (digits).
const OTP_LENGTH: usize = 6;

/// Maximum phone number length in characters (E.164 max is 15 digits + `+`).
const MAX_PHONE_LEN: usize = 16;

/// Minimum phone number length (country code + subscriber number).
const MIN_PHONE_LEN: usize = 8;

/// SMS delivery trait — abstraction over Twilio, Vonage, etc.
// Reason: used as dyn Trait (Arc<dyn SmsSender>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait SmsSender: Send + Sync {
    /// Send an OTP code via SMS.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the SMS delivery fails.
    async fn send_sms_otp(&self, to: &str, code: &str) -> crate::error::Result<()>;
}

/// No-op SMS sender for testing — records sent messages.
#[derive(Debug)]
pub struct InMemorySmsSender {
    /// Sent SMS messages: (to, code).
    pub messages: RwLock<Vec<(String, String)>>,
}

impl InMemorySmsSender {
    /// Create a new in-memory SMS sender.
    pub fn new() -> Self {
        Self {
            messages: RwLock::new(Vec::new()),
        }
    }

    /// Get the number of SMS messages sent.
    pub async fn sms_count(&self) -> usize {
        self.messages.read().await.len()
    }

    /// Get the last OTP code sent to a phone number.
    pub async fn last_otp_for(&self, phone: &str) -> Option<String> {
        let messages = self.messages.read().await;
        messages
            .iter()
            .rev()
            .find(|(to, _)| to == phone)
            .map(|(_, code)| code.clone())
    }
}

impl Default for InMemorySmsSender {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: SmsSender is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SmsSender for InMemorySmsSender {
    async fn send_sms_otp(&self, to: &str, code: &str) -> crate::error::Result<()> {
        let mut messages = self.messages.write().await;
        messages.push((to.to_string(), code.to_string()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// E.164 normalization
// ---------------------------------------------------------------------------

/// Normalize a phone number to E.164 format.
///
/// Strips whitespace, dashes, dots, and parentheses. Ensures the result starts
/// with `+` and contains only digits after the prefix.
///
/// # Errors
///
/// Returns `None` if the input cannot be normalized to a valid E.164 number.
pub fn normalize_e164(phone: &str) -> Option<String> {
    // Strip whitespace, dashes, dots, parens
    let cleaned: String = phone
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '+')
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    // Ensure it starts with '+'
    let normalized = if cleaned.starts_with('+') {
        cleaned
    } else {
        format!("+{cleaned}")
    };

    // Validate: '+' followed by 7–15 digits
    let digits = &normalized[1..];
    if digits.len() < 7 || digits.len() > 15 {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(normalized)
}

/// Build the OTP store key for a phone number (prefixed to avoid email collisions).
fn phone_otp_key(e164: &str) -> String {
    format!("sms:{e164}")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /auth/v1/otp/sms`.
#[derive(Debug, Deserialize)]
pub struct SmsOtpRequest {
    /// Phone number (any common format — will be normalized to E.164).
    pub phone: String,
}

/// Response body for `POST /auth/v1/otp/sms`.
#[derive(Debug, Serialize)]
pub struct SmsOtpResponse {
    /// Always "otp_sent" (anti-enumeration).
    pub status: String,
    /// Seconds until the OTP expires.
    pub expires_in: u64,
}

/// Request body for `POST /auth/v1/verify/sms`.
#[derive(Debug, Deserialize)]
pub struct SmsVerifyRequest {
    /// Phone number (will be normalized to E.164 for lookup).
    pub phone: String,
    /// 6-digit OTP code.
    pub code: String,
}

/// Response body for `POST /auth/v1/verify/sms`.
#[derive(Debug, Serialize)]
pub struct SmsVerifyResponse {
    /// Access token for API requests.
    pub access_token: String,
    /// Refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token type (always "Bearer").
    pub token_type: String,
    /// Seconds until the access token expires.
    pub expires_in: u64,
}

/// Shared state for SMS OTP endpoints.
#[derive(Clone)]
pub struct SmsOtpAuthState {
    /// OTP store backend (shared with email OTP).
    pub otp_store: Arc<dyn OtpStore>,
    /// SMS delivery backend.
    pub sms_sender: Arc<dyn SmsSender>,
    /// Session store for creating sessions after verification.
    pub session_store: Arc<dyn SessionStore>,
    /// Optional user store for account linking.
    pub user_store: Option<Arc<dyn UserStore>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/otp/sms
// ---------------------------------------------------------------------------

/// Send a 6-digit OTP code via SMS to the provided phone number.
///
/// The phone number is normalized to E.164 format before sending.
/// The response always returns `200` with `"status": "otp_sent"` regardless of
/// whether the phone number is registered, to prevent enumeration attacks.
///
/// # Errors
///
/// Returns `400` if the phone number is empty or invalid.
pub async fn send_sms_otp(
    State(state): State<Arc<SmsOtpAuthState>>,
    Json(req): Json<SmsOtpRequest>,
) -> Response {
    if req.phone.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "phone is required");
    }
    if req.phone.len() > MAX_PHONE_LEN * 2 {
        return json_error(StatusCode::BAD_REQUEST, "phone number too long");
    }

    let Some(e164) = normalize_e164(&req.phone) else {
        return json_error(StatusCode::BAD_REQUEST, "invalid phone number format");
    };

    if e164.len() < MIN_PHONE_LEN {
        return json_error(StatusCode::BAD_REQUEST, "phone number too short");
    }

    let code = generate_otp_code();
    let expires_at = unix_now() + OTP_TTL_SECS;
    let key = phone_otp_key(&e164);

    if let Err(e) = state.otp_store.store_otp(&key, &code, expires_at).await {
        tracing::error!(error = %e, "OTP store failed for SMS");
    }

    if let Err(e) = state.sms_sender.send_sms_otp(&e164, &code).await {
        tracing::error!(error = %e, "SMS delivery failed");
    }

    Json(SmsOtpResponse {
        status: "otp_sent".to_string(),
        expires_in: OTP_TTL_SECS,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/verify/sms
// ---------------------------------------------------------------------------

/// Verify a 6-digit SMS OTP code and issue a session.
///
/// # Errors
///
/// Returns `400` if the phone or code is empty/invalid.
/// Returns `429` if max verification attempts exceeded.
pub async fn verify_sms_otp(
    State(state): State<Arc<SmsOtpAuthState>>,
    Json(req): Json<SmsVerifyRequest>,
) -> Response {
    if req.phone.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "phone is required");
    }

    let Some(e164) = normalize_e164(&req.phone) else {
        return json_error(StatusCode::BAD_REQUEST, "invalid phone number format");
    };

    if req.code.len() != OTP_LENGTH {
        return json_error(StatusCode::BAD_REQUEST, "invalid OTP code format");
    }

    let key = phone_otp_key(&e164);

    match state.otp_store.verify_otp(&key, &req.code).await {
        Ok(true) => {},
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
            tracing::error!(error = %e, "SMS OTP verification error");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "verification failed");
        },
    }

    // Resolve user ID
    let user_id = if let Some(user_store) = &state.user_store {
        let user_info = UserInfo {
            id:         e164.clone(),
            email:      format!("{e164}@phone.local"),
            name:       None,
            picture:    None,
            raw_claims: serde_json::json!({ "phone": e164 }),
        };
        match user_store.find_or_create_user("phone", &user_info).await {
            Ok(user) => user.id,
            Err(e) => {
                tracing::error!(error = %e, "user store lookup failed");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "user resolution failed");
            },
        }
    } else {
        e164
    };

    let session_expiry = unix_now() + (7 * 24 * 60 * 60);
    match state.session_store.create_session(&user_id, session_expiry).await {
        Ok(tokens) => Json(SmsVerifyResponse {
            access_token: tokens.access_token,
            refresh_token: Some(tokens.refresh_token),
            token_type: "Bearer".to_string(),
            expires_in: tokens.expires_in,
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
    use crate::{
        account_linking::InMemoryUserStore,
        otp::InMemoryOtpStore,
        session::InMemorySessionStore,
    };

    fn build_sms_state() -> (Arc<SmsOtpAuthState>, Arc<InMemorySmsSender>, Arc<InMemoryOtpStore>) {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let sms_sender = Arc::new(InMemorySmsSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());

        let state = Arc::new(SmsOtpAuthState {
            otp_store: otp_store.clone(),
            sms_sender: sms_sender.clone(),
            session_store,
            user_store: None,
        });

        (state, sms_sender, otp_store)
    }

    fn sms_router(state: Arc<SmsOtpAuthState>) -> Router {
        Router::new()
            .route("/auth/v1/otp/sms", post(send_sms_otp))
            .route("/auth/v1/verify/sms", post(verify_sms_otp))
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

    // ── E.164 normalization tests ───────────────────────────────────────

    #[test]
    fn test_normalize_e164_already_valid() {
        assert_eq!(normalize_e164("+14155551234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_strips_formatting() {
        assert_eq!(normalize_e164("+1 (415) 555-1234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_adds_plus() {
        assert_eq!(normalize_e164("14155551234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_strips_dots_and_dashes() {
        assert_eq!(normalize_e164("+1.415.555.1234"), Some("+14155551234".to_string()));
    }

    #[test]
    fn test_normalize_e164_rejects_empty() {
        assert_eq!(normalize_e164(""), None);
    }

    #[test]
    fn test_normalize_e164_rejects_too_short() {
        assert_eq!(normalize_e164("+123"), None);
    }

    #[test]
    fn test_normalize_e164_rejects_too_long() {
        assert_eq!(normalize_e164("+1234567890123456"), None);
    }

    #[test]
    fn test_normalize_e164_french_number() {
        assert_eq!(normalize_e164("+33 6 12 34 56 78"), Some("+33612345678".to_string()));
    }

    // ── send_sms_otp tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_send_sms_otp_returns_success() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "otp_sent");
        assert_eq!(json["expires_in"], OTP_TTL_SECS);

        assert_eq!(sms_sender.sms_count().await, 1);
    }

    #[tokio::test]
    async fn test_send_sms_otp_empty_phone_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_sms_otp_invalid_phone_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "abc" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_send_sms_normalizes_phone_number() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+1 (415) 555-1234" }));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // SMS should have been sent to the normalized number
        let code = sms_sender.last_otp_for("+14155551234").await;
        assert!(code.is_some(), "SMS should be sent to E.164 normalized number");
    }

    // ── verify_sms_otp tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_verify_sms_otp_full_flow() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        // Send OTP
        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let code = sms_sender.last_otp_for("+14155551234").await.unwrap();

        // Verify OTP
        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": code }),
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
    async fn test_verify_sms_wrong_code_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        // Send OTP
        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        app.clone().oneshot(req).await.unwrap();

        // Verify with wrong code
        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": "000000" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_sms_normalizes_phone_on_verify() {
        let (state, sms_sender, _) = build_sms_state();
        let app = sms_router(state);

        // Send to normalized number
        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+14155551234" }));
        app.clone().oneshot(req).await.unwrap();

        let code = sms_sender.last_otp_for("+14155551234").await.unwrap();

        // Verify with formatted number (should still work after normalization)
        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+1 (415) 555-1234", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_verify_sms_invalid_code_length_returns_400() {
        let (state, _, _) = build_sms_state();
        let app = sms_router(state);

        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+14155551234", "code": "123" }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_verify_sms_with_user_store() {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let sms_sender = Arc::new(InMemorySmsSender::new());
        let session_store: Arc<dyn SessionStore> = Arc::new(InMemorySessionStore::new());
        let user_store = Arc::new(InMemoryUserStore::new());

        let state = Arc::new(SmsOtpAuthState {
            otp_store: otp_store.clone(),
            sms_sender: sms_sender.clone(),
            session_store,
            user_store: Some(user_store.clone() as Arc<dyn UserStore>),
        });
        let app = sms_router(state);

        // Send OTP
        let req = post_json("/auth/v1/otp/sms", serde_json::json!({ "phone": "+33612345678" }));
        app.clone().oneshot(req).await.unwrap();

        let code = sms_sender.last_otp_for("+33612345678").await.unwrap();

        // Verify
        let req = post_json(
            "/auth/v1/verify/sms",
            serde_json::json!({ "phone": "+33612345678", "code": code }),
        );
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // User should have been created with phone provider
        assert_eq!(user_store.user_count().await, 1);
    }
}
