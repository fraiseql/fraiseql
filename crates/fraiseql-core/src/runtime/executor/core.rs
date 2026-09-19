//! `Executor<A>` struct definition, constructors, and basic accessors.

use std::{collections::HashMap, sync::Arc};

use moka::sync::Cache as MokaCache;

use super::{
    context::ExecutorContext,
    runners,
    support::relay::{RelayDispatch, RelayDispatchImpl},
};
use crate::{
    backend::{
        AdminSqlOutcome, AdminSqlRequest, RelayDatabaseAdapter, ResultCacheStats,
        traits::DatabaseAdapter,
        types::{DatabaseType, PoolMetrics, QueryStatEntry},
    },
    cache::ViewName,
    error::Result,
    runtime::{QueryMatcher, QueryPlanner, RuntimeConfig, matcher::QueryMatch},
    schema::{CompiledSchema, IntrospectionResponses},
    security::SecurityContext,
};

/// Build the pre-computed introspection responses for a schema, with federation
/// `@inaccessible` fields filtered out of `__type`/`__schema`.
///
/// Shared by [`Executor::with_config`] and [`Executor::with_config_and_relay`] so the
/// relay-enabled and non-relay constructors apply identical introspection filtering — a
/// relay executor must never expose a field in introspection that the non-relay path
/// would hide (L-relay-inaccessible). The filter only affects `__type`/`__schema`; it does
/// not touch data responses or `_entities` resolution.
fn build_introspection(schema: &CompiledSchema) -> IntrospectionResponses {
    // `mut` is required by the `#[cfg(feature = "federation")]` block below.
    #[cfg_attr(not(feature = "federation"), allow(unused_mut))]
    let mut introspection = IntrospectionResponses::build(schema);

    #[cfg(feature = "federation")]
    if let Some(fed_meta) = schema.federation_metadata() {
        let inaccessible: HashMap<String, Vec<String>> = fed_meta
            .types
            .iter()
            .filter(|t| !t.inaccessible_fields.is_empty())
            .map(|t| (t.name.clone(), t.inaccessible_fields.clone()))
            .collect();
        introspection.filter_inaccessible(&inaccessible);
    }

    introspection
}

/// Resolve the GATE-1 validator for an executor (#379).
///
/// The embedder-installed `RuntimeConfig::query_validation` wins (it is the
/// programmatic API, preserved across hot-reloads). Otherwise the compiled
/// schema's declared `[validation]` limits are derived into a gate so the
/// operator's declared bound binds on **every** transport that reaches the
/// executor — MCP, the functions bridge, direct embedders — not only on the
/// `/graphql` HTTP stage, which applies the same limits independently.
///
/// Derivation enforces exactly what the schema declares: an undeclared depth or
/// complexity limit stays unbounded rather than acquiring a new default, and a
/// schema with no `[validation]` section derives no gate at all. An embedder
/// that must disable validation despite declared schema limits can install an
/// explicit all-`usize::MAX` config.
fn resolve_gate1(
    config: &RuntimeConfig,
    schema: &CompiledSchema,
) -> Option<crate::security::QueryValidator> {
    let effective = config.query_validation.clone().or_else(|| {
        let declared = schema.validation_config.as_ref()?;
        if declared.max_query_depth.is_none() && declared.max_query_complexity.is_none() {
            return None;
        }
        Some(crate::security::QueryValidatorConfig {
            max_depth:      declared.max_query_depth.map_or(usize::MAX, |d| d as usize),
            max_complexity: declared.max_query_complexity.map_or(usize::MAX, |c| c as usize),
            max_size_bytes: usize::MAX,
            max_aliases:    usize::MAX,
        })
    })?;
    Some(crate::security::QueryValidator::from_config(effective))
}

/// Maximum number of distinct query strings whose parsed ASTs are cached in memory.
///
/// 1 024 entries covers the full distinct-query vocabulary of any realistic workload.
/// Each entry holds an `Arc<(QueryType, Option<ParsedQuery>)>` — the AST is shared,
/// not duplicated.
const PARSE_CACHE_CAPACITY: u64 = 1_024;

/// Distinct introspection selection-set shapes to memoise.
///
/// Small on purpose: the realistic population is one shape per client tool.
const INTROSPECTION_PROJECTION_CAPACITY: u64 = 64;

/// PostgreSQL's identifier length limit (`NAMEDATALEN - 1`). A longer schema name is
/// silently truncated by the server, so a `DROP` built from one would target a
/// *different* schema than the caller named.
const MAX_PG_IDENTIFIER_LEN: usize = 63;

/// Query executor - executes compiled GraphQL queries.
///
/// This is the main entry point for runtime query execution.
/// It coordinates matching, planning, execution, and projection.
///
/// # Type Parameters
///
/// * `A` - The database adapter type (implements `DatabaseAdapter` trait)
///
/// # Ownership and Lifetimes
///
/// The executor holds owned references to schema and runtime data, with no borrowed pointers:
/// - `schema`: Owned `CompiledSchema` (immutable after construction)
/// - `adapter`: Shared via `Arc<A>` to allow multiple executors/tasks to use the same connection
///   pool
/// - `introspection`: Owned cached GraphQL schema responses
/// - `config`: Owned runtime configuration
///
/// **No explicit lifetimes required** - all data is either owned or wrapped in `Arc`,
/// so the executor can be stored in long-lived structures without lifetime annotations or
/// borrow-checker issues.
///
/// # Concurrency
///
/// `Executor<A>` is `Send + Sync` when `A` is `Send + Sync`. It can be safely shared across
/// threads and tasks without cloning:
/// ```no_run
/// // Requires: a live database adapter.
/// // See: tests/integration/ for runnable examples.
/// # use std::sync::Arc;
/// // let executor = Arc::new(Executor::new(schema, adapter));
/// // Can be cloned into multiple tasks
/// // let exec_clone = executor.clone();
/// // tokio::spawn(async move {
/// //     let result = exec_clone.execute(query, vars).await;
/// // });
/// ```
///
/// # Query Timeout
///
/// Queries are protected by the `query_timeout_ms` configuration in `RuntimeConfig` (default: 30s).
/// When a query exceeds this timeout, it returns `FraiseQLError::Timeout` without panicking.
/// Set `query_timeout_ms` to 0 to disable timeout enforcement.
pub struct Executor<A: DatabaseAdapter> {
    /// All shared state — schema, adapter, config, caches, relay.
    pub(super) ctx: Arc<ExecutorContext<A>>,
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Create new executor.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live PostgreSQL database.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::runtime::Executor;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let schema_json = r#"{"types":[],"queries":[]}"#;
    /// # let connection_string = "postgresql://localhost/mydb";
    /// let schema = CompiledSchema::from_json(schema_json, false)?;
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new(schema, Arc::new(adapter));
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn new(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config(schema, adapter, RuntimeConfig::default())
    }

    /// Create new executor with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    /// * `config` - Runtime configuration
    #[must_use]
    pub fn with_config(schema: CompiledSchema, adapter: Arc<A>, config: RuntimeConfig) -> Self {
        Self::build(schema, adapter, config, None)
    }

    /// The one construction path. Every public constructor and every rebuild funnels
    /// through here, so a new executor cannot differ from the others by an omitted
    /// field — which is the drift #750 set out to prevent by recording a closure.
    ///
    /// `relay` is passed in rather than built here because constructing a
    /// `RelayDispatchImpl` needs a `RelayDatabaseAdapter` bound that this impl block
    /// deliberately does not carry.
    fn build(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
        relay: Option<Arc<dyn RelayDispatch>>,
    ) -> Self {
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        // Build introspection responses at startup (zero-cost at runtime),
        // with `@inaccessible` fields filtered out. Shared with the relay
        // constructor so both paths apply the identical filtering (L-relay-inaccessible).
        let introspection = build_introspection(&schema);

        // Build O(1) node-type index: return_type → sql_source.
        // The first query with a matching return_type and a non-None sql_source wins
        // (consistent with the previous linear-scan behaviour).
        let mut node_type_index: HashMap<String, Arc<str>> = HashMap::new();
        for q in &schema.queries {
            if let Some(src) = q.sql_source.as_deref() {
                node_type_index.entry(q.return_type.clone()).or_insert_with(|| Arc::from(src));
            }
        }

        // Compute the schema version (content hash) once — it is stamped onto
        // every change-log outbox row and is too expensive to recompute per call.
        let schema_version: Arc<str> = Arc::from(schema.content_hash());

        let gate1 = resolve_gate1(&config, &schema);
        let ctx = Arc::new(ExecutorContext {
            schema,
            schema_version,
            adapter,
            relay,
            matcher,
            planner,
            config,
            introspection,
            node_type_index,
            gate1,
            parse_cache: MokaCache::new(PARSE_CACHE_CAPACITY),
            introspection_projections: MokaCache::new(INTROSPECTION_PROJECTION_CAPACITY),
            response_cache: None,
        });

        Self { ctx }
    }

    /// Return current connection pool metrics from the underlying database adapter.
    ///
    /// Values are sampled live on each call — not cached — so callers (e.g., the
    /// `/metrics` endpoint) always observe up-to-date pool health.
    #[must_use]
    pub fn pool_metrics(&self) -> PoolMetrics {
        self.ctx.pool_metrics()
    }

    /// Get the compiled schema.
    #[must_use]
    pub fn schema(&self) -> &CompiledSchema {
        &self.ctx.schema
    }

    /// Get runtime configuration.
    #[must_use]
    pub fn config(&self) -> &RuntimeConfig {
        &self.ctx.config
    }

    /// Whether this executor can dispatch relay (cursor) pagination.
    ///
    /// `true` only for executors built by [`Executor::new_with_relay`] or
    /// [`Executor::with_config_and_relay`]; a relay query issued against any
    /// other executor returns a `Validation` error. Exposed so a rebuild of the
    /// executor — a hot-reload, a per-tenant provision — can assert it preserved
    /// the capability rather than silently downgrading it (#750).
    #[must_use]
    pub fn relay_enabled(&self) -> bool {
        self.ctx.relay.is_some()
    }

    /// Which backend this executor is bound to.
    #[must_use]
    pub fn database_type(&self) -> DatabaseType {
        self.ctx.database_type()
    }

    /// Whether the backend is reachable.
    ///
    /// # Errors
    ///
    /// Returns the backend's own error when the probe fails.
    pub async fn health_check(&self) -> Result<()> {
        self.ctx.health_check().await
    }

    /// The slowest `limit` statements the backend is willing to report.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` on a backend with no statement-stats
    /// facility, or the backend's own error when the read fails.
    pub async fn query_stats(&self, limit: u32) -> Result<Vec<QueryStatEntry>> {
        self.ctx.query_stats(limit).await
    }

    /// One statement's stats by backend-assigned id.
    ///
    /// # Errors
    ///
    /// As [`Executor::query_stats`].
    pub async fn query_stats_by_id(&self, id: &str) -> Result<Option<QueryStatEntry>> {
        self.ctx.query_stats_by_id(id).await
    }

    /// Discard the backend's accumulated statement statistics.
    ///
    /// # Errors
    ///
    /// As [`Executor::query_stats`].
    pub async fn reset_query_stats(&self) -> Result<()> {
        self.ctx.reset_query_stats().await
    }

    /// Adapter-level result-cache counters, or `None` when no cache is active.
    #[must_use]
    pub fn result_cache_stats(&self) -> Option<ResultCacheStats> {
        self.ctx.result_cache_stats()
    }

    /// Evict every entry from the adapter-level result cache.
    ///
    /// `Ok(None)` means the backend has no such cache to clear.
    ///
    /// # Errors
    ///
    /// Returns the backend's own error when eviction fails.
    pub async fn clear_result_cache(&self) -> Result<Option<usize>> {
        self.ctx.clear_result_cache().await
    }

    /// Evict adapter-level result-cache entries derived from the given views.
    ///
    /// Returns the number of entries removed; `0` on a backend with no such cache.
    ///
    /// # Errors
    ///
    /// Returns the backend's own error when eviction fails.
    pub async fn invalidate_views(&self, views: &[ViewName]) -> Result<u64> {
        self.ctx.invalidate_views(views).await
    }

    /// The backend's plan for a statement, as its own JSON shape.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` on a backend with no `EXPLAIN`, or the
    /// backend's own error when planning fails.
    pub async fn explain_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        self.ctx.explain_query(sql, params).await
    }

    /// Run the admin-SQL route's bounded statement.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Unsupported` on a backend that declines admin SQL, or
    /// the backend's own error when the statement fails.
    pub async fn execute_admin_sql(&self, request: &AdminSqlRequest) -> Result<AdminSqlOutcome> {
        self.ctx.execute_admin_sql(request).await
    }

    /// Tell the backend the compiled schema changed, so it can drop anything it
    /// derived from the old one — a result cache keyed by the old views, say.
    ///
    /// Call this *before* [`Executor::rebuild_with`]: the backend is shared between
    /// the old executor and the new one, so anything stale it holds would otherwise
    /// outlive the swap.
    pub fn on_schema_reload(&self) {
        self.ctx.on_schema_reload();
    }

    /// Drop a tenant's PostgreSQL schema and everything in it.
    ///
    /// Takes the schema *name*, not a statement: the engine composes the DDL, so no
    /// caller-supplied SQL reaches the backend through this door. Deleting
    /// `Executor::adapter` removed the transports' general raw-SQL reach, and this is
    /// deliberately not a replacement for it — it does one thing.
    ///
    /// The name is re-validated here even though `fraiseql-server` validates the
    /// tenant key before deriving it. This is the interpolation site, so it is the
    /// site that has to be safe on its own; a guard that holds only while every
    /// caller remembers to validate is not a guard.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if `schema_name` is not a bare identifier
    /// (ASCII alphanumeric and underscore, non-empty, at most 63 bytes), and the
    /// backend's own error if the DDL fails.
    pub async fn drop_tenant_schema(&self, schema_name: &str) -> Result<()> {
        if schema_name.is_empty()
            || schema_name.len() > MAX_PG_IDENTIFIER_LEN
            || !schema_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(crate::error::FraiseQLError::validation(format!(
                "refusing to drop schema '{schema_name}': not a bare identifier"
            )));
        }
        self.ctx.execute_ddl(&format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE")).await
    }

    /// Build a new executor over *this* executor's backend, for a hot-reload or a
    /// re-provision.
    ///
    /// Supersedes the recorded-rebuilder closure of #750. That closure existed
    /// because relay dispatch needs a `RelayDatabaseAdapter` bound that is only in
    /// scope at the relay constructor, so a rebuild had to re-run the constructor
    /// that had it — and the recording could be wrong, which is what #750 guarded
    /// against by hand. Nothing needs re-running: `RelayDispatchImpl` holds the
    /// adapter and nothing schema-derived, so the *same* dispatch object is still
    /// correct for the new schema and is carried over as-is. A rebuild cannot
    /// downgrade a relay executor to a non-relay one, because there is no longer a
    /// step that could omit it.
    #[must_use]
    pub fn rebuild_with(&self, schema: CompiledSchema, config: RuntimeConfig) -> Self {
        Self::build(schema, Arc::clone(&self.ctx.adapter), config, self.ctx.relay.clone())
    }

    /// Return the number of entries currently held in the parsed-query AST cache.
    ///
    /// Exposed for testing only — callers outside `#[cfg(test)]` code should not
    /// rely on the exact count, which may lag by one maintenance cycle in moka.
    #[cfg(test)]
    #[must_use]
    pub fn parse_cache_entry_count(&self) -> u64 {
        self.ctx.parse_cache.entry_count()
    }

    /// Attach an executor-level response cache.
    ///
    /// When enabled, the executor caches the final projected response
    /// (after RBAC, projection, and envelope wrapping) to skip all
    /// redundant work on cache hits.
    ///
    /// # Panics
    ///
    /// Panics if called after the internal `Arc<ExecutorContext>` has been shared
    /// (i.e., after the executor has been cloned).  Always call this immediately
    /// after construction, before sharing the executor.
    #[must_use]
    pub fn with_response_cache(mut self, cache: Arc<crate::cache::ResponseCache>) -> Self {
        Arc::get_mut(&mut self.ctx)
            .expect("with_response_cache called after Arc was shared")
            .response_cache = Some(cache);
        self
    }

    /// Get response cache reference (if configured).
    #[must_use]
    pub fn response_cache(&self) -> Option<&Arc<crate::cache::ResponseCache>> {
        self.ctx.response_cache.as_ref()
    }

    /// Rebuild an executor view over an already-shared context.
    ///
    /// `Executor` *is* its `Arc<ExecutorContext<A>>`, so this is one atomic
    /// increment — the same zero-cost move the runner accessors make. It exists so
    /// a component holding only the context (the `before:mutation` read bridge,
    /// which is built inside `execute_mutation_impl`) can reach the read entry
    /// points, which are `impl Executor<A>`.
    pub(super) const fn from_ctx(ctx: Arc<ExecutorContext<A>>) -> Self {
        Self { ctx }
    }

    /// Construct a query runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    pub(super) fn query_runner(&self) -> runners::query::QueryRunner<A> {
        runners::query::QueryRunner::new(Arc::clone(&self.ctx))
    }

    /// Construct an aggregate runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    pub(super) fn aggregate_runner(&self) -> runners::aggregate::AggregateRunner<A> {
        runners::aggregate::AggregateRunner::new(Arc::clone(&self.ctx))
    }

    /// Execute an aggregate query directly.
    ///
    /// # Errors
    ///
    /// Returns error if query parsing, plan generation, SQL generation, database execution,
    /// or result projection fails.
    pub async fn execute_aggregate_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<serde_json::Value> {
        // #422: operation-level authorization for the public aggregate embedder entry
        //       (the GraphQL aggregate path is gated at the chokepoint, not here, to
        //       avoid double-gating). Fail-closed → 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = [(crate::security::OperationKind::Query, query_name.to_string())];
            crate::security::authorizer::enforce_authz(
                authorizer.as_ref(),
                None,
                &ops,
                Some(query_json),
            )?;
        }
        self.aggregate_runner()
            .execute_aggregate_query(query_json, query_name, metadata, None)
            .await
    }

    /// Execute a window query directly.
    ///
    /// # Errors
    ///
    /// Returns error if query parsing, plan generation, SQL generation, database execution,
    /// or result projection fails.
    pub async fn execute_window_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &crate::compiler::fact_table::FactTableMetadata,
    ) -> Result<serde_json::Value> {
        // #422: operation-level authorization for the public window embedder entry
        //       (the GraphQL window path is gated at the chokepoint, not here).
        //       Fail-closed → 403.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let ops = [(crate::security::OperationKind::Query, query_name.to_string())];
            crate::security::authorizer::enforce_authz(
                authorizer.as_ref(),
                None,
                &ops,
                Some(query_json),
            )?;
        }
        self.aggregate_runner()
            .execute_window_query(query_json, query_name, metadata, None)
            .await
    }

    /// Count rows matching a query's filters.
    ///
    /// Delegates to `QueryRunner::count_rows`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source, or if
    /// inject params are required but no security context is provided.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn count_rows(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<u64> {
        // #1336 backstop: REST reads enter here rather than through the GraphQL
        // document path, so the guard cannot live in `execute_with_timeout` alone.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        self.query_runner().count_rows(query_match, variables, security_context).await
    }

    /// Execute a pre-resolved query match directly, bypassing GraphQL parsing.
    ///
    /// Used by the REST transport after route resolution: the `QueryMatch` is
    /// already computed from HTTP path/query parameters, so there is no need
    /// to re-parse a GraphQL query string.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the query has no SQL source.
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn execute_query_direct(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // #1336 backstop: REST reads enter here rather than through the GraphQL
        // document path, so the guard cannot live in `execute_with_timeout` alone.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        self.query_runner()
            .execute_query_direct(query_match, variables, security_context)
            .await
    }

    /// The same read as [`execute_query_direct`](Self::execute_query_direct),
    /// delivered as a stream of projected rows (#958).
    ///
    /// The export representations — NDJSON, CSV, XLSX — consume rows, not a
    /// response envelope, and used to obtain them by re-executing
    /// `execute_query_direct` with a walking `OFFSET`. That is `O(offset)` per page
    /// and gives each page its own snapshot, so a concurrent write can move a row
    /// across a page boundary and the export duplicates or drops it. One statement
    /// has neither problem.
    ///
    /// The read resolves through the same authorization, RLS, `inject_params` and
    /// field-RBAC path as the buffered call — see
    /// `QueryRunner::resolve_direct_read`.
    ///
    /// ⚠ On PostgreSQL the returned stream holds a pooled connection until it is
    /// dropped. Consume it promptly and drop it when the response ends.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` if the operation or a selected field is
    /// refused, `FraiseQLError::Validation` if the query has no SQL source or a
    /// required principal is absent, and `FraiseQLError::Database` if the read fails
    /// before its first row.
    pub async fn stream_query_direct(
        &self,
        query_match: QueryMatch,
        variables: Option<serde_json::Value>,
        security_context: Option<SecurityContext>,
    ) -> Result<crate::runtime::JsonRowStream>
    where
        A: 'static,
    {
        // #1336 backstop — the streaming twin of `execute_query_direct`.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context.as_ref(),
        )?;

        self.query_runner()
            .stream_query_direct(query_match, variables, security_context)
            .await
    }
}

impl<A: DatabaseAdapter + RelayDatabaseAdapter + 'static> Executor<A> {
    /// Create a new executor with relay cursor pagination enabled.
    ///
    /// Only callable when `A: RelayDatabaseAdapter`.  The relay capability is
    /// encoded once at construction time as a type-erased `Arc<dyn RelayDispatch>`,
    /// so there is no per-query overhead beyond an `Option::is_some()` check.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live PostgreSQL database with relay support.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::runtime::Executor;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let connection_string = "postgresql://localhost/mydb";
    /// # let schema: CompiledSchema = panic!("example");
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Executor::new_with_relay(schema, Arc::new(adapter));
    /// # Ok(()) }
    /// ```
    #[must_use]
    pub fn new_with_relay(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config_and_relay(schema, adapter, RuntimeConfig::default())
    }

    /// Create a new executor with relay support and custom configuration.
    #[must_use]
    pub fn with_config_and_relay(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
    ) -> Self {
        let relay_dispatch: Arc<dyn RelayDispatch> =
            Arc::new(RelayDispatchImpl(Arc::clone(&adapter)));
        Self::build(schema, adapter, config, Some(relay_dispatch))
    }
}
