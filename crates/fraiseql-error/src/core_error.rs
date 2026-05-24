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
/// The following would **not** compile, because the `#[non_exhaustive]`
/// attribute forces downstream crates to handle the possibility of new
/// variants — a wildcard arm is required even when every currently-defined
/// variant is enumerated:
///
/// ```compile_fail
/// use fraiseql_error::FraiseQLError;
///
/// fn describe(e: &FraiseQLError) -> &'static str {
///     // Missing wildcard arm: rejected by rustc even though it lists
///     // a few real variants — the `#[non_exhaustive]` attribute makes
///     // the enum effectively open from any downstream crate's point
///     // of view.
///     match e {
///         FraiseQLError::Parse { .. } => "parse",
///         FraiseQLError::Validation { .. } => "validation",
///         FraiseQLError::Database { .. } => "database",
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

    /// The service is temporarily unavailable (e.g. tenant suspended).
    ///
    /// Maps to HTTP 503. `retry_after` is the number of seconds to wait, if known.
    #[error("Service unavailable: {message}")]
    ServiceUnavailable {
        /// Human-readable reason for the unavailability.
        message:     String,
        /// Number of seconds to wait before retrying, if known.
        retry_after: Option<u64>,
    },

    // ========================================================================
    // Domain Subsystem Errors (composed via `From` impls in subsystem crates)
    // ========================================================================
    /// An authentication or authorisation error originating from the auth subsystem.
    ///
    /// The boxed source is the subsystem-specific error type (e.g.
    /// `fraiseql_auth::AuthError`). To preserve subsystem vocabulary while
    /// keeping `fraiseql-error` a leaf crate, the boxed payload is
    /// type-erased here; subsystem crates provide their own
    /// `impl From<SubsystemError> for FraiseQLError` (the sqlx pattern).
    ///
    /// `#[source]` is explicit: `thiserror` 2.x does not auto-detect a single
    /// tuple field as the source, and downstream chain-walkers (`tracing`,
    /// `miette`, `anyhow`) rely on `Error::source()` returning the underlying
    /// subsystem error rather than `None`.
    ///
    /// # Pattern-matching on the inner error
    ///
    /// Because the payload is boxed and `dyn`-erased, downstream `match`
    /// statements cannot bind on subsystem variants directly. Recover the
    /// concrete type via [`std::error::Error::source`] + `downcast_ref`:
    ///
    /// ```ignore
    /// use std::error::Error;
    /// use fraiseql_error::FraiseQLError;
    /// use fraiseql_auth::AuthError;
    ///
    /// if let FraiseQLError::Auth(_) = &err {
    ///     if let Some(inner) = err.source().and_then(|s| s.downcast_ref::<AuthError>()) {
    ///         match inner {
    ///             AuthError::TokenExpired => {/* handle */},
    ///             _ => {},
    ///         }
    ///     }
    /// }
    /// ```
    #[error("Auth error: {0}")]
    Auth(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// A webhook-processing error originating from the webhook subsystem.
    ///
    /// `#[source]` is explicit for the same reason as [`Self::Auth`]; see
    /// that variant for the `downcast_ref` recovery pattern on the boxed
    /// `fraiseql_webhooks::WebhookError`.
    #[error("Webhook error: {0}")]
    Webhook(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// An observer subsystem error (event dispatch, action execution, retry exhaustion).
    ///
    /// `#[source]` is explicit for the same reason as [`Self::Auth`]; see
    /// that variant for the `downcast_ref` recovery pattern on the boxed
    /// `fraiseql_observers::ObserverError`.
    #[error("Observer error: {0}")]
    Observer(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// A file-handling error (size limit, unsupported type, virus scan, quota).
    ///
    /// Unlike `Auth`, `Webhook`, and `Observer`, the file-domain vocabulary
    /// lives inside `fraiseql-error` itself ([`crate::FileError`]) because no
    /// subsystem crate owns it — file operations are spread across
    /// `fraiseql-storage` and `fraiseql-server/storage`.
    #[error("File error: {0}")]
    File(#[from] crate::FileError),

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
    ///
    /// Derived from [`Self::status_code`] so the answer stays consistent
    /// with the variant's actual HTTP routing (notably for `File(_)`, which
    /// straddles 4xx and 5xx after F050).
    #[must_use]
    pub const fn is_client_error(&self) -> bool {
        let code = self.status_code();
        code >= 400 && code < 500
    }

    /// Check if this is a server error (5xx equivalent).
    ///
    /// Derived from [`Self::status_code`] for the same consistency reason
    /// as [`Self::is_client_error`].
    #[must_use]
    pub const fn is_server_error(&self) -> bool {
        let code = self.status_code();
        code >= 500 && code < 600
    }

    /// Check if this error is retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionPool { .. }
                | Self::Timeout { .. }
                | Self::Cancelled { .. }
                | Self::ServiceUnavailable { .. }
        )
    }

    /// Get HTTP status code equivalent.
    ///
    /// # Future variants
    ///
    /// `FraiseQLError` is `#[non_exhaustive]`. The trailing `_ => 500` arm
    /// is a deliberate safety net so a new variant added without updating
    /// this match still gets a defined (and *safe*) HTTP status — generic
    /// 500 Internal Server Error — rather than leaking implementation
    /// details to the client by returning the wrong code.
    //
    // Reason: the trailing wildcard arm intentionally duplicates the 500
    // server-error arm above (silencing `match_same_arms`), and is currently
    // unreachable within this crate because the match enumerates every
    // existing variant (silencing `unreachable_patterns`). The duplication is
    // the security guarantee: when a future variant is added to this
    // `#[non_exhaustive]` enum, the wildcard becomes reachable and prevents a
    // wrong-status leak. See IMPROVEMENTS.md F055.
    #[allow(clippy::match_same_arms, unreachable_patterns)]
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::Parse { .. }
            | Self::Validation { .. }
            | Self::UnknownField { .. }
            | Self::UnknownType { .. }
            | Self::Webhook(_) => 400,
            // File is per-variant: validation failures stay 400, backend
            // failures escalate to 5xx, NotFound → 404, PermissionDenied →
            // 403 (matches the legacy `storage_error_response` routing of
            // `FraiseQLError::Storage` which F050 replaces).
            Self::File(e) => e.status_code(),
            Self::Authentication { .. } | Self::Auth(_) => 401,
            Self::Authorization { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::Conflict { .. } => 409,
            Self::RateLimited { .. } => 429,
            Self::Timeout { .. } | Self::Cancelled { .. } => 408,
            Self::Database { .. }
            | Self::ConnectionPool { .. }
            | Self::Configuration { .. }
            | Self::Internal { .. }
            | Self::Observer(_) => 500,
            Self::Unsupported { .. } => 501,
            Self::ServiceUnavailable { .. } => 503,
            // SECURITY: any future variant defaults to 500 Internal Server
            // Error until explicitly mapped, so we never accidentally return
            // an inappropriate status (e.g. 200, 401) to clients.
            _ => 500,
        }
    }

    /// Get error code for GraphQL response.
    ///
    /// # Future variants
    ///
    /// `FraiseQLError` is `#[non_exhaustive]`. The trailing `_` arm returns
    /// `"INTERNAL_SERVER_ERROR"` so a new variant added without updating
    /// this match still receives a stable (if generic) error code.
    // Reason: see `status_code` — same security-defence rationale; same two
    // lints (`match_same_arms` for the duplicated arm body, `unreachable_patterns`
    // because the in-crate match currently covers every variant).
    #[allow(clippy::match_same_arms, unreachable_patterns)]
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
            Self::Auth(_) => "AUTH_ERROR",
            Self::Webhook(_) => "WEBHOOK_ERROR",
            Self::Observer(_) => "OBSERVER_ERROR",
            Self::File(_) => "FILE_ERROR",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::Conflict { .. } => "CONFLICT",
            Self::Configuration { .. } => "CONFIGURATION_ERROR",
            Self::Unsupported { .. } => "UNSUPPORTED_OPERATION",
            Self::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            Self::Internal { .. } => "INTERNAL_SERVER_ERROR",
            // SECURITY: see `status_code` — fallback to the safe generic
            // category until the new variant is explicitly classified.
            _ => "INTERNAL_SERVER_ERROR",
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
        let chars2: Vec<char> = s2.chars().collect();
        let len2 = chars2.len();

        // Two-row rolling buffer eliminates the nested `Vec<Vec<_>>` indexing.
        // `prev[j]` = distance(s1[..i],   s2[..j])
        // `curr[j]` = distance(s1[..i+1], s2[..j])
        // Both rows have `len2 + 1` entries; iteration is bounded by their
        // length, so every access uses `.get()?` with an `unreachable_or_zero`
        // saturating fallback (provably unreachable in this control flow).
        let mut prev: Vec<usize> = (0..=len2).collect();
        let mut curr: Vec<usize> = vec![0; len2 + 1];

        for (i, c1) in s1.chars().enumerate() {
            // Initialise the leftmost column for this row.
            // Reason: `curr` is sized `len2 + 1 >= 1`, so index 0 is always valid.
            if let Some(slot) = curr.get_mut(0) {
                *slot = i + 1;
            }

            for (j, &c2) in chars2.iter().enumerate() {
                let cost = usize::from(c1 != c2);
                // All four lookups read positions in `[0, len2]`, which are
                // valid by construction (`prev`/`curr` both have len `len2+1`,
                // and `j` ranges over `0..len2`). The `.get()` + `unwrap_or(0)`
                // pattern keeps the function panic-free without changing the
                // computed result.
                let deletion = prev.get(j + 1).copied().unwrap_or(0).saturating_add(1);
                let insertion = curr.get(j).copied().unwrap_or(0).saturating_add(1);
                let substitution = prev.get(j).copied().unwrap_or(0).saturating_add(cost);
                let value = deletion.min(insertion).min(substitution);
                if let Some(slot) = curr.get_mut(j + 1) {
                    *slot = value;
                }
            }

            std::mem::swap(&mut prev, &mut curr);
        }

        // Final answer sits at `prev[len2]` after the last swap.
        // Reason: `prev` is always sized `len2 + 1`, so this index is valid.
        prev.get(len2).copied().unwrap_or(0)
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
    /// Returns `Err` if the original value was `Err`, wrapping it in an `Internal` error with the
    /// given message.
    fn context(self, message: impl Into<String>) -> Result<T>;

    /// Add context lazily (only computed on error).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the original value was `Err`, wrapping it in an `Internal` error with the
    /// context message.
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
mod tests;
