//! Cached database adapter wrapper.
//!
//! Provides transparent caching for `DatabaseAdapter` implementations by wrapping
//! `execute_where_query()` calls with cache lookup and storage.
//!
//! # Security: Cache Isolation via RLS
//!
//! Automatic Persisted Query (APQ) caching provides no user-level isolation on its own.
//! Cache key isolation derives entirely from Row-Level Security: different users MUST
//! produce different WHERE clauses via their RLS policies. If RLS is disabled or
//! returns an empty WHERE clause, two users with the same query and variables will
//! receive the same cached response.
//!
//! **Always verify RLS is active when caching is enabled in multi-tenant deployments.**
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────┐
//! │ CachedDatabaseAdapter   │
//! │                         │
//! │  execute_where_query()  │
//! └───────────┬─────────────┘
//!             │
//!             ↓ generate_cache_key()
//! ┌─────────────────────────┐
//! │ Cache Hit?              │
//! └───────────┬─────────────┘
//!             │
//!       ┌─────┴─────┐
//!       │           │
//!      HIT         MISS
//!       │           │
//!       ↓           ↓ DatabaseAdapter
//! Return Cached   Execute Query
//! Result          + Store in Cache
//! ```
//!
//! # Example
//!
//! ```no_run
//! use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig};
//! use fraiseql_core::db::{postgres::PostgresAdapter, DatabaseAdapter};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create underlying database adapter
//! let db_adapter = PostgresAdapter::new("postgresql://localhost/db").await?;
//!
//! // Wrap with caching
//! let cache = QueryResultCache::new(CacheConfig::default());
//! let cached_adapter = CachedDatabaseAdapter::new(
//!     db_adapter,
//!     cache,
//!     "1.0.0".to_string()  // schema version
//! );
//!
//! // Use as normal DatabaseAdapter - caching is transparent
//! let users = cached_adapter
//!     .execute_where_query("v_user", None, Some(10), None, None)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use super::{
    cascade_invalidator::CascadeInvalidator,
    fact_table_version::{FactTableCacheConfig, FactTableVersionProvider},
    result::QueryResultCache,
};
use crate::{
    cache::config::RlsEnforcement,
    db::{
        ChangeLogWrite, DatabaseAdapter, DatabaseType, DirectMutationContext, MutationStrategy,
        PoolMetrics, SupportsMutations, WhereClause, quote_postgres_identifier,
        types::{JsonbValue, OrderByClause},
    },
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, SourceKind, SourceProbe, sql_source_probes},
};

mod mutation;
mod query;
#[cfg(test)]
mod tests;

pub use query::view_name_to_entity_type;

/// One source relation's Row-Level Security posture, as read from `pg_class`.
#[derive(Debug, Clone, Copy)]
struct RelationRls {
    /// `relkind = 'v'` — a view, which carries no policies of its own.
    is_view:          bool,
    /// `relrowsecurity` — RLS switched on for the relation.
    rls_enabled:      bool,
    /// At least one `pg_policy` row targets it.
    has_policy:       bool,
    /// `security_invoker = true` in `reloptions` (views only, PG 15+).
    security_invoker: bool,
}

impl RelationRls {
    /// A view protects rows only by deferring to the caller's policies; a table
    /// protects rows only when RLS is on *and* a policy exists — RLS enabled with
    /// no policy denies everything to non-owners and is silently bypassed by the
    /// owner, so it is not an isolation mechanism either way.
    const fn is_protected(self) -> bool {
        if self.is_view {
            self.security_invoker
        } else {
            self.rls_enabled && self.has_policy
        }
    }

    /// Why it is not protected, in the operator's terms.
    const fn explain(self) -> &'static str {
        if self.is_view {
            "view is not `security_invoker`, so it runs with its owner's privileges and \
             bypasses the caller's RLS policies"
        } else if !self.rls_enabled {
            "row level security is not enabled on this table"
        } else {
            "row level security is enabled but no policy is defined, so it isolates nothing"
        }
    }
}

/// Cached database adapter wrapper.
///
/// Wraps any `DatabaseAdapter` implementation with transparent query result caching.
/// Cache keys include query, variables, WHERE clause, and schema version for security
/// and correctness.
///
/// # Cache Behavior
///
/// - **Cache Hit**: Returns cached result in ~0.1ms (50-200x faster than database)
/// - **Cache Miss**: Executes query via underlying adapter, stores result in cache
/// - **Invalidation**: Call `invalidate_views()` after mutations to clear affected caches
///
/// # Thread Safety
///
/// This adapter is `Send + Sync` and can be safely shared across async tasks.
/// The underlying cache uses `Arc<Mutex<>>` for thread-safe access.
///
/// # Example
///
/// ```no_run
/// use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig, InvalidationContext};
/// use fraiseql_core::db::{postgres::PostgresAdapter, DatabaseAdapter};
/// use fraiseql_db::ViewName;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let db = PostgresAdapter::new("postgresql://localhost/db").await?;
/// let cache = QueryResultCache::new(CacheConfig::default());
/// let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string());
///
/// // First query - cache miss (slower)
/// let users1 = adapter.execute_where_query("v_user", None, None, None, None).await?;
///
/// // Second query - cache hit (fast!)
/// let users2 = adapter.execute_where_query("v_user", None, None, None, None).await?;
///
/// // After mutation, invalidate. `InvalidationContext` keeps its `Vec<String>`
/// // shape for compatibility with the audit-logging facade; callers convert at
/// // the adapter boundary.
/// let invalidation = InvalidationContext::for_mutation(
///     "createUser",
///     vec!["v_user".to_string()],
/// );
/// let views: Vec<ViewName> = invalidation
///     .modified_views
///     .iter()
///     .map(ViewName::from)
///     .collect();
/// adapter.invalidate_views(&views)?;
/// # Ok(())
/// # }
/// ```
pub struct CachedDatabaseAdapter<A: DatabaseAdapter> {
    /// Underlying database adapter.
    pub(super) adapter: A,

    /// Query result cache.
    pub(super) cache: Arc<QueryResultCache>,

    /// Schema version for cache key generation.
    ///
    /// When schema version changes (e.g., after deployment), all cache entries
    /// with old version become invalid automatically.
    pub(super) schema_version: String,

    /// Per-view TTL overrides in seconds.
    ///
    /// Populated from `QueryDefinition::cache_ttl_seconds` at server startup:
    /// view name → TTL seconds.  `None` for a view falls back to the global
    /// `CacheConfig::ttl_seconds`.
    pub(super) view_ttl_overrides: HashMap<String, u64>,

    /// Set of views that explicitly opt into caching via `cache_ttl_seconds`.
    ///
    /// Derived from `view_ttl_overrides` keys.  When `opt_in_mode` is active,
    /// views **not** in this set bypass cache key generation entirely, eliminating
    /// allocation overhead for uncached queries.
    pub(super) cacheable_views: HashSet<String>,

    /// Primary view → the secondary views queries over it also read.
    ///
    /// Populated from `QueryDefinition::additional_views` by
    /// [`CachedDatabaseAdapter::with_cache_metadata_from_schema`]. A cached entry
    /// for the primary view is registered under all of them, so a mutation on a
    /// secondary view evicts it (#761).
    pub(super) view_secondary_views: HashMap<String, Vec<String>>,

    /// Whether opt-in caching mode is active.
    ///
    /// Set to `true` by [`CachedDatabaseAdapter::with_view_ttl_overrides`] and
    /// [`CachedDatabaseAdapter::with_cache_metadata_from_schema`] to indicate that
    /// the caller has intentionally configured per-view TTL overrides.  In this
    /// mode, **only** views in `cacheable_views` are cached; all others bypass
    /// key-generation entirely.
    ///
    /// When `false` (default, adapter created with [`Self::new`] or
    /// [`Self::with_fact_table_config`] without a schema call), all views remain
    /// cacheable — preserving backward-compatible behaviour for tests and direct
    /// usage that do not use per-query TTL annotations.
    pub(super) opt_in_mode: bool,

    /// Whether the schema has RLS configured (affects caching for unauthenticated requests).
    pub(super) has_rls: bool,

    /// Configuration for fact table aggregation caching.
    pub(super) fact_table_config: FactTableCacheConfig,

    /// Version provider for fact tables (caches version lookups).
    pub(super) version_provider: Arc<FactTableVersionProvider>,

    /// Optional cascade invalidator for transitive view dependency expansion.
    ///
    /// When set, `invalidate_views()` uses BFS to expand the initial view list
    /// to include all transitively dependent views before clearing cache entries.
    pub(super) cascade_invalidator: Option<Arc<Mutex<CascadeInvalidator>>>,
}

impl<A: DatabaseAdapter> CachedDatabaseAdapter<A> {
    /// Create new cached database adapter.
    ///
    /// # Arguments
    ///
    /// * `adapter` - Underlying database adapter to wrap
    /// * `cache` - Query result cache instance
    /// * `schema_version` - Uniquely identifies the compiled schema. Use `schema.content_hash()`
    ///   (NOT `env!("CARGO_PKG_VERSION")`) so that any schema content change automatically
    ///   invalidates cached entries across deploys.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig};
    /// use fraiseql_core::db::postgres::PostgresAdapter;
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let schema = CompiledSchema::default();
    /// let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// let cache = QueryResultCache::new(CacheConfig::default());
    /// let adapter = CachedDatabaseAdapter::new(
    ///     db,
    ///     cache,
    ///     schema.content_hash()  // Use content hash for automatic invalidation
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new(adapter: A, cache: QueryResultCache, schema_version: String) -> Self {
        Self {
            adapter,
            cache: Arc::new(cache),
            schema_version,
            view_ttl_overrides: HashMap::new(),
            cacheable_views: HashSet::new(),
            view_secondary_views: HashMap::new(),
            opt_in_mode: false,
            has_rls: false,
            fact_table_config: FactTableCacheConfig::default(),
            version_provider: Arc::new(FactTableVersionProvider::default()),
            cascade_invalidator: None,
        }
    }

    /// Set per-view TTL overrides.
    ///
    /// Maps `sql_source` (view name) → TTL in seconds.  Built at server startup
    /// from compiled `QueryDefinition::cache_ttl_seconds` entries.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig};
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// # let cache = QueryResultCache::new(CacheConfig::default());
    /// let overrides = std::collections::HashMap::from([
    ///     ("v_country".to_string(), 3600_u64),      // 1 h for reference data
    ///     ("v_live_price".to_string(), 0_u64),      // no TTL — mutation-invalidated only
    /// ]);
    /// let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string())
    ///     .with_view_ttl_overrides(overrides);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_view_ttl_overrides(mut self, overrides: HashMap<String, u64>) -> Self {
        self.cacheable_views = overrides.keys().cloned().collect();
        self.view_ttl_overrides = overrides;
        self.opt_in_mode = true;
        self
    }

    /// Enable or disable the RLS unauthenticated-request cache bypass.
    ///
    /// When `true`, any request without a `SecurityContext` will bypass both
    /// cache read and write, preventing unauthenticated requests from being
    /// served stale data that belongs to an authenticated tenant.
    ///
    /// Set this to `schema.has_rls_configured()` at server startup.
    #[must_use]
    pub const fn with_rls(mut self, has_rls: bool) -> Self {
        self.has_rls = has_rls;
        self
    }

    /// Set a cascade invalidator for transitive view dependency expansion.
    ///
    /// When set, `invalidate_views()` uses BFS to expand the initial view list
    /// to include all views that transitively depend on the invalidated views.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig, CascadeInvalidator};
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// # let cache = QueryResultCache::new(CacheConfig::default());
    /// let mut cascade = CascadeInvalidator::new();
    /// cascade.add_dependency("v_user_stats", "v_user")?;
    /// cascade.add_dependency("v_dashboard", "v_user_stats")?;
    /// let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string())
    ///     .with_cascade_invalidator(cascade);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_cascade_invalidator(mut self, invalidator: CascadeInvalidator) -> Self {
        self.cascade_invalidator = Some(Arc::new(Mutex::new(invalidator)));
        self
    }

    /// Read every per-query cache declaration out of a compiled schema.
    ///
    /// This is the single seam between the compiled schema and the row cache.
    /// Both things it reads are per-query annotations that the runtime otherwise
    /// has no way to see, because the `DatabaseAdapter` boundary carries only a
    /// view name:
    ///
    /// - `cache_ttl_seconds` → the view's TTL override, and its membership of `cacheable_views`
    ///   (opt-in mode).
    /// - `additional_views` → the secondary views a query also reads, so a mutation on one of them
    ///   evicts the joined query's cached rows (#761).
    ///
    /// Add any further per-query cache annotation here, not at a call site:
    /// `rebuilt_for_schema` (hot reload) delegates to the same code, so an
    /// annotation wired in one place and not the other silently stops working
    /// after the first schema reload.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::{CachedDatabaseAdapter, QueryResultCache, CacheConfig};
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// # let cache = QueryResultCache::new(CacheConfig::default());
    /// # let schema = CompiledSchema::default();
    /// let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string())
    ///     .with_cache_metadata_from_schema(&schema);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_cache_metadata_from_schema(mut self, schema: &CompiledSchema) -> Self {
        self.read_schema_cache_metadata(schema);
        // Always activate opt-in mode when this method is called, regardless of
        // whether any annotations were found.  If the schema has no cache_ttl_seconds
        // annotations, cacheable_views stays empty and every query bypasses the cache
        // entirely — zero overhead.  If annotations are present, only the annotated
        // views are cached; all others bypass.
        self.opt_in_mode = true;
        self
    }

    /// Populate `view_ttl_overrides`, `cacheable_views` and `view_secondary_views`
    /// from the schema's per-query annotations.
    ///
    /// `view_secondary_views` is keyed by *primary* view because that is all the
    /// adapter boundary knows at put time. Two queries over the same primary view
    /// that declare different secondaries therefore share the union — which
    /// over-invalidates one of them by at most the other's declarations. Being
    /// evicted early costs a re-read; not being evicted serves the wrong answer.
    fn read_schema_cache_metadata(&mut self, schema: &CompiledSchema) {
        for query in &schema.queries {
            let Some(view) = &query.sql_source else {
                continue;
            };
            if let Some(ttl) = query.cache_ttl_seconds {
                self.cacheable_views.insert(view.clone());
                self.view_ttl_overrides.insert(view.clone(), ttl);
            }
            // `extract_accessed_views` is the one definition of "which views does
            // this query read"; the response cache uses it too.
            let secondary: Vec<String> = crate::cache::key::extract_accessed_views(query)
                .into_iter()
                .filter(|v| v != view)
                .collect();
            if !secondary.is_empty() {
                let entry = self.view_secondary_views.entry(view.clone()).or_default();
                for v in secondary {
                    if !entry.contains(&v) {
                        entry.push(v);
                    }
                }
            }
        }
    }

    /// Every view a cached entry for `view` must be registered under: the view
    /// itself plus any secondary views declared by queries reading it (#761).
    pub(super) fn accessed_views_for(&self, view: &str) -> Vec<String> {
        let mut views = Vec::with_capacity(1);
        views.push(view.to_string());
        if let Some(secondary) = self.view_secondary_views.get(view) {
            views.extend(secondary.iter().cloned());
        }
        views
    }

    /// Rebuild this adapter for a new compiled schema.
    ///
    /// Returns a new `CachedDatabaseAdapter` that:
    /// - Shares the same underlying `QueryResultCache` (via `Arc`)
    /// - Uses the new schema's content hash as schema version
    /// - Has per-view TTL overrides from the new schema
    /// - Clears the shared cache (stale entries from old schema)
    ///
    /// The underlying database adapter (connection pool) is **reused** — no
    /// connections are closed or reopened.
    ///
    /// This is the hot-reload counterpart to the startup path
    /// (`new()` + `with_cache_metadata_from_schema()`).
    #[must_use]
    pub fn rebuilt_for_schema(self, schema: &CompiledSchema) -> Self {
        // Clear existing cache entries (stale under new schema).
        let _ = self.cache.clear();

        // Construct a new adapter with updated schema version and TTL overrides,
        // reusing the same inner adapter and shared cache.
        let mut rebuilt = Self {
            adapter:              self.adapter,
            cache:                self.cache,
            schema_version:       schema.content_hash(),
            view_ttl_overrides:   HashMap::new(),
            cacheable_views:      HashSet::new(),
            view_secondary_views: HashMap::new(),
            opt_in_mode:          true,
            has_rls:              self.has_rls,
            fact_table_config:    self.fact_table_config,
            version_provider:     self.version_provider,
            cascade_invalidator:  self.cascade_invalidator,
        };
        // Same reader as the startup path, so a new per-query cache annotation
        // cannot work at boot and stop working after a hot reload.
        rebuilt.read_schema_cache_metadata(schema);
        rebuilt
    }

    /// Create new cached database adapter with fact table caching configuration.
    ///
    /// # Arguments
    ///
    /// * `adapter` - Underlying database adapter to wrap
    /// * `cache` - Query result cache instance
    /// * `schema_version` - Current schema version (e.g., git hash, semver)
    /// * `fact_table_config` - Configuration for fact table aggregation caching
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fraiseql_core::cache::{
    ///     CachedDatabaseAdapter, QueryResultCache, CacheConfig,
    ///     FactTableCacheConfig, FactTableVersionStrategy,
    /// };
    /// use fraiseql_core::db::postgres::PostgresAdapter;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = PostgresAdapter::new("postgresql://localhost/db").await?;
    /// let cache = QueryResultCache::new(CacheConfig::default());
    ///
    /// // Configure fact table caching strategies
    /// let mut ft_config = FactTableCacheConfig::default();
    /// ft_config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
    /// ft_config.set_strategy("tf_events", FactTableVersionStrategy::time_based(300));
    ///
    /// let adapter = CachedDatabaseAdapter::with_fact_table_config(
    ///     db,
    ///     cache,
    ///     "1.0.0".to_string(),
    ///     ft_config,
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_fact_table_config(
        adapter: A,
        cache: QueryResultCache,
        schema_version: String,
        fact_table_config: FactTableCacheConfig,
    ) -> Self {
        Self {
            adapter,
            cache: Arc::new(cache),
            schema_version,
            view_ttl_overrides: HashMap::new(),
            cacheable_views: HashSet::new(),
            view_secondary_views: HashMap::new(),
            opt_in_mode: false,
            has_rls: false,
            fact_table_config,
            version_provider: Arc::new(FactTableVersionProvider::default()),
            cascade_invalidator: None,
        }
    }

    /// Get reference to underlying adapter.
    ///
    /// Useful for accessing adapter-specific methods not in the `DatabaseAdapter` trait.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::CachedDatabaseAdapter;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # fn example(adapter: CachedDatabaseAdapter<PostgresAdapter>) {
    /// // Access PostgreSQL-specific functionality
    /// let pg_adapter = adapter.inner();
    /// # }
    /// ```
    #[must_use]
    pub const fn inner(&self) -> &A {
        &self.adapter
    }

    /// Get reference to cache.
    ///
    /// Useful for metrics and monitoring.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::CachedDatabaseAdapter;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # async fn example(adapter: CachedDatabaseAdapter<PostgresAdapter>) -> Result<(), Box<dyn std::error::Error>> {
    /// let metrics = adapter.cache().metrics()?;
    /// println!("Cache hit rate: {:.1}%", metrics.hit_rate() * 100.0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cache(&self) -> &QueryResultCache {
        &self.cache
    }

    /// Get schema version.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::cache::CachedDatabaseAdapter;
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # fn example(adapter: CachedDatabaseAdapter<PostgresAdapter>) {
    /// println!("Schema version: {}", adapter.schema_version());
    /// # }
    /// ```
    #[must_use]
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    /// Get fact table cache configuration.
    #[must_use]
    pub const fn fact_table_config(&self) -> &FactTableCacheConfig {
        &self.fact_table_config
    }

    /// Get the version provider for fact tables.
    #[must_use]
    pub fn version_provider(&self) -> &FactTableVersionProvider {
        &self.version_provider
    }

    /// Verify that Row-Level Security is genuinely enforceable on the relations
    /// this schema reads.
    ///
    /// Call this during server initialization when a multi-tenant schema declares
    /// `[security.rls] enabled = true`. Without real RLS, users sharing the same
    /// query parameters receive the same cached response regardless of tenant.
    ///
    /// # What this checks
    ///
    /// Every relation named by a query's `sql_source`, resolved through
    /// `to_regclass`, must be one of:
    ///
    /// * a **table** with `relrowsecurity` set and at least one `pg_policy` row, or
    /// * a **`security_invoker` view** — a default view executes with its owner's privileges and
    ///   bypasses the caller's RLS entirely, so a non-invoker view over an RLS-protected table
    ///   provides no isolation at all.
    ///
    /// A relation that does not exist fails the check: an absent relation cannot be
    /// protected.
    ///
    /// # What it used to check, and why that was worthless
    ///
    /// It ran `SELECT current_setting('row_security', true)` and passed on `'on'` or
    /// `'force'`. `row_security` governs whether *existing* policies are applied; it
    /// defaults to `on` and says nothing about whether any policy exists. So the
    /// documented "refuse startup if RLS appears inactive" gate returned `Ok(())` on
    /// a stock PostgreSQL with no RLS anywhere — reporting success for a guarantee it
    /// never checked (#762).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Configuration`] naming every relation that is not
    /// RLS-protected, or [`FraiseQLError::Database`] if the catalog cannot be read
    /// (including on non-PostgreSQL adapters, which have no equivalent mechanism and
    /// therefore fail closed).
    pub async fn validate_rls_active(&self, schema: &CompiledSchema) -> Result<()> {
        let mut unprotected: Vec<String> = Vec::new();

        for probe in sql_source_probes(schema) {
            if probe.kind != SourceKind::Relation {
                continue;
            }
            let display = probe.display_name();
            match self.relation_rls_status(&probe).await? {
                None => unprotected.push(format!("{display} — relation does not exist")),
                Some(status) if status.is_protected() => {},
                Some(status) => unprotected.push(format!("{display} — {}", status.explain())),
            }
        }

        if unprotected.is_empty() {
            return Ok(());
        }

        Err(FraiseQLError::Configuration {
            message: format!(
                "Row-Level Security is declared for this multi-tenant schema but is not in                  force on {} of its source relation(s), so tenants are not isolated and                  cached responses can cross tenant boundaries:\n  - {}\n\
                 Enable RLS and declare a policy on each source table                  (`ALTER TABLE … ENABLE ROW LEVEL SECURITY` + `CREATE POLICY …`), define                  views `WITH (security_invoker = true)` so they honour the caller's                  policies, or remove `[security.rls] enabled = true` if this deployment                  does not rely on database RLS.",
                unprotected.len(),
                unprotected.join("\n  - ")
            ),
        })
    }

    /// Read one relation's RLS posture from the catalog. `None` = no such relation.
    async fn relation_rls_status(&self, probe: &SourceProbe) -> Result<Option<RelationRls>> {
        // `execute_raw_query` takes no bind parameters, so the identifier is embedded.
        // It comes from the compiled schema, and single quotes are doubled defensively —
        // the same posture `sql_source_check` takes for the existence probe.
        let ident = match &probe.schema {
            Some(s) => format!(
                "{}.{}",
                quote_postgres_identifier(s),
                quote_postgres_identifier(&probe.name)
            ),
            None => quote_postgres_identifier(&probe.name),
        };
        let literal = ident.replace('\'', "''");
        let sql = format!(
            "SELECT c.relkind::text AS kind, \
                    c.relrowsecurity AS rls_enabled, \
                    (SELECT count(*) FROM pg_policy p WHERE p.polrelid = c.oid)::text \
                      AS policy_count, \
                    COALESCE((SELECT bool_or(lower(o.option_value) IN ('true','on','1')) \
                              FROM pg_options_to_table(c.reloptions) o \
                              WHERE o.option_name = 'security_invoker'), false) \
                      AS security_invoker \
             FROM pg_class c WHERE c.oid = to_regclass('{literal}')"
        );

        let rows = self.adapter.execute_raw_query(&sql).await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let cell_bool = |k: &str| row.get(k).and_then(serde_json::Value::as_bool).unwrap_or(false);
        Ok(Some(RelationRls {
            is_view:          row.get("kind").and_then(serde_json::Value::as_str) == Some("v"),
            rls_enabled:      cell_bool("rls_enabled"),
            has_policy:       row
                .get("policy_count")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|n| n != "0"),
            security_invoker: cell_bool("security_invoker"),
        }))
    }

    /// Apply the RLS enforcement policy from `CacheConfig`.
    ///
    /// Runs [`validate_rls_active`](Self::validate_rls_active) and handles the result
    /// according to `enforcement`:
    /// - [`RlsEnforcement::Error`]: propagates the error (default)
    /// - [`RlsEnforcement::Warn`]: logs a warning and returns `Ok(())`
    /// - [`RlsEnforcement::Off`]: skips the check entirely
    ///
    /// # Errors
    ///
    /// Returns the error from `validate_rls_active` when enforcement is `Error`.
    pub async fn enforce_rls(
        &self,
        schema: &CompiledSchema,
        enforcement: RlsEnforcement,
    ) -> Result<()> {
        if enforcement == RlsEnforcement::Off {
            return Ok(());
        }

        match self.validate_rls_active(schema).await {
            Ok(()) => Ok(()),
            Err(e) => match enforcement {
                RlsEnforcement::Error => Err(e),
                RlsEnforcement::Warn => {
                    tracing::warn!(
                        "RLS check failed (rls_enforcement = \"warn\"): {}. \
                         Cross-tenant cache leakage is possible.",
                        e
                    );
                    Ok(())
                },
                RlsEnforcement::Off => Ok(()), // unreachable but exhaustive
            },
        }
    }
}

impl<A: DatabaseAdapter + Clone> Clone for CachedDatabaseAdapter<A> {
    fn clone(&self) -> Self {
        Self {
            adapter:              self.adapter.clone(),
            cache:                Arc::clone(&self.cache),
            schema_version:       self.schema_version.clone(),
            view_ttl_overrides:   self.view_ttl_overrides.clone(),
            cacheable_views:      self.cacheable_views.clone(),
            view_secondary_views: self.view_secondary_views.clone(),
            opt_in_mode:          self.opt_in_mode,
            has_rls:              self.has_rls,
            fact_table_config:    self.fact_table_config.clone(),
            version_provider:     Arc::clone(&self.version_provider),
            cascade_invalidator:  self.cascade_invalidator.clone(),
        }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl<A: DatabaseAdapter> DatabaseAdapter for CachedDatabaseAdapter<A> {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection_impl(view, projection, where_clause, limit, offset, order_by)
            .await
            .map(Arc::unwrap_or_clone)
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query_impl(view, where_clause, limit, offset, order_by)
            .await
            .map(Arc::unwrap_or_clone)
    }

    async fn execute_with_projection_arc(
        &self,
        request: &crate::db::ProjectionRequest<'_>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        self.execute_with_projection_impl(
            request.view,
            request.projection,
            request.where_clause,
            request.limit,
            request.offset,
            request.order_by,
        )
        .await
    }

    async fn execute_where_query_arc(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Arc<Vec<JsonbValue>>> {
        self.execute_where_query_impl(view, where_clause, limit, offset, order_by).await
    }

    fn database_type(&self) -> DatabaseType {
        self.adapter.database_type()
    }

    async fn health_check(&self) -> Result<()> {
        self.adapter.health_check().await
    }

    fn pool_metrics(&self) -> PoolMetrics {
        self.adapter.pool_metrics()
    }

    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Use the aggregation caching method which handles fact table versioning
        self.execute_aggregation_query(sql).await
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Parameterized aggregate results are not cacheable by SQL template alone;
        // delegate directly to the underlying adapter to avoid caching with an
        // incorrect key (the same SQL template with different params would return
        // different results).
        self.adapter.execute_parameterized_aggregate(sql, params).await
    }

    async fn execute_parameterized_aggregate_with_session(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Not cached (see execute_parameterized_aggregate); forward with session
        // affinity so current_setting()-backed aggregate RLS sees the values (#329).
        self.adapter
            .execute_parameterized_aggregate_with_session(sql, params, session_vars)
            .await
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Mutations are never cached — always delegate to the underlying adapter
        self.adapter.execute_function_call(function_name, args).await
    }

    async fn execute_function_call_with_session(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Mutations are never cached; pass through with session affinity.
        self.adapter
            .execute_function_call_with_session(function_name, args, session_vars)
            .await
    }

    async fn execute_function_call_with_changelog(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        changelog: Option<&ChangeLogWrite<'_>>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Mutations are never cached; pass through so the in-txn outbox write
        // reaches the underlying adapter (the Change Spine transactional outbox).
        self.adapter
            .execute_function_call_with_changelog(function_name, args, session_vars, changelog)
            .await
    }

    // Mutation-strategy delegation: a cache-wrapped adapter must report and use the
    // inner adapter's strategy, so a wrapped SqliteAdapter still dispatches DirectSql
    // instead of falling back to the trait defaults (FunctionCall / Unsupported).
    fn supports_mutations(&self) -> bool {
        self.adapter.supports_mutations()
    }

    fn mutation_strategy(&self) -> MutationStrategy {
        self.adapter.mutation_strategy()
    }

    async fn execute_direct_mutation(
        &self,
        ctx: &DirectMutationContext<'_>,
    ) -> Result<Vec<serde_json::Value>> {
        // Mutations are never cached; pass through to the underlying adapter.
        self.adapter.execute_direct_mutation(ctx).await
    }

    async fn execute_where_query_arc_with_session(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
        session_vars: &[(&str, &str)],
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // No session variables => preserve the cached read path unchanged.
        if session_vars.is_empty() {
            return self
                .execute_where_query_impl(view, where_clause, limit, offset, order_by)
                .await;
        }
        // Security: the result-cache key is NOT session-variable-aware, so a
        // tenant-scoped read (RLS via current_setting) could otherwise leak
        // another tenant's cached rows. Bypass the cache and run the read with
        // session affinity on the inner adapter. Tracked for a cache-key fix:
        // see #329 follow-up.
        self.adapter
            .execute_where_query_arc_with_session(
                view,
                where_clause,
                limit,
                offset,
                order_by,
                session_vars,
            )
            .await
    }

    async fn execute_with_projection_arc_with_session(
        &self,
        request: &crate::db::ProjectionRequest<'_>,
        session_vars: &[(&str, &str)],
    ) -> Result<Arc<Vec<JsonbValue>>> {
        // No session variables => preserve the cached read path unchanged.
        if session_vars.is_empty() {
            return self
                .execute_with_projection_impl(
                    request.view,
                    request.projection,
                    request.where_clause,
                    request.limit,
                    request.offset,
                    request.order_by,
                )
                .await;
        }
        // Security: see execute_where_query_arc_with_session — bypass the
        // non-tenant-aware cache for session-scoped reads.
        self.adapter
            .execute_with_projection_arc_with_session(request, session_vars)
            .await
    }

    async fn invalidate_views(&self, views: &[fraiseql_db::ViewName]) -> Result<u64> {
        // Delegate to the inherent (synchronous) method which handles cascade
        // expansion and cache eviction.
        CachedDatabaseAdapter::invalidate_views(self, views)
    }

    async fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        CachedDatabaseAdapter::invalidate_by_entity(self, entity_type, entity_id)
    }

    /// The operator-visible snapshot of the cache that actually serves queries (#941).
    ///
    /// `None` when the cache is disabled: an operator asking "what is cached?" of a
    /// server running `cache_enabled = false` must be told there is no cache, not
    /// handed a zero-entry snapshot of one.
    fn result_cache_stats(&self) -> Option<fraiseql_db::ResultCacheStats> {
        if !self.cache.is_enabled() {
            return None;
        }
        // Settle moka's pending writes first: without this an entry cached moments ago
        // is not counted, and the endpoint reports an empty cache that is not empty.
        self.cache.run_pending_tasks();
        let metrics = self.cache.metrics().ok()?;
        let config = self.cache.config();
        Some(fraiseql_db::ResultCacheStats {
            entries:       metrics.size,
            hits:          metrics.hits,
            misses:        metrics.misses,
            invalidations: metrics.invalidations,
            memory_bytes:  metrics.memory_bytes,
            ttl_seconds:   config.ttl_seconds,
            max_entries:   config.max_entries,
        })
    }

    async fn clear_result_cache(&self) -> Result<Option<usize>> {
        if !self.cache.is_enabled() {
            return Ok(None);
        }
        self.cache.run_pending_tasks();
        let before = self.cache.metrics().map(|m| m.size).unwrap_or_default();
        self.cache.clear()?;
        Ok(Some(before))
    }

    async fn bump_fact_table_versions(&self, tables: &[String]) -> Result<()> {
        self.bump_fact_table_versions_impl(tables).await
    }

    async fn query_stats(&self, limit: u32) -> Result<Vec<fraiseql_db::QueryStatEntry>> {
        self.adapter.query_stats(limit).await
    }

    async fn query_stats_by_id(&self, id: &str) -> Result<Option<fraiseql_db::QueryStatEntry>> {
        self.adapter.query_stats_by_id(id).await
    }

    async fn reset_query_stats(&self) -> Result<()> {
        self.adapter.reset_query_stats().await
    }

    fn on_schema_reload(&self) {
        // Clear all cached entries — they reference the old schema's content hash
        // and per-view TTL configuration.
        let _ = self.cache.clear();
    }
}

impl<A: SupportsMutations + Send + Sync> SupportsMutations for CachedDatabaseAdapter<A> {}
