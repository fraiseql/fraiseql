//! Anonymous session management.
//!
//! `POST /auth/v1/signup` with no credentials creates an anonymous user with a
//! temporary identity. Anonymous sessions can be upgraded to full sessions when
//! the user links a social login or email OTP.
//!
//! Configurable TTL (default: 7 days). Anonymous users are subject to separate
//! rate limits.

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    account_linking::UserStore,
    session::SessionStore,
};

/// Default anonymous session TTL in seconds (7 days).
const DEFAULT_ANON_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Request body for `POST /auth/v1/signup`.
///
/// All fields are optional — an empty body (or `{}`) creates an anonymous user.
#[derive(Debug, Default, Deserialize)]
pub struct SignupRequest {
    /// Optional display name for the anonymous user.
    pub name: Option<String>,
}

/// Response body for `POST /auth/v1/signup`.
#[derive(Debug, Serialize)]
pub struct SignupResponse {
    /// The temporary user ID (UUID).
    pub user_id:       String,
    /// Access token for API requests.
    pub access_token:  String,
    /// Refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Token type (always "Bearer").
    pub token_type:    String,
    /// Seconds until the session expires.
    pub expires_in:    u64,
    /// Whether this is an anonymous session.
    pub is_anonymous:  bool,
}

/// Shared state for the anonymous auth endpoint.
#[derive(Clone)]
pub struct AnonAuthState {
    /// Session backend for creating sessions.
    pub session_store: Arc<dyn SessionStore>,
    /// Optional user store for creating a local user record.
    pub user_store:    Option<Arc<dyn UserStore>>,
    /// Anonymous session TTL in seconds.
    pub ttl_secs:      u64,
}

impl AnonAuthState {
    /// Create a new anonymous auth state with default TTL (7 days).
    pub fn new(session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_store,
            user_store: None,
            ttl_secs: DEFAULT_ANON_TTL_SECS,
        }
    }

    /// Set the user store for creating local user records.
    pub fn with_user_store(mut self, user_store: Arc<dyn UserStore>) -> Self {
        self.user_store = Some(user_store);
        self
    }

    /// Set custom TTL for anonymous sessions.
    pub const fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = ttl_secs;
        self
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// POST /auth/v1/signup
// ---------------------------------------------------------------------------

/// Create an anonymous session.
///
/// Generates a temporary user identity (UUID) and issues a session with the
/// configured TTL. When a [`UserStore`] is configured, a local user record is
/// created so the anonymous identity can later be upgraded by linking a social
/// login or email.
///
/// # Responses
///
/// - `200` JSON `{ user_id, access_token, refresh_token?, token_type, expires_in, is_anonymous }`
/// - `500` if session creation fails.
pub async fn signup_anonymous(
    State(state): State<Arc<AnonAuthState>>,
    Json(req): Json<SignupRequest>,
) -> Response {
    let anon_id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();

    // Optionally create a local user record
    let user_id = if let Some(user_store) = &state.user_store {
        let user_info = crate::provider::UserInfo {
            id:         anon_id.clone(),
            email:      format!("anon+{anon_id}@anonymous.local"),
            name:       req.name.clone(),
            picture:    None,
            raw_claims: serde_json::json!({ "anonymous": true }),
        };
        match user_store.find_or_create_user("anonymous", &user_info).await {
            Ok(user) => user.id,
            Err(e) => {
                tracing::error!(error = %e, "user store creation failed for anonymous user");
                return json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "anonymous session could not be created",
                );
            },
        }
    } else {
        anon_id
    };

    let session_expiry = now + state.ttl_secs;
    match state.session_store.create_session(&user_id, session_expiry).await {
        Ok(tokens) => Json(SignupResponse {
            user_id,
            access_token:  tokens.access_token,
            refresh_token: Some(tokens.refresh_token),
            token_type:    "Bearer".to_string(),
            expires_in:    tokens.expires_in,
            is_anonymous:  true,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "session creation failed for anonymous user");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "anonymous session could not be created",
            )
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
