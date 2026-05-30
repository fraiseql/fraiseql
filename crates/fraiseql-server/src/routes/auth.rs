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
    Extension, Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{OidcServerClient, PkceStateStore},
    middleware::{AuthUser, SessionJti},
};

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
///
/// As of v2.4.0, the route revokes the caller's currently-authenticated
/// session — identified by the `jti` of the validated bearer token, not by
/// any token submitted in the request body. The `token` field is accepted
/// for wire-shape backwards compatibility but is ignored by the handler.
#[derive(Deserialize)]
pub struct RevokeTokenRequest {
    /// Legacy field; ignored as of v2.4.0. The route revokes the caller's
    /// own session, identified by the `jti` of the bearer token used to
    /// authenticate the request.
    #[serde(default)]
    pub token: Option<String>,
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

/// Revoke the caller's currently-authenticated session.
///
/// The route is mounted behind `oidc_auth_middleware`, so the bearer token has
/// been validated by the time this handler runs.  The `jti` of that validated
/// token is what gets revoked — never an attacker-supplied token from the
/// body.  This closes the FW-21 class anonymous-revocation primitive
/// (issue #358) as well as the authenticated-spoof primitive that the
/// previous `insecure_decode(body.token)` design left open.
///
/// # Responses
///
/// - `200` — token revoked successfully.
/// - `401` — no valid session (enforced by `oidc_auth_middleware` before this handler is called).
/// - `409` — the validated token has no `jti` claim, so there is no per-token identifier the
///   revocation store can record.
pub async fn revoke_token(
    State(state): State<std::sync::Arc<RevocationRouteState>>,
    Extension(auth_user): Extension<AuthUser>,
    Extension(session_jti): Extension<SessionJti>,
    Json(_body): Json<RevokeTokenRequest>,
) -> Response {
    let jti = match session_jti.0 {
        Some(j) if !j.is_empty() => j,
        _ => {
            return auth_error(
                StatusCode::CONFLICT,
                "Bearer token has no jti claim; cannot revoke",
            );
        },
    };

    // TTL = remaining token lifetime, clamped to >= 0. If the token were
    // already expired, the auth middleware would have rejected the request,
    // so a positive TTL is the only path that reaches this point.
    let ttl_secs = {
        let remaining = (auth_user.0.expires_at - chrono::Utc::now()).num_seconds();
        u64::try_from(remaining).unwrap_or(0)
    };

    if let Err(e) = state.revocation_manager.revoke(&jti, ttl_secs).await {
        tracing::error!(error = %e, "Failed to revoke token");
        return auth_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to revoke token");
    }

    let expires_at = Some(auth_user.0.expires_at.to_rfc3339());

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

/// Scope name that grants the bearer permission to revoke other users'
/// sessions via `POST /auth/revoke-all`.
const REVOKE_ALL_ADMIN_SCOPE: &str = "admin";

/// Revoke all tokens for a user.
///
/// The route is mounted behind `oidc_auth_middleware`. The caller may only
/// revoke sessions for their own `sub` unless they hold the
/// `REVOKE_ALL_ADMIN_SCOPE` scope. This closes the FW-21 class
/// anonymous-revocation primitive (issue #358) as well as cross-user
/// revocation via authenticated requests.
///
/// # Responses
///
/// - `200` — tokens revoked.
/// - `400` — `sub` is missing or empty.
/// - `401` — no valid session (enforced by `oidc_auth_middleware`).
/// - `403` — caller's `sub` does not match `body.sub` and caller lacks the admin scope.
pub async fn revoke_all_tokens(
    State(state): State<std::sync::Arc<RevocationRouteState>>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<RevokeAllRequest>,
) -> Response {
    if body.sub.is_empty() {
        return auth_error(StatusCode::BAD_REQUEST, "sub is required");
    }

    let caller_sub = auth_user.0.user_id.as_str();
    if caller_sub != body.sub && !auth_user.0.has_scope(REVOKE_ALL_ADMIN_SCOPE) {
        tracing::warn!(
            caller_sub = %caller_sub,
            target_sub = %body.sub,
            "Cross-user revoke-all rejected: caller is not admin"
        );
        return auth_error(StatusCode::FORBIDDEN, "Cannot revoke another user's sessions");
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
/// The `user_id` alias for `sub` is always present and does **not** need to
/// be listed in `expose_claims`.  Listing `"user_id"` there would silently
/// return nothing because the JWT only carries `sub`, not `user_id`.
///
/// # Responses
///
/// - `200` JSON `{ sub, user_id, expires_at, ...expose_claims }`
/// - `401` when no valid session is present (enforced by `oidc_auth_middleware` before this handler
///   is called).
pub async fn auth_me(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<AuthMeState>>,
    axum::Extension(auth_user): axum::Extension<crate::middleware::AuthUser>,
) -> axum::response::Response {
    use axum::{Json, response::IntoResponse as _};

    let user = &auth_user.0;

    let mut map = serde_json::Map::new();
    map.insert("sub".to_owned(), serde_json::Value::String(user.user_id.0.clone()));
    map.insert("user_id".to_owned(), serde_json::Value::String(user.user_id.0.clone()));
    map.insert("expires_at".to_owned(), serde_json::Value::String(user.expires_at.to_rfc3339()));

    // Always include normalised email/display_name when available (not gated by expose_claims).
    if let Some(ref email) = user.email {
        map.insert("email".to_owned(), serde_json::Value::String(email.clone()));
    }
    if let Some(ref name) = user.display_name {
        map.insert("display_name".to_owned(), serde_json::Value::String(name.clone()));
    }

    for claim_name in &state.expose_claims {
        if let Some(value) = user.extra_claims.get(claim_name) {
            map.insert(claim_name.clone(), value.clone());
        }
    }

    Json(serde_json::Value::Object(map)).into_response()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
