//! Core error types for FraiseQL operations.
//!
//! This module provides the primary error enum `FraiseQLError` used throughout
//! the FraiseQL compilation and execution pipeline.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result type alias for FraiseQL operations.
pub type Result<T> = std::result::Result<T, FraiseQLError>;

/// Main error type for FraiseQL operations.
///
/// All errors in the core library are converted to this type.
/// Language bindings convert this to their native error types.
///
/// # Error Categories
///
/// Errors are organized by domain:
///
/// ## GraphQL Errors
/// - `Parse` — Malformed GraphQL syntax
/// - `Validation` — Schema validation failures
/// - `UnknownField` — Field doesn't exist on type
/// - `UnknownType` — Type doesn't exist in schema
///
/// ## Database Errors
/// - `Database` — PostgreSQL/MySQL/SQLite errors (includes SQL state code)
/// - `ConnectionPool` — Connection pool exhausted or unavailable
/// - `Timeout` — Query exceeded configured timeout
/// - `Cancelled` — Query was cancelled by caller
///
/// ## Authorization/Security Errors
/// - `Authorization` — User lacks permission for operation
/// - `Authentication` — Invalid/expired JWT token
/// - `RateLimited` — Too many requests (includes retry-after)
///
/// ## Resource Errors
/// - `NotFound` — Resource doesn't exist (404)
/// - `Conflict` — Operation would violate constraints (409)
///
/// ## Configuration Errors
/// - `Configuration` — Invalid setup/configuration
/// - `Unsupported` — Operation not supported by current database backend
///
/// ## Internal Errors
/// - `Internal` — Unexpected internal failures
///
/// # Stability
///
/// This enum is marked `#[non_exhaustive]` to allow adding new error variants
/// in future minor versions without breaking backward compatibility.
///
/// External `match` expressions must include a wildcard `_` arm:
///
/// ```rust
/// use fraiseql_error::FraiseQLError;
///
/// fn describe(e: &FraiseQLError) -> &'static str {
///     match e {
///         FraiseQLError::Parse { .. } => "parse error",
///         FraiseQLError::Validation { .. } => "validation error",
///         _ => "other error", // required: FraiseQLError is #[non_exhaustive]
///     }
/// }
/// ```
///
/// The following would **not** compile (missing wildcard arm):
///
/// ```compile_fail
/// use fraiseql_error::FraiseQLError;
///
/// fn describe(e: &FraiseQLError) -> &'static str {
///     match e {
///         FraiseQLError::Parse { .. } => "parse",
///         FraiseQLError::Validation { .. } => "validation",
///         FraiseQLError::Database { .. } => "database",
///         FraiseQLError::Network { .. } => "network",
///         FraiseQLError::Authorization { .. } => "authorization",
///         FraiseQLError::NotFound { .. } => "not found",
///         FraiseQLError::Conflict { .. } => "conflict",
///         FraiseQLError::Configuration { .. } => "configuration",
///         FraiseQLError::Unsupported { .. } => "unsupported",
///         FraiseQLError::Internal { .. } => "internal",
///         FraiseQLError::UnknownField { .. } => "unknown field",
///         FraiseQLError::UnknownType { .. } => "unknown type",
///         FraiseQLError::FieldExclusion { .. } => "field exclusion",
///         FraiseQLError::TypeMismatch { .. } => "type mismatch",
///         FraiseQLError::RateLimitExceeded { .. } => "rate limit",
///         FraiseQLError::Forbidden { .. } => "forbidden",
///     }
/// }
/// ```
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum FraiseQLError {
    // ========================================================================
    // GraphQL Errors
    // ========================================================================
    /// GraphQL parsing error.
    #[error("Parse error at {location}: {message}")]
    Parse {
        /// Error message describing the parse failure.
        message:  String,
        /// Location in the query where the error occurred.
        location: String,
    },

    /// GraphQL validation error.
    #[error("Validation error: {message}")]
    Validation {
        /// Error message describing the validation failure.
        message: String,
        /// Path to the field with the error (e.g., "user.posts.0.title").
        path:    Option<String>,
    },

    /// Unknown field error.
    #[error("Unknown field '{field}' on type '{type_name}'")]
    UnknownField {
        /// The field name that was not found.
        field:     String,
        /// The type on which the field was queried.
        type_name: String,
    },

    /// Unknown type error.
    #[error("Unknown type '{type_name}'")]
    UnknownType {
        /// The type name that was not found.
        type_name: String,
    },

    // ========================================================================
    // Database Errors
    // ========================================================================
    /// Database operation error.
    #[error("Database error: {message}")]
    Database {
        /// Error message from the database.
        message:   String,
        /// SQL state code if available (e.g., "23505" for unique violation).
        sql_state: Option<String>,
    },

    /// Connection pool error.
    #[error("Connection pool error: {message}")]
    ConnectionPool {
        /// Error message.
        message: String,
    },

    /// Query timeout error.
    #[error("Query timeout after {timeout_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
        /// The query that timed out (truncated if too long).
        query:      Option<String>,
    },

    /// Query cancellation error.
    #[error("Query cancelled: {reason}")]
    Cancelled {
        /// Query identifier for tracking/logging.
        query_id: String,
        /// Reason for cancellation.
        reason:   String,
    },

    // ========================================================================
    // Authorization Errors
    // ========================================================================
    /// Authorization error.
    #[error("Authorization error: {message}")]
    Authorization {
        /// Error message.
        message:  String,
        /// The action that was denied.
        action:   Option<String>,
        /// The resource that was being accessed.
        resource: Option<String>,
    },

    /// Authentication error.
    #[error("Authentication error: {message}")]
    Authentication {
        /// Error message.
        message: String,
    },

    /// Rate limiting error.
    #[error("Rate limit exceeded: {message}")]
    RateLimited {
        /// Error message.
        message:          String,
        /// Number of seconds to wait before retrying.
        retry_after_secs: u64,
    },

    // ========================================================================
    // Resource Errors
    // ========================================================================
    /// Resource not found error.
    #[error("{resource_type} not found: {identifier}")]
    NotFound {
        /// Type of resource (e.g., "User", "Post").
        resource_type: String,
        /// Identifier that was looked up.
        identifier:    String,
    },

    /// Conflict error.
    #[error("Conflict: {message}")]
    Conflict {
        /// Error message.
        message: String,
    },

    // ========================================================================
    // Configuration Errors
    // ========================================================================
    /// Configuration error.
    #[error("Configuration error: {message}")]
    Configuration {
        /// Error message.
        message: String,
    },

    /// Unsupported operation error.
    #[error("Unsupported operation: {message}")]
    Unsupported {
        /// Error message describing what is not supported.
        message: String,
    },

    // ========================================================================
    // Internal Errors
    // ========================================================================
    /// Internal error.
    #[error("Internal error: {message}")]
    Internal {
        /// Error message.
        message: String,
        /// Optional source error for debugging.
        #[source]
        source:  Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl FraiseQLError {
    /// Create a parse error.
    #[must_use]
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse {
            message:  message.into(),
            location: "unknown".to_string(),
        }
    }

    /// Create a parse error with location.
    #[must_use]
    pub fn parse_at(message: impl Into<String>, location: impl Into<String>) -> Self {
        Self::Parse {
            message:  message.into(),
            location: location.into(),
        }
    }

    /// Create a validation error.
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            path:    None,
        }
    }

    /// Create a validation error with path.
    #[must_use]
    pub fn validation_at(message: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            path:    Some(path.into()),
        }
    }

    /// Create a database error.
    #[must_use]
    pub fn database(message: impl Into<String>) -> Self {
        Self::Database {
            message:   message.into(),
            sql_state: None,
        }
    }

    /// Create an authorization error.
    #[must_use]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Authorization {
            message:  message.into(),
            action:   None,
            resource: None,
        }
    }

    /// Create a not found error.
    #[must_use]
    pub fn not_found(resource_type: impl Into<String>, identifier: impl Into<String>) -> Self {
        Self::NotFound {
            resource_type: resource_type.into(),
            identifier:    identifier.into(),
        }
    }

    /// Create a configuration error.
    #[must_use]
    pub fn config(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// Create an internal error.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            source:  None,
        }
    }

    /// Create a cancellation error.
    #[must_use]
    pub fn cancelled(query_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Cancelled {
            query_id: query_id.into(),
            reason:   reason.into(),
        }
    }

    /// Check if this is a client error (4xx equivalent).
    #[must_use]
    pub const fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::Parse { .. }
                | Self::Validation { .. }
                | Self::UnknownField { .. }
                | Self::UnknownType { .. }
                | Self::Authorization { .. }
                | Self::Authentication { .. }
                | Self::NotFound { .. }
                | Self::Conflict { .. }
                | Self::RateLimited { .. }
        )
    }

    /// Check if this is a server error (5xx equivalent).
    #[must_use]
    pub const fn is_server_error(&self) -> bool {
        matches!(
            self,
            Self::Database { .. }
                | Self::ConnectionPool { .. }
                | Self::Timeout { .. }
                | Self::Cancelled { .. }
                | Self::Configuration { .. }
                | Self::Unsupported { .. }
                | Self::Internal { .. }
        )
    }

    /// Check if this error is retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionPool { .. } | Self::Timeout { .. } | Self::Cancelled { .. }
        )
    }

    /// Get HTTP status code equivalent.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::Parse { .. }
            | Self::Validation { .. }
            | Self::UnknownField { .. }
            | Self::UnknownType { .. } => 400,
            Self::Authentication { .. } => 401,
            Self::Authorization { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::Conflict { .. } => 409,
            Self::RateLimited { .. } => 429,
            Self::Timeout { .. } | Self::Cancelled { .. } => 408,
            Self::Database { .. }
            | Self::ConnectionPool { .. }
            | Self::Configuration { .. }
            | Self::Internal { .. } => 500,
            Self::Unsupported { .. } => 501,
        }
    }

    /// Get error code for GraphQL response.
    #[must_use]
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::Parse { .. } => "GRAPHQL_PARSE_FAILED",
            Self::Validation { .. } => "GRAPHQL_VALIDATION_FAILED",
            Self::UnknownField { .. } => "UNKNOWN_FIELD",
            Self::UnknownType { .. } => "UNKNOWN_TYPE",
            Self::Database { .. } => "DATABASE_ERROR",
            Self::ConnectionPool { .. } => "CONNECTION_POOL_ERROR",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Cancelled { .. } => "CANCELLED",
            Self::Authorization { .. } => "FORBIDDEN",
            Self::Authentication { .. } => "UNAUTHENTICATED",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::Conflict { .. } => "CONFLICT",
            Self::Configuration { .. } => "CONFIGURATION_ERROR",
            Self::Unsupported { .. } => "UNSUPPORTED_OPERATION",
            Self::Internal { .. } => "INTERNAL_SERVER_ERROR",
        }
    }

    /// Create an unknown field error with helpful suggestions.
    #[must_use]
    pub fn unknown_field_with_suggestion(
        field: impl Into<String>,
        type_name: impl Into<String>,
        available_fields: &[&str],
    ) -> Self {
        let field = field.into();
        let type_name = type_name.into();

        let suggestion = available_fields
            .iter()
            .map(|f| (*f, Self::levenshtein_distance(&field, f)))
            .filter(|(_, distance)| *distance <= 2)
            .min_by_key(|(_, distance)| *distance)
            .map(|(f, _)| f);

        if let Some(suggested_field) = suggestion {
            Self::UnknownField {
                field: format!("{field} (did you mean '{suggested_field}'?)"),
                type_name,
            }
        } else {
            Self::UnknownField { field, type_name }
        }
    }

    fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        let len1 = s1.chars().count();
        let len2 = s2.chars().count();

        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for (i, row) in matrix.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, val) in matrix[0].iter_mut().enumerate() {
            *val = j;
        }

        for (i, c1) in s1.chars().enumerate() {
            for (j, c2) in s2.chars().enumerate() {
                let cost = usize::from(c1 != c2);
                matrix[i + 1][j + 1] = std::cmp::min(
                    std::cmp::min(
                        matrix[i][j + 1] + 1,
                        matrix[i + 1][j] + 1,
                    ),
                    matrix[i][j] + cost,
                );
            }
        }

        matrix[len1][len2]
    }

    /// Create a database error from PostgreSQL error code.
    #[must_use]
    pub fn from_postgres_code(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        match code {
            "42P01" => Self::Database {
                message: "The table or view you're querying doesn't exist. \
                          Check that the schema is compiled and the database is initialized."
                    .to_string(),
                sql_state: Some(code.to_string()),
            },
            "42703" => Self::Database {
                message: "A column referenced in the query doesn't exist in the table. \
                          This may indicate the database schema is out of sync with the compiled schema."
                    .to_string(),
                sql_state: Some(code.to_string()),
            },
            "23505" => Self::Conflict {
                message: "A unique constraint was violated. This value already exists in the database.".to_string(),
            },
            "23503" => Self::Conflict {
                message: "A foreign key constraint was violated. The referenced record doesn't exist."
                    .to_string(),
            },
            "23502" => Self::Conflict {
                message: "A NOT NULL constraint was violated. The field cannot be empty.".to_string(),
            },
            "22P02" => Self::Validation {
                message: "Invalid input value. The provided value doesn't match the expected data type.".to_string(),
                path: None,
            },
            _ => Self::Database {
                message,
                sql_state: Some(code.to_string()),
            },
        }
    }

    /// Create a rate limit error with retry information.
    #[must_use]
    pub fn rate_limited_with_retry(retry_after_secs: u64) -> Self {
        Self::RateLimited {
            message: format!(
                "Rate limit exceeded. Please try again in {retry_after_secs} seconds. \
                 For permanent increases, contact support."
            ),
            retry_after_secs,
        }
    }

    /// Create an authentication error with context.
    #[must_use]
    pub fn auth_error(reason: impl Into<String>) -> Self {
        Self::Authentication {
            message: reason.into(),
        }
    }
}

impl From<serde_json::Error> for FraiseQLError {
    fn from(e: serde_json::Error) -> Self {
        Self::Parse {
            message:  e.to_string(),
            location: format!("line {}, column {}", e.line(), e.column()),
        }
    }
}

impl From<std::io::Error> for FraiseQLError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal {
            message: format!("I/O error: {e}"),
            source:  Some(Box::new(e)),
        }
    }
}

impl From<std::env::VarError> for FraiseQLError {
    fn from(e: std::env::VarError) -> Self {
        Self::Configuration {
            message: format!("Environment variable error: {e}"),
        }
    }
}

/// Extension trait for adding context to errors.
pub trait ErrorContext<T> {
    /// Add context to an error.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the original value was `Err`, wrapping it in an `Internal` error with the given message.
    fn context(self, message: impl Into<String>) -> Result<T>;

    /// Add context lazily (only computed on error).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the original value was `Err`, wrapping it in an `Internal` error with the context message.
    fn with_context<F, M>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> M,
        M: Into<String>;
}

impl<T, E: Into<FraiseQLError>> ErrorContext<T> for std::result::Result<T, E> {
    fn context(self, message: impl Into<String>) -> Result<T> {
        self.map_err(|e| {
            let inner = e.into();
            FraiseQLError::Internal {
                message: format!("{}: {inner}", message.into()),
                source:  None,
            }
        })
    }

    fn with_context<F, M>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> M,
        M: Into<String>,
    {
        self.map_err(|e| {
            let inner = e.into();
            FraiseQLError::Internal {
                message: format!("{}: {inner}", f().into()),
                source:  None,
            }
        })
    }
}

/// A validation error for a specific field in an input object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFieldError {
    /// Path to the field that failed validation.
    pub field:     String,
    /// Type of validation rule that failed.
    pub rule_type: String,
    /// Human-readable error message.
    pub message:   String,
}

impl ValidationFieldError {
    /// Create a new validation field error.
    #[must_use]
    pub fn new(
        field: impl Into<String>,
        rule_type: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field:     field.into(),
            rule_type: rule_type.into(),
            message:   message.into(),
        }
    }
}

impl std::fmt::Display for ValidationFieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.field, self.rule_type, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error() {
        let err = FraiseQLError::parse("unexpected token");
        assert!(err.is_client_error());
        assert!(!err.is_server_error());
        assert_eq!(err.status_code(), 400);
        assert_eq!(err.error_code(), "GRAPHQL_PARSE_FAILED");
    }

    #[test]
    fn test_database_error() {
        let err = FraiseQLError::database("connection refused");
        assert!(!err.is_client_error());
        assert!(err.is_server_error());
        assert_eq!(err.status_code(), 500);
    }

    #[test]
    fn test_not_found_error() {
        let err = FraiseQLError::not_found("User", "123");
        assert!(err.is_client_error());
        assert_eq!(err.status_code(), 404);
        assert_eq!(err.to_string(), "User not found: 123");
    }

    #[test]
    fn test_retryable_errors() {
        assert!(
            FraiseQLError::ConnectionPool {
                message: "timeout".to_string(),
            }
            .is_retryable()
        );
        assert!(
            FraiseQLError::Timeout {
                timeout_ms: 5000,
                query:      None,
            }
            .is_retryable()
        );
        assert!(!FraiseQLError::parse("bad query").is_retryable());
    }

    #[test]
    fn test_unsupported_is_501() {
        let err = FraiseQLError::Unsupported {
            message: "execute_function_call not supported on SQLite".to_string(),
        };
        assert_eq!(err.status_code(), 501);
        assert!(err.is_server_error());
        assert_eq!(err.error_code(), "UNSUPPORTED_OPERATION");
    }

    #[test]
    fn test_from_serde_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json")
            .expect_err("'not json' must fail to parse");
        let err: FraiseQLError = json_err.into();
        assert!(matches!(err, FraiseQLError::Parse { .. }));
    }

    #[test]
    fn test_validation_field_error_creation() {
        let field_err = ValidationFieldError::new("user.email", "pattern", "Invalid email format");
        assert_eq!(field_err.field, "user.email");
        assert_eq!(field_err.rule_type, "pattern");
        assert_eq!(field_err.message, "Invalid email format");
    }

    #[test]
    fn test_levenshtein_ascii() {
        // Basic sanity
        assert_eq!(FraiseQLError::levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(FraiseQLError::levenshtein_distance("", "abc"), 3);
        assert_eq!(FraiseQLError::levenshtein_distance("abc", ""), 3);
        assert_eq!(FraiseQLError::levenshtein_distance("same", "same"), 0);
    }

    #[test]
    fn test_levenshtein_multibyte_utf8() {
        // "café" is 4 chars but 5 bytes — previously the byte-length bug returned
        // matrix[5][5] instead of matrix[4][4], which was an unmodified zero cell.
        assert_eq!(FraiseQLError::levenshtein_distance("café", "cafe"), 1);
        assert_eq!(FraiseQLError::levenshtein_distance("naïve", "naive"), 1);
        // Two multi-byte strings: distance should equal number of differing chars
        assert_eq!(FraiseQLError::levenshtein_distance("café", "café"), 0);
    }
}
