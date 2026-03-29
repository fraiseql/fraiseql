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
    collections::HashMap,
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
        DatabaseAdapter, DatabaseType, PoolMetrics, SupportsMutations, WhereClause,
        types::{JsonbValue, OrderByClause},
    },
    error::{FraiseQLError, Result},
    schema::CompiledSchema,
};

mod mutation;
mod query;
#[cfg(test)]
mod tests;

pub use query::view_name_to_entity_type;

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
/// // After mutation, invalidate
/// let invalidation = InvalidationContext::for_mutation(
///     "createUser",
///     vec!["v_user".to_string()]
/// );
/// adapter.invalidate_views(&invalidation.modified_views)?;
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
    ///     ("v_country".to_string(), 3600_u64),   // 1 h for reference data
    ///     ("v_live_price".to_string(), 0_u64),   // never cache live data
    /// ]);
    /// let adapter = CachedDatabaseAdapter::new(db, cache, "1.0.0".to_string())
    ///     .with_view_ttl_overrides(overrides);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_view_ttl_overrides(mut self, overrides: HashMap<String, u64>) -> Self {
        self.view_ttl_overrides = overrides;
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

    /// Populate per-view TTL overrides from a compiled schema.
    ///
    /// For each query that has `cache_ttl_seconds` set and a non-null `sql_source`,
    /// this maps the view name → TTL so the cache adapter uses the per-query TTL
    /// instead of the global default.
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
    ///     .with_ttl_overrides_from_schema(&schema);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_ttl_overrides_from_schema(mut self, schema: &CompiledSchema) -> Self {
        for query in &schema.queries {
            if let (Some(view), Some(ttl)) = (&query.sql_source, query.cache_ttl_seconds) {
                self.view_ttl_overrides.insert(view.clone(), ttl);
            }
        }
        self
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

    /// Verify that Row-Level Security is active on the database connection.
    ///
    /// Call this during server initialization when both caching and multi-tenancy
    /// (`schema.is_multi_tenant()`) are enabled. Without RLS, users sharing the same
    /// query parameters will receive the same cached response regardless of tenant.
    ///
    /// # What this checks
    ///
    /// Runs `SELECT current_setting('row_security', true) AS rls_setting`. The result
    /// must be `'on'` or `'force'` for the check to pass. Non-PostgreSQL databases
    /// (which return an error or unsupported) are treated as "RLS not active".
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Configuration`] if RLS appears inactive.
    pub async fn validate_rls_active(&self) -> Result<()> {
        let result = self
            .adapter
            .execute_raw_query("SELECT current_setting('row_security', true) AS rls_setting")
            .await;

        let rls_active = match result {
            Ok(rows) => rows
                .first()
                .and_then(|row| row.get("rls_setting"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| s == "on" || s == "force"),
            Err(_) => false, // Non-PostgreSQL or query failure: RLS not active
        };

        if rls_active {
            Ok(())
        } else {
            Err(FraiseQLError::Configuration {
                message: "Caching is enabled in a multi-tenant schema but Row-Level Security \
                          does not appear to be active on the database. This would allow \
                          cross-tenant data leakage through the cache. \
                          Either disable caching, enable RLS, or set \
                          `rls_enforcement = \"off\"` in CacheConfig for single-tenant \
                          deployments."
                    .to_string(),
            })
        }
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
    pub async fn enforce_rls(&self, enforcement: RlsEnforcement) -> Result<()> {
        if enforcement == RlsEnforcement::Off {
            return Ok(());
        }

        match self.validate_rls_active().await {
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
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection_impl(view, projection, where_clause, limit, offset)
            .await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query_impl(view, where_clause, limit, offset).await
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

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Mutations are never cached — always delegate to the underlying adapter
        self.adapter.execute_function_call(function_name, args).await
    }

    async fn invalidate_views(&self, views: &[String]) -> Result<u64> {
        // Delegate to the inherent (synchronous) method which handles cascade
        // expansion and cache eviction.
        CachedDatabaseAdapter::invalidate_views(self, views)
    }

    async fn invalidate_by_entity(&self, entity_type: &str, entity_id: &str) -> Result<u64> {
        CachedDatabaseAdapter::invalidate_by_entity(self, entity_type, entity_id)
    }

    async fn bump_fact_table_versions(&self, tables: &[String]) -> Result<()> {
        self.bump_fact_table_versions_impl(tables).await
    }
}

impl<A: SupportsMutations + Send + Sync> SupportsMutations for CachedDatabaseAdapter<A> {}
