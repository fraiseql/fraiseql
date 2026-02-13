//! Database adapter trait for Arrow Flight service.
//!
//! This module defines a minimal database adapter interface for executing
//! raw SQL queries and returning results as JSON. It's designed to be independent
//! of fraiseql-core to avoid circular dependencies.
//!
//! # Note
//!
//! This trait is simpler than fraiseql_core::db::DatabaseAdapter and only includes
//! the methods needed for Arrow Flight streaming. In fraiseql-server, a wrapper
//! can implement both traits by delegating to the core adapter.

use std::collections::HashMap;

use async_trait::async_trait;

/// Error type for database operations.
///
/// This is a simplified error type that can be created from various
/// database drivers without requiring fraiseql-core dependencies.
#[derive(Debug, Clone)]
pub struct DatabaseError {
    message: String,
}

impl DatabaseError {
    /// Create a new database error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DatabaseError {}

/// Result type for database operations.
pub type DatabaseResult<T> = Result<T, DatabaseError>;

/// Database adapter for executing queries against views.
///
/// This trait abstracts over different database backends (PostgreSQL, MySQL, SQLite, etc.)
/// and provides a minimal interface for executing raw SQL and returning results as JSON.
///
/// # Implementation Notes
///
/// In fraiseql-server, the PostgresAdapter from fraiseql-core should be wrapped
/// to implement this trait by delegating to its execute_raw_query method.
///
/// # Example
///
/// ```rust,ignore
/// // Implemented by a wrapper in fraiseql-server
/// struct FlightDatabaseAdapter {
///     inner: Arc<PostgresAdapter>,
/// }
///
/// impl DatabaseAdapter for FlightDatabaseAdapter {
///     async fn execute_raw_query(
///         &self,
///         sql: &str,
///     ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
///         self.inner.execute_raw_query(sql)
///             .await
///             .map_err(|e| DatabaseError::new(e.to_string()))
///     }
/// }
/// ```
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Execute a raw SQL query and return rows as JSON objects.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query string
    ///
    /// # Returns
    ///
    /// Vec of HashMap where each HashMap represents a row with column names as keys
    /// and column values as serde_json::Value
    ///
    /// # Errors
    ///
    /// Returns DatabaseError if the query fails for any reason.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>>;
}
