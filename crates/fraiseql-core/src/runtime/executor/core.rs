//! `Executor` struct definition, constructors, and basic accessors.

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
        traits::{DatabaseAdapter, Writer},
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
/// schema's declared `[validation]` depth and complexity limits are derived into
/// a gate.
///
/// # Which transports this binds on
///
/// Every transport that executes a **GraphQL document** — `/graphql`, MCP, the
/// functions bridge, direct embedders — because the gate is applied by `run_gate1`
/// and `run_gate1` takes a query string. The `/graphql` HTTP stage applies the
/// same limits independently, so a document reaching the executor by any other
/// route is scored here rather than not at all.
///
/// It does **not** bind on a read that never had a document: the REST direct-read
/// surface (the GET resolver, the three exports, the embedding sub-query, the bulk
/// row selection) and both gRPC read arms resolve through
/// `QueryRunner::resolve_direct_read`, which does not call `run_gate1` and cannot
/// — there is no query string to score.
///
/// This doc previously said the derived gate binds on "every transport that
/// reaches the executor". It does not, it never did, and the claim is the reason
/// #1351 recorded the engine read path as covered when no direct read could reach
/// the gate at all. What follows is the written answer to "which transports does
/// this control bind on", so the next reader does not have to re-derive it.
///
/// | control | GraphQL document | REST direct read | gRPC row read |
/// |---|---|---|---|
/// | `[validation] max_query_depth` / `max_query_complexity` | yes — `run_gate1` | no | no |
/// | `[security.cost_budget] per_request_max` | yes — `run_gate1`, whole document | yes — `resolve_direct_read`, summed over the request's reads | yes — via `resolve_direct_read` |
/// | `[validation] max_page_size` | yes — `enforce_max_page_size` | yes | yes |
/// | `[validation] max_response_bytes` | yes — charged on the returned rows | yes | yes, per frame when streamed |
/// | `[rest] max_embedding_depth` | n/a | yes — `parse_select_with_embeddings` | n/a |
///
/// The two "no" cells are not gaps to be closed by routing more paths through
/// `run_gate1`. Depth and complexity score a document because a document is where
/// runtime-resolved work is described; a direct read of a materialised view is one
/// row fetch whatever its nesting, and what bounds it is the page size and the
/// bytes it returns. Scoring it by the document's multiply-per-level arithmetic
/// would model an execution engine this framework does not have. See
/// [`ResponseBudget`](crate::security::ResponseBudget).
///
/// The two per-request ceilings — `per_request_max` and `max_response_bytes` — are
/// charged against **one budget per request**, which a transport lends to every read it
/// issues for that request. See [`RequestBudget`](crate::security::RequestBudget).
///
/// A REST `?select=` embed is **one** read: its embedded levels are composed into the
/// parent's statement (`execute_query_composed`), and the statement is scored as the tree
/// it is — a nested `DirectReadProjection`, which charges exactly what the old
/// one-sub-read-per-parent-row fan-out accumulated — before it is sent. So the per-read
/// rows above bound an embed whole, and the aggregate control the fan-out needed
/// (`[rest] max_embedded_reads`, a tally of sub-reads issued) is gone with the fan-out.
/// An embedded `.count` is part of that statement and is charged with it.
///
/// The `Prefer: count=exact` total is the one read of a REST request that carries
/// neither ceiling: it goes through `count_rows`, a second chokepoint that has never had
/// a cost gate at all.
///
/// Derivation enforces what the schema declares, with one default: an undeclared
/// depth is [`DEFAULT_MAX_QUERY_DEPTH`](crate::schema::DEFAULT_MAX_QUERY_DEPTH), so
/// every executor has a depth gate. The projectors follow the selection to any depth,
/// and without a bound the selection is whatever the client sends. An undeclared
/// complexity stays unbounded. An embedder that must disable validation can install
/// an explicit all-`usize::MAX` config.
fn resolve_gate1(
    config: &RuntimeConfig,
    schema: &CompiledSchema,
) -> crate::security::QueryValidator {
    let effective = config.query_validation.clone().unwrap_or_else(|| {
        let declared = schema.validation_config.as_ref();
        let depth = declared
            .and_then(|v| v.max_query_depth)
            .unwrap_or(crate::schema::DEFAULT_MAX_QUERY_DEPTH);
        crate::security::QueryValidatorConfig {
            max_depth:      depth as usize,
            max_complexity: declared
                .and_then(|v| v.max_query_complexity)
                .map_or(usize::MAX, |c| c as usize),
            max_size_bytes: usize::MAX,
            max_aliases:    usize::MAX,
        }
    });
    crate::security::QueryValidator::from_config(effective)
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
/// # The adapter has no name here
///
/// `Executor` is not generic over its adapter. The concrete type is consumed by the
/// constructor and stored erased, as `Arc<dyn DatabaseAdapter>`, so a holder of an
/// `Executor` cannot name the adapter, cannot recover it, and cannot call anything on
/// it that the engine does not offer. That is the whole point: a transport handed an
/// executor is handed the engine's surface, not a database handle it can go around the
/// engine with.
///
/// The capability that the type parameter used to carry is carried by a value instead.
/// A constructor bounded on [`Writer`] holds the adapter as its write handle; everything
/// else holds `None` and refuses every write. See [`Executor::new`] and
/// [`Executor::read_only`].
///
/// # Ownership and Lifetimes
///
/// The executor holds owned references to schema and runtime data, with no borrowed pointers:
/// - `schema`: Owned `CompiledSchema` (immutable after construction)
/// - `adapter`: Shared via `Arc<dyn DatabaseAdapter>` so multiple executors and tasks use the same
///   connection pool
/// - `introspection`: Owned cached GraphQL schema responses
/// - `config`: Owned runtime configuration
///
/// **No explicit lifetimes required** - all data is either owned or wrapped in `Arc`,
/// so the executor can be stored in long-lived structures without lifetime annotations or
/// borrow-checker issues.
///
/// # Concurrency
///
/// `Executor` is `Send + Sync` — `DatabaseAdapter` requires both of every implementor, so
/// erasing the adapter cannot lose them. It can be safely shared across threads and tasks
/// without cloning:
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
pub struct Executor {
    /// All shared state — schema, adapter, config, caches, relay.
    pub(super) ctx: Arc<ExecutorContext>,
}

/// Compile-time enforcement lives here, on the **constructor**.
///
/// The bound on this block is what keeps a read-only adapter from ever being handed a
/// write handle. Its witness is `FraiseWireAdapter`, which implements `DatabaseAdapter`
/// and deliberately not `Writer`.
///
/// ⚠ **The two blocks below are a pair, and only the pair is the assertion.** A
/// `compile_fail` block is satisfied by *any* compile error, including one with nothing
/// to do with the rule — the previous version of this pair named `SqliteAdapter`, and
/// when #374 deleted that adapter the block passed for a full release while proving
/// nothing. The two differ by exactly the constructor called. If the witness ever stops
/// resolving, the *first* block goes red rather than the second going quietly green.
///
/// `FraiseWireAdapter` needs `--all-features`; every `--doc` invocation in this
/// repository passes it (`Makefile`, `.dagger/main.go`).
///
/// The witness resolves, and the read-only constructor accepts it:
///
/// ```
/// use fraiseql_core::{db::FraiseWireAdapter, runtime::Executor, schema::CompiledSchema};
/// use std::sync::Arc;
/// fn _read_only_is_available(schema: CompiledSchema, adapter: Arc<FraiseWireAdapter>) {
///     let _ = Executor::read_only(schema, adapter);
/// }
/// ```
///
/// …and the write-capable constructor does not compile for it:
///
/// ```compile_fail
/// use fraiseql_core::{db::FraiseWireAdapter, runtime::Executor, schema::CompiledSchema};
/// use std::sync::Arc;
/// fn _wont_compile(schema: CompiledSchema, adapter: Arc<FraiseWireAdapter>) {
///     let _ = Executor::new(schema, adapter);
/// }
/// ```
impl Executor {
    /// Create a new write-capable executor.
    ///
    /// Available only for adapters that implement [`Writer`] — implementing its one
    /// method is the write capability (ruling AA 6). For one that does not — or one whose
    /// write capability you do not want to grant — use [`read_only`](Executor::read_only),
    /// which is bounded only on `DatabaseAdapter`.
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
    pub fn new<A: Writer>(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::with_config(schema, adapter, RuntimeConfig::default())
    }

    /// Create a new write-capable executor with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    /// * `config` - Runtime configuration
    #[must_use]
    pub fn with_config<A: Writer>(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
    ) -> Self {
        let writer: Arc<dyn Writer> = Arc::clone(&adapter) as Arc<dyn Writer>;
        let adapter: Arc<dyn DatabaseAdapter> = adapter;
        Self::build(schema, adapter, std::any::type_name::<A>(), config, None, Some(writer))
    }

    /// Create a new executor that cannot write.
    ///
    /// The entry for an adapter that is not a [`Writer`] — a read-replica handle,
    /// `FraiseWireAdapter`, a read-only test double — and for a write-capable adapter you
    /// want to expose read-only. Mutations are refused on every transport, because the
    /// refusal is the absence of a handle rather than a check somebody has to remember.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    #[must_use]
    pub fn read_only<A: DatabaseAdapter>(schema: CompiledSchema, adapter: Arc<A>) -> Self {
        Self::read_only_with_config(schema, adapter, RuntimeConfig::default())
    }

    /// Create a new read-only executor with custom configuration.
    ///
    /// # Arguments
    ///
    /// * `schema` - Compiled schema
    /// * `adapter` - Database adapter
    /// * `config` - Runtime configuration
    #[must_use]
    pub fn read_only_with_config<A: DatabaseAdapter>(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
    ) -> Self {
        let adapter: Arc<dyn DatabaseAdapter> = adapter;
        Self::build(schema, adapter, std::any::type_name::<A>(), config, None, None)
    }

    /// The one construction path. Every public constructor and every rebuild funnels
    /// through here, so a new executor cannot differ from the others by an omitted
    /// field — which is the drift #750 set out to prevent by recording a closure.
    ///
    /// `relay` is passed in rather than built here because constructing a
    /// `RelayDispatchImpl` needs a `RelayDatabaseAdapter` bound that this impl block
    /// deliberately does not carry. `adapter_name` is the adapter's type, named by the
    /// constructor that still knows it, for the one diagnostic that has to say which
    /// adapter it means.
    ///
    /// `writer` is the write handle: `Some` exactly when a constructor bounded on [`Writer`]
    /// built the executor.
    fn build(
        schema: CompiledSchema,
        adapter: Arc<dyn DatabaseAdapter>,
        adapter_name: &'static str,
        config: RuntimeConfig,
        relay: Option<Arc<dyn RelayDispatch>>,
        writer: Option<Arc<dyn Writer>>,
    ) -> Self {
        let gate1 = resolve_gate1(&config, &schema);
        // One depth bound: the resolver refuses at the gate's depth, so a document the
        // gate admits is never refused by a second, fixed limit — and one it refuses is
        // refused by the resolver too on the paths that classify before the gate runs.
        let max_depth = u32::try_from(gate1.config().max_depth).unwrap_or(u32::MAX);
        let matcher = QueryMatcher::new(schema.clone()).with_max_depth(max_depth);
        let planner = QueryPlanner::new(config.cache_query_plans);
        // Build introspection responses at startup (zero-cost at runtime),
        // with `@inaccessible` fields filtered out. Shared with the relay
        // constructor so both paths apply the identical filtering (L-relay-inaccessible).
        let introspection = build_introspection(&schema);
        let output_types = crate::runtime::completion::OutputTypes::from_schema(&schema);

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

        // Rulings X 2 and Z 1: every write commits only once its response is built, so
        // every write needs the commit-gated write. An adapter without it gets no
        // mutations, decided here and said once, rather than a refusal per request or a
        // write committed before it was adjudicated.
        let nested_row_gates = super::runners::query_nested::NestedRowGates::build(
            &schema,
            config.rls_policy.as_deref(),
        );
        let ctx = Arc::new(ExecutorContext {
            schema,
            schema_version,
            nested_row_gates,
            adapter,
            adapter_name,
            writer,
            relay,
            matcher,
            planner,
            config,
            introspection,
            output_types,
            node_type_index,
            gate1,
            parse_cache: MokaCache::new(PARSE_CACHE_CAPACITY),
            introspection_projections: MokaCache::new(INTROSPECTION_PROJECTION_CAPACITY),
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

    /// How deep a selection may nest on this executor: its depth gate's
    /// `max_query_depth` — declared, installed, or `DEFAULT_MAX_QUERY_DEPTH`. Every
    /// resolution of a document's selection set refuses past it, including the
    /// transport's own (the SSE planner), so no path answers depth differently.
    #[must_use]
    pub fn max_query_depth(&self) -> u32 {
        u32::try_from(self.ctx.gate1.config().max_depth).unwrap_or(u32::MAX)
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

    /// Refuse `schema` when this executor's reads go to hot standbys and a source of it
    /// depends on an UNLOGGED or temporary table (#1390).
    ///
    /// The boot-time check, re-run by a hot reload before it swaps: a query added by a
    /// reload reads through the same replicas the server was started with.
    ///
    /// # Errors
    ///
    /// See [`crate::schema::refuse_standby_unreadable_sources`].
    pub async fn refuse_standby_unreadable_sources(&self, schema: &CompiledSchema) -> Result<()> {
        self.ctx.refuse_standby_unreadable_sources(schema).await
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
        self.ctx
            .execute_ddl(&format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE"))
            .await
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
    ///
    /// The write grant is carried over for the same reason and in the same way. It is
    /// resolved from the adapter, and the adapter is unchanged, so recomputing it could
    /// only ever agree — but a rebuild that *recomputed* a capability is precisely the
    /// step #750 showed can omit one. Carrying it makes a downgrade unrepresentable
    /// rather than merely unlikely. The refusal beside it is recomputed from the same
    /// adapter, so it agrees with the boot (ruling Z 1).
    #[must_use]
    pub fn rebuild_with(&self, schema: CompiledSchema, config: RuntimeConfig) -> Self {
        Self::build(
            schema,
            Arc::clone(&self.ctx.adapter),
            self.ctx.adapter_name,
            config,
            self.ctx.relay.clone(),
            self.ctx.writer.clone(),
        )
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

    /// Rebuild an executor view over an already-shared context.
    ///
    /// `Executor` *is* its `Arc<ExecutorContext>`, so this is one atomic
    /// increment — the same zero-cost move the runner accessors make. It exists so
    /// a component holding only the context (the `before:mutation` read bridge,
    /// which is built inside `execute_mutation_impl`) can reach the read entry
    /// points, which are `impl Executor`.
    pub(super) const fn from_ctx(ctx: Arc<ExecutorContext>) -> Self {
        Self { ctx }
    }

    /// Construct a query runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    pub(super) fn query_runner(&self) -> runners::query::QueryRunner {
        runners::query::QueryRunner::new(Arc::clone(&self.ctx))
    }

    /// Construct an aggregate runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    pub(super) fn aggregate_runner(&self) -> runners::aggregate::AggregateRunner {
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
            let ops = [super::support::authz::root_operation(
                &self.ctx.schema,
                crate::security::OperationKind::Query,
                query_name,
            )];
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
            let ops = [super::support::authz::root_operation(
                &self.ctx.schema,
                crate::security::OperationKind::Query,
                query_name,
            )];
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
        request_budget: Option<&crate::security::RequestBudget>,
    ) -> Result<serde_json::Value> {
        // #1336 backstop: REST reads enter here rather than through the GraphQL
        // document path, so the guard cannot live in `execute_with_timeout` alone.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        let response = self
            .query_runner()
            .execute_query_direct(query_match, variables, security_context, request_budget)
            .await?;
        self.complete_direct(response, query_match)
    }

    /// § 6.4.4 for a read with no response document of its own to carry errors (REST, its
    /// bulk reads): completed as the same read through GraphQL would be (#1522), a value
    /// that cannot be completed refuses the read, naming the field and where it was.
    fn complete_direct(
        &self,
        mut response: serde_json::Value,
        query_match: &QueryMatch,
    ) -> Result<serde_json::Value> {
        // A single resource with no row is "not found", which the transport answers in its
        // own way; only a row that exists is completed against its type.
        if response["data"][query_match.response_key()].is_null() {
            return Ok(response);
        }
        self.ctx.output_types.complete(
            &mut response,
            "Query",
            &query_match.selections,
            &std::collections::HashMap::new(),
        );
        let Some(error) = response.get("errors").and_then(|e| e.get(0)) else {
            return Ok(response);
        };
        let message = error["message"].as_str().unwrap_or("Cannot return null");
        Err(crate::error::FraiseQLError::Internal {
            message: format!("{message} At {}: the stored value is missing.", error["path"]),
            source:  None,
        })
    }

    /// Whether this executor's database adapter can compose related resources into a read
    /// ([`execute_query_composed`](Self::execute_query_composed)).
    ///
    /// The REST mount reads it to warn at boot; `execute_query_composed` refuses from it with
    /// `501`. One answer for both, so the boot and the request cannot disagree.
    #[must_use]
    pub fn supports_composed_reads(&self) -> bool {
        self.ctx.adapter.supports_composed_reads()
    }

    /// The physical health of every TVIEW this schema reads (#1392), from
    /// `tviews.pg_tviews_profile()`: the server's `/metrics` reports it. Empty when the schema
    /// reads none, or the database has no `pg_tviews`.
    ///
    /// # Errors
    ///
    /// The database errors of the catalog read.
    pub async fn tview_profiles(&self) -> Result<Vec<crate::backend::TviewProfile>> {
        self.ctx.adapter.tview_profiles(&self.ctx.schema.read_sources()).await
    }

    /// Execute a pre-resolved query match with related resources composed into it —
    /// the REST `?select=` embed.
    ///
    /// One statement, not one read per parent row per level: each embedded level is a
    /// correlated subquery joined into the parent's, and every level is gated as the read
    /// of its own target it replaces — the target query's authorization and role gates,
    /// its RLS predicate, and field-level RBAC against the target type, all decided before
    /// anything is sent. See `runners::query_composed`.
    ///
    /// `request_budget` is charged exactly as by
    /// [`execute_query_direct`](Self::execute_query_direct): the cost of the whole tree
    /// once, before the statement, and the bytes of what it returns.
    ///
    /// # Errors
    ///
    /// Everything [`execute_query_direct`](Self::execute_query_direct) returns, for the
    /// root and for every embedded level; `FraiseQLError::Validation` for a relationship
    /// the parent type does not declare; `FraiseQLError::Unsupported` from a database
    /// adapter that cannot compose a read.
    pub async fn execute_query_composed(
        &self,
        query_match: &QueryMatch,
        embeds: &[crate::runtime::EmbedSelection],
        counts: &[crate::runtime::CountSelection],
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        request_budget: Option<&crate::security::RequestBudget>,
    ) -> Result<serde_json::Value> {
        // #1336 backstop, as on every direct-read entry.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        let response = self
            .query_runner()
            .execute_query_composed(
                query_match,
                embeds,
                counts,
                variables,
                security_context,
                request_budget,
            )
            .await?;
        self.complete_direct(response, query_match)
    }

    /// The allowances **one request** holds, to be shared by every read that request
    /// issues.
    ///
    /// A transport that answers one request with several reads — the REST `?select=`
    /// embed, which resolves one sub-read per parent row per level — builds one of these
    /// and passes it by reference to each
    /// [`execute_query_direct`](Self::execute_query_direct). Without it each sub-read
    /// gets allowances of its own, and then `[validation] max_response_bytes` bounds
    /// each sub-read rather than the response they add up to, and `[security.cost_budget]
    /// per_request_max` scores each sub-read rather than the request they are all part
    /// of — two ceilings named for a request and enforced on something smaller.
    ///
    /// Built here rather than in the transport so the ceilings are read from the compiled
    /// configuration in the one place that owns them, and a transport cannot supply a
    /// budget with ceilings of its own choosing.
    ///
    /// Always a budget, never an `Option`: the two ceilings are declared independently
    /// and either may be absent, which is a distinction
    /// [`RequestBudget`](crate::security::RequestBudget) keeps internally rather than
    /// collapsing into "no budget".
    #[must_use]
    pub fn request_budget(&self) -> crate::security::RequestBudget {
        crate::security::RequestBudget::new(
            self.ctx.config.max_response_bytes,
            self.ctx.config.max_operation_cost,
        )
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
    ) -> Result<crate::runtime::JsonRowStream> {
        // #1336 backstop — the streaming twin of `execute_query_direct`.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context.as_ref(),
        )?;

        self.query_runner()
            .stream_query_direct(query_match, variables, security_context)
            .await
    }

    /// Execute a row-shaped read — the same read as
    /// [`execute_query_direct`](Self::execute_query_direct), answered as typed
    /// column values rather than GraphQL-shaped JSON (#1351).
    ///
    /// This is the gRPC transport's read entry. gRPC answers **row-shaped** results
    /// projected through protobuf `ColumnSpec`s built from the message descriptor,
    /// and there is no selection set on the wire — so it cannot use the JSON entry
    /// without a round-trip through a shape it chose this transport to avoid. It
    /// used to resolve its own predicate and call
    /// [`DatabaseAdapter::execute_row_query`] directly instead, which made it a
    /// second read implementation: the operation `Authorizer` (#422), the
    /// `requires_role` gate (#1122), the actor allow-list (#966), the field gate
    /// (#423) and the compiled page-size ceiling (#421) applied to every transport
    /// but that one.
    ///
    /// The caller must project with [`RowRead::columns`](crate::runtime::RowRead::columns),
    /// not with the columns it
    /// passed in — see that field for why.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` when the operation, the role gate, the
    /// actor gate or a selected gated field is refused; `FraiseQLError::Validation`
    /// when the query has no SQL source or a configured policy has no principal to
    /// evaluate for (fail closed, #784); `FraiseQLError::Unsupported` for a resolved
    /// shape the row path cannot carry; and `FraiseQLError::Database` if the read
    /// fails.
    pub async fn execute_row_read(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<super::RowRead> {
        // #1336 backstop: a gRPC read enters here rather than through the GraphQL
        // document path, so the guard cannot live in `execute_with_timeout` alone.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        self.query_runner()
            .execute_row_read(query_match, variables, security_context, columns)
            .await
    }

    /// The same read as [`execute_row_read`](Self::execute_row_read), delivered one
    /// row at a time (#1351).
    ///
    /// The gRPC server-streaming arm's source. Both arms resolve through one
    /// function, because the two have already drifted apart once: #1348 found the
    /// unary arm fail-closed and the streaming arm fail-open on the same RLS
    /// evaluation failure.
    ///
    /// ⚠ On PostgreSQL the returned stream holds a pooled connection until it is
    /// dropped. Consume it promptly and drop it when the response ends.
    ///
    /// # Errors
    ///
    /// The same as [`execute_row_read`](Self::execute_row_read).
    pub async fn stream_row_read(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        columns: &[fraiseql_db::types::ColumnSpec],
    ) -> Result<super::StreamedRowRead> {
        // #1336 backstop — the streaming twin of `execute_row_read`.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        self.query_runner()
            .stream_row_read(query_match, variables, security_context, columns)
            .await
    }
}

impl Executor {
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
    pub fn new_with_relay<A: Writer + RelayDatabaseAdapter + 'static>(
        schema: CompiledSchema,
        adapter: Arc<A>,
    ) -> Self {
        Self::with_config_and_relay(schema, adapter, RuntimeConfig::default())
    }

    /// Create a new executor with relay support and custom configuration.
    #[must_use]
    pub fn with_config_and_relay<A: Writer + RelayDatabaseAdapter + 'static>(
        schema: CompiledSchema,
        adapter: Arc<A>,
        config: RuntimeConfig,
    ) -> Self {
        let relay_dispatch: Arc<dyn RelayDispatch> =
            Arc::new(RelayDispatchImpl(Arc::clone(&adapter)));
        let writer: Arc<dyn Writer> = Arc::clone(&adapter) as Arc<dyn Writer>;
        let adapter: Arc<dyn DatabaseAdapter> = adapter;
        Self::build(
            schema,
            adapter,
            std::any::type_name::<A>(),
            config,
            Some(relay_dispatch),
            Some(writer),
        )
    }
}
