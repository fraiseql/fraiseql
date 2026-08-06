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
    pub validator:        Arc<AuthMiddleware>,
    /// Realm advertised in `WWW-Authenticate` challenges.
    pub realm:            String,
    /// Service-account authenticator, when `[security.service_accounts]` declares any.
    ///
    /// Consulted only to decide whether a **bearer-less** request may proceed to the
    /// handler, which is where the principal is actually resolved (ADR-0018). This
    /// layer never builds a `SecurityContext` from it — it decides pass or refuse.
    pub service_accounts: Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
}

impl Hs256AuthState {
    /// Create new HS256 auth state.
    #[must_use]
    pub const fn new(validator: Arc<AuthMiddleware>, realm: String) -> Self {
        Self {
            validator,
            realm,
            service_accounts: None,
        }
    }

    /// Attach the service-account authenticator so bearer-less service-account
    /// requests reach the handler's seam instead of being refused here (#934).
    #[must_use]
    pub fn with_service_accounts(
        mut self,
        authenticator: Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
    ) -> Self {
        self.service_accounts = authenticator;
        self
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

    // #934: a service account presents its secret on `x-api-key` and carries NO
    // bearer, because the seam that resolves it lives in the GraphQL handler
    // (ADR-0018). This layer is a `route_layer`, so it ran first and refused the
    // request before that seam was reached — service accounts worked under `[auth]`
    // and were unreachable under `[auth_hs256]`.
    //
    // The deferral is deliberately narrower than the OIDC layer's pass-through: it
    // applies only when there is no `Authorization` header AND the secret presented
    // resolves to a declared account. A bearer that is present but invalid still
    // 401s here, and a request with no credentials at all still 401s here — so this
    // cannot become an anonymous door into an authenticated transport. The handler
    // re-resolves the same secret to build the principal; this layer only decides
    // whether the request may reach it.
    if auth_header.is_none() {
        if let Some(ref sa) = auth_state.service_accounts {
            if matches!(
                sa.resolve(request.headers(), false),
                crate::service_account::SaAuth::Authenticated(_)
            ) {
                tracing::debug!(
                    "bearer-less request carries a valid service-account secret — deferring \
                     to the handler's ADR-0018 seam"
                );
                return next.run(request).await;
            }
        }
    }

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
