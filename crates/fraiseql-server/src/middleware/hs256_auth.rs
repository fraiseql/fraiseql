//! HS256 Authentication Middleware
//!
//! Provides local JWT authentication for GraphQL endpoints using an HS256
//! shared-secret configured via `[auth_hs256]` in `fraiseql.toml`.
//!
//! Intended primarily for integration testing and internal service-to-service
//! auth — no network calls are made to validate tokens. For public-facing
//! production, prefer OIDC (`[auth]`).

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fraiseql_core::security::{AuthMiddleware, AuthRequest};

use super::oidc_auth::AuthUser;

/// State for HS256 authentication middleware.
#[derive(Clone)]
pub struct Hs256AuthState {
    /// The local JWT validator configured with an HS256 signing key.
    pub validator: Arc<AuthMiddleware>,
    /// Realm advertised in `WWW-Authenticate` challenges.
    pub realm: String,
}

impl Hs256AuthState {
    /// Create new HS256 auth state.
    #[must_use]
    pub const fn new(validator: Arc<AuthMiddleware>, realm: String) -> Self {
        Self { validator, realm }
    }
}

/// HS256 authentication middleware.
///
/// Validates JWT tokens from the `Authorization: Bearer` header using a
/// shared-secret HS256 key. All validation is local — no network calls.
///
/// On success, inserts an [`AuthUser`] extension into the request so
/// downstream handlers see the same extension shape as the OIDC path.
pub async fn hs256_auth_middleware(
    State(auth_state): State<Hs256AuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);

    let auth_req = AuthRequest::new(auth_header);

    match auth_state.validator.validate_request(&auth_req) {
        Ok(user) => {
            tracing::debug!(
                user_id = %user.user_id,
                scopes = ?user.scopes,
                "User authenticated successfully (HS256)"
            );
            request.extensions_mut().insert(AuthUser(user));
            next.run(request).await
        },
        Err(e) => {
            tracing::debug!(error = %e, "HS256 token validation failed");
            let (status, www_authenticate, body) = match &e {
                fraiseql_core::security::SecurityError::AuthRequired => (
                    StatusCode::UNAUTHORIZED,
                    format!("Bearer realm=\"{}\"", auth_state.realm),
                    "Authentication required",
                ),
                fraiseql_core::security::SecurityError::TokenExpired { .. } => (
                    StatusCode::UNAUTHORIZED,
                    "Bearer error=\"invalid_token\", error_description=\"Token has expired\""
                        .to_string(),
                    "Token has expired",
                ),
                _ => (
                    StatusCode::UNAUTHORIZED,
                    "Bearer error=\"invalid_token\"".to_string(),
                    "Invalid or expired token",
                ),
            };
            (status, [(header::WWW_AUTHENTICATE, www_authenticate)], body).into_response()
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use fraiseql_core::security::{AuthConfig, AuthMiddleware};

    use super::*;

    #[test]
    fn hs256_auth_state_is_cloneable() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<Hs256AuthState>();

        let mw = AuthMiddleware::from_config(AuthConfig::with_hs256("test-secret-123"));
        let _state = Hs256AuthState::new(Arc::new(mw), "test".to_string());
    }
}
