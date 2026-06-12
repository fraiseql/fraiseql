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

    /// Whether an internal/server-fault error message should be replaced with a generic
    /// one before reaching the client.
    ///
    /// This is the same gate [`sanitize`](Self::sanitize) applies to
    /// `InternalServerError`/`DatabaseError` on the GraphQL path, exposed so the REST
    /// surface can apply identical sanitization at its error-rendering site (H7 — the
    /// REST path previously wrote raw DB error text into 5xx bodies).
    #[must_use]
    pub const fn should_sanitize_internal(&self) -> bool {
        self.config.enabled && self.config.sanitize_database_errors
    }

    /// The generic, client-safe message used to replace internal error detail.
    ///
    /// Matches the replacement [`sanitize`](Self::sanitize) uses on the GraphQL path
    /// (the configured `custom_error_message`, or `"An internal error occurred"`).
    #[must_use]
    pub fn internal_error_message(&self) -> String {
        self.config
            .custom_error_message
            .clone()
            .unwrap_or_else(|| "An internal error occurred".to_string())
    }
}
