//! `POST /auth/v1/session/refresh` and `POST /auth/v1/session/logout` (#1853).
//!
//! Mounted only when `[auth.session] absolute_ttl_secs` is set: a deployment that does not
//! opt in keeps sessions that end with their access token, and gains no route.
//!
//! Both routes sit under `/auth/v1/session/`, so the refresh token's cookie in cookie
//! transport is scoped to that path and sent nowhere else. In cookie transport a request
//! may present its refresh token in that cookie instead of the body, and is answered the
//! way it presented it: a token from the cookie gets its next pair in cookies, and one from
//! the body gets it as `JSON`. Both routes still require a `JSON` body (`{}` when the token
//! is in the cookie), so a form another site submits is refused before it is read.
//!
//! # What logout ends
//!
//! Logout ends the session's refresh tokens. It does **not** end an access token already
//! issued, unless a [`DenyAccessTokens`] is configured (in `fraiseql-server`:
//! `[security.token_revocation]`): the validator checks a token's signature and expiry,
//! and never reads the session table. Without one, an access token lives out its
//! `ttl_secs` after logout.
use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;

use crate::{
    handlers::{MAX_REFRESH_TOKEN_BYTES, validate_auth_input_len},
    session_cookies::{SessionCookies, refresh_token_cookie, with_cookies},
    session_postgres::{PostgresSessionStore, Refresh},
};

/// Denies access tokens before they expire: one session's by `jti` at logout, or every one
/// a user holds when their credentials or their account change.
// Reason: dyn-dispatch required for the server's revocation manager; remove when RTN + Send
// is stable (RFC 3425)
#[async_trait]
pub trait DenyAccessTokens: Send + Sync {
    /// Deny each of `jtis` until it would have expired.
    ///
    /// # Errors
    ///
    /// A message when a token could not be denied: logout then answers 500, because a
    /// caller told it was logged out must have been.
    async fn deny(&self, jtis: &[String]) -> Result<(), String>;

    /// Deny every access token `user_id` holds, issued up to now.
    ///
    /// A token issued in the same second as the denial is denied too, since tokens carry
    /// their issue time in whole seconds: a sign-in in that second is refused once.
    ///
    /// # Errors
    ///
    /// A message when the denial could not be recorded.
    async fn deny_all_for_user(&self, user_id: &str) -> Result<(), String>;
}

/// State for [`session_routes`].
#[derive(Clone)]
pub struct SessionRouteState {
    /// The store the sessions were minted by, built with a refreshable
    /// [`SessionLifetime`](crate::SessionLifetime).
    pub sessions: Arc<PostgresSessionStore>,
    /// Ends the access tokens of a session that logs out. `None`: they live out their
    /// lifetime.
    pub denial:   Option<Arc<dyn DenyAccessTokens>>,
    /// Cookie transport: a refresh token may come in its cookie, and logout clears both
    /// cookies. `None`: the token comes in the body only, and no cookie is read or set.
    pub cookies:  Option<SessionCookies>,
}

/// The body of both routes.
#[derive(Deserialize)]
pub struct SessionTokenRequest {
    /// The session's current refresh token. In cookie transport it may be left out, and
    /// is then read from its cookie.
    #[serde(default)]
    pub refresh_token: Option<String>,
}

/// The refresh token a request presents, and whether it came in the cookie. The body wins
/// over the cookie; the cookie is read only in cookie transport.
fn presented(
    state: &SessionRouteState,
    headers: &HeaderMap,
    req: SessionTokenRequest,
) -> Option<(String, Option<SessionCookies>)> {
    match (req.refresh_token, state.cookies) {
        (Some(token), _) => Some((token, None)),
        (None, Some(cookies)) => refresh_token_cookie(headers).map(|token| (token, Some(cookies))),
        (None, None) => None,
    }
}

/// The session routes.
pub fn session_routes(state: SessionRouteState) -> Router {
    Router::new()
        .route("/auth/v1/session/refresh", post(session_refresh))
        .route("/auth/v1/session/logout", post(session_logout))
        .with_state(state)
}

fn refused() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error":   "invalid_refresh_token",
            "message": "the refresh token is not valid; sign in again"
        })),
    )
        .into_response()
}

/// `POST /auth/v1/session/refresh`: exchange the refresh token for the session's next pair.
///
/// - 200 with the new pair; the token given is used up.
/// - 409 when the token was already exchanged within the last 10 seconds: the client raced itself,
///   and already holds the pair that exchange produced. Nothing is minted or ended.
/// - 401 otherwise. A token exchanged more than 10 seconds ago means two holders, and the whole
///   session is ended.
pub async fn session_refresh(
    State(state): State<SessionRouteState>,
    headers: HeaderMap,
    Json(req): Json<SessionTokenRequest>,
) -> Response {
    let Some((token, from_cookie)) = presented(&state, &headers, req) else {
        return refused();
    };
    if validate_auth_input_len(&token, MAX_REFRESH_TOKEN_BYTES, "refresh_token").is_err() {
        return refused();
    }
    match state.sessions.refresh(&token).await {
        Ok(Refresh::Rotated(pair)) => match from_cookie {
            Some(cookies) => {
                let expires_in = pair.expires_in;
                with_cookies(&cookies, &pair, serde_json::json!({ "expires_in": expires_in }))
            },
            None => (StatusCode::OK, Json(pair)).into_response(),
        },
        Ok(Refresh::AlreadyRotated) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error":   "refresh_token_already_rotated",
                "message": "this refresh token was exchanged moments ago; use the pair it returned"
            })),
        )
            .into_response(),
        Ok(Refresh::Refused) => refused(),
        Err(e) => {
            tracing::error!(error = %e, "session refresh failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "session refresh failed").into_response()
        },
    }
}

/// `POST /auth/v1/session/logout`: end the session the refresh token belongs to.
///
/// 204 whether the token was known or not, so the route says nothing about which tokens
/// exist. 500 only when the session, or a configured denial, could not be carried out: a
/// caller told it was logged out must have been. The access tokens the session issued are
/// denied only when [`SessionRouteState::denial`] is configured. In cookie transport a 204
/// also clears both cookies.
pub async fn session_logout(
    State(state): State<SessionRouteState>,
    headers: HeaderMap,
    Json(req): Json<SessionTokenRequest>,
) -> Response {
    let logged_out = || {
        let mut response = StatusCode::NO_CONTENT.into_response();
        if state.cookies.is_some() {
            for cookie in SessionCookies::clear() {
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
        }
        response
    };
    let Some((token, _)) = presented(&state, &headers, req) else {
        return logged_out();
    };
    if validate_auth_input_len(&token, MAX_REFRESH_TOKEN_BYTES, "refresh_token").is_err() {
        return logged_out();
    }
    match state.sessions.end_session(&token).await {
        Ok(jtis) => match &state.denial {
            Some(denial) => match denial.deny(&jtis).await {
                Ok(()) => logged_out(),
                Err(e) => {
                    tracing::error!(error = %e, "session logout could not deny its access tokens");
                    (StatusCode::INTERNAL_SERVER_ERROR, "session logout failed").into_response()
                },
            },
            None => logged_out(),
        },
        Err(e) => {
            tracing::error!(error = %e, "session logout failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "session logout failed").into_response()
        },
    }
}
