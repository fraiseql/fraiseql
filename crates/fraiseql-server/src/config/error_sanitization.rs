//! Error sanitization configuration and service.
//!
//! When `enabled = true`, strips internal error details (SQL fragments, stack
//! traces, raw DB error messages) from GraphQL responses before they reach
//! the client.

use serde::Deserialize;

use crate::error::{ErrorCode, GraphQLError};

/// Configuration for error sanitization (mirrors `ErrorSanitizationConfig` from
/// `fraiseql-cli`, deserialized from `compiled.security.error_sanitization`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ErrorSanitizationConfig {
    /// Enable error sanitization (default: false — opt-in for backwards compat).
    pub enabled:                     bool,
    /// Strip stack traces, SQL fragments, file paths (default: true).
    pub hide_implementation_details: bool,
    /// Replace raw database error messages with a generic message (default: true).
    pub sanitize_database_errors:    bool,
    /// Replacement message shown to clients when an internal error is sanitized.
    pub custom_error_message:        Option<String>,
}

impl Default for ErrorSanitizationConfig {
    fn default() -> Self {
        Self {
            enabled:                     false,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        }
    }
}

/// Sanitizes GraphQL errors before they reach the client.
///
/// When configured with `enabled = true`, strips internal details from
/// `DatabaseError` and `InternalServerError` responses. Client-facing error
/// codes (validation, auth, not-found, etc.) are always passed through
/// unchanged so the client can act on them.
pub struct ErrorSanitizer {
    config: ErrorSanitizationConfig,
}

impl ErrorSanitizer {
    /// Create a new sanitizer with the given configuration.
    #[must_use]
    pub const fn new(config: ErrorSanitizationConfig) -> Self {
        Self { config }
    }

    /// Create a disabled sanitizer — current behaviour unchanged.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(ErrorSanitizationConfig::default())
    }

    /// Sanitize a single GraphQL error.
    ///
    /// Returns the error unchanged when:
    /// - sanitization is disabled, or
    /// - the error code is client-facing (validation, auth, not-found, etc.)
    #[must_use]
    pub fn sanitize(&self, mut error: GraphQLError) -> GraphQLError {
        if !self.config.enabled {
            return error;
        }

        let is_internal =
            matches!(error.code, ErrorCode::InternalServerError | ErrorCode::DatabaseError);

        if is_internal && self.config.sanitize_database_errors {
            error.message = self
                .config
                .custom_error_message
                .clone()
                .unwrap_or_else(|| "An internal error occurred".to_string());
        }

        if self.config.hide_implementation_details {
            if let Some(ext) = error.extensions.as_mut() {
                ext.detail = None;
            }
        }

        error
    }

    /// Sanitize a batch of errors (the GraphQL `errors` response array).
    #[must_use]
    pub fn sanitize_all(&self, errors: Vec<GraphQLError>) -> Vec<GraphQLError> {
        errors.into_iter().map(|e| self.sanitize(e)).collect()
    }

    /// Whether sanitization is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorExtensions;

    fn enabled_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled:                     true,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        })
    }

    fn disabled_sanitizer() -> ErrorSanitizer {
        ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: false,
            ..ErrorSanitizationConfig::default()
        })
    }

    #[test]
    fn test_sanitizer_strips_db_error_when_enabled() {
        let s = enabled_sanitizer();
        let err = GraphQLError::database(r#"ERROR: relation "tb_users" does not exist"#);
        let out = s.sanitize(err);
        assert_eq!(out.message, "An internal error occurred");
    }

    #[test]
    fn test_sanitizer_passes_through_when_disabled() {
        let s = disabled_sanitizer();
        let original = r#"ERROR: relation "tb_users" does not exist"#;
        let err = GraphQLError::database(original);
        let out = s.sanitize(err);
        assert_eq!(out.message, original);
    }

    #[test]
    fn test_sanitizer_preserves_user_facing_errors() {
        let s = enabled_sanitizer();
        let cases = [
            (ErrorCode::ValidationError, "field is required"),
            (ErrorCode::Unauthenticated, "Authentication required"),
            (ErrorCode::Forbidden, "Access denied"),
            (ErrorCode::NotFound, "Resource not found"),
        ];
        for (code, msg) in cases {
            let err = GraphQLError::new(msg, code);
            let out = s.sanitize(err);
            assert_eq!(out.message, msg, "code {code:?} should not be sanitized");
        }
    }

    #[test]
    fn test_sanitizer_custom_message() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            custom_error_message: Some("Contact support".to_string()),
            ..ErrorSanitizationConfig::default()
        });
        let err = GraphQLError::database("pg error detail");
        assert_eq!(s.sanitize(err).message, "Contact support");
    }

    #[test]
    fn test_sanitizer_strips_extensions_detail_when_hide_impl() {
        let s = enabled_sanitizer();
        let mut err = GraphQLError::internal("internal");
        err.extensions = Some(ErrorExtensions {
            category:         None,
            status:           None,
            request_id:       None,
            retry_after_secs: None,
            detail:           Some("panic at line 42".to_string()),
        });
        let out = s.sanitize(err);
        assert!(
            out.extensions.as_ref().and_then(|e| e.detail.as_ref()).is_none(),
            "detail should be stripped when hide_implementation_details = true"
        );
    }

    #[test]
    fn test_sanitize_database_errors_false_allows_db_message_through() {
        let s = ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            sanitize_database_errors: false,
            ..ErrorSanitizationConfig::default()
        });
        let err = GraphQLError::database("duplicate key value");
        assert_eq!(s.sanitize(err).message, "duplicate key value");
    }
}
