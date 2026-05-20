//! Error sanitization layer — separates user-facing messages from internal details.
//!
//! Authentication errors often contain internal information that must never reach API
//! clients (database query details, key material references, stack traces). This module
//! provides [`SanitizedError`] and [`AuthErrorSanitizer`] to ensure only safe, generic
//! messages are returned in API responses while full details are retained for server-side
//! logging.

use std::fmt;

/// A sanitizable error that separates user-facing and internal messages
#[derive(Debug, Clone)]
pub struct SanitizedError {
    /// User-facing message (safe for API responses)
    user_message: String,
    /// Internal message (for logging only)
    internal_message: String,
}

impl SanitizedError {
    /// Create a new sanitized error
    pub fn new(user_message: impl Into<String>, internal_message: impl Into<String>) -> Self {
        Self {
            user_message: user_message.into(),
            internal_message: internal_message.into(),
        }
    }

    /// Get the user-facing message (safe for API responses)
    #[must_use]
    pub fn user_facing(&self) -> &str {
        &self.user_message
    }

    /// Get the internal message (for logging only)
    #[must_use]
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

/// Helper trait for creating sanitized errors from standard error types.
///
/// The method `.sanitized(msg)` converts any `Display` type into a [`SanitizedError`],
/// keeping the original message for server-side logging while returning a safe message
/// in the API response.
pub trait Sanitize {
    /// Convert to a sanitized error
    fn sanitized(self, user_message: &str) -> SanitizedError;
}

impl<E: fmt::Display> Sanitize for E {
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
    #[must_use]
    pub fn jwt_validation_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize OIDC provider error
    #[must_use]
    pub fn oidc_provider_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize session token error
    #[must_use]
    pub fn session_token_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::AUTH_FAILED, internal_error)
    }

    /// Sanitize CSRF state error
    #[must_use]
    pub fn csrf_state_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::INVALID_STATE, internal_error)
    }

    /// Sanitize permission/authorization error
    #[must_use]
    pub fn permission_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::PERMISSION_DENIED, internal_error)
    }

    /// Sanitize database error
    #[must_use]
    pub fn database_error(internal_error: &str) -> SanitizedError {
        SanitizedError::new(messages::SERVICE_UNAVAILABLE, internal_error)
    }
}
