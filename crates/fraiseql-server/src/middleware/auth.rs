//! Authentication middleware.
//!
//! Provides bearer token authentication for protected endpoints.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use subtle::ConstantTimeEq as _;

/// Window length (in seconds) for the admin brute-force rate limiter.
const ADMIN_AUTH_WINDOW_SECS: u64 = 60;

/// Per-IP failure record for the admin brute-force guard.
#[derive(Clone)]
struct FailureRecord {
    count:        u32,
    window_start: u64,
}

/// Per-IP sliding-window counter for failed bearer token attempts.
///
/// Shared inside `BearerAuthState` via an `Arc`-wrapped `DashMap` so that
/// the state can be `Clone`d cheaply across requests.
#[derive(Clone)]
struct FailureLimiter {
    records:     Arc<DashMap<String, FailureRecord>>,
    max_failures: u32,
}

impl FailureLimiter {
    fn new(max_failures: u32) -> Self {
        Self {
            records: Arc::new(DashMap::new()),
            max_failures,
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Record a failed attempt and return `true` if the IP is now rate-limited.
    fn record_failure(&self, ip: &str) -> bool {
        let now = Self::now_secs();
        let mut entry = self
            .records
            .entry(ip.to_string())
            .or_insert_with(|| FailureRecord { count: 0, window_start: now });

        if now >= entry.window_start + ADMIN_AUTH_WINDOW_SECS {
            // Window expired — start fresh
            entry.count = 1;
            entry.window_start = now;
            false
        } else {
            entry.count = entry.count.saturating_add(1);
            entry.count >= self.max_failures
        }
    }

    /// Return `true` if the IP is already rate-limited (without recording a new failure).
    fn is_blocked(&self, ip: &str) -> bool {
        let now = Self::now_secs();
        if let Some(entry) = self.records.get(ip) {
            if now < entry.window_start + ADMIN_AUTH_WINDOW_SECS {
                return entry.count >= self.max_failures;
            }
        }
        false
    }

    /// Reset the failure counter for an IP after a successful authentication.
    fn record_success(&self, ip: &str) {
        self.records.remove(ip);
    }

    /// Return the current failure count for an IP (used in tests).
    #[cfg(test)]
    fn failure_count(&self, ip: &str) -> u32 {
        self.records.get(ip).map_or(0, |e| e.count)
    }
}

/// Shared state for bearer token authentication.
#[derive(Clone)]
pub struct BearerAuthState {
    /// Expected bearer token.
    pub token: Arc<String>,
    /// Per-IP brute-force guard.
    failure_limiter: FailureLimiter,
}

impl BearerAuthState {
    /// Create new bearer auth state with the default max-failures limit (10).
    #[must_use]
    pub fn new(token: String) -> Self {
        Self::with_max_failures(token, 10)
    }

    /// Create new bearer auth state with a custom max-failures limit.
    ///
    /// After `max_failures` failed attempts from the same IP within a 60-second
    /// window, further requests receive **429 Too Many Requests**.
    #[must_use]
    pub fn with_max_failures(token: String, max_failures: u32) -> Self {
        Self {
            token:           Arc::new(token),
            failure_limiter: FailureLimiter::new(max_failures),
        }
    }
}


/// Bearer token authentication middleware.
///
/// Validates that requests include a valid `Authorization: Bearer <token>` header.
///
/// # Response
///
/// - **401 Unauthorized**: Missing or malformed Authorization header
/// - **403 Forbidden**: Invalid token
///
/// # Example
///
/// ```text
/// // Requires: running Axum application with a route handler.
/// use axum::{Router, middleware};
/// use fraiseql_server::middleware::{bearer_auth_middleware, BearerAuthState};
///
/// let auth_state = BearerAuthState::new("my-secret-token".to_string());
///
/// let app = Router::new()
///     .route("/protected", get(handler))
///     .layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
/// ```
pub async fn bearer_auth_middleware(
    State(auth_state): State<BearerAuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Derive a best-effort peer key for rate limiting.
    // ConnectInfo is only available when the server was started with
    // `into_make_service_with_connect_info`; fall back to a header-based key.
    use std::net::SocketAddr;

    use axum::extract::ConnectInfo;
    let peer_key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Reject immediately if already rate-limited (avoids any header work).
    if auth_state.failure_limiter.is_blocked(&peer_key) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many failed auth attempts").into_response();
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    match auth_header {
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                "Missing Authorization header",
            )
                .into_response();
        },
        Some(header_value) => {
            // Check for "Bearer " prefix
            if !header_value.starts_with("Bearer ") {
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "Bearer")],
                    "Invalid Authorization header format. Expected: Bearer <token>",
                )
                    .into_response();
            }

            // Extract token
            let token = &header_value[7..]; // Skip "Bearer "

            // Constant-time comparison to prevent timing attacks
            if !constant_time_compare(token, &auth_state.token) {
                // Record failure; return 429 once the limit is crossed.
                if auth_state.failure_limiter.record_failure(&peer_key) {
                    return (
                        StatusCode::TOO_MANY_REQUESTS,
                        "Too many failed auth attempts",
                    )
                        .into_response();
                }
                return (StatusCode::FORBIDDEN, "Invalid token").into_response();
            }

            // Successful auth — reset the failure counter.
            auth_state.failure_limiter.record_success(&peer_key);
        },
    }

    // Token is valid, proceed with request
    next.run(request).await
}

/// Extract the bearer token from an `Authorization` header value.
///
/// Returns `Some(token)` if the header has the `Bearer ` prefix (with trailing space),
/// `None` for all other formats (Basic, Digest, missing prefix, etc.).
///
/// Exposed as `pub` for property testing.
pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

/// Constant-time string comparison to prevent timing attacks.
///
/// Uses [`subtle::ConstantTimeEq`] to compare the byte representations of
/// both strings, preventing the compiler from optimising the comparison into
/// an early-exit branch that would leak information about where the strings
/// differ (timing oracle, RFC 6749 §10.12).
///
/// Strings of different lengths return `false` without inspecting bytes;
/// token lengths are considered non-secret (administrators choose them).
fn constant_time_compare(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    async fn protected_handler() -> &'static str {
        "secret data"
    }

    fn create_test_app(token: &str) -> Router {
        let auth_state = BearerAuthState::new(token.to_string());

        Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
    }

    #[tokio::test]
    async fn test_valid_token_allows_access() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer secret-token-12345")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_missing_auth_header_returns_401() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder().uri("/protected").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key("www-authenticate"));
    }

    #[tokio::test]
    async fn test_invalid_auth_format_returns_401() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Basic dXNlcjpwYXNz") // Basic auth, not Bearer
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_wrong_token_returns_403() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_empty_bearer_token_returns_403() {
        let app = create_test_app("secret-token-12345");

        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer ")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_constant_time_compare_equal() {
        assert!(constant_time_compare("hello", "hello"));
        assert!(constant_time_compare("", ""));
        assert!(constant_time_compare("a-long-token-123", "a-long-token-123"));
    }

    #[test]
    fn test_constant_time_compare_not_equal() {
        assert!(!constant_time_compare("hello", "world"));
        assert!(!constant_time_compare("hello", "hello!"));
        assert!(!constant_time_compare("hello", "hell"));
        assert!(!constant_time_compare("abc", "abd"));
    }

    #[test]
    fn test_constant_time_compare_different_lengths() {
        assert!(!constant_time_compare("short", "longer-string"));
        assert!(!constant_time_compare("", "notempty"));
    }

    // ── subtle-based comparison tests (15-1) ────────────────────────────────

    #[test]
    fn test_subtle_compare_identical_tokens() {
        // Verify the subtle-based helper accepts identical tokens of various lengths.
        assert!(constant_time_compare("x", "x"));
        assert!(constant_time_compare(
            "super-secret-32-char-admin-token",
            "super-secret-32-char-admin-token"
        ));
    }

    #[test]
    fn test_subtle_compare_off_by_one_byte() {
        // A single byte difference anywhere must be rejected.
        assert!(!constant_time_compare("token-abc", "token-abd")); // last byte differs
        assert!(!constant_time_compare("Aoken-abc", "token-abc")); // first byte differs
    }

    #[test]
    fn test_subtle_compare_empty_strings() {
        // Two empty strings are equal; empty vs non-empty is not.
        assert!(constant_time_compare("", ""));
        assert!(!constant_time_compare("", "a"));
        assert!(!constant_time_compare("a", ""));
    }

    // ── S48 brute-force rate limiter tests ──────────────────────────────────

    #[test]
    fn test_failure_limiter_not_blocked_initially() {
        let limiter = FailureLimiter::new(3);
        assert!(!limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_blocks_after_max_failures() {
        let limiter = FailureLimiter::new(3);
        assert!(!limiter.record_failure("1.2.3.4")); // 1st → not blocked
        assert!(!limiter.record_failure("1.2.3.4")); // 2nd → not blocked
        assert!(limiter.record_failure("1.2.3.4")); // 3rd → now blocked
        assert!(limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_success_resets_counter() {
        let limiter = FailureLimiter::new(3);
        limiter.record_failure("1.2.3.4");
        limiter.record_failure("1.2.3.4");
        assert_eq!(limiter.failure_count("1.2.3.4"), 2);
        limiter.record_success("1.2.3.4");
        assert_eq!(limiter.failure_count("1.2.3.4"), 0);
        assert!(!limiter.is_blocked("1.2.3.4"));
    }

    #[test]
    fn test_failure_limiter_independent_per_ip() {
        let limiter = FailureLimiter::new(2);
        limiter.record_failure("10.0.0.1");
        limiter.record_failure("10.0.0.1");
        assert!(limiter.is_blocked("10.0.0.1"));
        // Different IP should not be blocked.
        assert!(!limiter.is_blocked("10.0.0.2"));
    }

    #[tokio::test]
    async fn test_middleware_returns_429_after_max_failures() {
        // Use max_failures = 2 so the test does not send too many requests.
        let auth_state = BearerAuthState::with_max_failures("correct-token".to_string(), 2);
        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state,
                bearer_auth_middleware,
            ));

        // Two bad attempts (from unknown peer since ConnectInfo not wired in tests).
        for _ in 0..2 {
            let req = Request::builder()
                .uri("/protected")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap();
            let _ = app.clone().oneshot(req).await.unwrap();
        }

        // Third attempt should be 429 (already blocked after 2 failures).
        let req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_middleware_resets_counter_on_success() {
        let auth_state = BearerAuthState::with_max_failures("good-token".to_string(), 2);
        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                auth_state,
                bearer_auth_middleware,
            ));

        // One bad attempt, then a successful one.
        let bad_req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer bad-token")
            .body(Body::empty())
            .unwrap();
        let r = app.clone().oneshot(bad_req).await.unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);

        let good_req = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer good-token")
            .body(Body::empty())
            .unwrap();
        let r = app.clone().oneshot(good_req).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);

        // After success the counter should have been reset; one more bad attempt
        // should be 403, not 429.
        let bad_req2 = Request::builder()
            .uri("/protected")
            .header("Authorization", "Bearer bad-token")
            .body(Body::empty())
            .unwrap();
        let r = app.oneshot(bad_req2).await.unwrap();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_admin_auth_max_failures_default_is_ten() {
        use crate::server_config::ServerConfig;
        let cfg = ServerConfig::default();
        assert_eq!(cfg.admin_auth_max_failures, 10);
    }
}
