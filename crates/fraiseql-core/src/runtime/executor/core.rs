//! `Executor<A>` struct definition, constructors, and basic accessors.

use std::{collections::HashMap, sync::Arc};

use moka::sync::Cache as MokaCache;

use super::{
    QueryType,
    relay::{RelayDispatch, RelayDispatchImpl},
};
use crate::{
    db::{RelayDatabaseAdapter, traits::DatabaseAdapter, types::PoolMetrics},
    graphql::ParsedQuery,
    runtime::{QueryMatcher, QueryPlanner, RuntimeConfig},
    schema::{CompiledSchema, IntrospectionResponses},
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
    /// Compiled schema with optimized SQL templates
    pub(super) schema: CompiledSchema,

    /// Shared database adapter for query execution
    /// Wrapped in Arc to allow multiple executors to use the same connection pool
    pub(super) adapter: Arc<A>,

    /// Type-erased relay capability slot.
    ///
    /// `Some` when the executor was constructed via `new_with_relay` (requires
    /// `A: RelayDatabaseAdapter`).  `None` causes relay queries to return a
    /// `FraiseQLError::Validation` — no `unreachable!()`, no capability flag.
    pub(super) relay: Option<Arc<dyn RelayDispatch>>,

    /// Query matching engine (stateless)
    pub(super) matcher: QueryMatcher,

    /// Query execution planner (stateless)
    pub(super) planner: QueryPlanner,

    /// Runtime configuration (timeouts, complexity limits, etc.)
    pub(super) config: RuntimeConfig,

    /// Pre-built introspection responses cached for `__schema` and `__type` queries
    /// Avoids recomputing schema introspection on every request
    pub(super) introspection: IntrospectionResponses,

    /// O(1) lookup index for Relay `node(id)` queries: maps `return_type → sql_source`.
    ///
    /// Built once at `Executor::with_config()` from the compiled schema, so every
    /// `execute_node_query()` call is a single `HashMap::get()` rather than an O(N)
    /// linear scan over `schema.queries`.
    pub(super) node_type_index: HashMap<String, Arc<str>>,

    /// Parsed GraphQL AST cache, keyed by xxHash64 of the query string.
    ///
    /// Repeated identical queries skip re-parsing entirely — a lock-free moka hit
    /// instead of a full lexer + recursive-descent parse.  No TTL: parsed ASTs are
    /// immutable and deterministic; the same query string always produces the same result.
    /// Only successful parses are stored; errors are never cached.
    pub(super) parse_cache: MokaCache<u64, Arc<(QueryType, Option<ParsedQuery>)>>,

    /// Optional executor-level response cache.
    ///
    /// When enabled, caches the final projected GraphQL response (after RBAC,
    /// projection, envelope wrapping) to skip all redundant work on cache hits.
    /// Keyed by `(query_cache_key, security_context_hash)`.
    pub(super) response_cache: Option<Arc<crate::cache::ResponseCache>>,
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
    /// let schema = CompiledSchema::from_json(schema_json)?;
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
        let introspection = IntrospectionResponses::build(&schema);

        // Build O(1) node-type index: return_type → sql_source.
        // The first query with a matching return_type and a non-None sql_source wins
        // (consistent with the previous linear-scan behaviour).
        let mut node_type_index: HashMap<String, Arc<str>> = HashMap::new();
        for q in &schema.queries {
            if let Some(src) = q.sql_source.as_deref() {
                node_type_index.entry(q.return_type.clone()).or_insert_with(|| Arc::from(src));
            }
        }

        Self {
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
        }
    }

    /// Return current connection pool metrics from the underlying database adapter.
    ///
    /// Values are sampled live on each call — not cached — so callers (e.g., the
    /// `/metrics` endpoint) always observe up-to-date pool health.
    pub fn pool_metrics(&self) -> PoolMetrics {
        self.adapter.pool_metrics()
    }

    /// Get the compiled schema.
    #[must_use]
    pub const fn schema(&self) -> &CompiledSchema {
        &self.schema
    }

    /// Get runtime configuration.
    #[must_use]
    pub const fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get database adapter reference.
    #[must_use]
    pub const fn adapter(&self) -> &Arc<A> {
        &self.adapter
    }

    /// Return the number of entries currently held in the parsed-query AST cache.
    ///
    /// Exposed for testing only — callers outside `#[cfg(test)]` code should not
    /// rely on the exact count, which may lag by one maintenance cycle in moka.
    #[cfg(test)]
    #[must_use]
    pub fn parse_cache_entry_count(&self) -> u64 {
        self.parse_cache.entry_count()
    }

    /// Attach an executor-level response cache.
    ///
    /// When enabled, the executor caches the final projected response
    /// (after RBAC, projection, and envelope wrapping) to skip all
    /// redundant work on cache hits.
    #[must_use]
    pub fn with_response_cache(mut self, cache: Arc<crate::cache::ResponseCache>) -> Self {
        self.response_cache = Some(cache);
        self
    }

    /// Get response cache reference (if configured).
    #[must_use]
    pub const fn response_cache(&self) -> Option<&Arc<crate::cache::ResponseCache>> {
        self.response_cache.as_ref()
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
        let relay: Arc<dyn RelayDispatch> = Arc::new(RelayDispatchImpl(adapter.clone()));
        let matcher = QueryMatcher::new(schema.clone());
        let planner = QueryPlanner::new(config.cache_query_plans);
        let introspection = IntrospectionResponses::build(&schema);

        let mut node_type_index: HashMap<String, Arc<str>> = HashMap::new();
        for q in &schema.queries {
            if let Some(src) = q.sql_source.as_deref() {
                node_type_index.entry(q.return_type.clone()).or_insert_with(|| Arc::from(src));
            }
        }

        Self {
            schema,
            adapter,
            relay: Some(relay),
            matcher,
            planner,
            config,
            introspection,
            node_type_index,
            parse_cache: MokaCache::new(PARSE_CACHE_CAPACITY),
            response_cache: None,
        }
    }
}
