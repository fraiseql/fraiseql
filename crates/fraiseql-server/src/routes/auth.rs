//! PKCE OAuth2 route handlers: `/auth/start` and `/auth/callback`.
//!
//! These routes implement the OAuth2 Authorization Code flow with PKCE
//! (RFC 7636) for server-side relying-party use.  FraiseQL acts as the
//! OAuth client; the OIDC provider performs the actual authentication.
//!
//! # Flow
//!
//! ```text
//! GET /auth/start?redirect_uri=https://app.example.com/after-login
//!   → 302 → OIDC provider /authorize?...&code_challenge=...&state=...
//!
//! GET /auth/callback?code=<code>&state=<state>
//!   → [verify state, exchange code+verifier for tokens]
//!   → 200 JSON { access_token, id_token, expires_in, token_type }
//!   OR 302 + Set-Cookie (when post_login_redirect_uri is configured)
//! ```
//!
//! Routes are only mounted when `[security.pkce] enabled = true` AND `[auth]`
//! is configured in the compiled schema.  See `server.rs` for the wiring.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};

use crate::auth::{OidcServerClient, PkceStateStore};

/// Shared state injected into both PKCE route handlers.
pub struct AuthPkceState {
    /// In-memory PKCE state store (encrypted when `state_encryption` is on).
    pub pkce_store:              Arc<PkceStateStore>,
    /// Server-side OIDC client for building authorize URLs and exchanging codes.
    pub oidc_client:             Arc<OidcServerClient>,
    /// Shared HTTP client for token-endpoint calls.
    pub http_client:             Arc<reqwest::Client>,
    /// When set, the callback redirects here with the token in a
    /// `Secure; HttpOnly; SameSite=Strict` cookie instead of returning JSON.
    pub post_login_redirect_uri: Option<String>,
}

// ---------------------------------------------------------------------------
// Query parameter structs
// ---------------------------------------------------------------------------

/// Query parameters accepted by `GET /auth/start`.
#[derive(Deserialize)]
pub struct AuthStartQuery {
    /// The URI within the **client application** to redirect to after a
    /// successful login.  This is stored in the PKCE state store and
    /// returned to the caller at callback time via the `redirect_uri` in
    /// the consumed state.
    redirect_uri: String,
}

/// Query parameters sent by the OIDC provider to `GET /auth/callback`.
#[derive(Deserialize)]
pub struct AuthCallbackQuery {
    /// Authorization code to exchange for tokens.
    code:  Option<String>,
    /// State token for CSRF and PKCE state lookup.
    state: Option<String>,
    /// OIDC provider error code (e.g. `"access_denied"`).
    error: Option<String>,
    /// Human-readable error description from the provider.
    error_description: Option<String>,
}

// ---------------------------------------------------------------------------
// Response body (JSON path)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TokenJson {
    access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id_token:     Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in:   Option<u64>,
    token_type:   &'static str,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn auth_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

// ---------------------------------------------------------------------------
// GET /auth/start
// ---------------------------------------------------------------------------

/// Initiate a PKCE authorization code flow.
///
/// Generates a `code_verifier` and `code_challenge`, stores state in the
/// [`PkceStateStore`], then redirects the user-agent to the OIDC provider.
///
/// # Query parameters
///
/// - `redirect_uri` — **required**: the client application's callback URI.
///
/// # Responses
///
/// - `302` — redirect to the OIDC provider's `/authorize` endpoint.
/// - `400` — `redirect_uri` is missing.
/// - `500` — internal error generating state (essentially impossible).
pub async fn auth_start(
    State(state): State<Arc<AuthPkceState>>,
    Query(q): Query<AuthStartQuery>,
) -> Response {
    if q.redirect_uri.is_empty() {
        return auth_error(StatusCode::BAD_REQUEST, "redirect_uri is required");
    }

    let (outbound_token, verifier) = match state.pkce_store.create_state(&q.redirect_uri) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("pkce create_state failed: {e}");
            return auth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "authorization flow could not be started",
            );
        }
    };

    let challenge = PkceStateStore::s256_challenge(&verifier);
    let location  = state
        .oidc_client
        .authorization_url(&outbound_token, &challenge, "S256");

    Redirect::to(&location).into_response()
}

// ---------------------------------------------------------------------------
// GET /auth/callback
// ---------------------------------------------------------------------------

/// Complete the PKCE authorization code flow.
///
/// Validates the `state` parameter, recovers the `code_verifier`, then
/// exchanges the authorization `code` at the OIDC token endpoint.
///
/// # Query parameters
///
/// - `code`  — authorization code from the provider.
/// - `state` — state token (may be encrypted).
///
/// The provider may also call this endpoint with `?error=…` when the user
/// denies access; those are surfaced as `400` responses.
///
/// # Responses
///
/// - `200` JSON `{ access_token, id_token?, expires_in?, token_type }`.
///   Or `302` with `Set-Cookie` when `post_login_redirect_uri` is configured.
/// - `400` — invalid/expired state, missing parameters, or provider error.
/// - `502` — token exchange with the OIDC provider failed.
pub async fn auth_callback(
    State(state): State<Arc<AuthPkceState>>,
    Query(q): Query<AuthCallbackQuery>,
) -> Response {
    // ── Surface OIDC provider errors immediately ──────────────────────────
    if let Some(err) = q.error {
        let desc = q.error_description.unwrap_or_default();
        tracing::warn!(oidc_error = %err, description = %desc, "OIDC provider returned error");
        return auth_error(
            StatusCode::BAD_REQUEST,
            &format!("{err}: {desc}"),
        );
    }

    // ── Validate required parameters ──────────────────────────────────────
    let (code, state_token) = match (q.code, q.state) {
        (Some(c), Some(s)) => (c, s),
        _ => return auth_error(StatusCode::BAD_REQUEST, "missing code or state parameter"),
    };

    // ── Consume PKCE state (atomic remove) ───────────────────────────────
    let pkce = match state.pkce_store.consume_state(&state_token) {
        Ok(s) => s,
        Err(e) => {
            // Both StateNotFound and StateExpired are client errors.
            // Log at debug to avoid spamming warnings from probing attacks.
            tracing::debug!(error = %e, "pkce consume_state failed");
            return auth_error(StatusCode::BAD_REQUEST, &e.to_string());
        }
    };

    // ── Exchange code + verifier at the OIDC provider ────────────────────
    let tokens = match state
        .oidc_client
        .exchange_code(&code, &pkce.verifier, &state.http_client)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("token exchange failed: {e}");
            return auth_error(StatusCode::BAD_GATEWAY, "token exchange with OIDC provider failed");
        }
    };

    // ── Return tokens ─────────────────────────────────────────────────────
    if let Some(redirect_uri) = &state.post_login_redirect_uri {
        // Browser flow: redirect to frontend, set token in HttpOnly cookie.
        // The redirect target is server-configured (no open-redirect risk).
        let max_age = tokens.expires_in.unwrap_or(3600);
        let cookie  = format!(
            "access_token={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}",
            tokens.access_token,
        );
        let mut resp = Redirect::to(redirect_uri).into_response();
        if let Ok(value) = cookie.parse() {
            resp.headers_mut().insert(header::SET_COOKIE, value);
        }
        resp
    } else {
        // API / native app flow: return tokens as JSON.
        Json(TokenJson {
            access_token: tokens.access_token,
            id_token:     tokens.id_token,
            expires_in:   tokens.expires_in,
            token_type:   "Bearer",
        })
        .into_response()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt as _;

    use crate::auth::PkceStateStore;

    fn mock_pkce_store() -> Arc<PkceStateStore> {
        Arc::new(PkceStateStore::new(600, None))
    }

    fn mock_oidc_client() -> Arc<OidcServerClient> {
        Arc::new(OidcServerClient::new(
            "test-client",
            "test-secret",
            "https://api.example.com/auth/callback",
            "https://provider.example.com/authorize",
            "https://provider.example.com/token",
        ))
    }

    fn auth_router() -> Router {
        let auth_state = Arc::new(AuthPkceState {
            pkce_store:              mock_pkce_store(),
            oidc_client:             mock_oidc_client(),
            http_client:             Arc::new(reqwest::Client::new()),
            post_login_redirect_uri: None,
        });
        Router::new()
            .route("/auth/start",    get(auth_start))
            .route("/auth/callback", get(auth_callback))
            .with_state(auth_state)
    }

    #[tokio::test]
    async fn test_auth_start_redirects_with_pkce_params() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/start?redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // axum's Redirect::to() returns 303 See Other; allow any 3xx redirect.
        assert!(
            resp.status().is_redirection(),
            "expected redirect, got {}",
            resp.status()
        );
        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be present");

        assert!(location.contains("response_type=code"),         "missing response_type");
        assert!(location.contains("code_challenge="),            "missing code_challenge");
        assert!(location.contains("code_challenge_method=S256"), "missing challenge method");
        assert!(location.contains("state="),                     "missing state param");
        assert!(location.contains("client_id=test-client"),      "missing client_id");
    }

    #[tokio::test]
    async fn test_auth_start_missing_redirect_uri_returns_400() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/start")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Missing required query param → axum returns 422 (or our guard returns 400).
        // Either is acceptable; what matters is it's not 200 or 302.
        assert!(
            resp.status().is_client_error(),
            "missing redirect_uri must be a client error, got {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn test_auth_callback_unknown_state_returns_400() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?code=abc&state=completely-unknown-state")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Client receives a generic error string, not an internal panic.
        assert!(json["error"].is_string(), "error field must be a string: {json}");
    }

    #[tokio::test]
    async fn test_auth_callback_missing_code_returns_400() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?state=some-state-no-code")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
