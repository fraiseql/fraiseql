//! Error types for `FraiseQL` core.
//!
//! This module provides language-agnostic error types that can be
//! converted to Python exceptions, `JavaScript` errors, etc. by binding layers.
//!
//! # Error Hierarchy
//!
//! ```text
//! FraiseQLError
//! ├── Parse           - GraphQL parsing errors
//! ├── Validation      - Schema/input validation errors
//! ├── Database        - PostgreSQL errors
//! ├── Authorization   - Permission/RBAC errors
//! ├── Configuration   - Config/setup errors
//! ├── Timeout         - Operation timeout
//! ├── NotFound        - Resource not found
//! ├── Conflict        - Concurrent modification
//! └── Internal        - Unexpected internal errors
//! ```

use thiserror::Error;

/// Result type alias for `FraiseQL` operations.
pub type Result<T> = std::result::Result<T, FraiseQLError>;

/// Main error type for `FraiseQL` operations.
///
/// All errors in the core library are converted to this type.
/// Language bindings convert this to their native error types.
#[derive(Error, Debug)]
pub enum FraiseQLError {
    // ========================================================================
    // GraphQL Errors
    // ========================================================================
    /// GraphQL parsing error.
    ///
    /// Returned when the GraphQL query string cannot be parsed.
    #[error("Parse error at {location}: {message}")]
    Parse {
        /// Error message describing the parse failure.
        message:  String,
        /// Location in the query where the error occurred.
        location: String,
    },

    /// GraphQL validation error.
    ///
    /// Returned when a query is syntactically valid but semantically invalid.
    #[error("Validation error: {message}")]
    Validation {
        /// Error message describing the validation failure.
        message: String,
        /// Path to the field with the error (e.g., "user.posts.0.title").
        path:    Option<String>,
    },

    /// Unknown field error.
    ///
    /// Returned when a query references a field that doesn't exist in the schema.
    #[error("Unknown field '{field}' on type '{type_name}'")]
    UnknownField {
        /// The field name that was not found.
        field:     String,
        /// The type on which the field was queried.
        type_name: String,
    },

    /// Unknown type error.
    ///
    /// Returned when a query references a type that doesn't exist in the schema.
    #[error("Unknown type '{type_name}'")]
    UnknownType {
        /// The type name that was not found.
        type_name: String,
    },

    // ========================================================================
    // Database Errors
    // ========================================================================
    /// Database operation error.
    ///
    /// Wraps errors from `PostgreSQL` operations.
    #[error("Database error: {message}")]
    Database {
        /// Error message from the database.
        message:   String,
        /// SQL state code if available (e.g., "23505" for unique violation).
        sql_state: Option<String>,
    },

    /// Connection pool error.
    ///
    /// Returned when the database connection pool is exhausted or unavailable.
    #[error("Connection pool error: {message}")]
    ConnectionPool {
        /// Error message.
        message: String,
    },

    /// Query timeout error.
    ///
    /// Returned when a database query exceeds the configured timeout.
    #[error("Query timeout after {timeout_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
        /// The query that timed out (truncated if too long).
        query:      Option<String>,
    },

    /// Query cancellation error.
    ///
    /// Returned when a query execution is cancelled via a cancellation token
    /// or similar mechanism (e.g., client disconnection, explicit user request).
    #[error("Query cancelled: {reason}")]
    Cancelled {
        /// Query identifier for tracking/logging.
        query_id: String,
        /// Reason for cancellation (e.g., "user cancelled", "connection closed").
        reason:   String,
    },

    // ========================================================================
    // Authorization Errors
    // ========================================================================
    /// Authorization error.
    ///
    /// Returned when the user doesn't have permission for an operation.
    #[error("Authorization error: {message}")]
    Authorization {
        /// Error message.
        message:  String,
        /// The action that was denied (e.g., "read", "write", "delete").
        action:   Option<String>,
        /// The resource that was being accessed.
        resource: Option<String>,
    },

    /// Authentication error.
    ///
    /// Returned when authentication fails (invalid token, expired, etc.).
    #[error("Authentication error: {message}")]
    Authentication {
        /// Error message.
        message: String,
    },

    /// Rate limiting error.
    ///
    /// Returned when a request is rate limited due to too many errors.
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
    ///
    /// Returned when a requested resource doesn't exist.
    #[error("{resource_type} not found: {identifier}")]
    NotFound {
        /// Type of resource (e.g., "User", "Post").
        resource_type: String,
        /// Identifier that was looked up.
        identifier:    String,
    },

    /// Conflict error.
    ///
    /// Returned when an operation would conflict with existing data.
    #[error("Conflict: {message}")]
    Conflict {
        /// Error message.
        message: String,
    },

    // ========================================================================
    // Configuration Errors
    // ========================================================================
    /// Configuration error.
    ///
    /// Returned when configuration is invalid or missing.
    #[error("Configuration error: {message}")]
    Configuration {
        /// Error message.
        message: String,
    },

    // ========================================================================
    // Internal Errors
    // ========================================================================
    /// Internal error.
    ///
    /// Returned for unexpected internal errors. Should be rare.
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
    // ========================================================================
    // Constructor helpers
    // ========================================================================

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

    // ========================================================================
    // Error classification
    // ========================================================================

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
            Self::Internal { .. } => "INTERNAL_SERVER_ERROR",
        }
    }
}

// ============================================================================
// Conversions from other error types
// ============================================================================

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

// ============================================================================
// Error context extension trait
// ============================================================================

/// Extension trait for adding context to errors.
///
/// Provides methods to attach contextual information to errors, making debugging easier
/// and providing better error messages to users.
///
/// # Usage Examples
///
/// **Adding static context to an error:**
///
/// ```ignore
/// use fraiseql_core::error::ErrorContext;
///
/// fn load_schema(path: &str) -> Result<String> {
///     std::fs::read_to_string(path)
///         .map_err(|e| e.into())
///         .context(format!("Failed to load schema from {}", path))
/// }
/// ```
///
/// **Adding lazy context (computed only on error):**
///
/// ```ignore
/// use fraiseql_core::error::ErrorContext;
///
/// fn execute_query(query: &str) -> Result<Vec<()>> {
///     // ... query execution ...
///     Ok(vec![])
///         .with_context(|| format!("Query execution failed for query: {}", query))
/// }
/// ```
pub trait ErrorContext<T> {
    /// Add context to an error.
    ///
    /// Prepends a context message to the error. Useful for providing operation-specific
    /// information about where/why an error occurred.
    ///
    /// # Arguments
    ///
    /// * `message` - Context message to prepend to the error
    ///
    /// # Errors
    ///
    /// Returns the error with additional context message prepended.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::error::ErrorContext;
    /// # use fraiseql_core::error::Result;
    ///
    /// # async fn example() -> Result<()> {
    /// let result: Result<String> = Err(fraiseql_core::error::FraiseQLError::database("connection failed"));
    /// result.context("while connecting to primary database")?;
    /// # Ok(())
    /// # }
    /// ```
    fn context(self, message: impl Into<String>) -> Result<T>;

    /// Add context lazily (only computed on error).
    ///
    /// Similar to `context()`, but the message is only computed if an error actually occurs.
    /// Useful when building the context message is expensive or requires runtime information.
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that computes the context message on error
    ///
    /// # Errors
    ///
    /// Returns the error with additional context message prepended.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::error::ErrorContext;
    /// # use fraiseql_core::error::Result;
    ///
    /// # async fn example(rows: Vec<()>) -> Result<()> {
    /// // The expensive string formatting only happens if the operation fails
    /// let processed: Result<()> = Ok(());
    /// processed.with_context(|| {
    ///     format!("Failed to process {} rows", rows.len())
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
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

// ============================================================================
// Validation Field Error (Phase 1, Cycle 1)
// ============================================================================

/// A validation error for a specific field in an input object.
///
/// Used to report validation failures with field-level granularity,
/// including the field path, validation rule type, and error message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ValidationFieldError {
    /// Path to the field that failed validation (e.g., "user.email", "addresses.0.zipcode").
    pub field:     String,
    /// Type of validation rule that failed (e.g., "pattern", "required", "range").
    pub rule_type: String,
    /// Human-readable error message explaining what went wrong.
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

// ============================================================================
// Tests
// ============================================================================

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
    fn test_from_serde_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: FraiseQLError = json_err.into();
        assert!(matches!(err, FraiseQLError::Parse { .. }));
    }

    #[test]
    fn test_error_context() {
        fn may_fail() -> std::result::Result<(), std::io::Error> {
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"))
        }

        let result = may_fail().context("failed to load config");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("failed to load config"));
    }

    // Phase 1, Cycle 1: Enhanced Validation Error Types
    #[test]
    fn test_validation_field_error_creation() {
        let field_err = ValidationFieldError::new("user.email", "pattern", "Invalid email format");
        assert_eq!(field_err.field, "user.email");
        assert_eq!(field_err.rule_type, "pattern");
        assert_eq!(field_err.message, "Invalid email format");
    }

    #[test]
    fn test_validation_field_error_display() {
        let field_err =
            ValidationFieldError::new("address.zipcode", "length", "Zipcode must be 5 digits");
        let display = format!("{}", field_err);
        assert_eq!(display, "address.zipcode (length): Zipcode must be 5 digits");
    }

    #[test]
    fn test_validation_field_error_serialization() {
        let field_err = ValidationFieldError::new("user.phone", "pattern", "Invalid phone number");
        let json = serde_json::to_string(&field_err).expect("serialization failed");
        let deserialized: ValidationFieldError =
            serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(deserialized.field, "user.phone");
        assert_eq!(deserialized.rule_type, "pattern");
        assert_eq!(deserialized.message, "Invalid phone number");
    }
}
