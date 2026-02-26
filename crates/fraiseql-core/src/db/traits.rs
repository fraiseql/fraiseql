//! Database adapter trait definitions.

use async_trait::async_trait;

use super::{
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};
use crate::{compiler::aggregation::OrderByClause, error::Result, schema::SqlProjectionHint};

/// Result from a relay pagination query, containing rows and an optional total count.
#[derive(Debug, Clone)]
pub struct RelayPageResult {
    /// The page of JSONB rows (already trimmed to the requested page size).
    pub rows:        Vec<JsonbValue>,
    /// Total count of matching rows (only populated when requested via `include_total_count`).
    pub total_count: Option<u64>,
}

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

    /// Execute a WHERE query with SQL field projection optimization.
    ///
    /// Projects only the requested fields at the database level, reducing network payload
    /// and JSON deserialization overhead by **40-55%** based on production measurements.
    ///
    /// This is the primary query execution method for optimized GraphQL queries.
    /// It automatically selects only the fields requested in the GraphQL query, avoiding
    /// unnecessary network transfer and deserialization of unused fields.
    ///
    /// # Automatic Projection
    ///
    /// In most cases, you don't call this directly. The `Executor` automatically:
    /// 1. Determines which fields the GraphQL query requests
    /// 2. Generates a `SqlProjectionHint` using database-specific SQL
    /// 3. Calls this method with the projection hint
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "v_user", "v_post")
    /// * `projection` - Optional SQL projection hint with field list
    ///   - `Some(hint)`: Use projection to select only requested fields
    ///   - `None`: Falls back to standard query (full JSONB column)
    /// * `where_clause` - Optional WHERE clause AST for filtering
    /// * `limit` - Optional row limit (for pagination)
    ///
    /// # Returns
    ///
    /// Vec of JSONB values, either:
    /// - Full objects (when projection is None)
    /// - Projected objects with only requested fields (when projection is Some)
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure, including:
    /// - Connection pool exhaustion
    /// - SQL execution errors
    /// - Type mismatches
    ///
    /// # Performance Characteristics
    ///
    /// When projection is provided (recommended):
    /// - **Latency**: 40-55% reduction vs full object fetch
    /// - **Network**: 40-55% smaller payload (proportional to unused fields)
    /// - **Throughput**: Maintains 250+ Kelem/s (elements per second)
    /// - **Memory**: Proportional to projected fields only
    ///
    /// Improvement scales with:
    /// - Percentage of unused fields (more unused = more improvement)
    /// - Size of result set (larger sets benefit more)
    /// - Network latency (network-bound queries benefit most)
    ///
    /// When projection is None:
    /// - Behavior identical to `execute_where_query()`
    /// - Returns full JSONB column
    /// - Used for compatibility/debugging
    ///
    /// # Database Support
    ///
    /// | Database | Status | Implementation |
    /// |----------|--------|-----------------|
    /// | PostgreSQL | ✅ Optimized | `jsonb_build_object()` |
    /// | MySQL | ⏳ Fallback | Server-side filtering (planned) |
    /// | SQLite | ⏳ Fallback | Server-side filtering (planned) |
    /// | SQL Server | ⏳ Fallback | Server-side filtering (planned) |
    ///
    /// # Example: Direct Usage (Advanced)
    ///
    /// ```rust,ignore
    /// use fraiseql_core::schema::SqlProjectionHint;
    /// use fraiseql_core::db::DatabaseAdapter;
    ///
    /// let projection = SqlProjectionHint {
    ///     database: "postgresql".to_string(),
    ///     projection_template: "jsonb_build_object(\
    ///         'id', data->>'id', \
    ///         'name', data->>'name', \
    ///         'email', data->>'email'\
    ///     )".to_string(),
    ///     estimated_reduction_percent: 75,
    /// };
    ///
    /// let results = adapter
    ///     .execute_with_projection("v_user", Some(&projection), None, Some(100))
    ///     .await?;
    ///
    /// // results only contain id, name, email fields
    /// // 75% smaller than fetching all fields
    /// ```
    ///
    /// # Example: Fallback (No Projection)
    ///
    /// ```rust,ignore
    /// // For debugging or when projection not available
    /// let results = adapter
    ///     .execute_with_projection("v_user", None, None, Some(100))
    ///     .await?;
    ///
    /// // Equivalent to execute_where_query() - returns full objects
    /// ```
    ///
    /// # See Also
    ///
    /// - `execute_where_query()` - Standard query without projection
    /// - `SqlProjectionHint` - Structure defining field projection
    /// - [Projection Optimization Guide](https://docs.fraiseql.rs/performance/projection-optimization.md)
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
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
    /// # Security Warning
    ///
    /// This method executes arbitrary SQL. **NEVER** pass untrusted input directly to this method.
    /// Always:
    /// - Use parameterized queries with bound parameters
    /// - Validate and sanitize SQL templates before execution
    /// - Only execute SQL generated by the FraiseQL compiler
    /// - Log SQL execution for audit trails
    ///
    /// # Arguments
    ///
    /// * `sql` - Raw SQL query to execute (must be safe/trusted)
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
    /// // Safe: SQL generated by FraiseQL compiler
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

    /// Execute a PostgreSQL function call and return all columns as rows.
    ///
    /// Builds `SELECT * FROM {function_name}($1, $2, ...)` with one positional placeholder per
    /// argument, executes it with the provided JSON values, and returns each result row as a
    /// `HashMap<column_name, json_value>`.
    ///
    /// Used by the mutation execution pathway to call stored procedures that return the
    /// `app.mutation_response` composite type
    /// `(status, message, entity_id, entity_type, entity jsonb, updated_fields text[],
    ///   cascade jsonb, metadata jsonb)`.
    ///
    /// # Arguments
    ///
    /// * `function_name` - Fully-qualified PostgreSQL function name (e.g. `fn_create_machine`)
    /// * `args` - Positional JSON arguments passed as `$1, $2, …` bind parameters
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>>;

    /// Execute a Relay cursor-based (keyset) pagination query.
    ///
    /// Builds and executes:
    /// ```sql
    /// -- Forward (after cursor):
    /// SELECT data FROM {view}
    /// WHERE {cursor_column} > $1          -- keyset condition
    /// ORDER BY {cursor_column} ASC
    /// LIMIT $2                             -- first + 1 to detect hasNextPage
    ///
    /// -- Backward (before cursor):
    /// SELECT data FROM {view}
    /// WHERE {cursor_column} < $1
    /// ORDER BY {cursor_column} DESC
    /// LIMIT $2
    /// ```
    ///
    /// The `data` JSONB column in the view is expected to contain the entity
    /// fields as well as `pk_{entity}` (the BIGINT cursor column value) so
    /// the caller can build edge cursors without a second query.
    ///
    /// # Arguments
    ///
    /// * `view` - SQL view name (from `QueryDefinition.sql_source`)
    /// * `cursor_column` - BIGINT column used for keyset ordering (e.g. `"pk_user"`)
    /// * `after` - Decoded cursor value for forward pagination (`after` argument)
    /// * `before` - Decoded cursor value for backward pagination (`before` argument)
    /// * `limit` - Number of rows to return (should be `first + 1` to probe hasNextPage)
    /// * `forward` - `true` for forward (ASC) pagination, `false` for backward (DESC)
    ///
    /// # Default implementation
    ///
    /// The default returns `FraiseQLError::Validation` with a "not supported" message.
    /// Override in adapters that support relay pagination (e.g. PostgreSQL).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on SQL execution failure, or
    /// `FraiseQLError::Validation` if the adapter does not support relay pagination.
    async fn execute_relay_page(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<i64>,
        before: Option<i64>,
        limit: u32,
        forward: bool,
    ) -> Result<Vec<crate::db::types::JsonbValue>> {
        let _ = (view, cursor_column, after, before, limit, forward);
        Err(crate::error::FraiseQLError::Validation {
            message: "Relay pagination is not supported by this database adapter".to_string(),
            path:    None,
        })
    }

    /// Extended relay pagination with filtering, sorting, and optional total count.
    ///
    /// This is the full-featured relay pagination method. It supports:
    /// - `where_clause`: filter rows before pagination
    /// - `order_by`: custom sort order (cursor column is always appended as tiebreaker)
    /// - `include_total_count`: compute `COUNT(*) OVER()` for the `totalCount` field
    ///
    /// # Default implementation
    ///
    /// Ignores `where_clause`, `order_by`, and `include_total_count`, delegating to
    /// `execute_relay_page`. Override in adapters that support the extended parameters.
    ///
    /// # Arguments
    ///
    /// * `view` - SQL view name
    /// * `cursor_column` - BIGINT column for keyset pagination
    /// * `after` - Decoded cursor for forward pagination
    /// * `before` - Decoded cursor for backward pagination
    /// * `limit` - Row count (should be `first + 1` to probe `hasNextPage`)
    /// * `forward` - `true` for ASC, `false` for DESC
    /// * `where_clause` - Optional filter condition
    /// * `order_by` - Optional custom sort (cursor column appended as tiebreaker)
    /// * `include_total_count` - Whether to compute total matching row count
    async fn execute_relay_page_v2(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<i64>,
        before: Option<i64>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult> {
        let _ = (where_clause, order_by, include_total_count);
        let rows = self
            .execute_relay_page(view, cursor_column, after, before, limit, forward)
            .await?;
        Ok(RelayPageResult {
            rows,
            total_count: None,
        })
    }

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
