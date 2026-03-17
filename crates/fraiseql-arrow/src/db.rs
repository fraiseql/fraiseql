//! Database adapter trait for Arrow Flight service.
//!
//! This module defines a minimal database adapter interface for executing
//! raw SQL queries and returning results as JSON. It's designed to be independent
//! of fraiseql-core to avoid circular dependencies.
//!
//! # Note
//!
//! This trait is simpler than `fraiseql_core::db::DatabaseAdapter` and only includes
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

/// Arrow Flight-specific database adapter for executing raw SQL queries.
///
/// This trait abstracts over different database backends (PostgreSQL, MySQL, SQLite, etc.)
/// and provides a minimal interface for executing raw SQL and returning results as JSON.
/// It is intentionally simpler than `fraiseql_db::DatabaseAdapter` — only the methods
/// needed for Arrow Flight streaming are required.
///
/// # Why a separate trait?
///
/// `fraiseql-arrow` must not take a compile-time dependency on `fraiseql-core` (to avoid
/// circular crate dependencies). `ArrowDatabaseAdapter` carries only what the Flight layer
/// needs; `fraiseql-server` wraps a core adapter to satisfy both traits.
///
/// # Example
///
/// ```no_run
/// // Requires: a running database and an implementation of this trait.
/// use std::collections::HashMap;
/// use fraiseql_arrow::db::{ArrowDatabaseAdapter, DatabaseError, DatabaseResult};
///
/// struct MyAdapter { /* database connection */ }
///
/// #[async_trait::async_trait]
/// impl ArrowDatabaseAdapter for MyAdapter {
///     async fn execute_raw_query(
///         &self,
///         sql: &str,
///     ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>> {
///         // Execute SQL and return rows as JSON maps
///         todo!("connect to database and execute: {}", sql)
///     }
/// }
/// ```
// Reason: used as dyn Trait (Arc<dyn ArrowDatabaseAdapter>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait ArrowDatabaseAdapter: Send + Sync {
    /// Execute a raw SQL query and return rows as JSON objects.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query string
    ///
    /// # Returns
    ///
    /// Vec of `HashMap` where each `HashMap` represents a row with column names as keys
    /// and column values as `serde_json::Value`
    ///
    /// # Errors
    ///
    /// Returns `DatabaseError` if the query fails for any reason.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> DatabaseResult<Vec<HashMap<String, serde_json::Value>>>;
}

/// Deprecated alias — use [`ArrowDatabaseAdapter`] instead.
#[deprecated(since = "2.2.0", note = "Use `ArrowDatabaseAdapter` instead")]
pub trait DatabaseAdapter: ArrowDatabaseAdapter {}
