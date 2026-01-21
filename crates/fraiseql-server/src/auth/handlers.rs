// HTTP handlers for authentication endpoints
use crate::auth::error::{AuthError, Result};
use crate::auth::jwt::Claims;
use crate::auth::middleware::AuthenticatedUser;
use crate::auth::provider::OAuthProvider;
use crate::auth::session::SessionStore;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// AuthState holds the auth configuration and backends
#[derive(Clone)]
pub struct AuthState {
    /// OAuth provider
    pub oauth_provider: Arc<dyn OAuthProvider>,
    /// Session store backend
    pub session_store: Arc<dyn SessionStore>,
    /// In-memory state store for CSRF protection (TODO: replace with persistent store)
    pub state_store: Arc<dashmap::DashMap<String, (String, u64)>>,
}

/// Request body for auth/start endpoint
#[derive(Debug, Deserialize)]
pub struct AuthStartRequest {
    /// Optional provider name (for multi-provider setups)
    pub provider: Option<String>,
}

/// Response for auth/start endpoint
#[derive(Debug, Serialize)]
pub struct AuthStartResponse {
    /// Authorization URL to redirect user to
    pub authorization_url: String,
}

/// Query parameters for auth/callback endpoint
#[derive(Debug, Deserialize)]
pub struct AuthCallbackQuery {
    /// Authorization code from provider
    pub code: String,
    /// State parameter for CSRF protection
    pub state: String,
    /// Error from provider if present
    pub error: Option<String>,
    /// Error description from provider
    pub error_description: Option<String>,
}

/// Response for auth/callback endpoint
#[derive(Debug, Serialize)]
pub struct AuthCallbackResponse {
    /// Access token for API requests
    pub access_token: String,
    /// Optional refresh token
    pub refresh_token: Option<String>,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Time in seconds until token expires
    pub expires_in: u64,
}

/// Request body for auth/refresh endpoint
#[derive(Debug, Deserialize)]
pub struct AuthRefreshRequest {
    /// Refresh token to exchange for new access token
    pub refresh_token: String,
}

/// Response for auth/refresh endpoint
#[derive(Debug, Serialize)]
pub struct AuthRefreshResponse {
    /// New access token
    pub access_token: String,
    /// Token type
    pub token_type: String,
    /// Time in seconds until token expires
    pub expires_in: u64,
}

/// Request body for auth/logout endpoint
#[derive(Debug, Deserialize)]
pub struct AuthLogoutRequest {
    /// Refresh token to revoke
    pub refresh_token: Option<String>,
}

/// Response for successful auth operations
#[derive(Debug, Serialize)]
struct SuccessResponse<T: Serialize> {
    data: T,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponseBody {
    error: String,
    message: String,
}

/// POST /auth/start - Initiate OAuth flow
///
/// Returns an authorization URL that the client should redirect the user to.
pub async fn auth_start(
    State(state): State<AuthState>,
    Json(req): Json<AuthStartRequest>,
) -> Result<Json<AuthStartResponse>> {
    // Generate random state for CSRF protection
    let state_value = generate_state();

    // Store state with expiry (10 minutes)
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 600;

    state.state_store.insert(
        state_value.clone(),
        (req.provider.unwrap_or_else(|| "default".to_string()), expiry),
    );

    // Generate authorization URL
    let authorization_url = state.oauth_provider.authorization_url(&state_value);

    Ok(Json(AuthStartResponse { authorization_url }))
}

/// GET /auth/callback - OAuth provider redirects here
///
/// Exchanges the authorization code for tokens and creates a session.
pub async fn auth_callback(
    State(state): State<AuthState>,
    Query(query): Query<AuthCallbackQuery>,
) -> Result<impl IntoResponse> {
    // Check for provider error
    if let Some(error) = query.error {
        return Err(AuthError::OAuthError {
            message: format!(
                "{}: {}",
                error,
                query.error_description.unwrap_or_default()
            ),
        });
    }

    // Validate state (CSRF protection)
    let (_, entry) = state.state_store.remove(&query.state).ok_or(AuthError::InvalidState)?;
    let (_provider_name, expiry) = entry;

    // Check state expiry
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now > expiry {
        return Err(AuthError::InvalidState);
    }

    // Exchange code for tokens
    let token_response = state.oauth_provider.exchange_code(&query.code).await?;

    // Get user info
    let user_info = state.oauth_provider.user_info(&token_response.access_token).await?;

    // Create session (expires in 7 days)
    let expires_at = now + (7 * 24 * 60 * 60);
    let session_tokens = state
        .session_store
        .create_session(&user_info.id, expires_at)
        .await?;

    let response = AuthCallbackResponse {
        access_token: session_tokens.access_token,
        refresh_token: Some(session_tokens.refresh_token),
        token_type: "Bearer".to_string(),
        expires_in: session_tokens.expires_in,
    };

    // In a real app, would redirect to frontend with tokens in URL fragment
    // For now, return JSON
    Ok(Json(response))
}

/// POST /auth/refresh - Refresh access token
///
/// Uses refresh token to obtain a new access token.
pub async fn auth_refresh(
    State(state): State<AuthState>,
    Json(req): Json<AuthRefreshRequest>,
) -> Result<Json<AuthRefreshResponse>> {
    // Validate refresh token exists in session store
    use crate::auth::session::hash_token;
    let token_hash = hash_token(&req.refresh_token);
    let _session = state.session_store.get_session(&token_hash).await?;

    // In a real implementation, would generate new JWT here
    // For now, return a simple response
    let access_token = format!("new_access_token_{}", uuid::Uuid::new_v4());

    Ok(Json(AuthRefreshResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
    }))
}

/// POST /auth/logout - Logout and revoke session
///
/// Revokes the refresh token, effectively logging out the user.
pub async fn auth_logout(
    State(state): State<AuthState>,
    Json(req): Json<AuthLogoutRequest>,
) -> Result<StatusCode> {
    if let Some(refresh_token) = req.refresh_token {
        use crate::auth::session::hash_token;
        let token_hash = hash_token(&refresh_token);
        state.session_store.revoke_session(&token_hash).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Generate a cryptographically random state for CSRF protection
fn generate_state() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_state() {
        let state1 = generate_state();
        let state2 = generate_state();

        // States should be random and different
        assert_ne!(state1, state2);
        // Should be non-empty
        assert!(!state1.is_empty());
        assert!(!state2.is_empty());
        // Should be 32 characters
        assert_eq!(state1.len(), 32);
        assert_eq!(state2.len(), 32);
    }
}
