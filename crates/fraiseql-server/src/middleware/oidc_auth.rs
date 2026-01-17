//! OIDC Authentication Middleware
//!
//! Provides JWT authentication for GraphQL endpoints using OIDC discovery.
//! Supports Auth0, Keycloak, Okta, Cognito, Azure AD, and any OIDC-compliant provider.

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fraiseql_core::security::{AuthenticatedUser, OidcValidator};
use std::sync::Arc;

/// State for OIDC authentication middleware.
#[derive(Clone)]
pub struct OidcAuthState {
    /// The OIDC validator.
    pub validator: Arc<OidcValidator>,
}

impl OidcAuthState {
    /// Create new OIDC auth state.
    #[must_use]
    pub fn new(validator: Arc<OidcValidator>) -> Self {
        Self { validator }
    }
}

/// Request extension containing the authenticated user.
///
/// After authentication middleware runs, handlers can extract this
/// to access the authenticated user information.
#[derive(Clone, Debug)]
pub struct AuthUser(pub AuthenticatedUser);

/// OIDC authentication middleware.
///
/// Validates JWT tokens from the Authorization header using OIDC/JWKS.
///
/// # Behavior
///
/// - If auth is required and no token: returns 401 Unauthorized
/// - If token is invalid/expired: returns 401 Unauthorized
/// - If token is valid: adds `AuthUser` to request extensions
/// - If auth is optional and no token: allows request through (no AuthUser)
///
/// # Example
///
/// ```ignore
/// use axum::{middleware, Router};
///
/// let oidc_state = OidcAuthState::new(validator);
/// let app = Router::new()
///     .route("/graphql", post(graphql_handler))
///     .layer(middleware::from_fn_with_state(oidc_state, oidc_auth_middleware));
/// ```
pub async fn oidc_auth_middleware(
    State(auth_state): State<OidcAuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    match auth_header {
        None => {
            // No authorization header
            if auth_state.validator.is_required() {
                tracing::debug!("Authentication required but no Authorization header");
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, format!("Bearer realm=\"{}\"", auth_state.validator.issuer()))],
                    "Authentication required",
                )
                    .into_response();
            }
            // Auth is optional, continue without user context
            next.run(request).await
        }
        Some(header_value) => {
            // Extract bearer token
            if !header_value.starts_with("Bearer ") {
                tracing::debug!("Invalid Authorization header format");
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "Bearer error=\"invalid_request\"".to_string())],
                    "Invalid Authorization header format",
                )
                    .into_response();
            }

            let token = &header_value[7..];

            // Validate token
            match auth_state.validator.validate_token(token).await {
                Ok(user) => {
                    tracing::debug!(
                        user_id = %user.user_id,
                        scopes = ?user.scopes,
                        "User authenticated successfully"
                    );
                    // Add authenticated user to request extensions
                    request.extensions_mut().insert(AuthUser(user));
                    next.run(request).await
                }
                Err(e) => {
                    tracing::debug!(error = %e, "Token validation failed");
                    let error_description = match &e {
                        fraiseql_core::security::SecurityError::TokenExpired { .. } => {
                            "Bearer error=\"invalid_token\", error_description=\"Token has expired\""
                        }
                        fraiseql_core::security::SecurityError::InvalidToken => {
                            "Bearer error=\"invalid_token\", error_description=\"Token is invalid\""
                        }
                        _ => "Bearer error=\"invalid_token\"",
                    };
                    (
                        StatusCode::UNAUTHORIZED,
                        [(header::WWW_AUTHENTICATE, error_description.to_string())],
                        "Invalid or expired token",
                    )
                        .into_response()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_clone() {
        use chrono::Utc;

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec!["read".to_string()],
            expires_at: Utc::now(),
        };

        let auth_user = AuthUser(user.clone());
        let cloned = auth_user.clone();

        assert_eq!(auth_user.0.user_id, cloned.0.user_id);
    }

    #[test]
    fn test_oidc_auth_state_clone() {
        // Can't easily test without a real validator, but we can verify Clone is implemented
        // by verifying the type compiles with Clone trait bound
        fn assert_clone<T: Clone>() {}
        assert_clone::<OidcAuthState>();
    }
}
