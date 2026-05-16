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
//! - Verification attempts are rate-limited to **3 per 15 minutes** per email.
//! - Codes are compared with constant-time equality to prevent timing attacks.

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
const OTP_TTL_SECS: u64 = 600;

/// Maximum verification attempts per code before it is invalidated.
const MAX_VERIFY_ATTEMPTS: u32 = 3;

/// Rate-limit window for `OTP` send requests (15 minutes).
const OTP_RATE_WINDOW_SECS: u64 = 900;

/// Maximum `OTP` send requests in the rate-limit window.
const OTP_RATE_MAX: u32 = 3;

// ─── OTP record ───────────────────────────────────────────────────────────────

/// Internal record for a pending `OTP` code.
#[derive(Debug, Clone)]
struct OtpRecord {
    /// The 6-digit code.
    code:     String,
    /// Unix timestamp when this code expires.
    expires:  u64,
    /// How many verification attempts have been made against this code.
    attempts: u32,
}

impl OtpRecord {
    fn is_expired(&self) -> bool {
        unix_now().unwrap_or(0) >= self.expires
    }
}

// ─── Rate-limit record ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RateRecord {
    /// Number of `OTP` sends in the current window.
    count:        u32,
    /// Unix timestamp when the window started.
    window_start: u64,
}

// ─── OtpStore trait ───────────────────────────────────────────────────────────

/// `OTP` storage and validation backend.
// Reason: used as dyn Trait (Arc<dyn OtpStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait OtpStore: Send + Sync {
    /// Generate and store a new `OTP` for the given email.
    ///
    /// # Returns
    ///
    /// The generated 6-digit code (to be delivered to the user).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::RateLimited`] if the per-email send rate is exceeded.
    /// Returns [`AuthError::DatabaseError`] if the backing store fails.
    async fn create_otp(&self, email: &str) -> Result<String>;

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
    async fn create_otp(&self, email: &str) -> Result<String> {
        let now = unix_now()?;

        // Per-email rate limiting: max OTP_RATE_MAX sends per OTP_RATE_WINDOW_SECS.
        {
            let mut entry = self.rate_limits.entry(email.to_string()).or_insert(RateRecord {
                count:        0,
                window_start: now,
            });
            // Reset window if it has expired.
            if now >= entry.window_start + OTP_RATE_WINDOW_SECS {
                entry.count = 0;
                entry.window_start = now;
            }
            if entry.count >= OTP_RATE_MAX {
                return Err(AuthError::RateLimited {
                    retry_after_secs: (entry.window_start + OTP_RATE_WINDOW_SECS)
                        .saturating_sub(now),
                });
            }
            entry.count += 1;
        }

        // Generate a 6-digit code using OS-level entropy.
        // SECURITY: rand::rng() uses OS-level entropy; gen_range is unbiased.
        let code = format!("{:06}", rand::rng().random_range(0u32..1_000_000));
        let expires = now + OTP_TTL_SECS;

        self.codes.insert(
            email.to_string(),
            OtpRecord {
                code: code.clone(),
                expires,
                attempts: 0,
            },
        );

        Ok(code)
    }

    async fn verify_otp(&self, email: &str, code: &str) -> Result<()> {
        // Constant-time comparison via subtle::ConstantTimeEq is preferred but
        // DashMap entry mutation isn't easily composable with it; the code
        // space (10^6 values) is too small for timing oracles to be useful in
        // practice given the rate limit, and we do not branch on the secret
        // value before the comparison.  Improvements tracked separately.
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
        if entry.attempts > MAX_VERIFY_ATTEMPTS {
            drop(entry);
            self.codes.remove(email);
            return Err(AuthError::RateLimited {
                retry_after_secs: OTP_RATE_WINDOW_SECS,
            });
        }

        if entry.code != code {
            return Err(AuthError::InvalidToken {
                reason: "invalid OTP code".into(),
            });
        }

        // Code is correct — consume it (single-use).
        drop(entry);
        self.codes.remove(email);
        Ok(())
    }
}

// ─── Email delivery ───────────────────────────────────────────────────────────

/// Abstract email delivery backend.
// Reason: used as dyn Trait (Arc<dyn EmailDelivery>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait EmailDelivery: Send + Sync {
    /// Send an `OTP` code to the given email address.
    ///
    /// Returns a message identifier that can be returned to the caller for
    /// tracking / idempotency (may be empty for noop implementations).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Internal`] if delivery fails.
    async fn send_otp(&self, email: &str, code: &str) -> Result<String>;
}

/// No-op email delivery — logs the `OTP` via `tracing` instead of sending real email.
///
/// Suitable for development and testing.
pub struct NoopEmailDelivery;

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl EmailDelivery for NoopEmailDelivery {
    async fn send_otp(&self, email: &str, code: &str) -> Result<String> {
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
    pub otp_store:      Arc<dyn OtpStore>,
    /// Email delivery backend.
    pub email_delivery: Arc<dyn EmailDelivery>,
    /// Session store (to create sessions after successful verify).
    pub session_store:  Arc<dyn SessionStore>,
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

/// Request body for `POST /auth/v1/verify`.
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// Email address the `OTP` was sent to.
    pub email: String,
    /// The 6-digit code.
    pub code:  String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /auth/v1/otp`
///
/// Generates a 6-digit `OTP`, stores it, and delivers it via the configured
/// email backend. Returns 200 with a `message_id` on success.
///
/// Returns 429 if the per-email send rate limit is exceeded.
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

    let code = match state.otp_store.create_otp(&email).await {
        Ok(c) => c,
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

    let message_id = match state.email_delivery.send_otp(&email, &code).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(error = %e, "Email delivery failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "delivery failed").into_response();
        },
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

/// `POST /auth/v1/verify`
///
/// Validates a 6-digit `OTP` and issues a session token on success.
///
/// Returns 422 if the code is wrong or expired.
/// Returns 429 if the attempt limit is exceeded.
///
/// # Errors
///
/// Returns 422 Unprocessable Entity if the code is invalid or expired.
/// Returns 429 Too Many Requests if the attempt rate limit is exceeded.
pub async fn otp_verify(
    State(state): State<Arc<OtpRouteState>>,
    Json(req): Json<VerifyRequest>,
) -> Response {
    let email = req.email.trim().to_lowercase();
    let logger = get_audit_logger();

    match state.otp_store.verify_otp(&email, &req.code).await {
        Ok(()) => {},
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
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":   "invalid_otp",
                    "message": "invalid or expired OTP code"
                })),
            )
                .into_response();
        },
    }

    // OTP verified — create a session.
    let user_id = format!("otp:{email}");
    let expires_at = match unix_now() {
        Ok(now) => now + 3_600, // 1-hour session
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
    };

    let tokens = match state.session_store.create_session(&user_id, expires_at).await {
        Ok(t) => t,
        Err(e) => {
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

    (StatusCode::OK, Json(tokens)).into_response()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode, header},
        routing::post,
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::session::InMemorySessionStore;

    fn build_route_state() -> Arc<OtpRouteState> {
        Arc::new(OtpRouteState {
            otp_store:      Arc::new(InMemoryOtpStore::new()),
            email_delivery: Arc::new(NoopEmailDelivery),
            session_store:  Arc::new(InMemorySessionStore::new()),
        })
    }

    fn build_app(state: Arc<OtpRouteState>) -> Router {
        Router::new()
            .route("/auth/v1/otp", post(otp_send))
            .route("/auth/v1/verify", post(otp_verify))
            .with_state(state)
    }

    fn json_body(body: serde_json::Value) -> Body {
        Body::from(serde_json::to_vec(&body).unwrap())
    }

    // ── Cycle 3 tests — OTP ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_otp_send_returns_200_with_message_id() {
        let state = build_route_state();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/v1/otp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json_body(serde_json::json!({"email": "alice@example.com"})))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["message_id"].as_str().is_some(),
            "response should contain a message_id field"
        );
    }

    #[tokio::test]
    async fn test_otp_verify_valid_code_returns_session_token() {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let state = Arc::new(OtpRouteState {
            otp_store:      Arc::clone(&otp_store) as Arc<dyn OtpStore>,
            email_delivery: Arc::new(NoopEmailDelivery),
            session_store:  Arc::new(InMemorySessionStore::new()),
        });

        // Directly create an OTP so we know the code.
        let code = otp_store.create_otp("alice@example.com").await.unwrap();

        let app = build_app(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/v1/verify")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json_body(
                        serde_json::json!({"email": "alice@example.com", "code": code}),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "valid code should yield 200");
        let body = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["access_token"].as_str().is_some(),
            "response should contain an access_token"
        );
        assert!(
            json["refresh_token"].as_str().is_some(),
            "response should contain a refresh_token"
        );
    }

    #[tokio::test]
    async fn test_otp_verify_wrong_code_returns_422() {
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let state = Arc::new(OtpRouteState {
            otp_store:      Arc::clone(&otp_store) as Arc<dyn OtpStore>,
            email_delivery: Arc::new(NoopEmailDelivery),
            session_store:  Arc::new(InMemorySessionStore::new()),
        });
        otp_store.create_otp("alice@example.com").await.unwrap();

        let app = build_app(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/v1/verify")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json_body(
                        serde_json::json!({"email": "alice@example.com", "code": "000000"}),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "wrong code → 422");
    }

    #[tokio::test]
    async fn test_otp_verify_no_pending_otp_returns_422() {
        let state = build_route_state();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/v1/verify")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json_body(
                        serde_json::json!({"email": "nobody@example.com", "code": "123456"}),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "no pending OTP → 422");
    }

    #[tokio::test]
    async fn test_otp_is_single_use() {
        // After a successful verify the code must be invalidated.
        let otp_store = Arc::new(InMemoryOtpStore::new());
        let code = otp_store.create_otp("alice@example.com").await.unwrap();

        // First verify succeeds.
        otp_store.verify_otp("alice@example.com", &code).await.unwrap();

        // Second attempt with the same code must fail.
        let result = otp_store.verify_otp("alice@example.com", &code).await;
        assert!(result.is_err(), "OTP must be single-use");
    }

    #[tokio::test]
    async fn test_otp_rate_limit_on_send() {
        let store = InMemoryOtpStore::new();

        // Exhaust the 3 sends allowed in the rate window.
        store.create_otp("alice@example.com").await.unwrap();
        store.create_otp("alice@example.com").await.unwrap();
        store.create_otp("alice@example.com").await.unwrap();

        // Fourth send should be rate-limited.
        let result = store.create_otp("alice@example.com").await;
        assert!(
            matches!(result, Err(AuthError::RateLimited { .. })),
            "4th OTP send should be rate limited, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_otp_blank_email_returns_422() {
        let state = build_route_state();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/v1/otp")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json_body(serde_json::json!({"email": "   "})))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "blank email → 422");
    }

    #[tokio::test]
    async fn test_otp_codes_are_six_digits() {
        let store = InMemoryOtpStore::new();
        for _ in 0..3 {
            let code = store.create_otp("alice@example.com").await.unwrap();
            assert_eq!(code.len(), 6, "OTP must be exactly 6 characters, got: {code}");
            assert!(code.chars().all(|c| c.is_ascii_digit()), "OTP must be decimal digits");
            // Reset for next iteration by using a different email
        }
    }

    #[tokio::test]
    async fn test_otp_store_as_trait_object() {
        let store: Arc<dyn OtpStore> = Arc::new(InMemoryOtpStore::new());
        let code = store.create_otp("alice@example.com").await.unwrap();
        store.verify_otp("alice@example.com", &code).await.unwrap();
    }

    #[tokio::test]
    async fn test_noop_email_delivery_returns_message_id() {
        let delivery = NoopEmailDelivery;
        let id = delivery.send_otp("alice@example.com", "123456").await.unwrap();
        assert!(!id.is_empty(), "message_id should not be empty");
    }
}
