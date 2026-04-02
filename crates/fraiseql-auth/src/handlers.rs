//! HTTP handlers for the built-in authentication endpoints (`/auth/start`,
//! `/auth/callback`, `/auth/refresh`, `/auth/logout`).
use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
    provider::OAuthProvider,
    rate_limiting::RateLimiters,
    session::SessionStore,
    state_store::StateStore,
};

/// AuthState holds the auth configuration and backends
#[derive(Clone)]
pub struct AuthState {
    /// OAuth provider
    pub oauth_provider: Arc<dyn OAuthProvider>,
    /// Session store backend
    pub session_store: Arc<dyn SessionStore>,
    /// CSRF state store backend (in-memory for single-instance, Redis for distributed)
    pub state_store: Arc<dyn StateStore>,
    /// Rate limiters for auth endpoints (per-IP based)
    pub rate_limiters: Arc<RateLimiters>,
}

/// Request body for auth/start endpoint
#[derive(Debug, Deserialize)]
pub struct AuthStartRequest {
    /// Optional provider name (for multi-provider setups)
    pub provider: Option<String>,
}

/// Response body for the `POST /auth/start` endpoint.
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

/// Response body for the `GET /auth/callback` endpoint.
///
/// Returned after a successful OAuth authorization-code exchange.
/// In a production browser-facing flow, the server would instead redirect
/// the user agent to the frontend application with tokens in a URL fragment;
/// this JSON form is suitable for API clients and testing.
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

/// Response body for the `POST /auth/refresh` endpoint.
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

/// POST /auth/start - Initiate OAuth flow
///
/// Returns an authorization URL that the client should redirect the user to.
///
/// # Rate Limiting
///
/// This endpoint is rate-limited per IP address to prevent brute-force attacks.
/// The limit is configurable via FRAISEQL_AUTH_START_MAX_REQUESTS and
/// FRAISEQL_AUTH_START_WINDOW_SECS environment variables.
///
/// # Errors
///
/// Returns `AuthError::RateLimited` if the per-IP rate limit is exceeded.
/// Returns `AuthError::SystemTimeError` if the system clock is unavailable.
/// Returns `AuthError` if the state store write fails.
pub async fn auth_start(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<AuthStartRequest>,
) -> Result<Json<AuthStartResponse>> {
    // SECURITY: Check rate limiting for auth/start endpoint (per IP)
    let client_ip = addr.ip().to_string();
    if state.rate_limiters.auth_start.check(&client_ip).is_err() {
        return Err(AuthError::RateLimited {
            retry_after_secs: state.rate_limiters.auth_start.clone_config().window_secs,
        });
    }

    // Generate random state for CSRF protection using cryptographically secure RNG
    let state_value = generate_secure_state();

    // Get current time with explicit error handling (not unwrap_or_default)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AuthError::SystemTimeError {
            message: "Failed to get current system time".to_string(),
        })?
        .as_secs();

    // Store state with expiry (10 minutes)
    let expiry = now + 600;

    // SECURITY: Store state using configurable backend (in-memory or distributed)
    let provider = req.provider.unwrap_or_else(|| "default".to_string());
    state.state_store.store(state_value.clone(), provider, expiry).await?;

    // Generate authorization URL
    let authorization_url = state.oauth_provider.authorization_url(&state_value);

    Ok(Json(AuthStartResponse { authorization_url }))
}

/// GET /auth/callback - OAuth provider redirects here
///
/// Exchanges the authorization code for tokens and creates a session.
///
/// # Rate Limiting
///
/// This endpoint is rate-limited per IP address to prevent brute-force attacks.
/// The limit is configurable via FRAISEQL_AUTH_CALLBACK_MAX_REQUESTS and
/// FRAISEQL_AUTH_CALLBACK_WINDOW_SECS environment variables.
///
/// # Errors
///
/// Returns `AuthError::RateLimited` if the per-IP rate limit is exceeded.
/// Returns `AuthError::OAuthError` if the provider returned an error.
/// Returns `AuthError::InvalidState` if the CSRF state token is expired or invalid.
/// Returns `AuthError` if the token exchange or session creation fails.
pub async fn auth_callback(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<AuthCallbackQuery>,
) -> Result<impl IntoResponse> {
    // SECURITY: Check rate limiting for auth/callback endpoint (per IP)
    let client_ip = addr.ip().to_string();
    if state.rate_limiters.auth_callback.check(&client_ip).is_err() {
        return Err(AuthError::RateLimited {
            retry_after_secs: state.rate_limiters.auth_callback.clone_config().window_secs,
        });
    }

    // Check for provider error
    if let Some(error) = query.error {
        let audit_logger = get_audit_logger();
        audit_logger.log_failure(
            AuditEventType::OauthCallback,
            SecretType::AuthorizationCode,
            None,
            "exchange",
            &error,
        );
        return Err(AuthError::OAuthError {
            message: format!("{}: {}", error, query.error_description.unwrap_or_default()),
        });
    }

    // SECURITY: Validate state using configurable backend (distributed-safe)
    let (_provider_name, expiry) = state.state_store.retrieve(&query.state).await?;

    // Check state expiry with explicit error handling
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AuthError::SystemTimeError {
            message: "Failed to get current system time".to_string(),
        })?
        .as_secs();

    if now > expiry {
        let audit_logger = get_audit_logger();
        audit_logger.log_failure(
            AuditEventType::CsrfStateValidated,
            SecretType::StateToken,
            None,
            "validate",
            "State token expired",
        );
        return Err(AuthError::InvalidState);
    }

    // Audit log: CSRF state validation success
    let audit_logger = get_audit_logger();
    audit_logger.log_success(
        AuditEventType::CsrfStateValidated,
        SecretType::StateToken,
        None,
        "validate",
    );

    // Exchange code for tokens
    let token_response = state.oauth_provider.exchange_code(&query.code).await?;

    // Audit log: Token exchange success
    let audit_logger = get_audit_logger();
    audit_logger.log_success(
        AuditEventType::OauthCallback,
        SecretType::AuthorizationCode,
        None,
        "exchange",
    );

    // Get user info
    let user_info = state.oauth_provider.user_info(&token_response.access_token).await?;

    // Create session (expires in 7 days)
    let expires_at = now + (7 * 24 * 60 * 60);
    let session_tokens = state.session_store.create_session(&user_info.id, expires_at).await?;

    // Audit log: Session token created
    let audit_logger = get_audit_logger();
    audit_logger.log_success(
        AuditEventType::SessionTokenCreated,
        SecretType::SessionToken,
        Some(user_info.id.clone()),
        "create",
    );

    // Audit log: Auth success
    let audit_logger = get_audit_logger();
    audit_logger.log_success(
        AuditEventType::AuthSuccess,
        SecretType::SessionToken,
        Some(user_info.id),
        "oauth_flow",
    );

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
///
/// # Rate Limiting
///
/// This endpoint is rate-limited per user ID to prevent token refresh attacks.
/// The limit is configurable via FRAISEQL_AUTH_REFRESH_MAX_REQUESTS and
/// FRAISEQL_AUTH_REFRESH_WINDOW_SECS environment variables.
///
/// # Errors
///
/// Returns `AuthError::TokenExpired` if the session has expired.
/// Returns `AuthError::RateLimited` if the per-user rate limit is exceeded.
/// Returns `AuthError::Internal` if JWT signing is not yet configured.
pub async fn auth_refresh(
    State(state): State<AuthState>,
    Json(req): Json<AuthRefreshRequest>,
) -> Result<Json<AuthRefreshResponse>> {
    // Validate refresh token exists in session store
    use crate::session::hash_token;
    let token_hash = hash_token(&req.refresh_token);
    let session = state.session_store.get_session(&token_hash).await?;

    // SECURITY: Reject expired sessions before any further processing.
    // Without this check, a stolen refresh token from an expired session
    // could be used indefinitely to mint new access tokens.
    if session.is_expired() {
        let audit_logger = get_audit_logger();
        audit_logger.log_failure(
            AuditEventType::JwtRefresh,
            SecretType::RefreshToken,
            Some(session.user_id),
            "refresh",
            "Session expired",
        );
        return Err(AuthError::TokenExpired);
    }

    // SECURITY: Check rate limiting for auth/refresh endpoint (per user)
    if state.rate_limiters.auth_refresh.check(&session.user_id).is_err() {
        return Err(AuthError::RateLimited {
            retry_after_secs: state.rate_limiters.auth_refresh.clone_config().window_secs,
        });
    }

    // Audit log: Refresh token validation success
    let audit_logger = get_audit_logger();
    audit_logger.log_success(
        AuditEventType::SessionTokenValidation,
        SecretType::RefreshToken,
        Some(session.user_id),
        "validate",
    );

    // JWT signing requires an RSA/EC private key, which is not yet wired
    // into the auth state. Return an explicit error rather than a fake token.
    Err(AuthError::Internal {
        message: "JWT signing not yet implemented — configure an OIDC provider for token issuance"
            .to_string(),
    })
}

/// POST /auth/logout - Logout and revoke session
///
/// Revokes the refresh token, effectively logging out the user.
///
/// # Rate Limiting
///
/// This endpoint is rate-limited per user ID to prevent logout token exhaustion attacks.
/// The limit is configurable via FRAISEQL_AUTH_LOGOUT_MAX_REQUESTS and
/// FRAISEQL_AUTH_LOGOUT_WINDOW_SECS environment variables.
///
/// # Errors
///
/// Returns `AuthError::RateLimited` if the per-user rate limit is exceeded.
/// Returns `AuthError` if the session lookup or deletion fails.
pub async fn auth_logout(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<AuthLogoutRequest>,
) -> Result<StatusCode> {
    let client_ip = addr.ip().to_string();

    if let Some(refresh_token) = req.refresh_token {
        use crate::session::hash_token;
        let token_hash = hash_token(&refresh_token);

        // Get session to extract user ID for per-user rate limiting
        let session = state.session_store.get_session(&token_hash).await?;

        // SECURITY: Check rate limiting for auth/logout endpoint (per user)
        if state.rate_limiters.auth_logout.check(&session.user_id).is_err() {
            return Err(AuthError::RateLimited {
                retry_after_secs: state.rate_limiters.auth_logout.clone_config().window_secs,
            });
        }

        state.session_store.revoke_session(&token_hash).await?;

        // Audit log: Session revoked
        let audit_logger = get_audit_logger();
        audit_logger.log_success(
            AuditEventType::SessionTokenRevoked,
            SecretType::RefreshToken,
            Some(session.user_id),
            "revoke",
        );
    } else {
        // No refresh token - use IP-based rate limiting as fallback
        if state.rate_limiters.auth_logout.check(&client_ip).is_err() {
            return Err(AuthError::RateLimited {
                retry_after_secs: state.rate_limiters.auth_logout.clone_config().window_secs,
            });
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Generate a cryptographically random state for CSRF protection
/// Uses OsRng for cryptographically secure randomness
pub fn generate_secure_state() -> String {
    use rand::RngCore;

    // Generate 32 random bytes for 256 bits of entropy
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);

    // Encode as hex string for safe transmission in URLs/headers
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test module wildcard import; brings all items into test scope
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    #[test]
    fn test_generate_secure_state() {
        let state1 = generate_secure_state();
        let state2 = generate_secure_state();

        // States should be random and different
        assert_ne!(state1, state2);
        // Should be non-empty
        assert!(!state1.is_empty());
        assert!(!state2.is_empty());
        // Should be 64 hex characters (32 bytes encoded)
        assert_eq!(state1.len(), 64);
        assert_eq!(state2.len(), 64);
        // Should be valid hex
        hex::decode(&state1).unwrap_or_else(|e| panic!("state1 should be valid hex: {e}"));
        hex::decode(&state2).unwrap_or_else(|e| panic!("state2 should be valid hex: {e}"));
    }
}
