// Error sanitization layer
// Separates user-facing messages from internal details

use std::fmt;

/// A sanitizable error that separates user-facing and internal messages
#[derive(Debug, Clone)]
pub struct SanitizedError {
    /// User-facing message (safe for API responses)
    user_message:     String,
    /// Internal message (for logging only)
    internal_message: String,
}

impl SanitizedError {
    /// Create a new sanitized error
    pub fn new(user_message: impl Into<String>, internal_message: impl Into<String>) -> Self {
        Self {
            user_message:     user_message.into(),
            internal_message: internal_message.into(),
        }
    }

    /// Get the user-facing message (safe for API responses)
    pub fn user_facing(&self) -> &str {
        &self.user_message
    }

    /// Get the internal message (for logging only)
    pub fn internal(&self) -> &str {
        &self.internal_message
    }
}

impl fmt::Display for SanitizedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display uses user-facing message (safe for logs)
        write!(f, "{}", self.user_message)
    }
}

impl std::error::Error for SanitizedError {}

/// Helper trait for creating sanitized errors from standard error types
pub trait Sanitizable {
    /// Convert to a sanitized error
    fn sanitized(self, user_message: &str) -> SanitizedError;
}

impl<E: fmt::Display> Sanitizable for E {
    fn sanitized(self, user_message: &str) -> SanitizedError {
        SanitizedError::new(user_message, self.to_string())
    }
}

/// Pre-defined error messages for common authentication scenarios
pub mod messages {
    /// Generic authentication failure message
    pub const AUTH_FAILED: &str = "Authentication failed";

    /// Generic permission denied message
    pub const PERMISSION_DENIED: &str = "Permission denied";

    /// Generic service error message
    pub const SERVICE_UNAVAILABLE: &str = "Service temporarily unavailable";

    /// Generic request error message
    pub const REQUEST_FAILED: &str = "Request failed";

    /// Invalid state (CSRF token)
    pub const INVALID_STATE: &str = "Authentication failed";

    /// Token expired
    pub const TOKEN_EXPIRED: &str = "Authentication failed";

    /// Invalid signature
    pub const INVALID_SIGNATURE: &str = "Authentication failed";

    /// Session expired
    pub const SESSION_EXPIRED: &str = "Authentication failed";

    /// Session revoked
    pub const SESSION_REVOKED: &str = "Authentication failed";
}

/// Error sanitization for authentication errors
pub struct AuthErrorSanitizer;

impl AuthErrorSanitizer {
    /// Sanitize JWT validation error
    pub fn jwt_validation_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize OIDC provider error
    pub fn oidc_provider_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize session token error
    pub fn session_token_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize CSRF state error
    pub fn csrf_state_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::INVALID_STATE, internal_error)
    }

    /// Sanitize permission/authorization error
    pub fn permission_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::PERMISSION_DENIED, internal_error)
    }

    /// Sanitize database error
    pub fn database_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::SERVICE_UNAVAILABLE, internal_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_error_creation() {
        let error = SanitizedError::new(
            "Authentication failed",
            "JWT signature validation failed at cryptographic boundary",
        );

        assert_eq!(error.user_facing(), "Authentication failed");
        assert!(error.internal().contains("cryptographic"));
    }

    #[test]
    fn test_sanitized_error_display() {
        let error = SanitizedError::new(
            "Authentication failed",
            "Internal database error: constraint violation",
        );

        // Display should show user message
        assert_eq!(format!("{}", error), "Authentication failed");
    }

    #[test]
    fn test_auth_error_sanitizer_jwt() {
        let error =
            AuthErrorSanitizer::jwt_validation_error("RS256 signature mismatch at offset 512");

        assert_eq!(error.user_facing(), messages::AUTH_FAILED);
        assert!(error.internal().contains("RS256"));
    }

    #[test]
    fn test_auth_error_sanitizer_permission() {
        let error = AuthErrorSanitizer::permission_error(
            "User lacks role=admin for operation write:config",
        );

        assert_eq!(error.user_facing(), messages::PERMISSION_DENIED);
        assert!(error.internal().contains("role=admin"));
    }

    #[test]
    fn test_sanitizable_trait() {
        let std_error = "Socket error: Connection refused".to_string();
        let sanitized = std_error.sanitized("Service temporarily unavailable");

        assert_eq!(sanitized.user_facing(), "Service temporarily unavailable");
        assert_eq!(sanitized.internal(), "Socket error: Connection refused");
    }
}
