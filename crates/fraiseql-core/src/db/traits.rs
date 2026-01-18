//! Database adapter trait definitions.

use async_trait::async_trait;

use super::{
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};
use crate::error::Result;

/// Database adapter for executing queries against views.
///
/// This trait abstracts over different database backends (PostgreSQL, MySQL, SQLite, SQL Server).
/// All implementations must support:
/// - Executing simple WHERE queries against views
/// - Returning JSONB data from the `data` column
/// - Connection pooling and health checks
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example(adapter: impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
/// // Build WHERE clause
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// // Execute query
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), None, None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    /// Execute a WHERE query against a view and return JSONB rows.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "v_user", "v_post")
    /// * `where_clause` - Optional WHERE clause AST
    /// * `limit` - Optional row limit (for pagination)
    /// * `offset` - Optional row offset (for pagination)
    ///
    /// # Returns
    ///
    /// Vec of JSONB values from the `data` column.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    /// Returns `FraiseQLError::ConnectionPool` if connection pool is exhausted.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::db::DatabaseAdapter;
    /// # async fn example(adapter: impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple query without WHERE clause
    /// let all_users = adapter
    ///     .execute_where_query("v_user", None, Some(10), Some(0))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>>;

    /// Get database type (for logging/metrics).
    ///
    /// Used to identify which database backend is in use.
    fn database_type(&self) -> DatabaseType;

    /// Health check - verify database connectivity.
    ///
    /// Executes a simple query (e.g., `SELECT 1`) to verify the database is reachable.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if health check fails.
    async fn health_check(&self) -> Result<()>;

    /// Get connection pool metrics.
    ///
    /// Returns current statistics about the connection pool:
    /// - Total connections
    /// - Idle connections
    /// - Active connections
    /// - Waiting requests
    fn pool_metrics(&self) -> PoolMetrics;

    /// Execute raw SQL query and return rows as JSON objects.
    ///
    /// Used for aggregation queries where we need full row data, not just JSONB column.
    ///
    /// # Arguments
    ///
    /// * `sql` - Raw SQL query to execute
    ///
    /// # Returns
    ///
    /// Vec of rows, where each row is a HashMap of column name to JSON value.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::db::DatabaseAdapter;
    /// # async fn example(adapter: impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// let sql = "SELECT category, SUM(revenue) as total FROM tf_sales GROUP BY category";
    /// let rows = adapter.execute_raw_query(sql).await?;
    /// for row in rows {
    ///     println!("Category: {}, Total: {}", row["category"], row["total"]);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>>;

    /// Get database capabilities.
    ///
    /// Returns information about what features this database supports,
    /// including collation strategies and limitations.
    ///
    /// # Returns
    ///
    /// `DatabaseCapabilities` describing supported features.
    fn capabilities(&self) -> DatabaseCapabilities {
        DatabaseCapabilities::from_database_type(self.database_type())
    }
}

/// Database capabilities and feature support.
///
/// Describes what features a database backend supports, allowing the runtime
/// to adapt behavior based on database limitations.
#[derive(Debug, Clone, Copy)]
pub struct DatabaseCapabilities {
    /// Database type.
    pub database_type: DatabaseType,

    /// Supports locale-specific collations.
    pub supports_locale_collation: bool,

    /// Requires custom collation registration.
    pub requires_custom_collation: bool,

    /// Recommended collation provider.
    pub recommended_collation: Option<&'static str>,
}

impl DatabaseCapabilities {
    /// Create capabilities from database type.
    #[must_use]
    pub const fn from_database_type(db_type: DatabaseType) -> Self {
        match db_type {
            DatabaseType::PostgreSQL => Self {
                database_type:             db_type,
                supports_locale_collation: true,
                requires_custom_collation: false,
                recommended_collation:     Some("icu"),
            },
            DatabaseType::MySQL => Self {
                database_type:             db_type,
                supports_locale_collation: false,
                requires_custom_collation: false,
                recommended_collation:     Some("utf8mb4_unicode_ci"),
            },
            DatabaseType::SQLite => Self {
                database_type:             db_type,
                supports_locale_collation: false,
                requires_custom_collation: true,
                recommended_collation:     Some("NOCASE"),
            },
            DatabaseType::SQLServer => Self {
                database_type:             db_type,
                supports_locale_collation: true,
                requires_custom_collation: false,
                recommended_collation:     Some("Latin1_General_100_CI_AI_SC_UTF8"),
            },
        }
    }

    /// Get collation strategy description.
    #[must_use]
    pub const fn collation_strategy(&self) -> &'static str {
        match self.database_type {
            DatabaseType::PostgreSQL => "ICU collations (locale-specific)",
            DatabaseType::MySQL => "UTF8MB4 collations (general)",
            DatabaseType::SQLite => "NOCASE (limited)",
            DatabaseType::SQLServer => "Language-specific collations",
        }
    }
}
