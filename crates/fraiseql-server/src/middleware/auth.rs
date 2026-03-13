//! Authentication middleware.
//!
//! Provides bearer token authentication for protected endpoints.

use std::sync::Arc;

use subtle::ConstantTimeEq as _;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Shared state for bearer token authentication.
#[derive(Clone)]
pub struct BearerAuthState {
    /// Expected bearer token.
    pub token: Arc<String>,
}

impl BearerAuthState {
    /// Create new bearer auth state.
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token: Arc::new(token),
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
/// ```no_run
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
                return (StatusCode::FORBIDDEN, "Invalid token").into_response();
            }
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
        assert!(constant_time_compare("super-secret-32-char-admin-token", "super-secret-32-char-admin-token"));
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
}
