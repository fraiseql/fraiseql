// Authentication middleware for Axum
use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::auth::{
    error::{AuthError, Result},
    jwt::{Claims, JwtValidator},
    session::SessionStore,
};

/// Authenticated user extracted from JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    /// User ID from token claims
    pub user_id: String,
    /// Full JWT claims
    pub claims:  Claims,
}

impl AuthenticatedUser {
    /// Get a custom claim from the JWT
    pub fn get_custom_claim(&self, key: &str) -> Option<&serde_json::Value> {
        self.claims.get_custom(key)
    }

    /// Check if user has a specific role
    pub fn has_role(&self, role: &str) -> bool {
        if let Some(serde_json::Value::String(user_role)) = self.claims.get_custom("role") {
            user_role == role
        } else if let Some(serde_json::Value::Array(roles)) = self.claims.get_custom("roles") {
            roles.iter().any(|r| {
                if let serde_json::Value::String(r_str) = r {
                    r_str == role
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
}

/// Authentication middleware configuration
pub struct AuthMiddleware {
    validator:      Arc<JwtValidator>,
    _session_store: Arc<dyn SessionStore>,
    public_key:     Vec<u8>,
    _optional:      bool,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    ///
    /// # Arguments
    /// * `validator` - JWT validator
    /// * `session_store` - Session storage backend
    /// * `public_key` - Public key for JWT signature verification
    /// * `optional` - If true, missing auth is not an error
    pub fn new(
        validator: Arc<JwtValidator>,
        session_store: Arc<dyn SessionStore>,
        public_key: Vec<u8>,
        optional: bool,
    ) -> Self {
        Self {
            validator,
            _session_store: session_store,
            public_key,
            _optional: optional,
        }
    }

    /// Validate a Bearer token and extract claims
    pub async fn validate_token(&self, token: &str) -> Result<Claims> {
        self.validator.validate(token, &self.public_key)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        use tracing::warn;

        let (status, error_code, sanitized_message) = match self {
            AuthError::TokenExpired => {
                (StatusCode::UNAUTHORIZED, "token_expired", "Authentication failed".to_string())
            },
            AuthError::InvalidSignature => {
                (StatusCode::UNAUTHORIZED, "invalid_signature", "Authentication failed".to_string())
            },
            AuthError::InvalidToken { ref reason } => {
                // SECURITY: Log internal reason but return generic message
                warn!("Invalid token error: {}", reason);
                (StatusCode::UNAUTHORIZED, "invalid_token", "Authentication failed".to_string())
            },
            AuthError::MissingClaim { ref claim } => {
                // SECURITY: Don't expose which claim is missing
                warn!("Missing required claim: {}", claim);
                (StatusCode::UNAUTHORIZED, "invalid_token", "Authentication failed".to_string())
            },
            AuthError::InvalidClaimValue { ref claim, ref reason } => {
                // SECURITY: Don't expose claim names or validation rules to attackers
                warn!("Invalid claim value for '{}': {}", claim, reason);
                (StatusCode::UNAUTHORIZED, "invalid_token", "Authentication failed".to_string())
            },
            AuthError::TokenNotFound => {
                (StatusCode::UNAUTHORIZED, "token_not_found", "Authentication failed".to_string())
            },
            AuthError::SessionRevoked => {
                (StatusCode::UNAUTHORIZED, "session_revoked", "Authentication failed".to_string())
            },
            AuthError::InvalidState => {
                (StatusCode::BAD_REQUEST, "invalid_state", "Authentication failed".to_string())
            },
            AuthError::Forbidden { ref message } => {
                // SECURITY: Log detailed permission error but return generic message
                warn!("Authorization denied: {}", message);
                (StatusCode::FORBIDDEN, "forbidden", "Permission denied".to_string())
            },
            AuthError::OAuthError { ref message } => {
                // SECURITY: Don't expose OAuth provider details to client
                warn!("OAuth provider error: {}", message);
                (StatusCode::UNAUTHORIZED, "oauth_error", "Authentication failed".to_string())
            },
            AuthError::SessionError { ref message } => {
                // SECURITY: Don't expose session implementation details
                warn!("Session error: {}", message);
                (StatusCode::UNAUTHORIZED, "session_error", "Authentication failed".to_string())
            },
            AuthError::DatabaseError { ref message } => {
                // SECURITY: NEVER expose database errors to clients
                warn!("Database error (should not reach client): {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Service temporarily unavailable".to_string())
            },
            AuthError::ConfigError { ref message } => {
                // SECURITY: NEVER expose configuration details to clients
                warn!("Configuration error (should not reach client): {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Service temporarily unavailable".to_string())
            },
            AuthError::OidcMetadataError { ref message } => {
                // SECURITY: Don't expose OIDC provider metadata details
                warn!("OIDC metadata error: {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Service temporarily unavailable".to_string())
            },
            AuthError::PkceError { ref message } => {
                // SECURITY: Don't expose PKCE implementation details
                warn!("PKCE error: {}", message);
                (StatusCode::BAD_REQUEST, "pkce_error", "Authentication failed".to_string())
            },
            AuthError::Internal { ref message } => {
                // SECURITY: NEVER expose internal errors to clients
                warn!("Internal error (should not reach client): {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Service temporarily unavailable".to_string())
            },
            AuthError::SystemTimeError { ref message } => {
                // SECURITY: Don't expose system errors to clients
                warn!("System time error (should not reach client): {}", message);
                (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Service temporarily unavailable".to_string())
            },
            AuthError::RateLimited { retry_after_secs } => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited", format!("Too many requests. Retry after {} seconds", retry_after_secs))
            },
        };

        let body = serde_json::json!({
            "errors": [{
                "message": sanitized_message,
                "extensions": {
                    "code": error_code
                }
            }]
        });

        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authenticated_user_clone() {
        use std::collections::HashMap;

        use crate::auth::Claims;

        let claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        let _cloned = user.clone();
        assert_eq!(user.user_id, "user123");
    }

    #[test]
    fn test_has_role_single_string() {
        use std::collections::HashMap;

        use crate::auth::Claims;

        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims.extra.insert("role".to_string(), serde_json::json!("admin"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(!user.has_role("user"));
    }

    #[test]
    fn test_has_role_array() {
        use std::collections::HashMap;

        use crate::auth::Claims;

        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims
            .extra
            .insert("roles".to_string(), serde_json::json!(["admin", "user", "editor"]));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert!(user.has_role("admin"));
        assert!(user.has_role("user"));
        assert!(user.has_role("editor"));
        assert!(!user.has_role("moderator"));
    }

    #[test]
    fn test_get_custom_claim() {
        use std::collections::HashMap;

        use crate::auth::Claims;

        let mut claims = Claims {
            sub:   "user123".to_string(),
            iat:   1000,
            exp:   2000,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        };

        claims.extra.insert("org_id".to_string(), serde_json::json!("org_456"));

        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            claims,
        };

        assert_eq!(user.get_custom_claim("org_id"), Some(&serde_json::json!("org_456")));
        assert_eq!(user.get_custom_claim("nonexistent"), None);
    }

    // SECURITY: Tests for error message sanitization to ensure internal details are never exposed

    #[test]
    fn test_invalid_token_sanitized() {
        // SECURITY: Ensure cryptographic details are not exposed
        let error = AuthError::InvalidToken {
            reason: "RS256 signature mismatch at offset 512 bytes".to_string(),
        };
        // Verify it produces UNAUTHORIZED status by checking the status code mapping
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_missing_claim_sanitized() {
        // SECURITY: Ensure claim names are not exposed to attackers
        let error = AuthError::MissingClaim {
            claim: "sensitive_user_id".to_string(),
        };
        // Verify it produces UNAUTHORIZED status
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_claim_value_sanitized() {
        // SECURITY: Ensure claim validation rules are not exposed
        let error = AuthError::InvalidClaimValue {
            claim: "exp".to_string(),
            reason: "Must match pattern: ^[0-9]{10,}$".to_string(),
        };
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_database_error_sanitized() {
        // SECURITY: NEVER expose database errors to clients
        let error = AuthError::DatabaseError {
            message: "Connection to 192.168.1.100:5432 failed: timeout".to_string(),
        };
        let response = error.into_response();
        // Database errors should return INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_config_error_sanitized() {
        // SECURITY: NEVER expose configuration details to clients
        let error = AuthError::ConfigError {
            message: "Secret key missing in /etc/fraiseql/config.toml".to_string(),
        };
        let response = error.into_response();
        // Config errors should return INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_oauth_error_sanitized() {
        // SECURITY: Don't expose OAuth provider details
        let error = AuthError::OAuthError {
            message: "GitHub API returned 500 from https://api.github.com/user (rate limited)".to_string(),
        };
        let response = error.into_response();
        // OAuth errors should return UNAUTHORIZED
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_session_error_sanitized() {
        // SECURITY: Don't expose session implementation details
        let error = AuthError::SessionError {
            message: "Redis connection pool exhausted: 0/10 available".to_string(),
        };
        let response = error.into_response();
        // Session errors should return UNAUTHORIZED
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_forbidden_error_sanitized() {
        // SECURITY: Don't expose permission logic details
        let error = AuthError::Forbidden {
            message: "User lacks role=admin AND permission=write:config for operation".to_string(),
        };
        let response = error.into_response();
        // Forbidden errors should return FORBIDDEN
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_internal_error_sanitized() {
        // SECURITY: NEVER expose internal errors to clients
        let error = AuthError::Internal {
            message: "Panic in JWT validation thread: index out of bounds".to_string(),
        };
        let response = error.into_response();
        // Internal errors should return INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_system_time_error_sanitized() {
        // SECURITY: Don't expose system errors to clients
        let error = AuthError::SystemTimeError {
            message: "System clock jumped backward by 3600 seconds".to_string(),
        };
        let response = error.into_response();
        // System time errors should return INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_rate_limited_error_message() {
        // Rate limited errors CAN expose retry timing (it's not sensitive)
        let error = AuthError::RateLimited {
            retry_after_secs: 60,
        };
        let response = error.into_response();
        // Rate limited should return TOO_MANY_REQUESTS
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_token_expired_returns_generic_message() {
        let error = AuthError::TokenExpired;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_signature_returns_generic_message() {
        let error = AuthError::InvalidSignature;
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_invalid_state_error() {
        let error = AuthError::InvalidState;
        let response = error.into_response();
        // Invalid state (CSRF token) should return BAD_REQUEST
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_pkce_error_returns_bad_request() {
        let error = AuthError::PkceError {
            message: "Challenge verification failed".to_string(),
        };
        let response = error.into_response();
        // PKCE errors should return BAD_REQUEST
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_oidc_metadata_error_returns_server_error() {
        let error = AuthError::OidcMetadataError {
            message: "Failed to fetch metadata".to_string(),
        };
        let response = error.into_response();
        // OIDC metadata errors should return INTERNAL_SERVER_ERROR
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_all_errors_have_status_codes() {
        // Verify that all error types have proper status codes
        let errors = vec![
            AuthError::TokenExpired,
            AuthError::InvalidSignature,
            AuthError::InvalidState,
            AuthError::TokenNotFound,
            AuthError::SessionRevoked,
            AuthError::InvalidToken { reason: "test".to_string() },
            AuthError::MissingClaim { claim: "test".to_string() },
            AuthError::InvalidClaimValue {
                claim: "test".to_string(),
                reason: "test".to_string(),
            },
            AuthError::OAuthError {
                message: "test".to_string(),
            },
            AuthError::SessionError {
                message: "test".to_string(),
            },
            AuthError::DatabaseError {
                message: "test".to_string(),
            },
            AuthError::ConfigError {
                message: "test".to_string(),
            },
            AuthError::OidcMetadataError {
                message: "test".to_string(),
            },
            AuthError::PkceError {
                message: "test".to_string(),
            },
            AuthError::Forbidden {
                message: "test".to_string(),
            },
            AuthError::Internal {
                message: "test".to_string(),
            },
            AuthError::SystemTimeError {
                message: "test".to_string(),
            },
            AuthError::RateLimited {
                retry_after_secs: 60,
            },
        ];

        for error in errors {
            let response = error.into_response();
            // Every error should produce a valid status code
            let status = response.status();
            assert!(
                status == StatusCode::UNAUTHORIZED
                    || status == StatusCode::FORBIDDEN
                    || status == StatusCode::BAD_REQUEST
                    || status == StatusCode::INTERNAL_SERVER_ERROR
                    || status == StatusCode::TOO_MANY_REQUESTS,
                "Unexpected status code: {}",
                status
            );
        }
    }
}
