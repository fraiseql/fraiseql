//! `POST /auth/v1/session/refresh` and `POST /auth/v1/session/logout` (#1853).
//!
//! Mounted only when `[auth.session] absolute_ttl_secs` is set: a deployment that does not
//! opt in keeps sessions that end with their access token, and gains no route.
//!
//! Both routes sit under `/auth/v1/session/`, so a refresh token carried in a cookie can be
//! scoped to that path and sent nowhere else.
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
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;

use crate::{
    handlers::{MAX_REFRESH_TOKEN_BYTES, validate_auth_input_len},
    session_postgres::{PostgresSessionStore, Refresh},
};

/// Denies access tokens by `jti` before they expire, for logout.
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
}

/// The body of both routes.
#[derive(Deserialize)]
pub struct SessionTokenRequest {
    /// The session's current refresh token.
    pub refresh_token: String,
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
    Json(req): Json<SessionTokenRequest>,
) -> Response {
    if validate_auth_input_len(&req.refresh_token, MAX_REFRESH_TOKEN_BYTES, "refresh_token")
        .is_err()
    {
        return refused();
    }
    match state.sessions.refresh(&req.refresh_token).await {
        Ok(Refresh::Rotated(pair)) => (StatusCode::OK, Json(pair)).into_response(),
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
/// denied only when [`SessionRouteState::denial`] is configured.
pub async fn session_logout(
    State(state): State<SessionRouteState>,
    Json(req): Json<SessionTokenRequest>,
) -> Response {
    if validate_auth_input_len(&req.refresh_token, MAX_REFRESH_TOKEN_BYTES, "refresh_token")
        .is_err()
    {
        return StatusCode::NO_CONTENT.into_response();
    }
    match state.sessions.end_session(&req.refresh_token).await {
        Ok(jtis) => match &state.denial {
            Some(denial) => match denial.deny(&jtis).await {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(e) => {
                    tracing::error!(error = %e, "session logout could not deny its access tokens");
                    (StatusCode::INTERNAL_SERVER_ERROR, "session logout failed").into_response()
                },
            },
            None => StatusCode::NO_CONTENT.into_response(),
        },
        Err(e) => {
            tracing::error!(error = %e, "session logout failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "session logout failed").into_response()
        },
    }
}
