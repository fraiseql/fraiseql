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
//!     .execute_where_query("v_user", None, Some(10), None)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::json;

use super::{
    cascade_invalidator::CascadeInvalidator,
    fact_table_version::{
        FactTableCacheConfig, FactTableVersionProvider, FactTableVersionStrategy,
    },
    key::generate_cache_key,
    result::QueryResultCache,
};
use crate::{
    db::{DatabaseAdapter, DatabaseType, PoolMetrics, WhereClause, types::JsonbValue},
    error::Result,
    schema::CompiledSchema,
};

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
/// let users1 = adapter.execute_where_query("v_user", None, None, None).await?;
///
/// // Second query - cache hit (fast!)
/// let users2 = adapter.execute_where_query("v_user", None, None, None).await?;
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
    /// * `schema_version` - Uniquely identifies the compiled schema. Use
    ///   `schema.content_hash()` (NOT `env!("CARGO_PKG_VERSION")`) so that any schema
    ///   content change automatically invalidates cached entries across deploys.
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
    pub fn fact_table_config(&self) -> &FactTableCacheConfig {
        &self.fact_table_config
    }

    /// Get the version provider for fact tables.
    #[must_use]
    pub fn version_provider(&self) -> &FactTableVersionProvider {
        &self.version_provider
    }
}

#[async_trait]
impl<A: DatabaseAdapter> DatabaseAdapter for CachedDatabaseAdapter<A> {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Short-circuit when cache is disabled: skip SHA-256 key generation and result clone.
        if !self.cache.is_enabled() {
            return self
                .adapter
                .execute_with_projection(view, projection, where_clause, limit)
                .await;
        }

        // Generate cache key including projection info
        let query_string = format!("query {{ {view} }}");
        let projection_info = projection.map(|p| &p.projection_template[..]).unwrap_or("");
        let variables = json!({
            "limit": limit,
            "projection": projection_info,
        });

        let cache_key =
            generate_cache_key(&query_string, &variables, where_clause, &self.schema_version);

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            return Ok((*cached_result).clone());
        }

        // Cache miss - execute via underlying adapter
        let result = self
            .adapter
            .execute_with_projection(view, projection, where_clause, limit)
            .await?;

        // Store in cache
        let ttl = self.view_ttl_overrides.get(view).copied();
        self.cache.put(cache_key, result.clone(), vec![view.to_string()], ttl, None)?;

        Ok(result)
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // Short-circuit when cache is disabled: skip SHA-256 key generation and result clone.
        if !self.cache.is_enabled() {
            return self.adapter.execute_where_query(view, where_clause, limit, offset).await;
        }

        // Generate cache key
        let query_string = format!("query {{ {view} }}");
        let variables = json!({
            "limit": limit,
            "offset": offset,
        });

        let cache_key =
            generate_cache_key(&query_string, &variables, where_clause, &self.schema_version);

        // Try cache first
        if let Some(cached_result) = self.cache.get(&cache_key)? {
            // Cache hit - return cached result
            return Ok((*cached_result).clone());
        }

        // Cache miss - execute query
        let result = self.adapter.execute_where_query(view, where_clause, limit, offset).await?;

        // Store in cache
        // View-level tracking (single view name).
        // Cascade invalidation via CascadeInvalidator expands this to transitively
        // dependent views when invalidate_views() is called.
        let ttl = self.view_ttl_overrides.get(view).copied();
        self.cache.put(
            cache_key,
            result.clone(),
            vec![view.to_string()], // accessed views
            ttl,
            None, // No entity-type index for WHERE queries
        )?;

        Ok(result)
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
        for table in tables {
            // Only act when this table uses the version-table strategy.
            // TimeBased and SchemaVersion strategies are invalidated by their own
            // mechanisms (clock / schema hash); no runtime bump is needed.
            if !matches!(
                self.fact_table_config.get_strategy(table),
                FactTableVersionStrategy::VersionTable
            ) {
                continue;
            }

            // Call the PostgreSQL function that increments the counter and returns
            // the new version.  The table name originates from
            // `MutationDefinition.invalidates_fact_tables`, which the CLI compiler
            // validates as a safe SQL identifier — no string interpolation needed.
            let rows = self
                .adapter
                .execute_function_call("bump_tf_version", &[serde_json::json!(table)])
                .await?;

            // Extract the new version number from the function result.
            // The function must return a single-column row with the incremented
            // integer.  Accept whatever column name the function uses.
            if let Some(new_version) = rows
                .first()
                .and_then(|row| row.values().find_map(serde_json::Value::as_i64))
            {
                self.version_provider.set_cached_version(table, new_version);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cache::CacheConfig, db::WhereOperator};

    /// Mock database adapter for testing.
    struct MockAdapter {
        /// Number of times execute_where_query was called.
        call_count:     std::sync::atomic::AtomicU32,
        /// Number of times execute_raw_query was called.
        raw_call_count: std::sync::atomic::AtomicU32,
    }

    impl MockAdapter {
        fn new() -> Self {
            Self {
                call_count:     std::sync::atomic::AtomicU32::new(0),
                raw_call_count: std::sync::atomic::AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            // Return sum of both call counts for backward compatibility
            self.call_count.load(std::sync::atomic::Ordering::SeqCst)
                + self.raw_call_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MockAdapter {
        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            // Return mock data (same as execute_where_query)
            Ok(vec![
                JsonbValue::new(json!({"id": 1, "name": "Alice"})),
                JsonbValue::new(json!({"id": 2, "name": "Bob"})),
            ])
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            // Return mock data
            Ok(vec![
                JsonbValue::new(json!({"id": 1, "name": "Alice"})),
                JsonbValue::new(json!({"id": 2, "name": "Bob"})),
            ])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections:  10,
                idle_connections:   5,
                active_connections: 3,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            self.raw_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // Return mock aggregation data
            let mut row = std::collections::HashMap::new();
            row.insert("count".to_string(), json!(42));
            Ok(vec![row])
        }

        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_cache_miss_then_hit() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        // First query - cache miss
        let result1 = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(result1.len(), 2);
        assert_eq!(adapter.inner().call_count(), 1);

        // Second query - cache hit
        let result2 = adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(result2.len(), 2);
        assert_eq!(adapter.inner().call_count(), 1); // Still 1 - cache hit!
    }

    #[tokio::test]
    async fn test_different_where_clauses_produce_different_cache_entries() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        let where1 = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(1),
        };

        let where2 = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(2),
        };

        // Query 1
        adapter.execute_where_query("v_user", Some(&where1), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Query 2 - different WHERE - should miss cache
        adapter.execute_where_query("v_user", Some(&where2), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2);
    }

    #[tokio::test]
    async fn test_invalidation_clears_cache() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Query 1 - cache miss
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Query 2 - cache hit
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Invalidate
        let invalidated = adapter.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(invalidated, 1);

        // Query 3 - cache miss again (was invalidated)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2);
    }

    #[tokio::test]
    async fn test_different_limits_produce_different_cache_entries() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // Query with limit 10
        adapter.execute_where_query("v_user", None, Some(10), None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Query with limit 20 - should miss cache
        adapter.execute_where_query("v_user", None, Some(20), None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2);
    }

    #[tokio::test]
    async fn test_cache_disabled() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::disabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // First query
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Second query - should NOT hit cache (cache disabled)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2);
    }

    /// Test that ALL queries are cached — including those with no WHERE clause or small LIMIT.
    ///
    /// The previous "simple query bypass" (Issue #40 workaround) was removed.
    /// It skipped caching for `where_clause.is_none() && limit <= 1000`, which
    /// prevented caching for public / unauthenticated endpoints.  The cache
    /// overhead (SHA-256 + LRU lookup ≈ 100-150 µs) is negligible relative to a
    /// database round-trip; the bypass was a premature optimisation.
    #[tokio::test]
    async fn test_all_queries_are_cached() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // Query with no WHERE, no LIMIT — first call misses the cache
        adapter.execute_where_query("v_user", None, None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Identical query — now a cache hit, DB not called again
        adapter.execute_where_query("v_user", None, None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 1); // Still 1 - cache hit!

        // Query with small LIMIT — different cache key (different limit), so a miss
        adapter.execute_where_query("v_user", None, Some(1000), None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2);

        // Identical small-limit query — cache hit
        adapter.execute_where_query("v_user", None, Some(1000), None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2); // Still 2 - cache hit!

        // Query with WHERE clause — cached normally
        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(1),
        };
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 3);

        // Identical WHERE query — cache hit
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 3); // Still 3 - cache hit!
    }

    #[tokio::test]
    async fn test_schema_version_change_invalidates_cache() {
        let cache = Arc::new(QueryResultCache::new(CacheConfig::enabled()));
        let version_provider = Arc::new(FactTableVersionProvider::default());

        // Adapter with version 1.0.0
        let mock1 = MockAdapter::new();
        let adapter_v1 = CachedDatabaseAdapter {
            adapter:              mock1,
            cache:                Arc::clone(&cache),
            schema_version:       "1.0.0".to_string(),
            view_ttl_overrides:   HashMap::new(),
            fact_table_config:    FactTableCacheConfig::default(),
            version_provider:     Arc::clone(&version_provider),
            cascade_invalidator:  None,
        };

        // Query with v1
        adapter_v1.execute_where_query("v_user", None, None, None).await.unwrap();

        // Create new adapter with version 2.0.0 (same cache!)
        let mock2 = MockAdapter::new();
        let adapter_v2 = CachedDatabaseAdapter {
            adapter:              mock2,
            cache:                Arc::clone(&cache),
            schema_version:       "2.0.0".to_string(),
            view_ttl_overrides:   HashMap::new(),
            fact_table_config:    FactTableCacheConfig::default(),
            version_provider:     Arc::clone(&version_provider),
            cascade_invalidator:  None,
        };

        // Query with v2 - should miss cache (different schema version)
        adapter_v2.execute_where_query("v_user", None, None, None).await.unwrap();
        assert_eq!(adapter_v2.inner().call_count(), 1); // Cache miss
    }

    #[tokio::test]
    async fn test_forwards_database_type() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    }

    #[tokio::test]
    async fn test_forwards_health_check() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        adapter.health_check().await.unwrap();
    }

    #[tokio::test]
    async fn test_forwards_pool_metrics() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        let metrics = adapter.pool_metrics();
        assert_eq!(metrics.total_connections, 10);
        assert_eq!(metrics.idle_connections, 5);
    }

    #[tokio::test]
    async fn test_inner_and_cache_accessors() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // Test inner()
        assert_eq!(adapter.inner().call_count(), 0);

        // Test cache()
        let cache_metrics = adapter.cache().metrics().unwrap();
        assert_eq!(cache_metrics.hits, 0);

        // Test schema_version()
        assert_eq!(adapter.schema_version(), "1.0.0");
    }

    // ===== E2E Tests: Entity-Level Cascade Invalidation =====

    use super::super::cascade_response_parser::CascadeResponseParser;

    #[tokio::test]
    async fn test_invalidate_cascade_entities_with_single_entity() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache with query reading from v_user
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Cache hit on second query
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Parse cascade response with single User entity
        let cascade_response = json!({
            "createPost": {
                "cascade": {
                    "updated": [
                        {
                            "__typename": "User",
                            "id": "550e8400-e29b-41d4-a716-446655440000"
                        }
                    ],
                    "deleted": []
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 1 view (v_user)
        assert_eq!(invalidated, 1);

        // Next query should be a cache miss (was invalidated)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2);
    }

    #[tokio::test]
    async fn test_invalidate_cascade_entities_with_multiple_entities() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache with multiple views (WHERE clause required to enter cache)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_comment", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 3);

        // Cascade with multiple entity types
        let cascade_response = json!({
            "updateUser": {
                "cascade": {
                    "updated": [
                        {"__typename": "User", "id": "u-1"},
                        {"__typename": "Post", "id": "p-1"},
                        {"__typename": "Comment", "id": "c-1"}
                    ],
                    "deleted": []
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 3 views
        assert_eq!(invalidated, 3);

        // All queries should now be cache misses (same WHERE clause, different cache key after invalidation)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_comment", Some(&where_clause), None, None)
            .await
            .unwrap();
        // Should have 6 total calls (3 initial + 3 after invalidation)
        assert_eq!(adapter.inner().call_count(), 6);
    }

    #[tokio::test]
    async fn test_invalidate_cascade_entities_with_deleted_entities() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache with both views (WHERE clause required to enter cache)
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_comment", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2);

        // Cascade with deleted entities
        let cascade_response = json!({
            "deletePost": {
                "cascade": {
                    "updated": [],
                    "deleted": [
                        {"__typename": "Post", "id": "p-123"},
                        {"__typename": "Comment", "id": "c-456"}
                    ]
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 2 views (v_post and v_comment)
        assert_eq!(invalidated, 2);

        // Both queries should now be cache misses after invalidation
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_comment", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 4);
    }

    #[tokio::test]
    async fn test_invalidate_cascade_entities_with_no_cascade_field() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Response without cascade field (mutation with no side effects)
        let cascade_response = json!({
            "createPost": {
                "post": {
                    "id": "p-123",
                    "title": "Hello"
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 0 views (no cascade data)
        assert_eq!(invalidated, 0);

        // Cache should still be valid - should be a cache hit
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1); // Still 1 - cache hit!
    }

    #[tokio::test]
    async fn test_invalidate_cascade_entities_mixed_updated_and_deleted() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache (WHERE clause required to enter cache)
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2);

        // Cascade with both updated and deleted entities
        let cascade_response = json!({
            "mutation": {
                "cascade": {
                    "updated": [
                        {"__typename": "User", "id": "u-1"}
                    ],
                    "deleted": [
                        {"__typename": "Post", "id": "p-1"}
                    ]
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 2 views (v_user and v_post)
        assert_eq!(invalidated, 2);

        // Both queries should now be cache misses after invalidation
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 4);
    }

    #[tokio::test]
    async fn test_cascade_invalidation_deduplicates_entity_types() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Cascade with multiple instances of the same entity type
        // (should deduplicate to single v_user invalidation)
        let cascade_response = json!({
            "mutation": {
                "cascade": {
                    "updated": [
                        {"__typename": "User", "id": "u-1"},
                        {"__typename": "User", "id": "u-2"},
                        {"__typename": "User", "id": "u-3"}
                    ],
                    "deleted": []
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate only 1 view (v_user), not 3
        // (deduplicates by entity type)
        assert_eq!(invalidated, 1);
    }

    #[tokio::test]
    async fn test_cascade_invalidation_vs_view_invalidation_same_result() {
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Test 1: Cascade-based invalidation
        let mock1 = MockAdapter::new();
        let cache1 = QueryResultCache::new(CacheConfig::enabled());
        let adapter1 = CachedDatabaseAdapter::new(mock1, cache1, "1.0.0".to_string());

        adapter1
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter1
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();

        let cascade_response = json!({
            "mutation": {
                "cascade": {
                    "updated": [
                        {"__typename": "User", "id": "u-1"},
                        {"__typename": "Post", "id": "p-1"}
                    ],
                    "deleted": []
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated_cascade =
            adapter1.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Test 2: View-level invalidation (old approach)
        let mock2 = MockAdapter::new();
        let cache2 = QueryResultCache::new(CacheConfig::enabled());
        let adapter2 = CachedDatabaseAdapter::new(mock2, cache2, "1.0.0".to_string());

        adapter2
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        adapter2
            .execute_where_query("v_post", Some(&where_clause), None, None)
            .await
            .unwrap();

        let invalidated_views = adapter2
            .invalidate_views(&["v_user".to_string(), "v_post".to_string()])
            .unwrap();

        // Both approaches should invalidate the same number of views
        assert_eq!(invalidated_cascade, 2);
        assert_eq!(invalidated_views, 2);
    }

    #[tokio::test]
    async fn test_cascade_invalidation_with_empty_cascade() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // WHERE clause present (exercises the cache path)
        let where_clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("active"),
        };

        // Pre-populate cache
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Empty cascade (no entities affected)
        let cascade_response = json!({
            "mutation": {
                "cascade": {
                    "updated": [],
                    "deleted": []
                }
            }
        });

        let parser = CascadeResponseParser::new();
        let invalidated = adapter.invalidate_cascade_entities(&cascade_response, &parser).unwrap();

        // Should invalidate 0 views
        assert_eq!(invalidated, 0);

        // Cache should still be valid
        adapter
            .execute_where_query("v_user", Some(&where_clause), None, None)
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1); // Cache hit
    }

    // ===== Aggregation Caching Tests =====

    #[test]
    fn test_extract_fact_table_from_sql() {
        // Basic case
        assert_eq!(
            CachedDatabaseAdapter::<MockAdapter>::extract_fact_table_from_sql(
                "SELECT SUM(revenue) FROM tf_sales WHERE year = 2024"
            ),
            Some("tf_sales".to_string())
        );

        // With schema
        assert_eq!(
            CachedDatabaseAdapter::<MockAdapter>::extract_fact_table_from_sql(
                "SELECT COUNT(*) FROM   tf_page_views"
            ),
            Some("tf_page_views".to_string())
        );

        // Case insensitive
        assert_eq!(
            CachedDatabaseAdapter::<MockAdapter>::extract_fact_table_from_sql(
                "select sum(x) FROM TF_EVENTS"
            ),
            Some("tf_events".to_string())
        );

        // Not a fact table
        assert_eq!(
            CachedDatabaseAdapter::<MockAdapter>::extract_fact_table_from_sql(
                "SELECT * FROM users WHERE id = 1"
            ),
            None
        );

        // No FROM clause
        assert_eq!(
            CachedDatabaseAdapter::<MockAdapter>::extract_fact_table_from_sql("SELECT 1 + 1"),
            None
        );
    }

    #[test]
    fn test_generate_aggregation_cache_key() {
        let key1 = CachedDatabaseAdapter::<MockAdapter>::generate_aggregation_cache_key(
            "SELECT SUM(x) FROM tf_sales",
            "1.0.0",
            Some("tv:42"),
        );

        let key2 = CachedDatabaseAdapter::<MockAdapter>::generate_aggregation_cache_key(
            "SELECT SUM(x) FROM tf_sales",
            "1.0.0",
            Some("tv:43"), // Different version
        );

        let key3 = CachedDatabaseAdapter::<MockAdapter>::generate_aggregation_cache_key(
            "SELECT SUM(x) FROM tf_sales",
            "2.0.0", // Different schema
            Some("tv:42"),
        );

        // Keys should start with "agg:" prefix
        assert!(key1.starts_with("agg:"));
        assert!(key2.starts_with("agg:"));
        assert!(key3.starts_with("agg:"));

        // Different versions/schema produce different keys
        assert_ne!(key1, key2);
        assert_ne!(key1, key3);
        assert_ne!(key2, key3);
    }

    #[tokio::test]
    async fn test_aggregation_caching_time_based() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Configure time-based caching for tf_sales
        let mut ft_config = FactTableCacheConfig::default();
        ft_config
            .set_strategy("tf_sales", FactTableVersionStrategy::TimeBased { ttl_seconds: 300 });

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            mock,
            cache,
            "1.0.0".to_string(),
            ft_config,
        );

        // First query - cache miss
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Second query - cache hit (same time bucket)
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1); // Still 1 - cache hit!
    }

    #[tokio::test]
    async fn test_aggregation_caching_schema_version() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Configure schema version caching for tf_historical_rates
        let mut ft_config = FactTableCacheConfig::default();
        ft_config.set_strategy("tf_historical_rates", FactTableVersionStrategy::SchemaVersion);

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            mock,
            cache,
            "1.0.0".to_string(),
            ft_config,
        );

        // First query - cache miss
        let _ = adapter
            .execute_aggregation_query("SELECT AVG(rate) FROM tf_historical_rates")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Second query - cache hit
        let _ = adapter
            .execute_aggregation_query("SELECT AVG(rate) FROM tf_historical_rates")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1); // Cache hit!
    }

    #[tokio::test]
    async fn test_aggregation_caching_disabled_by_default() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::default());

        // Default config has Disabled strategy
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        // First query - no caching
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Second query - still no caching (disabled)
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2); // No cache - hits DB again
    }

    #[tokio::test]
    async fn test_aggregation_caching_non_fact_table() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        // Even with caching configured, non-fact tables bypass cache
        let ft_config = FactTableCacheConfig::with_default(FactTableVersionStrategy::SchemaVersion);
        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            mock,
            cache,
            "1.0.0".to_string(),
            ft_config,
        );

        // Query on regular table - never cached
        let _ = adapter.execute_aggregation_query("SELECT COUNT(*) FROM users").await.unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        let _ = adapter.execute_aggregation_query("SELECT COUNT(*) FROM users").await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2); // No cache
    }

    #[tokio::test]
    async fn test_aggregation_caching_different_queries() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let mut ft_config = FactTableCacheConfig::default();
        ft_config.set_strategy("tf_sales", FactTableVersionStrategy::SchemaVersion);

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            mock,
            cache,
            "1.0.0".to_string(),
            ft_config,
        );

        // Query 1
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales WHERE year = 2024")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 1);

        // Query 2 - different query, different cache key
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales WHERE year = 2023")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2); // Cache miss - different query

        // Query 1 again - cache hit
        let _ = adapter
            .execute_aggregation_query("SELECT SUM(revenue) FROM tf_sales WHERE year = 2024")
            .await
            .unwrap();
        assert_eq!(adapter.inner().call_count(), 2); // Cache hit
    }

    #[tokio::test]
    async fn test_fact_table_config_accessor() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let mut ft_config = FactTableCacheConfig::default();
        ft_config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            mock,
            cache,
            "1.0.0".to_string(),
            ft_config,
        );

        // Verify config is accessible
        assert_eq!(
            adapter.fact_table_config().get_strategy("tf_sales"),
            &FactTableVersionStrategy::VersionTable
        );
        assert_eq!(
            adapter.fact_table_config().get_strategy("tf_other"),
            &FactTableVersionStrategy::Disabled
        );
    }

    // ===== Cascade Invalidator Tests =====

    #[tokio::test]
    async fn test_cascade_invalidator_expands_transitive_views() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());

        let mut cascade = CascadeInvalidator::new();
        cascade.add_dependency("v_user_stats", "v_user").unwrap();
        cascade.add_dependency("v_dashboard", "v_user_stats").unwrap();

        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string())
            .with_cascade_invalidator(cascade);

        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(1),
        };

        // Populate cache with all three views
        adapter.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_user_stats", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_dashboard", Some(&where_clause), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 3);

        // Invalidate only the base view; cascade should evict dependents too
        let count = adapter.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(count, 3, "All three views should be invalidated via cascade");

        // All three should now be cache misses
        adapter.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_user_stats", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_dashboard", Some(&where_clause), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 6, "All three should be cache misses after cascade");
    }

    #[tokio::test]
    async fn test_no_cascade_invalidator_only_direct_views() {
        let mock = MockAdapter::new();
        let cache = QueryResultCache::new(CacheConfig::enabled());
        // No cascade invalidator — only direct view invalidation
        let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(1),
        };

        adapter.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_user_stats", Some(&where_clause), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 2);

        // Only v_user is invalidated — v_user_stats remains cached
        let count = adapter.invalidate_views(&["v_user".to_string()]).unwrap();
        assert_eq!(count, 1);

        adapter.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
        adapter.execute_where_query("v_user_stats", Some(&where_clause), None, None).await.unwrap();
        assert_eq!(adapter.inner().call_count(), 3, "Only v_user should be a miss; v_user_stats is still cached");
    }

    // ── bump_fact_table_versions tests ─────────────────────────────────────

    /// Adapter whose execute_function_call simulates bump_tf_version by returning
    /// the incremented version (starting at 2).
    struct BumpAdapter {
        bump_call_count: std::sync::atomic::AtomicU32,
    }

    impl BumpAdapter {
        fn new() -> Self {
            Self { bump_call_count: std::sync::atomic::AtomicU32::new(0) }
        }

        fn bump_call_count(&self) -> u32 {
            self.bump_call_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DatabaseAdapter for BumpAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections:  1,
                idle_connections:   1,
                active_connections: 0,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_function_call(
            &self,
            function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            if function_name == "bump_tf_version" {
                let n = self.bump_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let new_version = i64::from(n) + 2; // start at 2, then 3, 4, ...
                let mut row = std::collections::HashMap::new();
                row.insert("bump_tf_version".to_string(), json!(new_version));
                Ok(vec![row])
            } else {
                Ok(vec![])
            }
        }
    }

    #[tokio::test]
    async fn test_bump_fact_table_versions_updates_version_cache() {
        let mut ft_config = FactTableCacheConfig::default();
        ft_config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            BumpAdapter::new(),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
            ft_config,
        );

        // Version not yet cached
        assert!(adapter.version_provider().get_cached_version("tf_sales").is_none());

        // Bump
        adapter
            .bump_fact_table_versions(&["tf_sales".to_string()])
            .await
            .unwrap();

        // bump_tf_version was called once
        assert_eq!(adapter.inner().bump_call_count(), 1);

        // Version is now cached (inner returned 2)
        assert_eq!(adapter.version_provider().get_cached_version("tf_sales"), Some(2));
    }

    #[tokio::test]
    async fn test_bump_fact_table_versions_skips_non_version_table_strategy() {
        let mut ft_config = FactTableCacheConfig::default();
        ft_config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);
        ft_config.set_strategy("tf_events", FactTableVersionStrategy::TimeBased { ttl_seconds: 300 });
        ft_config.set_strategy("tf_hist", FactTableVersionStrategy::SchemaVersion);

        let adapter = CachedDatabaseAdapter::with_fact_table_config(
            BumpAdapter::new(),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
            ft_config,
        );

        // Mix of strategies — only tf_sales should trigger a bump_tf_version call
        adapter
            .bump_fact_table_versions(&[
                "tf_sales".to_string(),
                "tf_events".to_string(),
                "tf_hist".to_string(),
            ])
            .await
            .unwrap();

        assert_eq!(adapter.inner().bump_call_count(), 1, "Only VersionTable strategy calls bump_tf_version");
        assert!(adapter.version_provider().get_cached_version("tf_sales").is_some());
        assert!(adapter.version_provider().get_cached_version("tf_events").is_none());
        assert!(adapter.version_provider().get_cached_version("tf_hist").is_none());
    }

    #[tokio::test]
    async fn test_bump_fact_table_versions_empty_list_is_noop() {
        let adapter = CachedDatabaseAdapter::new(
            BumpAdapter::new(),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
        );

        adapter.bump_fact_table_versions(&[]).await.unwrap();
        assert_eq!(adapter.inner().bump_call_count(), 0);
    }
}
