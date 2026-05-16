//! `Executor<A>` struct definition, constructors, and basic accessors.

use std::{collections::HashMap, sync::Arc};

use moka::sync::Cache as MokaCache;

use super::{
    context::ExecutorContext,
    runners,
    support::relay::{RelayDispatch, RelayDispatchImpl},
};
use crate::{
    db::{RelayDatabaseAdapter, traits::DatabaseAdapter, types::PoolMetrics},
    error::Result,
    runtime::{QueryMatcher, QueryPlanner, RuntimeConfig, matcher::QueryMatch},
    schema::{CompiledSchema, IntrospectionResponses},
    security::SecurityContext,
};

/// Maximum number of distinct query strings whose parsed ASTs are cached in memory.
///
/// 1 024 entries covers the full distinct-query vocabulary of any realistic workload.
/// Each entry holds an `Arc<(QueryType, Option<ParsedQuery>)>` — the AST is shared,
/// not duplicated.
const PARSE_CACHE_CAPACITY: u64 = 1_024;

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
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        // Build introspection responses at startup (zero-cost at runtime)
        // `mut` is required by the `#[cfg(feature = "federation")]` block below.
        #[cfg_attr(not(feature = "federation"), allow(unused_mut))]
        let mut introspection = IntrospectionResponses::build(&schema);

        // Filter @inaccessible fields from introspection (DX defence-in-depth).
        // Does NOT affect data responses or _entities — only __type/__schema.
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

        // Build O(1) node-type index: return_type → sql_source.
        // The first query with a matching return_type and a non-None sql_source wins
        // (consistent with the previous linear-scan behaviour).
        let mut node_type_index: HashMap<String, Arc<str>> = HashMap::new();
        for q in &schema.queries {
            if let Some(src) = q.sql_source.as_deref() {
                node_type_index.entry(q.return_type.clone()).or_insert_with(|| Arc::from(src));
            }
        }

        let ctx = Arc::new(ExecutorContext {
            schema,
            adapter,
            relay: None,
            matcher,
            planner,
            config,
            introspection,
            node_type_index,
            parse_cache: MokaCache::new(PARSE_CACHE_CAPACITY),
            response_cache: None,
        });

        Self { ctx }
    }

    /// Return current connection pool metrics from the underlying database adapter.
    ///
    /// Values are sampled live on each call — not cached — so callers (e.g., the
    /// `/metrics` endpoint) always observe up-to-date pool health.
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

    /// Get database adapter reference.
    #[must_use]
    pub fn adapter(&self) -> &Arc<A> {
        &self.ctx.adapter
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
        self.query_runner()
            .execute_query_direct(query_match, variables, security_context)
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
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        let introspection = IntrospectionResponses::build(&schema);

        let mut node_type_index: HashMap<String, Arc<str>> = HashMap::new();
        for q in &schema.queries {
            if let Some(src) = q.sql_source.as_deref() {
                node_type_index.entry(q.return_type.clone()).or_insert_with(|| Arc::from(src));
            }
        }

        let ctx = Arc::new(ExecutorContext {
            schema,
            adapter,
            relay: Some(relay_dispatch),
            matcher,
            planner,
            config,
            introspection,
            node_type_index,
            parse_cache: MokaCache::new(PARSE_CACHE_CAPACITY),
            response_cache: None,
        });

        Self { ctx }
    }
}
