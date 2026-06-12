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

use crate::{
    middleware::admin_scope::ADMIN_SCOPE,
    token_revocation::{TokenRejection, TokenRevocationManager},
};

/// State for OIDC authentication middleware.
#[derive(Clone)]
pub struct OidcAuthState {
    /// The OIDC validator.
    pub validator:  Arc<OidcValidator>,
    /// Optional token-revocation manager. When present, every authenticated request is
    /// checked against the revocation store (single-`jti` revocation **and** the caller's
    /// `revoke-all` epoch) after token validation succeeds — closing H8, where revoked
    /// tokens were accepted until their natural `exp`.
    pub revocation: Option<Arc<TokenRevocationManager>>,
}

impl OidcAuthState {
    /// Create new OIDC auth state with no revocation enforcement.
    ///
    /// The server attaches a revocation manager via [`with_revocation`](Self::with_revocation)
    /// when `[security.token_revocation]` is configured; constructing without one (e.g. in
    /// tests) simply performs no revocation check.
    #[must_use]
    pub const fn new(validator: Arc<OidcValidator>) -> Self {
        Self {
            validator,
            revocation: None,
        }
    }

    /// Attach a token-revocation manager so authenticated requests are checked against
    /// the revocation store. `None` leaves revocation enforcement disabled.
    #[must_use]
    pub fn with_revocation(mut self, revocation: Option<Arc<TokenRevocationManager>>) -> Self {
        self.revocation = revocation;
        self
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

/// Minimal JWT payload deserializer used to extract the revocation-relevant claims
/// (`jti` and `iat`) from an already-validated bearer token. The validator has performed
/// the heavy integrity checks; this struct only pulls out the per-token identifier and
/// issued-at time used by the revocation check.
#[derive(serde::Deserialize)]
struct RevocationClaims {
    jti: Option<String>,
    iat: Option<i64>,
}

/// Build the 401 response for a revoked / un-revocation-checkable token.
fn revocation_rejected_response(rejection: &TokenRejection) -> Response {
    let (www_authenticate, body) = match rejection {
        TokenRejection::Revoked => (
            "Bearer error=\"invalid_token\", error_description=\"Token has been revoked\"",
            "Token has been revoked",
        ),
        TokenRejection::MissingJti => (
            "Bearer error=\"invalid_token\", error_description=\"Token lacks required jti claim\"",
            "Token lacks required jti claim",
        ),
        TokenRejection::StoreUnavailable => (
            "Bearer error=\"invalid_token\", error_description=\"Revocation store unavailable\"",
            "Revocation store unavailable",
        ),
    };
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, www_authenticate.to_string())],
        body,
    )
        .into_response()
}

/// Enforce token revocation for an already-validated bearer token.
///
/// Decodes `jti`/`iat` from the token (safe via `insecure_decode` — `validate_token` has
/// already verified signature/expiry/audience) and, when a revocation manager is present,
/// rejects the request if the token's `jti` is revoked or its `iat` predates the user's
/// `revoke-all` epoch. Returns the decoded `jti` (for `SessionJti`) on success, or a 401
/// response to short-circuit the request. With no revocation manager it is a pure
/// `jti`-decode (preserving prior behaviour).
async fn check_revocation(
    auth_state: &OidcAuthState,
    user: &AuthenticatedUser,
    token: &str,
) -> Result<Option<String>, Response> {
    let claims = jsonwebtoken::dangerous::insecure_decode::<RevocationClaims>(token)
        .ok()
        .map(|d| d.claims);
    let jti = claims.as_ref().and_then(|c| c.jti.clone());
    let iat = claims.as_ref().and_then(|c| c.iat);

    if let Some(revocation) = auth_state.revocation.as_ref() {
        if let Err(rejection) =
            revocation.check_token(jti.as_deref(), user.user_id.as_str(), iat).await
        {
            tracing::debug!(
                user_id = %user.user_id,
                ?rejection,
                "Token rejected by revocation check"
            );
            return Err(revocation_rejected_response(&rejection));
        }
    }

    Ok(jti)
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
                    // Enforce token revocation (single-`jti` + `revoke-all` epoch) and
                    // surface the `jti` claim for downstream handlers that need to revoke
                    // the caller's current session.  `check_revocation` re-decodes the
                    // already-validated token (safe via `insecure_decode`).
                    let jti = match check_revocation(&auth_state, &user, &token).await {
                        Ok(jti) => jti,
                        Err(response) => return response,
                    };
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

/// Outcome of pulling a bearer token from a request (header first, cookie fallback).
enum TokenExtraction {
    /// A token string was found (`Authorization: Bearer …` or `__Host-access_token`).
    Found(String),
    /// An `Authorization` header was present but not in `Bearer <token>` form.
    Malformed,
    /// No token in either the header or the `__Host-access_token` cookie.
    Absent,
}

/// Extract the bearer token from the `Authorization` header, falling back to the
/// `__Host-access_token` cookie. Distinguishes a malformed header from an absent
/// token so callers can return the right 401 body.
fn extract_bearer_or_cookie(headers: &axum::http::HeaderMap) -> TokenExtraction {
    let auth_header = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    match auth_header {
        Some(value) => match value.strip_prefix("Bearer ") {
            Some(token) => TokenExtraction::Found(token.to_owned()),
            None => TokenExtraction::Malformed,
        },
        None => match extract_access_token_cookie(headers) {
            Some(token) => TokenExtraction::Found(token),
            None => TokenExtraction::Absent,
        },
    }
}

/// Mandatory authentication shared by [`admin_auth_middleware`] and
/// [`required_auth_middleware`].
///
/// Extracts a bearer token (header or cookie) and rejects with 401 when it is absent,
/// malformed, or invalid; on success it inserts `AuthUser` / `SessionJti` into the
/// request extensions and returns the validated user.
///
/// Unlike [`oidc_auth_middleware`], the token is **always** required regardless of the
/// validator's global `is_required()` flag. That flag governs only the anonymous data
/// plane; honouring it on the admin plane is exactly the H5 bypass this layer closes
/// (an admin router silently un-authed whenever a deployment runs with optional data
/// auth).
async fn authenticate_required(
    auth_state: &OidcAuthState,
    request: &mut Request<Body>,
) -> Result<AuthenticatedUser, Response> {
    let token = match extract_bearer_or_cookie(request.headers()) {
        TokenExtraction::Found(token) => token,
        TokenExtraction::Malformed => {
            tracing::debug!("Admin/required auth: malformed Authorization header");
            return Err((
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer error=\"invalid_request\"".to_string())],
                "Invalid Authorization header format",
            )
                .into_response());
        },
        TokenExtraction::Absent => {
            tracing::debug!("Admin/required auth: no token (header or cookie)");
            return Err((
                StatusCode::UNAUTHORIZED,
                [(
                    header::WWW_AUTHENTICATE,
                    format!("Bearer realm=\"{}\"", auth_state.validator.issuer()),
                )],
                "Authentication required",
            )
                .into_response());
        },
    };

    match auth_state.validator.validate_token(&token).await {
        Ok(user) => {
            // Enforce token revocation on the admin/required planes too — a revoked token
            // must be rejected everywhere, not only on the data plane.
            let jti = match check_revocation(auth_state, &user, &token).await {
                Ok(jti) => jti,
                Err(response) => return Err(response),
            };
            request.extensions_mut().insert(AuthUser(user.clone()));
            request.extensions_mut().insert(SessionJti(jti));
            Ok(user)
        },
        Err(e) => {
            tracing::debug!(error = %e, "Admin/required auth: token validation failed");
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
            Err((
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, www_authenticate.to_string())],
                body,
            )
                .into_response())
        },
    }
}

/// Admin-plane authentication **and** authorization middleware (Phase 03 C3).
///
/// Requires a valid bearer token (always — see `authenticate_required`) **and** the
/// `fraiseql:admin` scope. A missing/invalid token returns 401; a valid token without
/// the admin scope returns 403. Applied to the true admin plane (observer admin API,
/// design-audit API), it closes both H5 (admin routers un-authed when the global data
/// plane is optional) and H6 (admin routers authenticated but not authorized — e.g. any
/// end-user token could read observer `actions[].headers` webhook secrets or drive DLQ
/// retry/delete).
pub async fn admin_auth_middleware(
    State(auth_state): State<OidcAuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match authenticate_required(&auth_state, &mut request).await {
        Ok(user) => {
            if user.has_scope(ADMIN_SCOPE) {
                next.run(request).await
            } else {
                tracing::debug!(user_id = %user.user_id, "Admin scope missing — denying");
                (StatusCode::FORBIDDEN, format!("Admin API requires '{ADMIN_SCOPE}' scope"))
                    .into_response()
            }
        },
        Err(response) => response,
    }
}

/// Mandatory-authentication middleware (Phase 03 C3).
///
/// Requires a valid bearer token (any scope). Unlike [`oidc_auth_middleware`] it never
/// defers to the validator's global `is_required()` flag, so a route an operator marked
/// "require auth" actually rejects anonymous callers even when the data plane is
/// optional (H5). Applied to the schema-exposing operator endpoints (introspection,
/// schema export, schema metadata) where a valid non-admin token is still legitimate.
pub async fn required_auth_middleware(
    State(auth_state): State<OidcAuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match authenticate_required(&auth_state, &mut request).await {
        Ok(_user) => next.run(request).await,
        Err(response) => response,
    }
}

#[cfg(test)]
mod revocation_tests {
    //! Request-path token-revocation enforcement (H8 + M-revoke-all).
    //!
    //! These exercise `check_revocation` — the exact post-validation step the middleware
    //! runs — with a real `OidcAuthState` + `TokenRevocationManager` and a crafted token.
    //! Driving the full middleware with a *valid* token needs a live JWKS endpoint
    //! (out of scope here, see `auth_regression_test.rs`), so the enforcement logic is
    //! tested at this seam; the wiring (both middlewares call it; the server attaches the
    //! manager to every `OidcAuthState`) is structural.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::{collections::HashMap, sync::Arc};

    use chrono::Utc;
    use fraiseql_core::security::{AuthenticatedUser, OidcConfig, OidcValidator};

    use super::{OidcAuthState, StatusCode, check_revocation};
    use crate::token_revocation::{
        InMemoryRevocationStore, RevocationStore, TokenRevocationManager,
    };

    fn validator() -> Arc<OidcValidator> {
        let config = OidcConfig {
            issuer:               "https://test.fraiseql.dev".to_string(),
            audience:             Some("https://api.test.fraiseql.dev".to_string()),
            required:             true,
            additional_audiences: vec![],
            jwks_cache_ttl_secs:  3600,
            allowed_algorithms:   vec!["RS256".to_string()],
            clock_skew_secs:      60,
            jwks_uri:             None,
            scope_claim:          "scope".to_string(),
            require_jti:          false,
            me:                   None,
        };
        // check_revocation never calls the validator; with_jwks_uri avoids async discovery.
        Arc::new(OidcValidator::with_jwks_uri(config, "https://192.0.2.1/jwks".to_string()))
    }

    fn user(sub: &str) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id:      fraiseql_core::types::UserId::new(sub),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            email:        None,
            display_name: None,
            extra_claims: HashMap::new(),
        }
    }

    fn manager(store: Arc<dyn RevocationStore>) -> Arc<TokenRevocationManager> {
        Arc::new(TokenRevocationManager::new(store, true, false, 3600))
    }

    /// Build a structurally-valid JWT. `insecure_decode` does not verify the signature, so
    /// any signing key produces a token the revocation decode can read.
    fn token(jti: Option<&str>, iat: Option<i64>) -> String {
        let mut claims = serde_json::Map::new();
        if let Some(j) = jti {
            claims.insert("jti".to_owned(), j.into());
        }
        if let Some(i) = iat {
            claims.insert("iat".to_owned(), i.into());
        }
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &serde_json::Value::Object(claims),
            &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn no_manager_is_a_noop_passthrough() {
        let state = OidcAuthState::new(validator());
        let jti = check_revocation(&state, &user("alice"), &token(Some("j1"), Some(1000)))
            .await
            .expect("with no revocation manager the check is a pure jti decode");
        assert_eq!(jti.as_deref(), Some("j1"));
    }

    #[tokio::test]
    async fn revoked_jti_is_rejected_on_the_request_path() {
        let store = Arc::new(InMemoryRevocationStore::new());
        store.revoke("j1", 3600).await.unwrap();
        let state = OidcAuthState::new(validator()).with_revocation(Some(manager(store)));

        // H8: before this wiring the middleware decoded the jti but never consulted the
        // revocation store, so a revoked token was accepted.
        let result = check_revocation(&state, &user("alice"), &token(Some("j1"), Some(1000))).await;
        let response = result.expect_err("a revoked jti must be rejected");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoke_all_epoch_rejects_pre_epoch_token_accepts_post_epoch() {
        let store = Arc::new(InMemoryRevocationStore::new());
        store.revoke_all_for_user("alice", 3600).await.unwrap();
        let now = Utc::now().timestamp();
        let state = OidcAuthState::new(validator())
            .with_revocation(Some(manager(Arc::clone(&store) as Arc<dyn RevocationStore>)));

        // A token issued before the revoke-all epoch is rejected even though its jti was
        // never individually revoked (M-revoke-all).
        assert!(
            check_revocation(&state, &user("alice"), &token(Some("j-old"), Some(now - 100)))
                .await
                .is_err(),
            "a token issued before revoke-all must be rejected"
        );
        // A token issued after the epoch is accepted.
        assert!(
            check_revocation(&state, &user("alice"), &token(Some("j-new"), Some(now + 100)))
                .await
                .is_ok(),
            "a token issued after revoke-all must be accepted"
        );
    }
}
