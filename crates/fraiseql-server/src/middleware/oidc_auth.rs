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

/// Request extension containing the `jti` claim of the validated bearer token.
///
/// Populated by [`oidc_auth_middleware`] immediately after `validate_token`
/// succeeds — at that point the token's signature, expiry, audience, and
/// (when enabled) replay-cache check have all been verified, so re-decoding
/// the payload to extract `jti` carries no integrity risk.
///
/// `None` indicates the token had no `jti` claim. Handlers that must revoke
/// the caller's current session (e.g. `POST /auth/revoke`) should treat
/// `Some(jti)` as the only valid input — there is no per-request identifier
/// to revoke without it.
#[derive(Clone, Debug)]
pub struct SessionJti(pub Option<String>);

/// Minimal JWT payload deserializer used to extract `jti` from an
/// already-validated bearer token. The validator has performed the heavy
/// integrity checks; this struct only pulls out the per-token identifier.
#[derive(serde::Deserialize)]
struct JtiOnlyClaims {
    jti: Option<String>,
}

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
                    // Re-decode the (already-validated) token payload to surface
                    // the `jti` claim for downstream handlers that need to
                    // revoke the caller's current session.  `insecure_decode`
                    // is safe here because `validate_token` above has already
                    // checked signature, expiry, audience, and (when enabled)
                    // replay-cache state.
                    let jti = jsonwebtoken::dangerous::insecure_decode::<JtiOnlyClaims>(&token)
                        .ok()
                        .and_then(|d| d.claims.jti);
                    request.extensions_mut().insert(AuthUser(user));
                    request.extensions_mut().insert(SessionJti(jti));
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
