//! OIDC Authentication Middleware
//!
//! Provides JWT authentication for GraphQL endpoints using OIDC discovery.
//! Supports Auth0, Keycloak, Okta, Cognito, Azure AD, and any OIDC-compliant provider.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fraiseql_core::security::{AuthenticatedUser, OidcValidator};

/// State for OIDC authentication middleware.
#[derive(Clone)]
pub struct OidcAuthState {
    /// The OIDC validator.
    pub validator: Arc<OidcValidator>,
}

impl OidcAuthState {
    /// Create new OIDC auth state.
    #[must_use]
    pub const fn new(validator: Arc<OidcValidator>) -> Self {
        Self { validator }
    }
}

/// Request extension containing the authenticated user.
///
/// After authentication middleware runs, handlers can extract this
/// to access the authenticated user information.
#[derive(Clone, Debug)]
pub struct AuthUser(pub AuthenticatedUser);

/// Extract the bearer token from a raw `Cookie` header value.
///
/// Looks for `__Host-access_token=<value>` in the semicolon-separated cookie
/// string and returns the token value, stripping RFC 6265 double-quotes if
/// present.  Returns `None` if the cookie is absent.
///
/// This is used as a fallback by [`oidc_auth_middleware`] when no
/// `Authorization: Bearer` header is present, to support browser flows where
/// the JWT is stored in an `HttpOnly` cookie inaccessible to client-side script.
pub(crate) fn extract_access_token_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).and_then(|cookies| {
        cookies.split(';').find_map(|part| {
            let part = part.trim();
            part.strip_prefix("__Host-access_token=")
                .map(|v| v.trim_matches('"').to_owned())
        })
    })
}

/// OIDC authentication middleware.
///
/// Validates JWT tokens from the `Authorization: Bearer` header using
/// OIDC/JWKS.  When no `Authorization` header is present, falls back to the
/// `__Host-access_token` `HttpOnly` cookie set by the PKCE callback.
///
/// # Behavior
///
/// - If auth is required and no token (header or cookie): returns 401 Unauthorized
/// - If token is invalid/expired: returns 401 Unauthorized
/// - If token is valid: adds `AuthUser` to request extensions
/// - If auth is optional and no token: allows request through (no `AuthUser`)
///
/// # Example
///
/// ```text
/// // Requires: OIDC provider reachable for JWKS discovery, running Axum application.
/// use axum::{middleware, Router};
///
/// let oidc_state = OidcAuthState::new(validator);
/// let app = Router::new()
///     .route("/graphql", post(graphql_handler))
///     .layer(middleware::from_fn_with_state(oidc_state, oidc_auth_middleware));
/// ```
#[allow(clippy::cognitive_complexity)] // Reason: OIDC authentication middleware with token parsing, validation, and claims extraction
pub async fn oidc_auth_middleware(
    State(auth_state): State<OidcAuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Prefer Authorization: Bearer header; fall back to __Host-access_token cookie.
    // The token is extracted as an owned String to avoid borrow conflicts with
    // request.extensions_mut() later in this function.
    let token_string: Option<String> = {
        let auth_header = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());

        match auth_header {
            Some(header_value) => {
                if !header_value.starts_with("Bearer ") {
                    tracing::debug!("Invalid Authorization header format");
                    return (
                        StatusCode::UNAUTHORIZED,
                        [(
                            header::WWW_AUTHENTICATE,
                            "Bearer error=\"invalid_request\"".to_string(),
                        )],
                        "Invalid Authorization header format",
                    )
                        .into_response();
                }
                Some(header_value[7..].to_owned())
            },
            None => extract_access_token_cookie(request.headers()),
        }
    };

    match token_string {
        None => {
            if auth_state.validator.is_required() {
                tracing::debug!("Authentication required but no token found (header or cookie)");
                return (
                    StatusCode::UNAUTHORIZED,
                    [(
                        header::WWW_AUTHENTICATE,
                        format!("Bearer realm=\"{}\"", auth_state.validator.issuer()),
                    )],
                    "Authentication required",
                )
                    .into_response();
            }
            // Auth is optional, continue without user context
            next.run(request).await
        },
        Some(token) => {
            // Validate token
            match auth_state.validator.validate_token(&token).await {
                Ok(user) => {
                    tracing::debug!(
                        user_id = %user.user_id,
                        scopes = ?user.scopes,
                        "User authenticated successfully"
                    );
                    // Add authenticated user to request extensions
                    request.extensions_mut().insert(AuthUser(user));
                    next.run(request).await
                },
                Err(e) => {
                    tracing::debug!(error = %e, "Token validation failed");
                    let (www_authenticate, body) = match &e {
                        fraiseql_core::security::SecurityError::TokenExpired { .. } => (
                            "Bearer error=\"invalid_token\", error_description=\"Token has expired\"",
                            "Token has expired",
                        ),
                        fraiseql_core::security::SecurityError::InvalidToken => (
                            "Bearer error=\"invalid_token\", error_description=\"Token is invalid\"",
                            "Token is invalid",
                        ),
                        _ => ("Bearer error=\"invalid_token\"", "Invalid or expired token"),
                    };
                    (
                        StatusCode::UNAUTHORIZED,
                        [(header::WWW_AUTHENTICATE, www_authenticate.to_string())],
                        body,
                    )
                        .into_response()
                },
            }
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_auth_user_clone() {
        use chrono::Utc;

        let user = AuthenticatedUser {
            user_id:      "user123".to_string(),
            scopes:       vec!["read".to_string()],
            expires_at:   Utc::now(),
            extra_claims: std::collections::HashMap::new(),
        };

        let auth_user = AuthUser(user);
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

    #[test]
    fn test_cookie_fallback_extracts_token() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "__Host-access_token=my.jwt.token; Path=/; SameSite=Strict".parse().unwrap(),
        );

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("my.jwt.token"));
    }

    #[test]
    fn test_cookie_fallback_strips_rfc6265_quotes() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, "__Host-access_token=\"my.jwt.token\"".parse().unwrap());

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("my.jwt.token"));
    }

    #[test]
    fn test_cookie_fallback_absent_returns_none() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, "session=abc; other=xyz".parse().unwrap());

        let token = extract_access_token_cookie(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_cookie_fallback_no_cookie_header_returns_none() {
        let headers = axum::http::HeaderMap::new();
        let token = extract_access_token_cookie(&headers);
        assert!(token.is_none());
    }

    #[test]
    fn test_cookie_fallback_multiple_cookies_finds_correct_one() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "session=abc; __Host-access_token=correct.token; csrf=xyz".parse().unwrap(),
        );

        let token = extract_access_token_cookie(&headers);
        assert_eq!(token.as_deref(), Some("correct.token"));
    }
}
