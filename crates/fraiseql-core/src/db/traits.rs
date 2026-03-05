//! Database adapter trait definitions.

use async_trait::async_trait;

use super::{
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};
use crate::{error::{FraiseQLError, Result}, types::sql_hints::{OrderByClause, SqlProjectionHint}};

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
/// - Executing parameterized WHERE queries against views
/// - Returning JSONB data from the `data` column
/// - Connection pooling and health checks
/// - Row-level security (RLS) WHERE clauses
///
/// # Architecture
///
/// The adapter is the runtime interface to the database. It receives:
/// - View/table name (e.g., "v_user", "tf_sales")
/// - Parameterized WHERE clauses (AST form, not strings)
/// - Projection hints (for performance optimization)
/// - Pagination parameters (LIMIT/OFFSET)
///
/// And returns:
/// - JSONB rows from the `data` column (most operations)
/// - Arbitrary rows as HashMap (for aggregation queries)
/// - Mutation results from stored procedures
///
/// # Implementing a New Adapter
///
/// To add support for a new database (e.g., Oracle, Snowflake):
///
/// 1. **Create a new module** in `src/db/your_database/`
/// 2. **Implement the trait**:
///    ```rust,ignore
///    pub struct YourDatabaseAdapter { /* fields */ }
///
///    #[async_trait]
///    impl DatabaseAdapter for YourDatabaseAdapter {
///        async fn execute_where_query(&self, ...) -> Result<Vec<JsonbValue>> {
///            // 1. Build parameterized SQL from WhereClause AST
///            // 2. Execute with bound parameters (NO string concatenation)
///            // 3. Return JSONB from data column
///        }
///        // Implement other required methods...
///    }
///    ```
/// 3. **Add feature flag** to `Cargo.toml` (e.g., `feature = "your-database"`)
/// 4. **Copy structure from PostgreSQL adapter** — see `src/db/postgres/adapter.rs`
/// 5. **Add tests** in `tests/integration/your_database_test.rs`
///
/// # Security Requirements
///
/// All implementations MUST:
/// - **Never concatenate user input into SQL strings**
/// - **Always use parameterized queries** with bind parameters
/// - **Validate parameter types** before binding
/// - **Preserve RLS WHERE clauses** (never filter them out)
/// - **Return errors, not silently fail** (e.g., connection loss)
///
/// # Connection Management
///
/// - Use a connection pool (recommended: 20 connections default)
/// - Implement `health_check()` for ping-based monitoring
/// - Provide `pool_metrics()` for observability
/// - Handle stale connections gracefully
///
/// # Performance Characteristics
///
/// Expected throughput when properly implemented:
/// - **Simple queries** (single table, no WHERE): 250+ Kelem/s
/// - **Complex queries** (JOINs, multiple conditions): 50+ Kelem/s
/// - **Mutations** (stored procedures): 1-10 RPS (depends on procedure)
/// - **Relay pagination** (keyset cursors): 15-30ms latency
///
/// # Example: PostgreSQL Implementation
///
/// ```rust,ignore
/// use sqlx::postgres::PgPool;
/// use async_trait::async_trait;
///
/// pub struct PostgresAdapter {
///     pool: PgPool,
/// }
///
/// #[async_trait]
/// impl DatabaseAdapter for PostgresAdapter {
///     async fn execute_where_query(
///         &self,
///         view: &str,
///         where_clause: Option<&WhereClause>,
///         limit: Option<u32>,
///         offset: Option<u32>,
///     ) -> Result<Vec<JsonbValue>> {
///         // 1. Build SQL: SELECT data FROM {view} WHERE {where_clause} LIMIT {limit}
///         let mut sql = format!(r#"SELECT data FROM "{}""#, view);
///
///         // 2. Add WHERE clause (converts AST to parameterized SQL)
///         let params = if let Some(where_clause) = where_clause {
///             sql.push_str(" WHERE ");
///             let (where_sql, params) = build_where_sql(where_clause)?;
///             sql.push_str(&where_sql);
///             params
///         } else {
///             vec![]
///         };
///
///         // 3. Add LIMIT and OFFSET
///         if let Some(limit) = limit {
///             sql.push_str(" LIMIT ");
///             sql.push_str(&limit.to_string());
///         }
///         if let Some(offset) = offset {
///             sql.push_str(" OFFSET ");
///             sql.push_str(&offset.to_string());
///         }
///
///         // 4. Execute with bound parameters (NO string interpolation)
///         let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&sql)
///             .bind(&params[0])
///             .bind(&params[1])
///             // ... bind all parameters
///             .fetch_all(&self.pool)
///             .await?;
///
///         // 5. Extract JSONB and return
///         Ok(rows.into_iter().map(|(data,)| data).collect())
///     }
///
///     // Implement other required methods...
/// }
/// ```
///
/// # Example: Basic Usage
///
/// ```rust,no_run
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example(adapter: impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
/// // Build WHERE clause (AST, not string)
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// // Execute query with parameters
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None)
///     .await?;
///
/// println!("Found {} users matching filter", results.len());
/// # Ok(())
/// # }
/// ```
///
/// # See Also
///
/// - `WhereClause` — AST for parameterized WHERE clauses
/// - `RelayDatabaseAdapter` — Optional trait for keyset pagination
/// - `DatabaseCapabilities` — Feature detection for the adapter
/// - [Performance Guide](https://docs.fraiseql.rs/performance/database-adapters.md)
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
    /// ```ignore
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
    /// ```ignore
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

    /// Execute a database function call and return all columns as rows.
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
    /// * `function_name` - Fully-qualified function name (e.g. `fn_create_machine`)
    /// * `args` - Positional JSON arguments passed as `$1, $2, …` bind parameters
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    /// Returns `FraiseQLError::Unsupported` on adapters that do not support mutations
    /// (default implementation — see [`MutationCapable`]).
    async fn execute_function_call(
        &self,
        function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Err(FraiseQLError::Unsupported {
            message: format!(
                "Mutations via function calls are not supported by this adapter. \
                 Function '{function_name}' cannot be executed. \
                 Use PostgreSQL, MySQL, or SQL Server for mutation support."
            ),
        })
    }

    /// Bump fact table version counters after a successful mutation.
    ///
    /// Called by the executor when a mutation definition declares
    /// `invalidates_fact_tables`. For each listed table the version counter is
    /// incremented so that subsequent aggregation queries miss the cache and
    /// re-fetch fresh data.
    ///
    /// The default implementation is a **no-op**: adapters that are not cache-
    /// aware (e.g. `PostgresAdapter`, `SqliteAdapter`) simply return `Ok(())`.
    /// `CachedDatabaseAdapter` overrides this to call `bump_tf_version($1)` for
    /// every `FactTableVersionStrategy::VersionTable` table and update the
    /// in-process version cache.
    ///
    /// # Arguments
    ///
    /// * `tables` - Fact table names declared by the mutation (validated SQL
    ///   identifiers; originate from `MutationDefinition.invalidates_fact_tables`)
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the version-bump SQL function fails.
    async fn bump_fact_table_versions(&self, _tables: &[String]) -> Result<()> {
        Ok(())
    }

    /// Invalidate cached query results for the specified views.
    ///
    /// Called by the executor after a mutation succeeds, so that stale cache
    /// entries reading from modified views are evicted. The default
    /// implementation is a no-op; `CachedDatabaseAdapter` overrides this.
    ///
    /// # Returns
    ///
    /// The number of cache entries evicted.
    async fn invalidate_views(&self, _views: &[String]) -> Result<u64> {
        Ok(0)
    }

    /// Evict cache entries that contain the given entity UUID.
    ///
    /// Called by the executor after a successful UPDATE or DELETE mutation when
    /// the `mutation_response` includes an `entity_id`. Only cache entries whose
    /// entity-ID index contains the given UUID are removed; unrelated entries
    /// remain warm.
    ///
    /// The default implementation is a no-op. `CachedDatabaseAdapter` overrides
    /// this to perform the selective eviction.
    ///
    /// # Returns
    ///
    /// The number of cache entries evicted.
    async fn invalidate_by_entity(&self, _entity_type: &str, _entity_id: &str) -> Result<u64> {
        Ok(0)
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

    /// Run the database's `EXPLAIN` on a SQL statement without executing it.
    ///
    /// Returns a JSON representation of the query plan. The format is
    /// database-specific (e.g. PostgreSQL returns JSON, SQLite returns rows).
    ///
    /// The default implementation returns `Unsupported`.
    async fn explain_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        Err(crate::error::FraiseQLError::Unsupported {
            message: "EXPLAIN not available for this database adapter".to_string(),
        })
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

/// A typed cursor value for keyset (relay) pagination.
///
/// The cursor type is determined at compile time by `QueryDefinition::relay_cursor_type`
/// and used at runtime to choose the correct SQL comparison and cursor
/// encoding/decoding path.
#[derive(Debug, Clone, PartialEq)]
pub enum CursorValue {
    /// BIGINT primary key cursor (default, backward-compatible).
    Int64(i64),
    /// UUID cursor — bound as text and cast to `uuid` in SQL.
    Uuid(String),
}

/// Database adapter supertrait for adapters that implement Relay cursor pagination.
///
/// Only adapters that genuinely support keyset pagination need to implement this trait.
/// Non-implementing adapters carry no relay code at all — no stubs, no flags.
///
/// # Implementors
///
/// - [`PostgresAdapter`](crate::db::postgres::PostgresAdapter) — full keyset pagination
/// - [`MySqlAdapter`](crate::db::mysql::MySqlAdapter) — keyset pagination with `?` params
/// - [`CachedDatabaseAdapter<A>`](crate::cache::CachedDatabaseAdapter) — delegates to inner `A`
///
/// # Usage
///
/// Construct an [`Executor`](crate::runtime::Executor) with
/// [`Executor::new_with_relay`](crate::runtime::Executor::new_with_relay) to enable relay
/// query execution. The bound `A: RelayDatabaseAdapter` is enforced at that call site.
#[async_trait]
pub trait RelayDatabaseAdapter: DatabaseAdapter {
    /// Execute keyset (cursor-based) pagination against a JSONB view.
    ///
    /// # Arguments
    ///
    /// * `view`                — SQL view name (will be quoted before use)
    /// * `cursor_column`       — column used as the pagination key (e.g. `pk_user`, `id`)
    /// * `after`               — forward cursor: return rows where `cursor_column > after`
    /// * `before`              — backward cursor: return rows where `cursor_column < before`
    /// * `limit`               — row fetch count (pass `page_size + 1` to detect `hasNextPage`)
    /// * `forward`             — `true` → ASC order; `false` → DESC (re-sorted ASC via subquery)
    /// * `where_clause`        — optional user-supplied filter applied after the cursor condition
    /// * `order_by`            — optional custom sort; cursor column appended as tiebreaker
    /// * `include_total_count` — when `true`, compute the matching row count before LIMIT
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on SQL execution failure.
    async fn execute_relay_page(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult>;
}

/// Marker trait for database adapters that support write operations via stored functions.
///
/// Adapters that implement this trait can execute GraphQL mutations by calling stored
/// database functions (e.g. `fn_create_user`, `fn_update_order`).
///
/// # Which adapters implement this?
///
/// | Adapter | Implements |
/// |---------|-----------|
/// | [`PostgresAdapter`](crate::db::postgres::PostgresAdapter) | ✅ Yes |
/// | [`MySqlAdapter`](crate::db::mysql::MySqlAdapter) | ✅ Yes |
/// | [`SqlServerAdapter`](crate::db::sqlserver::SqlServerAdapter) | ✅ Yes |
/// | [`SqliteAdapter`](crate::db::sqlite::SqliteAdapter) | ❌ No — SQLite does not support stored-function mutations |
/// | [`FraiseWireAdapter`](crate::db::fraiseql_wire_adapter::FraiseWireAdapter) | ❌ No — read-only wire protocol |
/// | [`CachedDatabaseAdapter<A>`](crate::cache::CachedDatabaseAdapter) | ✅ When `A: MutationCapable` |
///
/// # Compile-time enforcement
///
/// The mutation executor requires `A: MutationCapable`. Code that attempts to run mutations
/// against a `SqliteAdapter` or `FraiseWireAdapter` will fail to compile, surfacing the
/// limitation before any runtime behavior.
///
/// # Usage
///
/// SQLite is suitable for read-only development and testing. Use PostgreSQL, MySQL, or
/// SQL Server when your schema includes mutations.
pub trait MutationCapable: DatabaseAdapter {}
