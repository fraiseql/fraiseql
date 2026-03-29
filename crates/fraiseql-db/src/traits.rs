//! Database adapter trait definitions.

use std::future::Future;

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};

use super::{
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};
use crate::types::sql_hints::{OrderByClause, SqlProjectionHint};

/// Result from a relay pagination query, containing rows and an optional total count.
#[derive(Debug, Clone)]
pub struct RelayPageResult {
    /// The page of JSONB rows (already trimmed to the requested page size).
    pub rows:        Vec<JsonbValue>,
    /// Total count of matching rows (only populated when requested via `include_total_count`).
    pub total_count: Option<u64>,
}

/// How a database adapter executes mutations.
///
/// Adapters that support stored procedures (PostgreSQL, MySQL, SQL Server) use
/// [`FunctionCall`](MutationStrategy::FunctionCall). Adapters without stored procedure
/// support (SQLite, DuckDB) use [`DirectSql`](MutationStrategy::DirectSql) to execute
/// INSERT/UPDATE/DELETE statements directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutationStrategy {
    /// Call a named database function/stored procedure.
    ///
    /// Builds `SELECT * FROM "fn_name"($1, $2, ...)` (PostgreSQL) or equivalent.
    FunctionCall,
    /// Execute direct SQL statements (INSERT/UPDATE/DELETE … RETURNING *).
    DirectSql,
}

/// The kind of write operation for a direct-SQL mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirectMutationOp {
    /// INSERT INTO … RETURNING *
    Insert,
    /// UPDATE … SET … WHERE pk = ? RETURNING *
    Update,
    /// DELETE FROM … WHERE pk = ? RETURNING *
    Delete,
}

/// Structured context for direct-SQL mutation execution.
///
/// Carries all the information a [`DirectSql`](MutationStrategy::DirectSql) adapter
/// needs to build and execute the mutation statement without parsing function names.
#[derive(Debug)]
pub struct DirectMutationContext<'a> {
    /// The kind of mutation (INSERT, UPDATE, DELETE).
    pub operation:      DirectMutationOp,
    /// Target table name (will be identifier-quoted by the adapter).
    pub table:          &'a str,
    /// Column names from client-supplied mutation arguments, in positional order.
    ///
    /// For UPDATE/DELETE the first column is the primary key (used in the WHERE clause).
    pub columns:        &'a [String],
    /// All argument values: client args followed by inject params, in positional order.
    pub values:         &'a [serde_json::Value],
    /// Column names from server-injected parameters (JWT claims etc.).
    ///
    /// These are appended after `columns` in the SQL statement:
    /// - INSERT: additional columns in the INSERT
    /// - UPDATE: additional SET columns
    /// - DELETE: additional WHERE conditions (AND inject_col = ?)
    pub inject_columns: &'a [String],
    /// The GraphQL return type name (used as `entity_type` in the mutation response).
    pub return_type:    &'a str,
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
///
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
/// use fraiseql_db::{DatabaseAdapter, WhereClause, WhereOperator};
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
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None, None)
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
// POLICY: `#[async_trait]` placement for `DatabaseAdapter`
//
// `DatabaseAdapter` is used both generically (`Server<A: DatabaseAdapter>` in axum
// handlers, zero overhead via static dispatch) and dynamically (`Arc<dyn
// DatabaseAdapter + Send + Sync>` in federation, heap-boxed future per call).
//
// `#[async_trait]` is required on:
// - The trait definition (generates `Pin<Box<dyn Future + Send>>` return types)
// - Every `impl DatabaseAdapter for ConcreteType` block (generates the boxing)
// NOT required on callers (they see `Pin<Box<dyn Future + Send>>` from macro output).
//
// Why not native `async fn in trait` (Rust 1.75+)?
// Native dyn async trait does NOT propagate `+ Send` on generated futures. Tokio
// requires futures spawned with `tokio::spawn` to be `Send`. Until Return Type
// Notation (RFC 3425, tracking: github.com/rust-lang/rust/issues/109417) stabilises,
// `async_trait` is the only ergonomic path to `dyn DatabaseAdapter + Send + Sync`.
// Re-evaluate when Rust 1.90+ ships or when RTN is stabilised.
//
// MIGRATION TRACKING: async-trait → native async fn in trait
//
// Current status: BLOCKED on RFC 3425 (Return Type Notation)
// See: https://github.com/rust-lang/rfcs/pull/3425
//      https://github.com/rust-lang/rust/issues/109417
//
// Migration is safe when ALL of the following are true:
// 1. RTN with `+ Send` bounds is stable on rustc (e.g. `fn foo() -> impl Future + Send`)
// 2. FraiseQL MSRV is updated to that stabilising version
// 3. tokio::spawn() works with native dyn async trait objects (futures must be Send)
//
// Scope when criteria are met: 68 files (grep -rn "#\[async_trait\]" crates/)
// Effort: Medium (mostly mechanical — remove macro from impls, adjust trait defs)
// dynosaur was evaluated and rejected: does not propagate + Send (incompatible with Tokio)
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
    /// # use fraiseql_db::DatabaseAdapter;
    /// # async fn example(adapter: impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// // Simple query without WHERE clause
    /// let all_users = adapter
    ///     .execute_where_query("v_user", None, Some(10), Some(0), None)
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
        order_by: Option<&[OrderByClause]>,
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
    /// ```no_run
    /// // Requires: running PostgreSQL database and a DatabaseAdapter implementation.
    /// use fraiseql_db::types::SqlProjectionHint;
    /// use fraiseql_db::{traits::DatabaseAdapter, DatabaseType};
    ///
    /// # async fn example(adapter: &impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// let projection = SqlProjectionHint {
    ///     database: DatabaseType::PostgreSQL,
    ///     projection_template: "jsonb_build_object(\
    ///         'id', data->>'id', \
    ///         'name', data->>'name', \
    ///         'email', data->>'email'\
    ///     )".to_string(),
    ///     estimated_reduction_percent: 75,
    /// };
    ///
    /// let results = adapter
    ///     .execute_with_projection("v_user", Some(&projection), None, Some(100), None, None)
    ///     .await?;
    ///
    /// // results only contain id, name, email fields
    /// // 75% smaller than fetching all fields
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Example: Fallback (No Projection)
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database and a DatabaseAdapter implementation.
    /// # use fraiseql_db::traits::DatabaseAdapter;
    /// # async fn example(adapter: &impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// // For debugging or when projection not available
    /// let results = adapter
    ///     .execute_with_projection("v_user", None, None, Some(100), None, None)
    ///     .await?;
    ///
    /// // Equivalent to execute_where_query() - returns full objects
    /// # Ok(())
    /// # }
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
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
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
    /// # use fraiseql_db::DatabaseAdapter;
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

    /// Execute a parameterized aggregate SQL query (GROUP BY / HAVING / window).
    ///
    /// `sql` contains `$N` (PostgreSQL), `?` (MySQL / SQLite), or `@P1` (SQL Server)
    /// placeholders for string and array values; numeric and NULL values may be inlined.
    /// `params` are the corresponding values in placeholder order.
    ///
    /// Unlike `execute_raw_query`, this method accepts bind parameters so that
    /// user-supplied filter values never appear as string literals in the SQL text,
    /// eliminating the injection risk that `escape_sql_string` mitigated previously.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL with placeholders generated by
    ///   `AggregationSqlGenerator::generate_parameterized`
    /// * `params` - Bind parameters in placeholder order
    ///
    /// # Returns
    ///
    /// Vec of rows, where each row is a `HashMap` of column name to JSON value.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on execution failure.
    /// Returns `FraiseQLError::Database` on adapters that do not support raw SQL
    /// (e.g., [`crate::fraiseql_wire_adapter::FraiseWireAdapter`]).
    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
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

    /// Returns the mutation execution strategy for this adapter.
    ///
    /// Adapters that execute mutations via stored database functions (PostgreSQL,
    /// MySQL, SQL Server) return [`MutationStrategy::FunctionCall`] (the default).
    /// Adapters that execute direct SQL statements (SQLite) return
    /// [`MutationStrategy::DirectSql`].
    ///
    /// The executor uses this to choose between `execute_function_call` and
    /// `execute_direct_mutation`.
    fn mutation_strategy(&self) -> MutationStrategy {
        MutationStrategy::FunctionCall
    }

    /// Execute a direct SQL mutation (INSERT/UPDATE/DELETE) and return the result
    /// in `mutation_response` shape.
    ///
    /// Only adapters with [`MutationStrategy::DirectSql`] need to override this.
    /// The default implementation returns [`FraiseQLError::Unsupported`].
    ///
    /// The returned `HashMap` must contain the same columns as `app.mutation_response`:
    /// `status`, `entity`, `entity_type`, and optionally `entity_id`, `message`,
    /// `cascade`, `metadata`.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Structured mutation context with operation type, table, columns, and values
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` by default.
    /// Returns `FraiseQLError::Database` on SQL execution failure.
    async fn execute_direct_mutation(
        &self,
        _ctx: &DirectMutationContext<'_>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Err(FraiseQLError::Unsupported {
            message: "Direct SQL mutations are not supported by this adapter.".into(),
        })
    }

    /// Returns `true` if this adapter supports GraphQL mutation operations.
    ///
    /// **This is the authoritative mutation gate.** The executor checks this method
    /// before dispatching any mutation. Adapters that return `false` will cause
    /// mutations to fail with a clear `FraiseQLError::Validation` diagnostic instead
    /// of silently calling the unsupported `execute_function_call` default.
    ///
    /// Override to return `false` for read-only adapters (e.g., `SqliteAdapter`,
    /// `FraiseWireAdapter`). The compile-time [`MutationCapable`] marker trait
    /// complements this runtime check — see its documentation for the distinction.
    ///
    /// # Default
    ///
    /// Returns `true`. All adapters are assumed mutation-capable unless they override
    /// this method.
    fn supports_mutations(&self) -> bool {
        true
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
    /// * `tables` - Fact table names declared by the mutation (validated SQL identifiers; originate
    ///   from `MutationDefinition.invalidates_fact_tables`)
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

    /// Invalidate cache entries based on cascade response data.
    ///
    /// Called by the mutation executor after a successful cascade-enabled mutation.
    /// Parses the cascade JSONB to identify affected entities and invalidates
    /// corresponding cache entries.
    ///
    /// The default implementation is a no-op. `CachedDatabaseAdapter` overrides
    /// this to perform entity-level cache invalidation.
    ///
    /// # Returns
    ///
    /// The number of cache entries invalidated.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the cascade response fails to parse.
    async fn invalidate_cascade_entities(
        &self,
        _cascade_response: &serde_json::Value,
    ) -> Result<u64> {
        Ok(0)
    }

    /// Set transaction-scoped session variables before query execution.
    ///
    /// On PostgreSQL, this emits `SELECT set_config($1, $2, true)` for each
    /// variable, which behaves like `SET LOCAL` (transaction-scoped, auto-resets
    /// on commit/rollback).
    ///
    /// Views and functions can read these values via
    /// `current_setting('app.locale', true)`.
    ///
    /// The default implementation is a no-op. Only PostgreSQL supports session
    /// variables; other databases silently ignore this call.
    ///
    /// # Arguments
    ///
    /// * `variables` - Pairs of `(pg_guc_name, value)`. Names must be prefixed with `app.` or
    ///   `fraiseql.` to avoid collisions with built-in settings.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if `set_config()` fails.
    async fn set_session_variables(&self, _variables: &[(&str, &str)]) -> Result<()> {
        Ok(())
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
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "EXPLAIN not available for this database adapter".to_string(),
        })
    }

    /// Execute a query against a row-shaped `vr_*` view and return typed column vectors.
    ///
    /// Row-shaped views extract scalar fields from JSONB into native SQL columns,
    /// enabling efficient protobuf encoding without JSON deserialization. Each row
    /// is returned as a `Vec<ColumnValue>` with one element per requested column,
    /// in the same order as `columns`.
    ///
    /// # Arguments
    ///
    /// * `view` - Row-shaped view name (e.g., `"vr_user"`)
    /// * `columns` - Columns to select, with name and expected type
    /// * `where_clause` - Optional pre-built WHERE clause (from `GenericWhereGenerator`)
    /// * `order_by` - Optional pre-built ORDER BY clause
    /// * `limit` - Optional row limit
    /// * `offset` - Optional row offset
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` by default (adapters must opt in).
    /// Returns `FraiseQLError::Database` on query execution failure in implementations.
    #[cfg(feature = "grpc")]
    async fn execute_row_query(
        &self,
        _view: &str,
        _columns: &[crate::types::ColumnSpec],
        _where_clause: Option<&str>,
        _order_by: Option<&str>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<Vec<crate::types::ColumnValue>>> {
        Err(FraiseQLError::Unsupported {
            message: "Row-shaped view queries are not supported by this adapter. \
                      Use PostgreSQL, MySQL, or SQLite for gRPC transport support."
                .to_string(),
        })
    }

    /// Run `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` against a view with the
    /// same parameterized WHERE clause that `execute_where_query` would use.
    ///
    /// Unlike `explain_query`, this method uses **real bound parameters** and
    /// **actually executes the query** (ANALYZE mode), so the plan reflects
    /// PostgreSQL's runtime statistics for the given filter values.
    ///
    /// Only PostgreSQL supports this; other adapters return
    /// `FraiseQLError::Unsupported` by default.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "v_user")
    /// * `where_clause` - Optional filter (same as `execute_where_query`)
    /// * `limit` - Optional row limit
    /// * `offset` - Optional row offset
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on execution failure.
    /// Returns `FraiseQLError::Unsupported` for non-PostgreSQL adapters.
    async fn explain_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<serde_json::Value> {
        Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "EXPLAIN ANALYZE is not available for this database adapter. \
                      Only PostgreSQL supports explain_where_query."
                .to_string(),
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// - [`PostgresAdapter`](crate::postgres::PostgresAdapter) — full keyset pagination
/// - [`MySqlAdapter`](crate::mysql::MySqlAdapter) — keyset pagination with `?` params
/// - `CachedDatabaseAdapter<A>` — delegates to inner `A`
///
/// # Usage
///
/// Construct an `Executor` with
/// `Executor::new_with_relay` to enable relay
/// query execution. The bound `A: RelayDatabaseAdapter` is enforced at that call site.
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
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> impl Future<Output = Result<RelayPageResult>> + Send + 'a;
}

/// Marker trait for database adapters that support write operations.
///
/// Adapters that implement this trait signal that they can execute GraphQL mutations,
/// either via stored database functions ([`MutationStrategy::FunctionCall`]) or via
/// direct SQL statements ([`MutationStrategy::DirectSql`]).
///
/// # Role: documentation, generic bound, and compile-time enforcement
///
/// This trait serves three purposes:
/// 1. **Documentation**: it makes write-capable adapters self-describing at the type level.
/// 2. **Generic bounds**: code that only accepts write-capable adapters can constrain on `A:
///    MutationCapable` (e.g., `CachedDatabaseAdapter<A: MutationCapable>`).
/// 3. **Compile-time enforcement**: `Executor<A>::execute_mutation()` is only available when `A:
///    MutationCapable`.
///
/// The `execute()` method (which accepts raw GraphQL strings) still performs a runtime
/// `supports_mutations()` check because it cannot know the operation type at compile time.
/// For direct mutation dispatch, prefer `execute_mutation()` to get compile-time safety.
///
/// # Which adapters implement this?
///
/// | Adapter | Implements | Strategy |
/// |---------|-----------|----------|
/// | [`PostgresAdapter`](crate::postgres::PostgresAdapter) | ✅ Yes | `FunctionCall` |
/// | [`MySqlAdapter`](crate::mysql::MySqlAdapter) | ✅ Yes | `FunctionCall` |
/// | [`SqlServerAdapter`](crate::sqlserver::SqlServerAdapter) | ✅ Yes | `FunctionCall` |
/// | [`SqliteAdapter`](crate::sqlite::SqliteAdapter) | ✅ Yes | `DirectSql` |
/// | [`FraiseWireAdapter`](crate::fraiseql_wire_adapter::FraiseWireAdapter) | ❌ No — read-only wire protocol | — |
/// | `CachedDatabaseAdapter<A>` | ✅ When `A: MutationCapable` | Delegates |
pub trait MutationCapable: DatabaseAdapter {}

/// Type alias for boxed dynamic database adapters.
///
/// Used to store database adapters without generic type parameters in collections
/// or struct fields. The adapter type is determined at runtime.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::traits::{BoxDatabaseAdapter, DatabaseAdapter};
///
/// # fn example(adapter: impl DatabaseAdapter + 'static) {
/// let boxed: BoxDatabaseAdapter = Box::new(adapter);
/// # }
/// ```
pub type BoxDatabaseAdapter = Box<dyn DatabaseAdapter>;

/// Type alias for arc-wrapped dynamic database adapters.
///
/// Used for thread-safe, reference-counted storage of adapters in shared state.
///
/// # Example
///
/// ```no_run
/// use std::sync::Arc;
/// use fraiseql_db::traits::{ArcDatabaseAdapter, DatabaseAdapter};
///
/// # fn example(adapter: impl DatabaseAdapter + 'static) {
/// let shared: ArcDatabaseAdapter = Arc::new(adapter);
/// # }
/// ```
pub type ArcDatabaseAdapter = std::sync::Arc<dyn DatabaseAdapter>;

#[cfg(test)]
mod tests {
    #[allow(clippy::unwrap_used)] // Reason: test code
    #[test]
    fn database_adapter_is_send_sync() {
        // Static assertion: `dyn DatabaseAdapter` must be `Send + Sync`.
        // This test exists to catch accidental removal of `Send + Sync` bounds.
        // It only needs to compile — no runtime assertion required.
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn super::DatabaseAdapter>();
    }
}
