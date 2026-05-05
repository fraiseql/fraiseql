//! Authentication middleware.
//!
//! Provides bearer token authentication for protected endpoints.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq as _;

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
pub(crate) fn constant_time_compare(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

