//! Error sanitization configuration and service.
//!
//! When `enabled = true`, strips internal error details (SQL fragments, stack
//! traces, raw DB error messages) from GraphQL responses before they reach
//! the client.

/// The compiled `[security.error_sanitization]` shape — the schema seam owns it
/// (#977), so the CLI, the compiled artefact and this server share one type.
pub use fraiseql_core::schema::ErrorSanitizationConfig;

use crate::error::{ErrorCode, GraphQLError};

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

        if error.code.carries_database_text() && self.config.sanitize_database_errors {
            error.message = self.replacement_message(error.code);
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

    /// Whether a database-provenance error message should be replaced with a generic one
    /// before reaching the client.
    ///
    /// This is the same gate [`sanitize`](Self::sanitize) applies on the GraphQL path,
    /// exposed so the REST surface can apply identical sanitization at its
    /// error-rendering site (H7 — the REST path previously wrote raw DB error text into
    /// 5xx bodies; #1153 — and into 4xx bodies, which is the half a caller can trigger).
    #[must_use]
    pub const fn should_sanitize_internal(&self) -> bool {
        self.config.enabled && self.config.sanitize_database_errors
    }

    /// The generic, client-safe message that replaces a database-written one.
    ///
    /// A configured `custom_error_message` wins, as before. Otherwise the replacement is
    /// chosen by class, because a 400 answered with `"An internal error occurred"` is a
    /// false statement about what happened: the request *was* the fault, the server is
    /// fine, and a client told otherwise will retry or escalate for no reason. The error
    /// **code** still carries the actionable part (`BAD_USER_INPUT` /
    /// `CONSTRAINT_VIOLATION`); it is only the free text — which is where the schema,
    /// function and constraint names live — that is withheld.
    #[must_use]
    pub fn replacement_message(&self, code: ErrorCode) -> String {
        if let Some(custom) = self.config.custom_error_message.clone() {
            return custom;
        }
        match code {
            ErrorCode::BadUserInput => "The request contains an invalid value".to_string(),
            ErrorCode::ConstraintViolation => {
                "The request conflicts with the current state of the data".to_string()
            },
            _ => "An internal error occurred".to_string(),
        }
    }
}
