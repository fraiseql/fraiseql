//! Authentication middleware for Axum request handlers.
use std::sync::Arc;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{AuthError, Result},
    jwt::{Claims, JwtValidator},
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
    #[must_use]
    pub fn get_custom_claim(&self, key: &str) -> Option<&serde_json::Value> {
        self.claims.get_custom(key)
    }

    /// Check if user has a specific role
    #[must_use]
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
    validator:  Arc<JwtValidator>,
    public_key: Vec<u8>,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    ///
    /// # Arguments
    /// * `validator` - JWT validator
    /// * `public_key` - Public key for JWT signature verification
    ///
    /// Note: this type validates a presented Bearer token. The previous
    /// `session_store` and `optional` parameters were never consulted (no
    /// session-revocation check, no optional-auth handling), so they were removed
    /// rather than continue to advertise behavior that did not exist
    /// (L-authmw-ignores). Optional/anonymous handling belongs at the request layer;
    /// session-revocation checks belong on the refresh-token path.
    #[must_use]
    pub const fn new(validator: Arc<JwtValidator>, public_key: Vec<u8>) -> Self {
        Self {
            validator,
            public_key,
        }
    }

    /// Validate a Bearer token and extract claims.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::InvalidToken` if the token signature is invalid,
    /// expired, or does not match the expected issuer/audience.
    /// Returns `AuthError::KeyError` if the public key cannot be used for
    /// verification.
    pub async fn validate_token(&self, token: &str) -> Result<Claims> {
        self.validator.validate(token, &self.public_key)
    }
}

impl AuthError {
    /// Map each error variant to its HTTP response parts.
    ///
    /// SECURITY: Sanitized messages never expose internal details.
    #[allow(clippy::cognitive_complexity)] // Reason: exhaustive 1:1 mapping of AuthError variants to HTTP response tuples
    fn response_parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::TokenExpired => {
                (StatusCode::UNAUTHORIZED, "token_expired", "Authentication failed".to_string())
            },
            Self::InvalidSignature => (
                StatusCode::UNAUTHORIZED,
                "invalid_signature",
                "Authentication failed".to_string(),
            ),
            Self::InvalidToken { .. }
            | Self::MissingClaim { .. }
            | Self::InvalidClaimValue { .. }
            // OIDC replay-protection errors and JWT temporal guards: return 401 without
            // revealing which specific claim was invalid to avoid oracle attacks.
            | Self::MissingNonce
            | Self::NonceMismatch
            | Self::MissingAuthTime
            | Self::SessionTooOld { .. }
            | Self::TokenIssuedInFuture
            | Self::TokenTooOld
            | Self::TokenNotYetValid
            // Algorithm-substitution attacks: reject with 401 without revealing which algorithm
            // was rejected, to avoid giving an attacker information about the allowed set.
            | Self::ForbiddenAlgorithm { .. } => {
                (StatusCode::UNAUTHORIZED, "invalid_token", "Authentication failed".to_string())
            },
            Self::TokenNotFound => {
                (StatusCode::UNAUTHORIZED, "token_not_found", "Authentication failed".to_string())
            },
            Self::SessionRevoked => {
                (StatusCode::UNAUTHORIZED, "session_revoked", "Authentication failed".to_string())
            },
            Self::InvalidState => {
                (StatusCode::BAD_REQUEST, "invalid_state", "Authentication failed".to_string())
            },
            Self::Forbidden { .. } => {
                (StatusCode::FORBIDDEN, "forbidden", "Permission denied".to_string())
            },
            Self::OAuthError { .. } => {
                (StatusCode::UNAUTHORIZED, "oauth_error", "Authentication failed".to_string())
            },
            Self::SessionError { .. } => {
                (StatusCode::UNAUTHORIZED, "session_error", "Authentication failed".to_string())
            },
            Self::DatabaseError { .. }
            | Self::ConfigError { .. }
            | Self::OidcMetadataError { .. }
            | Self::Internal { .. }
            | Self::SystemTimeError { .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Service temporarily unavailable".to_string(),
            ),
            Self::PkceError { .. } => {
                (StatusCode::BAD_REQUEST, "pkce_error", "Authentication failed".to_string())
            },
            Self::RateLimited { retry_after_secs } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                format!("Too many requests. Retry after {retry_after_secs} seconds"),
            ),
        }
    }

    /// Log security-sensitive error details server-side before returning a sanitized response.
    #[allow(clippy::cognitive_complexity)] // Reason: exhaustive match logging security-sensitive details per AuthError variant
    fn log_security_details(&self) {
        use tracing::warn;

        match self {
            Self::InvalidToken { reason } => warn!("Invalid token error: {reason}"),
            Self::MissingClaim { claim } => warn!("Missing required claim: {claim}"),
            Self::InvalidClaimValue { claim, reason } => {
                warn!("Invalid claim value for '{claim}': {reason}");
            },
            Self::Forbidden { message } => warn!("Authorization denied: {message}"),
            Self::OAuthError { message } => warn!("OAuth provider error: {message}"),
            Self::SessionError { message } => warn!("Session error: {message}"),
            Self::DatabaseError { message } => {
                warn!("Database error (should not reach client): {message}");
            },
            Self::ConfigError { message } => {
                warn!("Configuration error (should not reach client): {message}");
            },
            Self::OidcMetadataError { message } => warn!("OIDC metadata error: {message}"),
            Self::PkceError { message } => warn!("PKCE error: {message}"),
            Self::Internal { message } => {
                warn!("Internal error (should not reach client): {message}");
            },
            Self::SystemTimeError { message } => {
                warn!("System time error (should not reach client): {message}");
            },
            Self::MissingNonce | Self::NonceMismatch => {
                warn!("OIDC nonce validation failed: {self}");
            },
            Self::MissingAuthTime | Self::SessionTooOld { .. } => {
                warn!("OIDC auth_time validation failed: {self}");
            },
            Self::TokenIssuedInFuture | Self::TokenTooOld | Self::TokenNotYetValid => {
                warn!("JWT temporal claim validation failed: {self}");
            },
            Self::ForbiddenAlgorithm { alg } => {
                warn!("OIDC algorithm-substitution attack rejected: forbidden algorithm '{alg}'");
            },
            // No server-side logging needed for these variants
            Self::TokenExpired
            | Self::InvalidSignature
            | Self::TokenNotFound
            | Self::SessionRevoked
            | Self::InvalidState
            | Self::RateLimited { .. } => {},
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        self.log_security_details();
        let (status, error_code, sanitized_message) = self.response_parts();

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
