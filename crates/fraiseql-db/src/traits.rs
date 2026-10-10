//! Database adapter trait definitions.
//!
//! The main [`DatabaseAdapter`] trait lives in this file. Supporting types
//! (`RelayPageResult`, enums, type aliases) are in
//! the `adapter_types` submodule.

mod adapter_types;
mod composed_read;
mod mutations;
mod relay;

use std::sync::Arc;

pub use adapter_types::*;
use async_trait::async_trait;
pub use composed_read::{
    COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedKeyset, ComposedLevel,
    EmbedShape, EmbedSource, LevelKeys, composed_read_unsupported,
};
use fraiseql_error::{FraiseQLError, Result};
pub use mutations::{DERIVED_CASCADE_KEY, DerivedCascade, WriteMode, WriteRequest, Writer};
pub use relay::RelayDatabaseAdapter;

/// A violated constraint as the catalog describes it (#1531): see
/// [`DatabaseAdapter::describe_constraint`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConstraintDescription {
    /// The constraint's (or unique index's) name; `None` when nothing names it.
    pub name:    Option<String>,
    /// Its key columns in order; `None` when they cannot be read without guessing.
    pub columns: Option<Vec<String>>,
}

use crate::{
    types::{
        DatabaseType, JsonbValue, PoolMetrics, ReadRouting,
        sql_hints::{OrderByClause, SqlProjectionHint},
    },
    where_clause::WhereClause,
};

/// Adjudicates the rows a mutation function returned, from **inside** the
/// transaction that produced them and before it commits.
///
/// `Ok(())` commits the transaction; `Err(e)` rolls it back and `e` reaches the
/// caller. See [`Writer::execute_write`] for why a write needs a decision seam this late
/// (#1353).
///
/// Synchronous by construction: the only decision taken here today is the field
/// authorizer's, whose `authorize_field` is itself synchronous, and holding an open
/// transaction across an arbitrary `await` would pin a pooled connection for as long
/// as app-supplied policy code chose to take.
pub type MutationRowGate<'a> = &'a (
        dyn Fn(&[std::collections::HashMap<String, serde_json::Value>]) -> Result<()> + Send + Sync
    );

/// The framework-owned change-log row the mutation executor writes in-txn.
///
/// Carries only the fields the adapter cannot derive from the
/// `app.mutation_response` row it already holds: the DML verb and a NOT-NULL
/// `object_type` fallback. The changed-entity identity + payload (`object_id`,
/// `object_data`, `updated_fields`, `cascade`) are read from the function's own
/// returned row inside the same transaction (see [`Writer::execute_write`]).
///
/// This is the Change Spine transactional-outbox contract. Beyond the
/// `object_type`/`modification_type` + changed-entity columns, it stamps the
/// envelope: `tenant_id` (carried here, from `SecurityContext`),
/// `trace_id` (the W3C trace id of the originating request), `schema_version`
/// (the compiled schema's content hash — a per-deployment constant),
/// `trace_context` (the full W3C trace context as JSON), `actor_type` /
/// `acting_for` (the request's actor classification and, for a delegated agent,
/// the underlying human — #390), `commit_time` (`clock_timestamp()` at INSERT),
/// and `seq` (the table's `SEQUENCE` default).
#[derive(Debug, Clone, Copy)]
pub struct ChangeLogWrite<'a> {
    /// NOT-NULL fallback for `object_type` when the row's `entity_type` is NULL.
    /// Sourced from `MutationDefinition.return_type` (always present).
    pub object_type:       &'a str,
    /// The DML verb written to `modification_type` (e.g. `"INSERT"`,
    /// `"UPDATE"`, `"DELETE"`, `"CUSTOM"`), from `MutationOperation`.
    pub modification_type: &'a str,
    /// The tenant partition stamp written to the `tenant_id UUID` column — the
    /// Trinity public-facing identifier, read from `SecurityContext.tenant_id`
    /// at write time and **never** reconstructed from connection / RLS state
    /// (RLS is PG-only; out-of-session spine consumers bypass it, so the row
    /// must carry tenant identity explicitly). `None` (→ SQL NULL) for an
    /// unauthenticated request, a request with no tenant, or a tenant
    /// identifier that is not a UUID.
    pub tenant_id:         Option<uuid::Uuid>,
    /// The W3C trace id of the originating request, written to the `trace_id`
    /// column so an outbox row links back to its distributed trace (the #392
    /// perf tooling surfaces it as the investigation handle). Read from the
    /// request's `traceparent` header at write time; `None` (→ SQL NULL) for a
    /// request without a trace context — e.g. an unauthenticated mutation, which
    /// carries no `SecurityContext` to stamp.
    pub trace_id:          Option<&'a str>,
    /// The compiled schema's version written to the `schema_version` column so an
    /// outbox row records which deployment produced it — the replay /
    /// zero-downtime correctness handle for #378 (reject a row replayed under a
    /// different schema). A per-deployment constant derived from the compiled
    /// schema (`CompiledSchema::content_hash()`), **not** from the request, so it
    /// changes on any schema change. `None` (→ SQL NULL) for producers with no
    /// compiled schema in scope — cooperative external producers (ETL) and the
    /// non-PostgreSQL no-op path.
    pub schema_version:    Option<&'a str>,
    /// The originating request's **full W3C trace context** as a JSON object
    /// (`{version, trace_id, parent_id, trace_flags, tracestate?}`), written to the
    /// `trace_context` JSONB column so a row carries enough to re-propagate /
    /// reconstruct the distributed trace — not just the scalar `trace_id`. Carried
    /// here as pre-serialized JSON **text** (the adapter binds it to the JSONB
    /// column). Built from the request's `traceparent` / `tracestate` headers at
    /// write time; `None` (→ SQL NULL) for a request without a valid trace context,
    /// consistent with `trace_id`.
    pub trace_context:     Option<&'a str>,
    /// The request's actor classification written to the `actor_type` column (the
    /// `snake_case` `ActorType` token: `"human_user"`, `"service_account"`,
    /// `"ai_agent"`, `"system_job"`), from `SecurityContext.actor_type()` at write
    /// time (#390). `None` (→ SQL NULL) for a request with no `SecurityContext` to
    /// stamp (an unauthenticated mutation), or a cooperative external producer.
    pub actor_type:        Option<&'a str>,
    /// For a delegated agent request, the **underlying human** the agent acts for
    /// — the public-facing UUID, written to the `acting_for UUID` column from
    /// `SecurityContext.acting_for()` (#390). Mirrors `tenant_id`'s UUID shape so
    /// it is stamped without a DB lookup. `None` (→ SQL NULL) for a non-delegated
    /// request, an unauthenticated mutation, or a subject that is not UUID-shaped.
    pub acting_for:        Option<uuid::Uuid>,
    /// The ingress transport the originating request declared (#376), e.g.
    /// `"mcp"` — merged into the row's `extra_metadata` JSONB as
    /// `{"transport": …}` so MCP-originated (and, later, other
    /// transport-tagged) writes are queryable
    /// (`extra_metadata->>'transport' = 'mcp'`). From
    /// `SecurityContext.transport()` at write time; `None` omits the key —
    /// for the HTTP GraphQL path (which does not stamp one today), an
    /// unauthenticated mutation, or a cooperative producer.
    pub transport:         Option<&'a str>,
    /// Whether this outbox write also records the changed entity's **pre-image**
    /// (before-state) into the `object_data_before JSONB` column, sourced from the
    /// function's own `entity_before` (the after-image comes from `entity`). Set
    /// from `MutationDefinition.changelog_pre_image`; `false` (the default) leaves
    /// `object_data_before` out of the INSERT entirely (NULL), byte-for-byte
    /// today's behavior. The changed-entity payload itself is read from the
    /// returned row inside the outbox CTE, so this flag only selects which SQL
    /// form the adapter emits.
    pub pre_image:         bool,
}

impl<'a> ChangeLogWrite<'a> {
    /// Build a change-log write descriptor with no envelope stamps (`tenant_id`,
    /// `trace_id`, `schema_version`, `trace_context`, `actor_type` and
    /// `acting_for` NULL). Chain [`with_tenant_id`](Self::with_tenant_id) /
    /// [`with_trace_id`](Self::with_trace_id) /
    /// [`with_schema_version`](Self::with_schema_version) /
    /// [`with_trace_context`](Self::with_trace_context) /
    /// [`with_actor_type`](Self::with_actor_type) /
    /// [`with_acting_for`](Self::with_acting_for) to stamp them.
    #[must_use]
    pub const fn new(object_type: &'a str, modification_type: &'a str) -> Self {
        Self {
            object_type,
            modification_type,
            tenant_id: None,
            trace_id: None,
            schema_version: None,
            trace_context: None,
            actor_type: None,
            acting_for: None,
            transport: None,
            pre_image: false,
        }
    }

    /// Stamp the tenant partition id (the Trinity public-facing UUID) onto the
    /// outbox row. `None` leaves `tenant_id` NULL — for system / unauthenticated
    /// rows, or a tenant identifier that is not UUID-shaped.
    #[must_use]
    pub const fn with_tenant_id(mut self, tenant_id: Option<uuid::Uuid>) -> Self {
        self.tenant_id = tenant_id;
        self
    }

    /// Stamp the originating request's W3C trace id onto the outbox row. `None`
    /// leaves `trace_id` NULL — for a request with no trace context.
    #[must_use]
    pub const fn with_trace_id(mut self, trace_id: Option<&'a str>) -> Self {
        self.trace_id = trace_id;
        self
    }

    /// Stamp the compiled schema's version (its content hash) onto the outbox
    /// row. `None` leaves `schema_version` NULL — for producers with no compiled
    /// schema in scope (cooperative external producers, the non-PostgreSQL no-op
    /// path).
    #[must_use]
    pub const fn with_schema_version(mut self, schema_version: Option<&'a str>) -> Self {
        self.schema_version = schema_version;
        self
    }

    /// Stamp the originating request's full W3C trace context (pre-serialized JSON
    /// text) onto the outbox row's `trace_context` JSONB column. `None` leaves it
    /// NULL — for a request with no valid trace context, or a non-PostgreSQL
    /// no-op / cooperative producer.
    #[must_use]
    pub const fn with_trace_context(mut self, trace_context: Option<&'a str>) -> Self {
        self.trace_context = trace_context;
        self
    }

    /// Stamp the request's actor classification (the `snake_case` `ActorType`
    /// token) onto the outbox row's `actor_type` column (#390). `None` leaves it
    /// NULL — for an unauthenticated mutation or a cooperative producer.
    #[must_use]
    pub const fn with_actor_type(mut self, actor_type: Option<&'a str>) -> Self {
        self.actor_type = actor_type;
        self
    }

    /// Stamp the delegated user's UUID (the human a delegated agent acts for) onto
    /// the outbox row's `acting_for` column (#390). `None` leaves it NULL — for a
    /// non-delegated request, an unauthenticated mutation, or a non-UUID subject.
    #[must_use]
    pub const fn with_acting_for(mut self, acting_for: Option<uuid::Uuid>) -> Self {
        self.acting_for = acting_for;
        self
    }

    /// Stamp the ingress transport (e.g. `"mcp"`) onto the outbox row's
    /// `extra_metadata.transport` key (#376). `None` omits the key — for a
    /// transport that does not declare itself, an unauthenticated mutation, or
    /// a cooperative producer.
    #[must_use]
    pub const fn with_transport(mut self, transport: Option<&'a str>) -> Self {
        self.transport = transport;
        self
    }

    /// Opt this outbox write into recording the changed entity's pre-image into
    /// the `object_data_before` column (from the function's `entity_before`). When
    /// `false` (the default), `object_data_before` is omitted from the INSERT
    /// entirely, byte-for-byte today's behavior. Set from
    /// `MutationDefinition.changelog_pre_image`.
    #[must_use]
    pub const fn with_pre_image(mut self, pre_image: bool) -> Self {
        self.pre_image = pre_image;
        self
    }
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
pub trait DatabaseAdapter: Send + Sync + 'static {
    /// Execute a WHERE query against a view and return JSONB rows.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "v_user", "v_post")
    /// * `where_clause` - Optional WHERE clause AST
    /// * `limit` - Optional row limit (for pagination)
    /// * `offset` - Optional row offset (for pagination)
    /// * `security_context` - Optional security context for RLS and caching decisions
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
    /// use fraiseql_db::traits::DatabaseAdapter;
    /// use fraiseql_db::DatabaseType;
    ///
    /// # async fn example(adapter: &impl DatabaseAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// let projection = SqlProjectionHint::new(
    ///     DatabaseType::PostgreSQL,
    ///     "jsonb_build_object(\
    ///         'id', data->>'id', \
    ///         'name', data->>'name', \
    ///         'email', data->>'email'\
    ///     )".to_string(),
    ///     75,
    /// );
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

    /// Like `execute_where_query` but returns the result wrapped in an `Arc`.
    ///
    /// The default implementation wraps the result of `execute_where_query` in a
    /// fresh `Arc`. `CachedDatabaseAdapter` overrides this to return the cached `Arc`
    /// directly — eliminating the full `Vec<JsonbValue>` clone that the non-`Arc`
    /// path requires on every cache hit.
    ///
    /// Callers on the hot query path should prefer this variant and borrow from the
    /// `Arc` via `&**arc` rather than taking ownership.
    ///
    /// # Errors
    ///
    /// Same errors as `execute_where_query`.
    async fn execute_where_query_arc(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        self.execute_where_query(view, where_clause, limit, offset, order_by)
            .await
            .map(Arc::new)
    }

    /// Like `execute_with_projection` but returns the result wrapped in an `Arc`.
    ///
    /// The default implementation wraps the result of `execute_with_projection` in a
    /// fresh `Arc`. `CachedDatabaseAdapter` overrides this to return the cached `Arc`
    /// directly — eliminating the full `Vec<JsonbValue>` clone that the non-`Arc`
    /// path requires on every cache hit.
    ///
    /// Parameters are passed in a `ProjectionRequest` struct (F043) so adapters
    /// and callers cannot misorder them.
    ///
    /// # Errors
    ///
    /// Same errors as `execute_with_projection`.
    async fn execute_with_projection_arc(
        &self,
        request: &ProjectionRequest<'_>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        self.execute_with_projection(
            request.view,
            request.projection,
            request.where_clause,
            request.limit,
            request.offset,
            request.order_by,
        )
        .await
        .map(Arc::new)
    }

    /// Get database type (for logging/metrics).
    ///
    /// Used to identify which database backend is in use.
    fn database_type(&self) -> DatabaseType;

    /// Whether this adapter applies session variables to the reads that carry them (#1115).
    ///
    /// The `*_with_session` defaults cannot: they delegate to the session-free read. Rather
    /// than drop the variables, which would read unscoped where an RLS policy reads
    /// `current_setting`, they refuse a non-empty set unless the adapter declares it applies
    /// them, by overriding them (the PostgreSQL adapter) or by being a test double. The
    /// server refuses at boot a schema that needs them (`[locale]`, session variables) on an
    /// adapter that does not.
    fn applies_session_variables(&self) -> bool {
        false
    }

    /// Whether reads may be served by a hot standby — read replicas are configured (#1390).
    ///
    /// A standby cannot read an UNLOGGED or temporary relation, so a server whose reads go
    /// to replicas checks at boot that none of its sources depends on one. A wrapper
    /// adapter must forward this to the adapter it wraps: inheriting the default would
    /// switch that check off.
    fn serves_reads_from_standbys(&self) -> bool {
        false
    }

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

    /// Execute a row-shaped query against a view, returning typed column values.
    ///
    /// Used by the gRPC transport for protobuf encoding of query results.
    /// The default implementation delegates to `execute_raw_query` and converts
    /// JSON results to `ColumnValue` vectors.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    async fn execute_row_query(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<Vec<crate::types::ColumnValue>>> {
        let sql = build_row_query_sql(view_name, where_sql, order_by, limit, offset);
        let results = self.execute_raw_query(&sql).await?;

        Ok(results.iter().map(|row| row_to_column_values(row, columns)).collect())
    }

    /// Connection-affine variant of [`execute_row_query`](Self::execute_row_query).
    ///
    /// See
    /// [`execute_where_query_arc_with_session`](Self::execute_where_query_arc_with_session)
    /// for the rationale: an RLS policy backed by `current_setting()` (#329) only sees
    /// the variables if they are applied transaction-locally on the **same** connection
    /// as the read.
    ///
    /// # Why the row shape needs its own pair
    ///
    /// The row-shaped read is the gRPC transport's source. Until #1351 it resolved its
    /// own predicate and never reached the engine, so it had no session variables to
    /// pin and none of these methods existed. Routing it through the direct-read
    /// chokepoint gives it `session_variables` like every other read — and a resolved
    /// value the read cannot apply is the failure mode the chokepoint exists to
    /// prevent, so it travels rather than being dropped.
    ///
    /// # Errors
    ///
    /// Same errors as [`execute_row_query`](Self::execute_row_query); additionally
    /// returns `FraiseQLError::Database` if `set_config` fails on any pair.
    async fn execute_row_query_with_session(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<Vec<crate::types::ColumnValue>>> {
        refuse_session_variables(self, session_vars)?;
        self.execute_row_query(view_name, columns, where_sql, order_by, limit, offset)
            .await
    }

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
    /// (e.g., `FraiseWireAdapter`).
    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>>;

    /// Connection-affine variant of
    /// [`execute_parameterized_aggregate`](Self::execute_parameterized_aggregate).
    ///
    /// Applies `session_vars` transaction-locally on the same connection that
    /// runs the aggregate, so aggregate views backed by `current_setting()` RLS
    /// observe the configured values (fixes #329 for the aggregate path). The default
    /// ignores `session_vars`: a backend without transaction-local settings has none to
    /// apply.
    ///
    /// # Errors
    ///
    /// Same errors as [`execute_parameterized_aggregate`](Self::execute_parameterized_aggregate);
    /// additionally returns `FraiseQLError::Database` if `set_config` fails.
    async fn execute_parameterized_aggregate_with_session(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        refuse_session_variables(self, session_vars)?;
        self.execute_parameterized_aggregate(sql, params).await
    }

    /// Whether this adapter implements
    /// [`execute_composed_with_session`](Self::execute_composed_with_session) — whether a
    /// read can have related resources composed into it (the REST `?select=` embed).
    ///
    /// Asked twice, of one answer: when the REST transport is mounted, so a deployment
    /// whose adapter cannot serve embeds is told at boot rather than by its first embed;
    /// and by the engine before a composed read is sent, so the `501` a request gets is
    /// that same fact rather than a second one.
    ///
    /// **Implementing `execute_composed_with_session` obliges you to override this too**,
    /// and a wrapping adapter must forward both. Defaults to `false`: a capability is not
    /// something a backend should acquire by omission.
    fn supports_composed_reads(&self) -> bool {
        false
    }

    /// What a constraint violation's typed error reports about its constraint (#1531),
    /// resolved from the catalog where the error itself does not say.
    ///
    /// A not-null violation names its column only; its catalogued not-null constraint is
    /// found from that column. `columns` is the constraint's (or unique index's) key columns,
    /// in order, and `None` when they cannot be read off the catalog without guessing (an
    /// expression index). Read on the failure path, on a connection of its own: the
    /// mutation's transaction has already rolled back.
    ///
    /// The default reports the violation as the error gave it: its name, no columns. **A
    /// wrapping adapter must forward this**, or a deployment configured for full metadata
    /// loses the columns in silence.
    ///
    /// # Errors
    ///
    /// The database errors of the catalog read.
    async fn describe_constraint(
        &self,
        violation: &fraiseql_error::ConstraintViolation,
    ) -> Result<ConstraintDescription> {
        Ok(ConstraintDescription {
            name:    violation.name.clone(),
            columns: None,
        })
    }

    /// The physical health of every TVIEW one of `sources` reads (#1392), from
    /// `tviews.pg_tviews_profile()`; empty when there is none, or no `pg_tviews`.
    ///
    /// The default reports none. **A wrapping adapter must forward this**, or `/metrics`
    /// loses the TVIEW health in silence.
    ///
    /// # Errors
    ///
    /// The database errors of the catalog read.
    async fn tview_profiles(&self, _sources: &[String]) -> Result<Vec<TviewProfile>> {
        Ok(Vec::new())
    }

    /// Invalidate cached query results for the specified views.
    ///
    /// Called by the executor after a mutation succeeds, so that stale cache
    /// entries reading from modified views are evicted. The default
    /// implementation is a no-op; `CachedDatabaseAdapter` overrides this.
    ///
    /// View names are passed as `&[ViewName]` so the wrapper's `Arc<str>`
    /// backing is preserved across the call. Callers that hold a `String`
    /// can convert in place with `ViewName::from(...)`.
    ///
    /// # Returns
    ///
    /// The number of cache entries evicted.
    async fn invalidate_views(&self, _views: &[crate::ViewName]) -> Result<u64> {
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

    /// A snapshot of this adapter's query-result cache, or `None` if it has none.
    ///
    /// The operator surface for the cache that actually serves GraphQL queries.
    /// `None` means "this adapter does not cache" — distinct from `Some(stats)` with
    /// zero entries, which means "the cache is there and currently empty". The admin
    /// API reported the second as the first for every non-Arrow deployment (#941).
    ///
    /// The default implementation returns `None`; `CachedDatabaseAdapter` overrides it.
    fn result_cache_stats(&self) -> Option<crate::ResultCacheStats> {
        None
    }

    /// Drop every entry in this adapter's query-result cache.
    ///
    /// Returns the number of entries dropped, or `None` if this adapter has no cache
    /// — again so the caller can say "there is no such cache" rather than reporting a
    /// successful clear of nothing.
    ///
    /// The default implementation returns `Ok(None)`; `CachedDatabaseAdapter` overrides it.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError` if the underlying cache cannot be cleared.
    async fn clear_result_cache(&self) -> Result<Option<usize>> {
        Ok(None)
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

    /// Connection-affine variant of [`execute_where_query_arc`](Self::execute_where_query_arc).
    ///
    /// Applies `session_vars` transaction-locally on the same connection that
    /// runs the read, so PostgreSQL Row-Level-Security policies backed by
    /// `current_setting()` see the configured values (fixes #329). The default ignores
    /// `session_vars`: a backend without transaction-local settings has none to apply.
    ///
    /// # Errors
    ///
    /// Same errors as [`execute_where_query_arc`](Self::execute_where_query_arc); additionally
    /// returns `FraiseQLError::Database` if `set_config` fails on any pair.
    async fn execute_where_query_arc_with_session(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
        session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        refuse_session_variables(self, session_vars)?;
        self.execute_where_query_arc(view, where_clause, limit, offset, order_by).await
    }

    /// Count the rows of `view` matching `where_clause`, without materialising them.
    ///
    /// Backs both the GraphQL `<name>Count` sibling (#938) and the REST
    /// `Prefer: count=exact` header, so the two cannot answer the same question
    /// differently.
    ///
    /// `session_vars` are applied transaction-locally on the connection that
    /// runs the count, exactly as for the read it describes (#329). This is
    /// load-bearing rather than symmetric-for-neatness: under RLS, a count that
    /// ran without the session variables would report the *unfiltered* total
    /// beside a filtered page — a row-count oracle over rows the caller cannot
    /// read.
    ///
    /// # Default implementation
    ///
    /// The default fetches the matching rows and counts them. That is *correct*
    /// — same view, same predicate, same session — but it is O(rows) in memory,
    /// so an adapter that can push the count into the database should override
    /// it. `PostgresAdapter` does, with `SELECT COUNT(*)`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the query fails.
    async fn count_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<u64> {
        let rows = self
            .execute_where_query_arc_with_session(
                view,
                where_clause,
                None,
                None,
                None,
                session_vars,
                routing,
            )
            .await?;
        Ok(rows.len() as u64)
    }

    /// Connection-affine variant of
    /// [`execute_with_projection_arc`](Self::execute_with_projection_arc).
    ///
    /// See [`execute_where_query_arc_with_session`](Self::execute_where_query_arc_with_session) for
    /// the rationale.
    ///
    /// # Errors
    ///
    /// Same errors as [`execute_with_projection_arc`](Self::execute_with_projection_arc);
    /// additionally returns `FraiseQLError::Database` if `set_config` fails on any pair.
    async fn execute_with_projection_arc_with_session(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        refuse_session_variables(self, session_vars)?;
        self.execute_with_projection_arc(request).await
    }

    /// The same read as
    /// [`execute_with_projection_arc_with_session`](Self::execute_with_projection_arc_with_session),
    /// delivered row by row instead of as a materialised `Vec` (#958).
    ///
    /// Same view, same predicate, same session variables, same routing. The only
    /// difference is *when* the rows arrive and how much memory the caller needs
    /// to hold them: a streamed read is `O(1)` in the number of rows, so an export
    /// of a table larger than memory is possible where the collecting method would
    /// have to fail.
    ///
    /// # Why this exists as its own method
    ///
    /// A paginated re-execution loop (`LIMIT n OFFSET k`, `k += n`) delivers the
    /// same rows in bounded memory and needs no new trait method, and that is what
    /// the export surfaces did. It has two properties a stream does not:
    ///
    /// - it is `O(offset)` per batch, so exporting `N` rows costs `O(N²)` row scans;
    /// - each batch is its own snapshot, so a concurrent insert or delete between batches shifts
    ///   rows across the page boundary — silently duplicating a row into two batches, or skipping
    ///   one entirely.
    ///
    /// One statement over one portal has neither problem.
    ///
    /// # Default implementation
    ///
    /// Collects the read and replays it. That is *correct* — same rows, same order —
    /// but it holds the whole result set, so an adapter that can stream should
    /// override it. `PostgresAdapter` does.
    ///
    /// ⚠ A wrapping adapter (caching, instrumentation) that inherits this default
    /// converts every streaming caller back into a buffering one **without failing**:
    /// the rows are right and the memory bound is gone. A wrapper must forward this
    /// method explicitly.
    ///
    /// # Errors
    ///
    /// Returns the same errors as
    /// [`execute_with_projection_arc_with_session`](Self::execute_with_projection_arc_with_session).
    /// Note that an implementation that truly streams can only report the errors it
    /// has seen *so far* when this returns: a failure raised after the first row —
    /// a lost connection, a runtime cast error on a later row — surfaces as an `Err`
    /// item inside the stream, not from this call.
    async fn stream_with_projection(
        &self,
        request: &ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<JsonbRowStream> {
        let rows = self
            .execute_with_projection_arc_with_session(request, session_vars, routing)
            .await?;
        // Unwrap when this call owns the only reference (the usual case, since the
        // read above just built it) so replaying the buffer costs no deep clone.
        let rows = Arc::try_unwrap(rows).unwrap_or_else(|shared| (*shared).clone());
        Ok(Box::pin(futures::stream::iter(rows.into_iter().map(Ok))))
    }

    /// Execute a read that composes its embedded levels into **one** statement.
    ///
    /// The REST `?select=` embed used to be a parent read plus one sub-read per parent
    /// row per level; this is the same answer in one round trip, one snapshot and one
    /// pooled connection. See [`ComposedLevel`] for the call shape and for the
    /// `{"d": …, "e": …}` rows it returns.
    ///
    /// Session variables and routing mean what they mean for
    /// [`execute_with_projection_arc_with_session`](Self::execute_with_projection_arc_with_session).
    ///
    /// # Default implementation
    ///
    /// Refuses. Composing needs correlated `LATERAL` subqueries and JSON aggregation,
    /// which an adapter has to render for its own dialect; answering with anything
    /// else — the root rows without their embeds, say — would be a partial response
    /// indistinguishable from a parent with no related rows. A wrapping adapter must
    /// forward this method explicitly.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Unsupported` from the default; otherwise the errors of
    /// [`execute_with_projection_arc_with_session`](Self::execute_with_projection_arc_with_session).
    async fn execute_composed_with_session(
        &self,
        read: &ComposedLevel,
        _session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        Err(composed_read_unsupported(&read.view))
    }

    /// The same read as [`execute_row_query`](Self::execute_row_query), delivered
    /// row by row (#958).
    ///
    /// The column-shaped counterpart of
    /// [`stream_with_projection`](Self::stream_with_projection); see that method for
    /// why streaming and paginated re-execution are not interchangeable. This is the
    /// gRPC transport's source, which encodes one protobuf message per row and has
    /// no use for a `Vec` of them.
    ///
    /// # Default implementation
    ///
    /// Collects and replays, with the same caveat about wrapping adapters.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`execute_row_query`](Self::execute_row_query);
    /// see [`stream_with_projection`](Self::stream_with_projection) for which of
    /// them can still arrive after this call has returned `Ok`.
    async fn stream_row_query(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<ColumnRowStream> {
        let rows = self
            .execute_row_query(view_name, columns, where_sql, order_by, limit, offset)
            .await?;
        Ok(Box::pin(futures::stream::iter(rows.into_iter().map(Ok))))
    }

    /// Connection-affine variant of [`stream_row_query`](Self::stream_row_query).
    ///
    /// The streaming twin of
    /// [`execute_row_query_with_session`](Self::execute_row_query_with_session); see
    /// that method for why the row shape carries session variables at all. The
    /// streaming arm is where dropping them costs the most, because it is the arm
    /// that delivers an unbounded number of rows.
    ///
    /// # Errors
    ///
    /// Same errors as [`stream_row_query`](Self::stream_row_query); additionally
    /// returns `FraiseQLError::Database` if `set_config` fails on any pair.
    async fn stream_row_query_with_session(
        &self,
        view_name: &str,
        columns: &[crate::types::ColumnSpec],
        where_sql: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
        session_vars: &[(&str, &str)],
    ) -> Result<ColumnRowStream> {
        refuse_session_variables(self, session_vars)?;
        self.stream_row_query(view_name, columns, where_sql, order_by, limit, offset)
            .await
    }

    /// Retrieve query performance statistics from the database.
    ///
    /// Returns the top-N queries ordered by total execution time (descending).
    /// The exact data source depends on the backend:
    /// - PostgreSQL: `pg_stat_statements` (requires extension)
    /// - MySQL: `performance_schema.events_statements_summary_by_digest`
    /// - SQL Server: `sys.dm_exec_query_stats`
    /// - SQLite / Wire: empty (no stats available)
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of entries to return.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the stats query fails.
    async fn query_stats(&self, _limit: u32) -> Result<Vec<crate::types::QueryStatEntry>> {
        Ok(vec![])
    }

    /// Retrieve statistics for a single query by its ID.
    ///
    /// The default implementation fetches up to 1000 entries via
    /// [`query_stats`](Self::query_stats) and filters client-side.
    /// Backends with efficient single-query lookup (PostgreSQL, SQL Server)
    /// should override with a `WHERE` clause.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the underlying query fails.
    async fn query_stats_by_id(&self, id: &str) -> Result<Option<crate::types::QueryStatEntry>> {
        let stats = self.query_stats(1000).await?;
        Ok(stats.into_iter().find(|e| e.query_id == id))
    }

    /// Reset query performance statistics.
    ///
    /// Only PostgreSQL supports this (via `pg_stat_statements_reset()`).
    /// All other adapters return `Unsupported`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` for adapters that cannot reset stats.
    /// Returns `FraiseQLError::Database` if the reset command fails.
    async fn reset_query_stats(&self) -> Result<()> {
        Err(FraiseQLError::Unsupported {
            message: "Query stats reset is not supported by this database adapter".to_string(),
        })
    }

    /// Notify the adapter that the schema has changed.
    ///
    /// Called during hot-reload after the new schema has been validated.
    /// Adapters that maintain schema-dependent state (e.g. cache keyed by schema
    /// version) should clear or rebuild that state here.
    ///
    /// The default implementation is a no-op.
    fn on_schema_reload(&self) {}

    /// Run one operator-supplied statement under the bounds in `request` (#962).
    ///
    /// This is the only method on this trait whose SQL FraiseQL did not generate.
    /// It exists for the Studio SQL console, and it is deliberately the *narrowest*
    /// possible shape for that: a single statement, in a transaction whose mode,
    /// timeout, row budget and session identity are all decided by the caller and
    /// enforced by the database. See [`AdminSqlRequest`] for what each bound buys.
    ///
    /// # Default implementation
    ///
    /// **Refuses.** An adapter that has not implemented containment does not get
    /// to run arbitrary SQL by inheriting a default that "just works" — the
    /// convenient default here would be `execute_raw_query`, which has no
    /// transaction, no timeout, no row cap and no way to roll back. Every
    /// adapter that should serve this endpoint says so by overriding, and the
    /// endpoint answers "unsupported" for the rest.
    ///
    /// A wrapping adapter must forward explicitly for the same reason it forwards
    /// [`stream_with_projection`](Self::stream_with_projection): inheriting the
    /// default here turns a capable adapter into an incapable one, and the only
    /// symptom is a refusal nobody asked for.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` by default. Implementations return
    /// `FraiseQLError::Database` for anything PostgreSQL rejects — which
    /// includes the write refused by a `READ ONLY` transaction (SQLSTATE
    /// `25006`) and the statement cancelled by the timeout (`57014`).
    async fn execute_admin_sql(&self, _request: &AdminSqlRequest) -> Result<AdminSqlOutcome> {
        Err(FraiseQLError::Unsupported {
            message: "Operator-supplied SQL execution is not supported by this database adapter."
                .to_string(),
        })
    }
}

/// The `SELECT` behind [`DatabaseAdapter::execute_row_query`] and its streaming
/// sibling.
///
/// Shared rather than written twice: the two methods answer the same question and
/// a transport that got a different row set depending on whether it streamed would
/// be the hardest kind of bug to see. `where_sql` and `order_by` are compiler-
/// generated SQL fragments, as the method contract requires.
#[must_use]
pub fn build_row_query_sql(
    view_name: &str,
    where_sql: Option<&str>,
    order_by: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> String {
    use std::fmt::Write as _;

    let mut sql = format!("SELECT * FROM \"{view_name}\"");
    if let Some(w) = where_sql {
        sql.push_str(" WHERE ");
        sql.push_str(w);
    }
    if let Some(ob) = order_by {
        sql.push_str(" ORDER BY ");
        sql.push_str(ob);
    }
    if let Some(l) = limit {
        let _ = write!(sql, " LIMIT {l}");
    }
    if let Some(o) = offset {
        let _ = write!(sql, " OFFSET {o}");
    }
    sql
}

/// Decode one JSON-shaped row into the declared column order.
///
/// A column the row does not carry decodes to [`crate::types::ColumnValue::Null`]
/// rather than shortening the row: the transport encodes positionally against
/// `columns`.
#[must_use]
pub fn row_to_column_values<S: std::hash::BuildHasher>(
    row: &std::collections::HashMap<String, serde_json::Value, S>,
    columns: &[crate::types::ColumnSpec],
) -> Vec<crate::types::ColumnValue> {
    use crate::types::ColumnValue;

    columns
        .iter()
        .map(|col| {
            row.get(&col.name).map_or(ColumnValue::Null, |v| match v {
                serde_json::Value::Null => ColumnValue::Null,
                serde_json::Value::Bool(b) => ColumnValue::Boolean(*b),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        ColumnValue::Int64(i)
                    } else if let Some(f) = n.as_f64() {
                        ColumnValue::Float64(f)
                    } else {
                        ColumnValue::Text(n.to_string())
                    }
                },
                serde_json::Value::String(s) => ColumnValue::Text(s.clone()),
                other => ColumnValue::Json(other.to_string()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

/// Refuse `session_vars` on an adapter that cannot apply them (#1115): reading without
/// them would read unscoped.
pub(crate) fn refuse_session_variables<A: DatabaseAdapter + ?Sized>(
    adapter: &A,
    session_vars: &[(&str, &str)],
) -> Result<()> {
    if session_vars.is_empty() || adapter.applies_session_variables() {
        return Ok(());
    }
    Err(fraiseql_error::FraiseQLError::Unsupported {
        message: format!(
            "the {} adapter ({}) cannot apply session variables, and a read without them \
             would ignore every policy that reads them; refused rather than read unscoped",
            std::any::type_name::<A>(),
            adapter.database_type()
        ),
    })
}

/// One TVIEW's physical health, from `tviews.pg_tviews_profile()` (#1392).
///
/// The columns `fraiseql doctor` and `/metrics` read. The profile's columns are a stable
/// contract (`pg_tviews` `docs/reference/profile.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct TviewProfile {
    /// The TVIEW entity (`book` for `tv_book`).
    pub entity:        String,
    /// The schema-qualified `tv_*` table.
    pub tview:         String,
    /// HOT updates / updates since the statistics reset; `None` without updates.
    pub hot_ratio:     Option<f64>,
    /// Dead tuples, from `pg_stat_all_tables`.
    pub n_dead_tup:    Option<i64>,
    /// Updates since the statistics reset.
    pub n_tup_upd:     Option<i64>,
    /// HOT updates since the statistics reset.
    pub n_tup_hot_upd: Option<i64>,
    /// `pg_tviews`' advice for it; empty when it has none.
    pub warnings:      Vec<String>,
}
