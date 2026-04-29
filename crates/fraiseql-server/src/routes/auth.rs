//! PKCE `OAuth2` route handlers: `/auth/start` and `/auth/callback`.
//!
//! These routes implement the `OAuth2` Authorization Code flow with PKCE
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
    code:              Option<String>,
    /// State token for CSRF and PKCE state lookup.
    state:             Option<String>,
    /// OIDC provider error code (e.g. `"access_denied"`).
    error:             Option<String>,
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
    // Enforce a length cap to prevent memory amplification via the PKCE state store
    // (in-memory or Redis) and to limit encrypted state blob size.
    if q.redirect_uri.len() > 2048 {
        return auth_error(StatusCode::BAD_REQUEST, "redirect_uri exceeds maximum length");
    }

    let (outbound_token, verifier) = match state.pkce_store.create_state(&q.redirect_uri).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("pkce create_state failed: {e}");
            return auth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "authorization flow could not be started",
            );
        },
    };

    let challenge = PkceStateStore::s256_challenge(&verifier);
    let location = state.oidc_client.authorization_url(&outbound_token, &challenge, "S256");

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
/// - `200` JSON `{ access_token, id_token?, expires_in?, token_type }`. Or `302` with `Set-Cookie`
///   when `post_login_redirect_uri` is configured.
/// - `400` — invalid/expired state, missing parameters, or provider error.
/// - `502` — token exchange with the OIDC provider failed.
#[allow(clippy::cognitive_complexity)] // Reason: OAuth callback handler with state validation, token exchange, and redirect logic
pub async fn auth_callback(
    State(state): State<Arc<AuthPkceState>>,
    Query(q): Query<AuthCallbackQuery>,
) -> Response {
    // ── Surface OIDC provider errors immediately ──────────────────────────
    if let Some(err) = q.error {
        let desc = q.error_description.as_deref().unwrap_or("(no description provided)");
        // Log the full provider response for debugging, but return only a
        // fixed allowlisted message to the client to avoid leaking internal
        // provider details (tenant info, stack traces) or enabling injection.
        tracing::warn!(oidc_error = %err, description = %desc, "OIDC provider returned error");
        let client_message = match err.as_str() {
            "access_denied" => "Access was denied",
            "login_required" => "Authentication is required",
            "invalid_request" | "invalid_scope" => "Invalid authorization request",
            "server_error" | "temporarily_unavailable" => "Authorization server error",
            _ => "Authorization failed",
        };
        return auth_error(StatusCode::BAD_REQUEST, client_message);
    }

    // ── Validate required parameters ──────────────────────────────────────
    let (Some(code), Some(state_token)) = (q.code, q.state) else {
        return auth_error(StatusCode::BAD_REQUEST, "missing code or state parameter");
    };

    // ── Consume PKCE state (atomic remove) ───────────────────────────────
    let pkce = match state.pkce_store.consume_state(&state_token).await {
        Ok(s) => s,
        Err(e) => {
            // Both StateNotFound and StateExpired are client errors.
            // Log at debug to avoid spamming warnings from probing attacks.
            tracing::debug!(error = %e, "pkce consume_state failed");
            return auth_error(StatusCode::BAD_REQUEST, &e.to_string());
        },
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
        },
    };

    // ── Return tokens ─────────────────────────────────────────────────────
    if let Some(redirect_uri) = &state.post_login_redirect_uri {
        // Browser flow: redirect to frontend, set token in HttpOnly cookie.
        // The redirect target is server-configured (not from pkce.redirect_uri —
        // IMPORTANT: pkce.redirect_uri MUST NOT be used to construct an HTTP
        // redirect without allowlist validation; its value is caller-supplied
        // and could be attacker-controlled).
        //
        // Cookie notes:
        // - `__Host-` prefix mandates Secure, Path=/, no Domain, blocking subdomain override.
        // - Token value is double-quoted (RFC 6265 quoted-string) to safely embed any printable
        //   ASCII that OAuth servers may include.
        // - Max-Age uses 300s when expires_in is absent — a conservative default that prevents the
        //   cookie outliving a short-lived token by a large margin.
        let max_age = tokens.expires_in.unwrap_or(300);
        // Escape '"' and '\' inside the token value per RFC 6265 quoted-string rules.
        let token_escaped = tokens.access_token.replace('\\', r"\\").replace('"', r#"\""#);
        let cookie = format!(
            r#"__Host-access_token="{token_escaped}"; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"#,
        );
        let mut resp = Redirect::to(redirect_uri).into_response();
        match cookie.parse() {
            Ok(value) => {
                resp.headers_mut().insert(header::SET_COOKIE, value);
            },
            Err(e) => {
                tracing::error!("Failed to parse Set-Cookie header: {e}");
                return auth_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session cookie could not be set",
                );
            },
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
// POST /auth/revoke
// ---------------------------------------------------------------------------

/// Request body for token revocation.
#[derive(Deserialize)]
pub struct RevokeTokenRequest {
    /// The JWT to revoke (we extract `jti` and `exp` from it).
    pub token: String,
}

/// Response body for token revocation.
#[derive(Serialize)]
pub struct RevokeTokenResponse {
    /// Whether the token was successfully revoked.
    pub revoked:    bool,
    /// ISO-8601 timestamp at which the revocation record will expire, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Shared state for revocation routes.
pub struct RevocationRouteState {
    /// Token revocation manager used to record and check revoked JTIs.
    pub revocation_manager: std::sync::Arc<crate::token_revocation::TokenRevocationManager>,
}

/// Revoke a single JWT by its `jti` claim.
///
/// The token is decoded (without verification — we only need the claims) to
/// extract `jti` and `exp`.  The revocation entry TTL is set to the remaining
/// token lifetime so the store auto-cleans.
///
/// # Responses
///
/// - `200` — token revoked successfully.
/// - `400` — token is missing or has no `jti` claim.
pub async fn revoke_token(
    State(state): State<std::sync::Arc<RevocationRouteState>>,
    Json(body): Json<RevokeTokenRequest>,
) -> Response {
    #[derive(serde::Deserialize)]
    struct MinimalClaims {
        jti: Option<String>,
        exp: Option<u64>,
    }

    // Decode without signature verification — we only need the claims for revocation.
    let claims = match jsonwebtoken::dangerous::insecure_decode::<MinimalClaims>(&body.token) {
        Ok(data) => data.claims,
        Err(e) => {
            return auth_error(StatusCode::BAD_REQUEST, &format!("Invalid token: {e}"));
        },
    };

    let jti = match claims.jti {
        Some(j) if !j.is_empty() => j,
        _ => {
            return auth_error(StatusCode::BAD_REQUEST, "Token has no jti claim");
        },
    };

    // TTL = remaining token lifetime, or 24h if no exp.
    let ttl_secs = claims
        .exp
        .and_then(|exp| {
            let now = chrono::Utc::now().timestamp().cast_unsigned();
            exp.checked_sub(now)
        })
        .unwrap_or(86400);

    if let Err(e) = state.revocation_manager.revoke(&jti, ttl_secs).await {
        tracing::error!(error = %e, "Failed to revoke token");
        return auth_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to revoke token");
    }

    let expires_at = claims.exp.map(|exp| {
        chrono::DateTime::from_timestamp(exp.cast_signed(), 0)
            .map_or_else(|| exp.to_string(), |dt| dt.to_rfc3339())
    });

    Json(RevokeTokenResponse {
        revoked: true,
        expires_at,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// POST /auth/revoke-all
// ---------------------------------------------------------------------------

/// Request body for revoking all tokens for a user.
#[derive(Deserialize)]
pub struct RevokeAllRequest {
    /// User subject (from JWT `sub` claim).
    pub sub: String,
}

/// Response body for bulk revocation.
#[derive(Serialize)]
pub struct RevokeAllResponse {
    /// Number of token revocation records that were created.
    pub revoked_count: u64,
}

/// Revoke all tokens for a user.
///
/// # Responses
///
/// - `200` — tokens revoked.
/// - `400` — `sub` is missing or empty.
pub async fn revoke_all_tokens(
    State(state): State<std::sync::Arc<RevocationRouteState>>,
    Json(body): Json<RevokeAllRequest>,
) -> Response {
    if body.sub.is_empty() {
        return auth_error(StatusCode::BAD_REQUEST, "sub is required");
    }

    match state.revocation_manager.revoke_all_for_user(&body.sub).await {
        Ok(count) => Json(RevokeAllResponse {
            revoked_count: count,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, sub = %body.sub, "Failed to revoke tokens for user");
            auth_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to revoke tokens")
        },
    }
}

// ---------------------------------------------------------------------------
// GET /auth/me
// ---------------------------------------------------------------------------

/// State for the [`auth_me`] handler, extracted from `[auth.me]` config.
pub struct AuthMeState {
    /// Raw JWT claim names that the handler should include in the response,
    /// beyond the always-present `sub`, `user_id`, and `expires_at`.
    pub expose_claims: Vec<String>,

    /// Optional enrichment config + DB pool for augmenting the response with
    /// application-specific fields fetched from the database.
    pub(crate) enrichment: Option<AuthMeEnrichmentState>,
}

/// Runtime state for claims enrichment.
pub(crate) struct AuthMeEnrichmentState {
    /// The enrichment configuration (query, map, cache TTL).
    pub(crate) config: fraiseql_core::security::oidc::MeEnrichmentConfig,
    /// PostgreSQL pool for executing the enrichment query.
    pub(crate) pool: sqlx::PgPool,
    /// Per-`sub` cache for enrichment results.
    pub(crate) cache: std::sync::Arc<super::enrichment::EnrichmentCache>,
}

/// Return the current session's identity as JSON.
///
/// Reads the [`crate::middleware::AuthUser`] request extension populated by
/// `oidc_auth_middleware` and reflects a configurable subset of the validated
/// JWT claims back to the caller.
///
/// The response always contains:
/// - `sub` — the standard JWT subject (user ID).
/// - `user_id` — hardcoded alias for `sub`; more ergonomic for frontend code.
/// - `expires_at` — ISO-8601 timestamp when the session expires.
///
/// Additional fields are included only when (a) the claim name appears in the
/// `expose_claims` allowlist **and** (b) the claim is present in the token.
/// Claims in the allowlist but absent from the token are silently omitted —
/// the response is never padded with `null` values.
///
/// When enrichment is configured, a SQL query is executed against the database
/// to augment the response with application-specific fields (roles, permissions,
/// plans). If the enrichment query fails or returns no rows, the response still
/// includes the JWT claims (graceful degradation).
///
/// # Responses
///
/// - `200` JSON `{ sub, user_id, expires_at, ...expose_claims, ...enrichment }`
/// - `401` when no valid session is present (enforced by `oidc_auth_middleware` before this handler
///   is called).
pub async fn auth_me(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<AuthMeState>>,
    axum::Extension(auth_user): axum::Extension<crate::middleware::AuthUser>,
) -> axum::response::Response {
    use axum::{Json, response::IntoResponse as _};

    let user = &auth_user.0;

    let mut map = serde_json::Map::new();
    map.insert("sub".to_owned(), serde_json::Value::String(user.user_id.clone()));
    map.insert("user_id".to_owned(), serde_json::Value::String(user.user_id.clone()));
    map.insert("expires_at".to_owned(), serde_json::Value::String(user.expires_at.to_rfc3339()));

    for claim_name in &state.expose_claims {
        if let Some(value) = user.extra_claims.get(claim_name) {
            map.insert(claim_name.clone(), value.clone());
        }
    }

    // Run enrichment if configured
    if let Some(ref enrichment) = state.enrichment {
        // Build claims map for param binding: always include `sub`, plus expose_claims
        let mut claims = user.extra_claims.clone();
        claims.insert("sub".to_owned(), serde_json::Value::String(user.user_id.clone()));

        // Compute token remaining seconds for cache TTL
        let now = chrono::Utc::now();
        let token_remaining_secs = user
            .expires_at
            .signed_duration_since(now)
            .num_seconds()
            .max(0).cast_unsigned();

        match super::enrichment::run_enrichment(
            &enrichment.pool,
            &enrichment.config,
            &claims,
            Some(&enrichment.cache),
            &user.user_id,
            token_remaining_secs,
        )
        .await
        {
            Ok(Some(enriched)) => {
                for (key, value) in enriched {
                    map.insert(key, value);
                }
            }
            Ok(None) => {
                // No rows — graceful, no extra fields
            }
            Err(e) => {
                tracing::error!(
                    sub = %user.user_id,
                    error = %e,
                    "Claims enrichment failed"
                );
                map.insert(
                    "enrichment_error".to_owned(),
                    serde_json::Value::String("unavailable".to_owned()),
                );
            }
        }
    }

    Json(serde_json::Value::Object(map)).into_response()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use axum::{Extension, Router, body::Body, http::Request, routing::get};
    use chrono::Utc;
    use tower::ServiceExt as _;

    use super::*;
    use crate::{auth::PkceStateStore, middleware::AuthUser};

    fn mock_pkce_store() -> Arc<PkceStateStore> {
        Arc::new(PkceStateStore::new(600, None))
    }

    // -------------------------------------------------------------------------
    // auth_me tests
    // -------------------------------------------------------------------------

    fn make_auth_user(
        user_id: &str,
        extra: std::collections::HashMap<String, serde_json::Value>,
    ) -> AuthUser {
        AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:      user_id.to_owned(),
            scopes:       vec![],
            expires_at:   Utc::now() + chrono::Duration::hours(1),
            extra_claims: extra,
        })
    }

    fn make_me_state(expose_claims: Vec<&str>) -> Arc<AuthMeState> {
        Arc::new(AuthMeState {
            expose_claims: expose_claims.into_iter().map(str::to_owned).collect(),
            enrichment: None,
        })
    }

    #[tokio::test]
    async fn test_auth_me_always_returns_sub_user_id_expires_at() {
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-123", std::collections::HashMap::new())))
            .with_state(make_me_state(vec![]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["sub"], "user-123");
        assert_eq!(json["user_id"], "user-123");
        assert!(json["expires_at"].is_string(), "expires_at must be present");
    }

    #[tokio::test]
    async fn test_auth_me_expose_claims_filters_correctly() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("email".to_owned(), serde_json::json!("alice@example.com"));
        extra.insert("https://myapp.com/role".to_owned(), serde_json::json!("admin"));

        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("alice", extra)))
            .with_state(make_me_state(vec!["email"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["email"], "alice@example.com", "listed claim must appear");
        assert!(json.get("https://myapp.com/role").is_none(), "unlisted claim must be absent");
    }

    #[tokio::test]
    async fn test_auth_me_claim_absent_from_token_silently_omitted() {
        // expose_claims lists "tenant_id" but the token doesn't have it.
        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-x", std::collections::HashMap::new())))
            .with_state(make_me_state(vec!["tenant_id"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("tenant_id").is_none(), "absent claim must not be null-padded");
        // Fixed fields must still be present.
        assert_eq!(json["sub"], "user-x");
    }

    #[tokio::test]
    async fn test_auth_me_namespaced_claim_in_expose_claims() {
        let mut extra = std::collections::HashMap::new();
        extra.insert("https://myapp.com/role".to_owned(), serde_json::json!("editor"));

        let app = Router::new()
            .route("/auth/me", get(auth_me))
            .layer(Extension(make_auth_user("user-y", extra)))
            .with_state(make_me_state(vec!["https://myapp.com/role"]));

        let req = Request::builder().uri("/auth/me").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["https://myapp.com/role"], "editor");
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
            .route("/auth/start", get(auth_start))
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
        assert!(resp.status().is_redirection(), "expected redirect, got {}", resp.status());
        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be present");

        assert!(location.contains("response_type=code"), "missing response_type");
        assert!(location.contains("code_challenge="), "missing code_challenge");
        assert!(location.contains("code_challenge_method=S256"), "missing challenge method");
        assert!(location.contains("state="), "missing state param");
        assert!(location.contains("client_id=test-client"), "missing client_id");
    }

    #[tokio::test]
    async fn test_auth_start_missing_redirect_uri_returns_400() {
        let app = auth_router();
        let req = Request::builder().uri("/auth/start").body(Body::empty()).unwrap();
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

    #[tokio::test]
    async fn test_auth_start_oversized_redirect_uri_returns_400() {
        let app = auth_router();
        let long_uri = "https://example.com/".to_string() + &"a".repeat(2100);
        let encoded = urlencoding::encode(&long_uri);
        let req = Request::builder()
            .uri(format!("/auth/start?redirect_uri={encoded}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"].as_str().unwrap_or("").contains("maximum length"),
            "error must mention length: {json}"
        );
    }

    #[tokio::test]
    async fn test_auth_callback_oidc_error_returns_mapped_message() {
        let app = auth_router();
        // access_denied should map to a fixed message, not reflect provider strings
        let req = Request::builder()
            .uri("/auth/callback?error=access_denied&error_description=internal+tenant+info")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let error_msg = json["error"].as_str().unwrap_or("");
        // Must not contain the raw provider description
        assert!(
            !error_msg.contains("internal tenant info"),
            "provider description must not be reflected to client: {error_msg}"
        );
        assert_eq!(error_msg, "Access was denied");
    }

    #[tokio::test]
    async fn test_auth_callback_unknown_oidc_error_returns_generic_message() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?error=unknown_vendor_error&error_description=secret+details")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"].as_str().unwrap_or(""), "Authorization failed");
    }

    #[tokio::test]
    async fn test_auth_callback_oidc_error_no_description_uses_fallback() {
        let app = auth_router();
        let req = Request::builder()
            .uri("/auth/callback?error=access_denied")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        // The tracing log (not the HTTP response) includes the desc; the HTTP
        // response is the sanitised allowlist message. We verify the handler does
        // not panic and returns the mapped client message.
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"].as_str().unwrap_or(""), "Access was denied");
    }

    /// Full HTTP-level PKCE round-trip: `/auth/start` → extract state → `/auth/callback`.
    ///
    /// Verifies that the state token embedded in the `/auth/start` redirect can be
    /// submitted to `/auth/callback`, proving the PKCE store correctly survives the
    /// round-trip through the HTTP layer (including encryption when enabled).
    ///
    /// The callback will fail at token exchange (no real OIDC provider) and return 502,
    /// but NOT 400 — a 400 would indicate the state was not found in the store.
    #[tokio::test]
    async fn test_auth_start_to_callback_state_roundtrip_with_encryption() {
        use crate::auth::{EncryptionAlgorithm, StateEncryptionService};

        let enc = Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        ));
        let pkce_store = Arc::new(PkceStateStore::new(600, Some(enc)));

        let auth_state = Arc::new(AuthPkceState {
            pkce_store,
            oidc_client: mock_oidc_client(),
            http_client: Arc::new(reqwest::Client::new()),
            post_login_redirect_uri: None,
        });

        let app = Router::new()
            .route("/auth/start", get(auth_start))
            .route("/auth/callback", get(auth_callback))
            .with_state(auth_state);

        // Step 1 — /auth/start: receive redirect containing the encrypted state token.
        let req = Request::builder()
            .uri("/auth/start?redirect_uri=https%3A%2F%2Fapp.example.com%2Fcb")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();

        assert!(
            resp.status().is_redirection(),
            "expected redirect from /auth/start, got {}",
            resp.status(),
        );

        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .expect("Location header must be set")
            .to_string();

        // Extract the state= token from the redirect URL using proper URL parsing to
        // avoid false matches when "state=" appears elsewhere in the URL (e.g. in path
        // or other parameters).
        let parsed_location =
            reqwest::Url::parse(&location).expect("Location header must be a valid URL");
        let state_token = parsed_location
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .expect("state= must appear in the redirect Location URL");

        assert!(!state_token.is_empty(), "extracted state token must not be empty");

        // Step 2 — /auth/callback: submit the real state token from step 1.
        // Expected result: 502 Bad Gateway (token exchange fails — no real OIDC provider).
        // A 400 would mean the PKCE state was not found, which would be a regression.
        let callback_uri = format!("/auth/callback?code=test_code&state={state_token}");
        let req2 = Request::builder().uri(&callback_uri).body(Body::empty()).unwrap();
        let resp2 = app.clone().oneshot(req2).await.unwrap();

        assert_ne!(
            resp2.status(),
            StatusCode::BAD_REQUEST,
            "state from /auth/start must be accepted by /auth/callback; \
             400 means the PKCE state was not found or decryption failed",
        );
        assert_eq!(
            resp2.status(),
            StatusCode::BAD_GATEWAY,
            "token exchange should fail 502 (no real OIDC provider); got {}",
            resp2.status(),
        );
    }
}
