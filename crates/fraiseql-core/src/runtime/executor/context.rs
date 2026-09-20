//! Shared execution context — holds all state shared across executor sub-components.
//!
//! [`ExecutorContext`] is the single source of truth for the schema, adapter,
//! configuration, and caches used during query execution. It is always accessed
//! via `Arc<ExecutorContext<A>>` so sub-executors can be cheaply cloned.

use std::{collections::HashMap, sync::Arc};

use moka::sync::Cache as MokaCache;

use super::{QueryType, support::relay::RelayDispatch};
use crate::{
    backend::{
        AdminSqlOutcome, AdminSqlRequest, ResultCacheStats,
        traits::DatabaseAdapter,
        types::{DatabaseType, PoolMetrics, QueryStatEntry},
    },
    cache::ViewName,
    error::Result,
    graphql::ParsedQuery,
    runtime::{QueryMatcher, QueryPlanner, RuntimeConfig},
    schema::{CompiledSchema, IntrospectionResponses},
};

/// All shared state for an executor instance.
///
/// Constructed once at `Executor::new()` / `Executor::with_config()` and then
/// stored as `Arc<ExecutorContext<A>>`. Sub-executors (query runner, mutation
/// runner, etc.) each hold a clone of this `Arc`.
pub(super) struct ExecutorContext<A: DatabaseAdapter> {
    /// Compiled schema with optimized SQL templates.
    pub(super) schema: CompiledSchema,

    /// The compiled schema's version (its content hash), computed **once** here
    /// at construction (`CompiledSchema::content_hash()` re-serialises + hashes
    /// the whole schema — far too expensive to call per-mutation). Stamped onto
    /// every change-log outbox row's `schema_version` column so a row records the
    /// deployment that produced it (the #378 replay / zero-downtime correctness
    /// handle). A per-deployment constant — it changes on any schema change.
    pub(super) schema_version: Arc<str>,

    /// Shared database adapter for query execution.
    pub(super) adapter: Arc<A>,

    /// Type-erased **write** capability slot.
    ///
    /// `Some` only when both capability gates agreed at construction: the executor
    /// was built through a constructor bounded on [`SupportsMutations`] (compile
    /// time, opt-in) *and* the adapter's
    /// [`supports_mutations()`](DatabaseAdapter::supports_mutations) returned `true`
    /// (runtime, opt-out backstop). Until now the two were, in the marker trait's own
    /// words, "stated rather than enforced" — an adapter could carry the marker and
    /// never override the method, and the typed write entries, which skipped the
    /// runtime check *because* they had the bound, would dispatch it anyway. This slot
    /// is their intersection, computed once, in one place.
    ///
    /// Every write dispatch takes its handle from here, so **obtaining the handle is
    /// the check**. That is the property a boolean does not have: there is no way to
    /// reach the database and forget to consult it. It is the shape `relay` next door
    /// already uses.
    pub(super) writer: Option<Arc<dyn DatabaseAdapter>>,

    /// Type-erased relay capability slot.
    ///
    /// `Some` when constructed via `new_with_relay`. `None` returns a
    /// `FraiseQLError::Validation` for relay queries — no `unreachable!()`.
    pub(super) relay: Option<Arc<dyn RelayDispatch>>,

    /// Query matching engine (stateless).
    pub(super) matcher: QueryMatcher,

    /// Query execution planner (stateless).
    pub(super) planner: QueryPlanner,

    /// Runtime configuration (timeouts, complexity limits, etc.).
    pub(super) config: RuntimeConfig,

    /// Pre-built introspection responses cached for `__schema` and `__type` queries.
    pub(super) introspection: IntrospectionResponses,

    /// O(1) lookup index for Relay `node(id)` queries.
    pub(super) node_type_index: HashMap<String, Arc<str>>,

    /// GATE-1 query-structure validator (depth / complexity / alias / size),
    /// resolved once at construction: the embedder-installed
    /// `RuntimeConfig::query_validation` when set, otherwise derived from the
    /// compiled schema's declared `[validation]` limits (#379). `None` only
    /// when neither declares anything — a declared bound binds on every
    /// transport that reaches the executor, not just the `/graphql` stage.
    pub(super) gate1: Option<crate::security::QueryValidator>,

    /// Parsed GraphQL AST cache, keyed by xxHash64 of the query string.
    pub(super) parse_cache: MokaCache<u64, Arc<(QueryType, Option<ParsedQuery>)>>,

    /// Projected introspection responses, keyed by a hash of the normalised
    /// selection set (#F7).
    ///
    /// `__schema` must follow the client's selection set (GraphQL § 6.3), which
    /// the pre-built response could not do. Projecting per request would trade
    /// away the "zero-cost at runtime" property the canned response existed for
    /// — except that projection is a **pure function of the selection set**, and
    /// the space of introspection selection sets seen in the wild is tiny and
    /// repetitive: `GraphiQL` sends one canonical query, Apollo sends one, each
    /// codegen tool sends one, and they do not vary between page loads. Memoised
    /// by shape, it is a table lookup again after the first hit.
    ///
    /// Stores the projected `Value` behind an `Arc` so a hit is an O(1)
    /// ref-count bump, matching how the unprojected response was served.
    pub(super) introspection_projections: MokaCache<u64, Arc<serde_json::Value>>,

    /// Optional executor-level response cache.
    pub(super) response_cache: Option<Arc<crate::cache::ResponseCache>>,
}

impl<A: DatabaseAdapter> ExecutorContext<A> {
    /// The write handle, or the refusal that names both gates.
    ///
    /// The single adjudication of "may this executor write?". Both the document path
    /// (`execute_mutation_query`) and the five typed write entries resolve it here, so
    /// there is one decision rather than one per entry point — and the entries that
    /// used to have no check at all cannot regain that state, because they need the
    /// return value to dispatch.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] naming the mutation and both gates.
    pub(super) fn writer(&self, mutation_name: &str) -> Result<&dyn DatabaseAdapter> {
        self.writer.as_deref().ok_or_else(|| crate::error::FraiseQLError::Validation {
            message: format!(
                "Mutation '{mutation_name}' cannot be executed: the configured database \
                 adapter is read-only. A write-capable adapter implements the \
                 `SupportsMutations` marker and returns `true` from \
                 `supports_mutations()` — both default to refusing, and an executor is \
                 write-capable only when both agree. `PostgresAdapter` does; \
                 `FraiseWireAdapter` deliberately does not."
            ),
            path:    None,
        })
    }

    /// Return current connection pool metrics.
    pub(super) fn pool_metrics(&self) -> PoolMetrics {
        self.adapter.pool_metrics()
    }

    /// Which backend this executor is bound to.
    pub(super) fn database_type(&self) -> DatabaseType {
        self.adapter.database_type()
    }

    /// Whether the backend is reachable.
    pub(super) async fn health_check(&self) -> Result<()> {
        self.adapter.health_check().await
    }

    /// The slowest `limit` statements the backend is willing to report.
    pub(super) async fn query_stats(&self, limit: u32) -> Result<Vec<QueryStatEntry>> {
        self.adapter.query_stats(limit).await
    }

    /// One statement's stats by backend-assigned id.
    pub(super) async fn query_stats_by_id(&self, id: &str) -> Result<Option<QueryStatEntry>> {
        self.adapter.query_stats_by_id(id).await
    }

    /// Discard the backend's accumulated statement statistics.
    pub(super) async fn reset_query_stats(&self) -> Result<()> {
        self.adapter.reset_query_stats().await
    }

    /// Adapter-level result-cache counters, or `None` when no cache is active.
    pub(super) fn result_cache_stats(&self) -> Option<ResultCacheStats> {
        self.adapter.result_cache_stats()
    }

    /// Evict every entry from the adapter-level result cache.
    pub(super) async fn clear_result_cache(&self) -> Result<Option<usize>> {
        self.adapter.clear_result_cache().await
    }

    /// Evict adapter-level result-cache entries derived from the given views.
    pub(super) async fn invalidate_views(&self, views: &[ViewName]) -> Result<u64> {
        self.adapter.invalidate_views(views).await
    }

    /// The backend's plan for a statement, as its own JSON shape.
    pub(super) async fn explain_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        self.adapter.explain_query(sql, params).await
    }

    /// Tell the backend the compiled schema changed, so it can drop anything it
    /// derived from the old one.
    pub(super) fn on_schema_reload(&self) {
        self.adapter.on_schema_reload();
    }

    /// Execute one server-composed DDL statement.
    ///
    /// Private to this module: the only public door onto it is
    /// [`Executor::drop_tenant_schema`], which composes the statement itself.
    pub(super) async fn execute_ddl(&self, ddl: &str) -> Result<()> {
        self.adapter.execute_raw_query(ddl).await.map(|_| ())
    }

    /// Run the admin-SQL route's bounded statement.
    pub(super) async fn execute_admin_sql(
        &self,
        request: &AdminSqlRequest,
    ) -> Result<AdminSqlOutcome> {
        self.adapter.execute_admin_sql(request).await
    }
}
