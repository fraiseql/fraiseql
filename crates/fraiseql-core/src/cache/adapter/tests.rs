#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::iter_on_single_items)] // Reason: test uses single-element iter for clarity

use async_trait::async_trait;
use fraiseql_db::ViewName;
use serde_json::json;

use super::*;
use crate::{
    backend::WhereOperator,
    cache::{CacheConfig, FactTableVersionStrategy},
    schema::CompiledSchema,
};

/// Mock database adapter for testing.
struct MockAdapter {
    /// Number of times `execute_where_query` was called.
    call_count:     std::sync::atomic::AtomicU32,
    /// Number of times `execute_raw_query` was called.
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

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for MockAdapter {
    // A test double: the session variables a read carries are accepted (#1115).
    fn applies_session_variables(&self) -> bool {
        true
    }

    // A test double that answers every projection read as text.
    async fn execute_with_projection_raw_with_session(
        &self,
        _request: &crate::backend::ProjectionRequest<'_>,
        _session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Option<fraiseql_db::RawRows>> {
        let mut rows = fraiseql_db::RawRows::new();
        rows.push(br#"{"id" : 1}"#);
        Ok(Some(rows))
    }

    // A test double: a function source is answered like a relation, so a test
    // counts what reaches the database.
    async fn execute_with_projection_arc_with_session(
        &self,
        request: &crate::backend::ProjectionRequest<'_>,
        _session_vars: &[(&str, &str)],
        _routing: ReadRouting,
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

    // A measurement the wrapper must pass through, not answer with the trait's `None` (#633).
    async fn schema_storage_bytes(&self, schema: &str) -> Result<Option<u64>> {
        Ok(Some(schema.len() as u64))
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
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
        _order_by: Option<&[OrderByClause]>,
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

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        self.raw_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut row = std::collections::HashMap::new();
        row.insert("count".to_string(), json!(42));
        Ok(vec![row])
    }

    async fn execute_parameterized_aggregate_exact(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
        _session_vars: &[(&str, &str)],
        _routing: fraiseql_db::types::ReadRouting,
        exact: &[&str],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![
            exact.iter().map(|c| ((*c).to_string(), json!("read as digits"))).collect(),
        ])
    }
}

// The writes this double answers, called by its `Writer` impl below.
impl MockAdapter {
    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait::async_trait]
impl crate::backend::traits::Writer for MockAdapter {
    async fn execute_write(
        &self,
        request: &crate::backend::traits::WriteRequest<'_>,
        gate: crate::backend::traits::MutationRowGate<'_>,
    ) -> std::result::Result<
        Vec<std::collections::HashMap<String, serde_json::Value>>,
        crate::error::FraiseQLError,
    > {
        {
            let rows = self.execute_function_call(request.function, request.args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }
}

/// #633: the cache wrapper forwards a schema's storage measurement. The trait's default
/// answers `None`, and a tenant whose quota cannot be measured is never refused.
#[tokio::test]
async fn schema_storage_bytes_is_forwarded_through_the_cache() {
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(MockAdapter::new(), cache, "1.0.0".to_string());
    assert_eq!(adapter.schema_storage_bytes("tenant_a").await.unwrap(), Some(8));
}

/// The server wraps every adapter in the cache, so the trait's default here would print
/// a number the wrapped adapter had already rounded instead of letting it read the digits.
#[tokio::test]
async fn an_exact_aggregate_is_read_by_the_wrapped_adapter() {
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(MockAdapter::new(), cache, "1.0.0".to_string());
    let rows = adapter
        .execute_parameterized_aggregate_exact(
            "SELECT SUM(amount) AS amount_sum FROM tf_sale",
            &[],
            &[],
            fraiseql_db::types::ReadRouting::Any,
            &["amount_sum"],
        )
        .await
        .unwrap();
    assert_eq!(rows[0]["amount_sum"], json!("read as digits"), "{rows:?}");
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
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("mock adapter must not fail on first query");
    assert_eq!(result1.len(), 2);
    assert_eq!(adapter.inner().call_count(), 1);

    // Second query - cache hit
    let result2 = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("mock adapter must not fail on second query");
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
    adapter
        .execute_where_query("v_user", Some(&where1), None, None, None)
        .await
        .expect("mock adapter must not fail");
    assert_eq!(adapter.inner().call_count(), 1);

    // Query 2 - different WHERE - should miss cache
    adapter
        .execute_where_query("v_user", Some(&where2), None, None, None)
        .await
        .expect("mock adapter must not fail");
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
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Query 2 - cache hit
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Invalidate
    adapter
        .invalidate_views(&[ViewName::from("v_user")])
        .expect("invalidate_views must succeed");

    // Query 3 - cache miss again (was invalidated)
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
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
    adapter
        .execute_where_query("v_user", None, Some(10), None, None)
        .await
        .expect("mock adapter must not fail");
    assert_eq!(adapter.inner().call_count(), 1);

    // Query with limit 20 - should miss cache
    adapter.execute_where_query("v_user", None, Some(20), None, None).await.unwrap();
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
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second query - should NOT hit cache (cache disabled)
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 2);
}

/// Test that ALL queries are cached — including those with no WHERE clause or small LIMIT.
///
/// The previous "simple query bypass" (Issue #40 workaround) was removed.
/// It skipped caching for `where_clause.is_none() && limit <= 1000`, which
/// prevented caching for public / unauthenticated endpoints.  The cache
/// overhead (ahash + LRU lookup) is negligible relative to a
/// database round-trip; the bypass was a premature optimisation.
#[tokio::test]
async fn test_all_queries_are_cached() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

    // Query with no WHERE, no LIMIT — first call misses the cache
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Identical query — now a cache hit, DB not called again
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1); // Still 1 - cache hit!

    // Query with small LIMIT — different cache key (different limit), so a miss
    adapter
        .execute_where_query("v_user", None, Some(1000), None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 2);

    // Identical small-limit query — cache hit
    adapter
        .execute_where_query("v_user", None, Some(1000), None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 2); // Still 2 - cache hit!

    // Query with WHERE clause — cached normally
    let where_clause = WhereClause::Field {
        path:     vec!["id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(1),
    };
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 3);

    // Identical WHERE query — cache hit
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
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
        cacheable_views:      std::collections::HashSet::new(),
        view_secondary_views: HashMap::new(),
        opt_in_mode:          false,
        has_rls:              false,
        fact_table_config:    FactTableCacheConfig::default(),
        version_provider:     Arc::clone(&version_provider),
        cascade_invalidator:  None,
        feed:                 None,
        collections:          crate::cache::collections::CollectionTagging::default(),
        by_id_sources:        Arc::default(),
    };

    // Query with v1
    adapter_v1.execute_where_query("v_user", None, None, None, None).await.unwrap();

    // Create new adapter with version 2.0.0 (same cache!)
    let mock2 = MockAdapter::new();
    let adapter_v2 = CachedDatabaseAdapter {
        adapter:              mock2,
        cache:                Arc::clone(&cache),
        schema_version:       "2.0.0".to_string(),
        view_ttl_overrides:   HashMap::new(),
        cacheable_views:      std::collections::HashSet::new(),
        view_secondary_views: HashMap::new(),
        opt_in_mode:          false,
        has_rls:              false,
        fact_table_config:    FactTableCacheConfig::default(),
        version_provider:     Arc::clone(&version_provider),
        cascade_invalidator:  None,
        feed:                 None,
        collections:          crate::cache::collections::CollectionTagging::default(),
        by_id_sources:        Arc::default(),
    };

    // Query with v2 - should miss cache (different schema version)
    adapter_v2.execute_where_query("v_user", None, None, None, None).await.unwrap();
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
    ft_config.set_strategy("tf_sales", FactTableVersionStrategy::TimeBased { ttl_seconds: 300 });

    let adapter =
        CachedDatabaseAdapter::with_fact_table_config(mock, cache, "1.0.0".to_string(), ft_config);

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

    let adapter =
        CachedDatabaseAdapter::with_fact_table_config(mock, cache, "1.0.0".to_string(), ft_config);

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
    let adapter =
        CachedDatabaseAdapter::with_fact_table_config(mock, cache, "1.0.0".to_string(), ft_config);

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

    let adapter =
        CachedDatabaseAdapter::with_fact_table_config(mock, cache, "1.0.0".to_string(), ft_config);

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

    let adapter =
        CachedDatabaseAdapter::with_fact_table_config(mock, cache, "1.0.0".to_string(), ft_config);

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
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_user_stats", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_dashboard", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 3);

    // Invalidate only the base view; cascade should evict dependents too
    adapter.invalidate_views(&[ViewName::from("v_user")]).unwrap();

    // All three should now be cache misses
    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_user_stats", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_dashboard", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        6,
        "All three should be cache misses after cascade"
    );
}

/// #1584: a write fences the fills of every view its sweep reaches, the expansion
/// included, and no other. A read of `v_dashboard` straddling a write to `v_user` would be
/// evicted by that write had it been stored a moment earlier, so it must not be stored.
#[test]
fn the_cascade_expansion_fences_the_fills_it_would_evict() {
    let mut cascade = CascadeInvalidator::new();
    cascade.add_dependency("v_dashboard", "v_user").unwrap();
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    )
    .with_cascade_invalidator(cascade);
    let rows = || vec![JsonbValue::new(json!({"id": "x"}))];

    let fence = adapter.cache().invalidation_generation();
    adapter.invalidate_views(&[ViewName::from("v_user")]).unwrap();
    for (key, view) in [(1_u64, "v_dashboard"), (2, "v_tag")] {
        adapter
            .cache()
            .put(key, rows(), vec![view.to_string()], None, None, Some(fence))
            .unwrap();
    }

    assert!(
        adapter.cache().get(1).unwrap().is_none(),
        "a fill of a view the write's expanded sweep reaches must be refused"
    );
    assert!(
        adapter.cache().get(2).unwrap().is_some(),
        "a fill of a view the sweep does not reach must be stored (#1584)"
    );
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

    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_user_stats", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 2);

    // Only v_user is invalidated — v_user_stats remains cached
    adapter.invalidate_views(&[ViewName::from("v_user")]).unwrap();

    adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    adapter
        .execute_where_query("v_user_stats", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        3,
        "Only v_user should be a miss; v_user_stats is still cached"
    );
}

// ── bump_fact_table_versions tests ─────────────────────────────────────

/// Adapter whose `execute_function_call` simulates `bump_tf_version` by returning
/// the incremented version (starting at 2).
struct BumpAdapter {
    bump_call_count: std::sync::atomic::AtomicU32,
}

impl BumpAdapter {
    fn new() -> Self {
        Self {
            bump_call_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn bump_call_count(&self) -> u32 {
        self.bump_call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for BumpAdapter {
    // A test double: the session variables a read carries are accepted (#1115).
    fn applies_session_variables(&self) -> bool {
        true
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
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

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

// The writes this double answers, called by its `Writer` impl below.
impl BumpAdapter {
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

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait::async_trait]
impl crate::backend::traits::Writer for BumpAdapter {
    async fn execute_write(
        &self,
        request: &crate::backend::traits::WriteRequest<'_>,
        gate: crate::backend::traits::MutationRowGate<'_>,
    ) -> std::result::Result<
        Vec<std::collections::HashMap<String, serde_json::Value>>,
        crate::error::FraiseQLError,
    > {
        {
            let rows = self.execute_function_call(request.function, request.args).await?;
            gate(&rows)?;
            Ok(rows)
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
    adapter.bump_fact_table_versions(&["tf_sales".to_string()]).await.unwrap();

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

    assert_eq!(
        adapter.inner().bump_call_count(),
        1,
        "Only VersionTable strategy calls bump_tf_version"
    );
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

// =========================================================================
// view_name_to_entity_type
// =========================================================================

#[test]
fn test_view_name_to_entity_type_single_word() {
    use crate::cache::adapter::view_name_to_entity_type;
    assert_eq!(view_name_to_entity_type("v_user"), Some("User".to_string()));
    assert_eq!(view_name_to_entity_type("v_product"), Some("Product".to_string()));
}

#[test]
fn test_view_name_to_entity_type_multi_word() {
    use crate::cache::adapter::view_name_to_entity_type;
    assert_eq!(view_name_to_entity_type("v_order_item"), Some("OrderItem".to_string()));
    assert_eq!(view_name_to_entity_type("v_user_profile"), Some("UserProfile".to_string()));
    assert_eq!(view_name_to_entity_type("v_a_b_c"), Some("ABC".to_string()));
}

#[test]
fn test_view_name_to_entity_type_arbitrary_prefix() {
    use crate::cache::adapter::view_name_to_entity_type;
    assert_eq!(view_name_to_entity_type("tv_user_event"), Some("UserEvent".to_string()));
    assert_eq!(view_name_to_entity_type("mat_order"), Some("Order".to_string()));
}

#[test]
fn test_view_name_to_entity_type_no_prefix() {
    use crate::cache::adapter::view_name_to_entity_type;
    // No underscore → not a typed view → None
    assert_eq!(view_name_to_entity_type("users"), None);
    assert_eq!(view_name_to_entity_type("orders"), None);
}

#[test]
fn test_view_name_to_entity_type_empty_after_prefix() {
    use crate::cache::adapter::view_name_to_entity_type;
    assert_eq!(view_name_to_entity_type("v_"), None);
    assert_eq!(view_name_to_entity_type("_"), None);
}

// ===== Tests: Opt-in per-query caching (#186, #187) =====

/// Views with no TTL annotation bypass key-generation entirely when
/// opt-in mode is active (i.e. `with_view_ttl_overrides` or
/// `with_cache_metadata_from_schema` was called).  This eliminates the
/// allocation overhead that caused the 2.4× throughput regression on
/// TV-table and on-the-fly JSONB workloads.
#[tokio::test]
async fn test_non_cacheable_view_always_hits_db() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    // Only "v_expensive" opts into caching; "v_user" does not.
    let overrides = HashMap::from([("v_expensive".to_string(), 300_u64)]);
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string())
        .with_view_ttl_overrides(overrides);

    // First call to a non-cacheable view.
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second call — should bypass the cache and hit the DB again.
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        2,
        "non-cacheable view must not be served from cache"
    );
}

/// A view that opts in via `cache_ttl_seconds` is still cached normally
/// even when other views in the schema have no TTL annotation.
#[tokio::test]
async fn test_cacheable_view_is_still_cached() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let overrides = HashMap::from([("v_expensive".to_string(), 300_u64)]);
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string())
        .with_view_ttl_overrides(overrides);

    // First call — cache miss.
    adapter
        .execute_where_query("v_expensive", None, None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second call — cache hit; DB must NOT be called again.
    adapter
        .execute_where_query("v_expensive", None, None, None, None)
        .await
        .unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        1,
        "opt-in view must be served from cache on second call"
    );
}

/// When no TTL overrides are set AND no schema was loaded (`opt_in_mode = false`),
/// all views remain cacheable — preserving backward-compatible behaviour for
/// adapters constructed without a schema (e.g. in unit tests or direct usage).
#[tokio::test]
async fn test_all_views_cacheable_when_no_overrides_set() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    // No schema loaded → opt_in_mode = false → all views are cached (backward compat).
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string());

    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second call — cache hit.
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        1,
        "with no schema loaded all views must be cached"
    );
}

/// Fixes #187: when a schema with NO `cache_ttl_seconds` annotations is loaded
/// (e.g. fraiseql-v on-the-fly JSONB views), opt-in mode is active but
/// `cacheable_views` is empty.  ALL views should bypass key-generation entirely
/// — restoring v2.1.2 throughput for unannotated schemas.
#[tokio::test]
async fn test_schema_without_ttl_annotations_bypasses_cache() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    // Schema loaded but no queries have cache_ttl_seconds → cacheable_views = {} but
    // opt_in_mode = true.
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string())
        .with_view_ttl_overrides(HashMap::new()); // simulates schema with no TTL annotations

    // Every call should bypass the cache and hit the DB directly.
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        2,
        "schema with no TTL annotations must bypass cache on every request (#187)"
    );
}

/// `with_cache_metadata_from_schema` activates opt-in mode unconditionally.
/// When the schema has no `cache_ttl_seconds` annotations, `cacheable_views`
/// is empty and every query bypasses the cache entirely — zero overhead and no
/// stale-data risk from unconfigured caching.
#[tokio::test]
async fn test_ttl_overrides_from_empty_schema_bypasses_cache() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    // Schema with no cache_ttl_seconds annotations on any query.
    let schema = CompiledSchema::default();
    let adapter = CachedDatabaseAdapter::new(mock, cache, "1.0.0".to_string())
        .with_cache_metadata_from_schema(&schema);

    // First call — cache bypass, hits DB.
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second call — still bypasses cache (opt-in mode, no annotations).
    adapter.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        2,
        "with_cache_metadata_from_schema on unannotated schema must bypass cache entirely"
    );
}

// RLS guard — has_rls initialization and cache key isolation

#[test]
fn with_rls_sets_has_rls_field() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter_rls = CachedDatabaseAdapter::new(mock, cache, "v1".to_string()).with_rls(true);
    assert!(adapter_rls.has_rls, "with_rls(true) must set has_rls=true");

    let mock2 = MockAdapter::new();
    let cache2 = QueryResultCache::new(CacheConfig::enabled());
    let adapter_no_rls =
        CachedDatabaseAdapter::new(mock2, cache2, "v1".to_string()).with_rls(false);
    assert!(!adapter_no_rls.has_rls, "with_rls(false) must set has_rls=false");
}

#[test]
fn default_has_rls_is_false() {
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    assert!(!adapter.has_rls, "default has_rls must be false");
}

#[tokio::test]
async fn cache_key_differs_for_different_where_clauses() {
    // Isolation in RLS deployments relies on tenants having different WHERE clauses
    // (because the RLS inject_params produce tenant-specific filters). Verify that
    // two otherwise identical queries with different WHERE clauses never share a
    // cache entry.
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string())
        .with_view_ttl_overrides(std::collections::HashMap::from([("v_item".to_string(), 60_u64)]));

    let where_tenant_a = WhereClause::Field {
        path:     vec!["tenant_id".to_string()],
        operator: WhereOperator::Eq,
        value:    serde_json::json!("tenant-a"),
    };
    let where_tenant_b = WhereClause::Field {
        path:     vec!["tenant_id".to_string()],
        operator: WhereOperator::Eq,
        value:    serde_json::json!("tenant-b"),
    };

    // Tenant A: 1 DB call, result cached
    adapter
        .execute_where_query("v_item", Some(&where_tenant_a), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Tenant A again: cache hit, no extra DB call
    adapter
        .execute_where_query("v_item", Some(&where_tenant_a), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        1,
        "second request for same tenant must be a cache hit"
    );

    // Tenant B: different WHERE clause → different cache key → DB call
    adapter
        .execute_where_query("v_item", Some(&where_tenant_b), None, None, None)
        .await
        .unwrap();
    assert_eq!(
        adapter.inner().call_count(),
        2,
        "different tenant WHERE clause must be a cache miss"
    );
}

#[tokio::test]
async fn with_rls_does_not_disable_cache() {
    // After the fix: has_rls=true must NOT bypass the cache for all requests.
    // Cache isolation is provided structurally by the WHERE clause; the has_rls
    // flag is for observability and future extension only.
    let mock = MockAdapter::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string())
        .with_view_ttl_overrides(std::collections::HashMap::from([("v_thing".to_string(), 60_u64)]))
        .with_rls(true);

    let where_clause = WhereClause::Field {
        path:     vec!["tenant_id".to_string()],
        operator: WhereOperator::Eq,
        value:    serde_json::json!("tenant-x"),
    };

    // First call → DB hit
    adapter
        .execute_where_query("v_thing", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1);

    // Second call with same WHERE → cache hit (has_rls=true must NOT disable cache)
    adapter
        .execute_where_query("v_thing", Some(&where_clause), None, None, None)
        .await
        .unwrap();
    assert_eq!(adapter.inner().call_count(), 1, "has_rls=true must not prevent cache hits");
}

// ---------------------------------------------------------------------------
// Streaming forwarding (#958)
// ---------------------------------------------------------------------------

/// Records which half of the read surface it was asked for.
struct StreamSpyAdapter {
    streamed:  std::sync::Arc<std::sync::atomic::AtomicU32>,
    collected: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl StreamSpyAdapter {
    fn new() -> Self {
        Self {
            streamed:  std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            collected: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}

// Reason: the trait is declared with the async-trait macro; impls must match.
#[async_trait]
impl DatabaseAdapter for StreamSpyAdapter {
    // A test double: the session variables a read carries are accepted (#1115).
    fn applies_session_variables(&self) -> bool {
        true
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.collected.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vec![JsonbValue::new(json!({"id": 1}))])
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.collected.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vec![JsonbValue::new(json!({"id": 1}))])
    }

    async fn stream_with_projection(
        &self,
        _request: &crate::backend::ProjectionRequest<'_>,
        _session_vars: &[(&str, &str)],
        _routing: fraiseql_db::types::ReadRouting,
    ) -> Result<fraiseql_db::JsonbRowStream> {
        self.streamed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Box::pin(futures::stream::iter(vec![Ok(JsonbValue::new(json!({"id": 1})))])))
    }

    fn database_type(&self) -> fraiseql_db::DatabaseType {
        fraiseql_db::DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> fraiseql_db::PoolMetrics {
        fraiseql_db::PoolMetrics {
            total_connections:  0,
            idle_connections:   0,
            active_connections: 0,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(Vec::new())
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(Vec::new())
    }
}

// The writes this double answers, called by its `Writer` impl below.
impl StreamSpyAdapter {
    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(Vec::new())
    }
}

// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait::async_trait]
impl crate::backend::traits::Writer for StreamSpyAdapter {
    async fn execute_write(
        &self,
        request: &crate::backend::traits::WriteRequest<'_>,
        gate: crate::backend::traits::MutationRowGate<'_>,
    ) -> std::result::Result<
        Vec<std::collections::HashMap<String, serde_json::Value>>,
        crate::error::FraiseQLError,
    > {
        {
            let rows = self.execute_function_call(request.function, request.args).await?;
            gate(&rows)?;
            Ok(rows)
        }
    }
}

/// The cache wrapper must **forward** a streaming read, not answer it.
///
/// This is the one regression in this feature that no functional test can see. The
/// trait's default satisfies `stream_with_projection` by calling the collecting read
/// and replaying the buffer, so a wrapper that inherits it returns exactly the right
/// rows, in the right order, with the right values — and quietly materialises the
/// whole result set that the caller asked not to materialise. Every export would keep
/// passing while the memory bound it exists for was gone.
///
/// So the assertion is about which method the inner adapter saw.
#[tokio::test]
async fn cached_adapter_forwards_a_streaming_read_instead_of_buffering_it() {
    use futures::StreamExt as _;

    let spy = StreamSpyAdapter::new();
    let (streamed, collected) =
        (std::sync::Arc::clone(&spy.streamed), std::sync::Arc::clone(&spy.collected));
    let adapter = CachedDatabaseAdapter::new(
        spy,
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );

    let rows: Vec<_> = adapter
        .stream_with_projection(
            &crate::backend::ProjectionRequest::new("v_user"),
            &[],
            fraiseql_db::types::ReadRouting::Any,
        )
        .await
        .unwrap()
        .collect()
        .await;

    assert_eq!(rows.len(), 1, "the forwarded stream must deliver the inner adapter's rows");
    assert_eq!(
        streamed.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the wrapper must call the inner adapter's streaming read"
    );
    assert_eq!(
        collected.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the wrapper must not satisfy a streaming read by collecting — that is the \
         trait default, and inheriting it silently removes the memory bound"
    );
}

// ── #1373: the row cache is keyed on the request's session variables ─────────────

mod session_keyed {
    use fraiseql_db::types::ReadRouting;

    use super::*;

    async fn read(
        adapter: &CachedDatabaseAdapter<MockAdapter>,
        session: &[(&str, &str)],
        routing: ReadRouting,
    ) {
        adapter
            .execute_where_query_arc_with_session(
                "v_user", None, None, None, None, session, routing,
            )
            .await
            .expect("mock read");
    }

    fn cached() -> CachedDatabaseAdapter<MockAdapter> {
        CachedDatabaseAdapter::new(
            MockAdapter::new(),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
        )
    }

    /// A read whose view may consult `current_setting()` is served from the cache to a
    /// caller with the same session — it used to bypass the cache whenever any session
    /// variable was set, so a deployment declaring `[session_variables]` never cached
    /// an authenticated read.
    #[tokio::test]
    async fn the_same_session_is_served_from_the_cache() {
        let adapter = cached();
        read(&adapter, &[("app.locale", "fr")], ReadRouting::Any).await;
        read(&adapter, &[("app.locale", "fr")], ReadRouting::Any).await;
        assert_eq!(adapter.inner().call_count(), 1, "the second read must be a hit");
    }

    /// The failure the key exists to prevent: callers with different session values
    /// never share an entry (#1373's translated view).
    #[tokio::test]
    async fn a_different_session_value_is_a_different_entry() {
        let adapter = cached();
        read(&adapter, &[("app.locale", "fr")], ReadRouting::Any).await;
        read(&adapter, &[("app.locale", "en")], ReadRouting::Any).await;
        read(&adapter, &[], ReadRouting::Any).await;
        assert_eq!(adapter.inner().call_count(), 3, "each session must miss once");
    }

    #[tokio::test]
    async fn the_order_session_variables_arrive_in_does_not_change_the_entry() {
        let adapter = cached();
        read(&adapter, &[("app.a", "1"), ("app.b", "2")], ReadRouting::Any).await;
        read(&adapter, &[("app.b", "2"), ("app.a", "1")], ReadRouting::Any).await;
        assert_eq!(adapter.inner().call_count(), 1);
    }

    /// A read that refused staleness (`read_routing = primary`, #957) still never
    /// touches the cache.
    #[tokio::test]
    async fn a_primary_routed_read_still_bypasses_the_cache() {
        let adapter = cached();
        read(&adapter, &[("app.locale", "fr")], ReadRouting::Primary).await;
        read(&adapter, &[("app.locale", "fr")], ReadRouting::Primary).await;
        assert_eq!(adapter.inner().call_count(), 2);
    }
}

/// #1115: the cache answers whether session variables are applied for the adapter it wraps.
#[test]
fn the_cache_answers_session_variable_support_for_its_adapter() {
    let applies = CachedDatabaseAdapter::new(
        fraiseql_test_utils::failing_adapter::FailingAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    assert!(applies.applies_session_variables());

    #[cfg(feature = "wire-backend")]
    {
        let wire = CachedDatabaseAdapter::new(
            fraiseql_db::FraiseWireAdapter::new("postgres://localhost/unused"),
            QueryResultCache::new(CacheConfig::enabled()),
            "1.0.0".to_string(),
        );
        assert!(!wire.applies_session_variables());
    }
}

// ── #1577: a write's evictions are announced to every other server, once ──

/// An adapter over an enabled cache announcing into a queue, and the queue.
fn announcing_adapter() -> (CachedDatabaseAdapter<MockAdapter>, crate::cache::feed::OutboundQueue) {
    let (outbox, queue) = crate::cache::feed::outbox(16);
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    )
    .with_feed(outbox);
    (adapter, queue)
}

/// Everything queued so far, without waiting for more.
async fn queued(
    queue: &mut crate::cache::feed::OutboundQueue,
) -> Vec<crate::cache::feed::Eviction> {
    tokio::time::timeout(std::time::Duration::from_millis(50), queue.next_batch())
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[tokio::test]
async fn a_writes_view_sweep_is_announced() {
    use crate::cache::feed::Eviction;
    let (adapter, mut queue) = announcing_adapter();
    DatabaseAdapter::invalidate_views(&adapter, &[ViewName::from("v_user")])
        .await
        .unwrap();
    assert_eq!(queued(&mut queue).await, vec![Eviction::Views(vec!["v_user".to_string()])]);
}

/// A write that reported its rows evicts the reads by id here by those rows, but another
/// server holds no report: it is told the views, and sweeps them whole (#1576).
#[tokio::test]
async fn a_write_reporting_its_rows_announces_its_views() {
    use crate::cache::feed::Eviction;
    let (adapter, mut queue) = announcing_adapter();
    let mut changed = fraiseql_db::ChangedRows::default();
    changed.rows.entry("User".to_string()).or_default().insert("a".to_string());
    DatabaseAdapter::invalidate_written(&adapter, &[ViewName::from("v_user")], &changed)
        .await
        .unwrap();
    assert_eq!(queued(&mut queue).await, vec![Eviction::Views(vec!["v_user".to_string()])]);
}

#[tokio::test]
async fn a_writes_entity_eviction_is_announced() {
    use crate::cache::feed::Eviction;
    let (adapter, mut queue) = announcing_adapter();
    DatabaseAdapter::invalidate_by_entity(&adapter, "User", "u1").await.unwrap();
    assert_eq!(
        queued(&mut queue).await,
        vec![Eviction::Entity {
            entity_type: "User".to_string(),
            entity_id:   "u1".to_string(),
        }]
    );
}

#[tokio::test]
async fn an_operator_clear_is_announced() {
    use crate::cache::feed::Eviction;
    let (adapter, mut queue) = announcing_adapter();
    DatabaseAdapter::clear_result_cache(&adapter).await.unwrap();
    assert_eq!(queued(&mut queue).await, vec![Eviction::Everything]);
}

#[tokio::test]
async fn an_announced_eviction_applied_here_is_not_announced_again() {
    use crate::cache::feed::Eviction;
    let (adapter, mut queue) = announcing_adapter();
    adapter.evictor().apply(&Eviction::Views(vec!["v_user".to_string()])).unwrap();
    adapter.evictor().apply(&Eviction::Everything).unwrap();
    assert!(queued(&mut queue).await.is_empty());
}

#[tokio::test]
async fn a_schema_reload_clear_is_not_announced() {
    let (adapter, mut queue) = announcing_adapter();
    DatabaseAdapter::on_schema_reload(&adapter);
    assert!(queued(&mut queue).await.is_empty(), "every server reloads its own schema");
}

#[tokio::test]
async fn a_disabled_cache_announces_nothing() {
    let (outbox, mut queue) = crate::cache::feed::outbox(16);
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::disabled()),
        "1.0.0".to_string(),
    )
    .with_feed(outbox);
    DatabaseAdapter::invalidate_views(&adapter, &[ViewName::from("v_user")])
        .await
        .unwrap();
    DatabaseAdapter::invalidate_by_entity(&adapter, "User", "u1").await.unwrap();
    assert!(queued(&mut queue).await.is_empty());
}

// ── #1603: a write reports the collections it changed while composed reads are tagged ──

/// Composed reads are indexed by collection only while `pg_tviews`' change feed is heard, and
/// only a write's report can then evict them by collection: a server with no read by id asks
/// for the report while the feed is heard, and not otherwise.
#[test]
fn a_write_reports_its_changes_while_composed_reads_are_indexed_by_collection() {
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    assert!(!DatabaseAdapter::reports_changed_rows(&adapter), "no read by id, no feed");
    adapter
        .collection_tagging()
        .hear(crate::cache::collections::CollectionMap::default());
    assert!(DatabaseAdapter::reports_changed_rows(&adapter), "the feed is heard");
    adapter.collection_tagging().deafen();
    assert!(!DatabaseAdapter::reports_changed_rows(&adapter), "the feed is lost");
}

/// An adapter over an enabled cache, and its cache.
fn adapter_and_cache() -> (CachedDatabaseAdapter<MockAdapter>, Arc<QueryResultCache>) {
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    let cache = Arc::clone(&adapter.cache);
    (adapter, cache)
}

/// A composed read of `parent`'s comments (`tv_comment`, by `Comment.fk_post`) from
/// `tv_post`, stored under `key` with `fence` (a snapshot taken before its round trip).
fn compose_comments_of(cache: &QueryResultCache, key: u64, parent: &str, fence: Option<u64>) {
    let tags = crate::cache::result::ComposedTags {
        collected_views: vec!["tv_comment".to_string()],
        collections:     vec![(
            crate::cache::tviews_feed::Collection::new("Comment", "fk_post"),
            parent.to_string(),
        )],
        entities:        Vec::new(),
    };
    let rows = Arc::new(vec![crate::db::types::JsonbValue::new(
        json!({"d": {"id": parent}}),
    )]);
    cache
        .put_composed(key, rows, vec!["tv_post".to_string()], None, None, tags, fence)
        .unwrap();
}

/// What a write of one comment under `p1` reports: the comment, and `p1`'s collection.
fn a_comment_under_p1(collections: Option<serde_json::Value>) -> fraiseql_db::ChangedRows {
    let mut report = json!({"updated": [{"__typename": "Comment", "id": "c1"}]});
    if let Some(collections) = collections {
        report["collections"] = collections;
    }
    fraiseql_db::ChangedRows::from_report(&report).unwrap()
}

async fn write_comments(
    adapter: &CachedDatabaseAdapter<MockAdapter>,
    changed: &fraiseql_db::ChangedRows,
) {
    DatabaseAdapter::invalidate_written(adapter, &[ViewName::from("tv_comment")], changed)
        .await
        .unwrap();
}

/// #1603: a write whose report names the collections it changed evicts the composed reads
/// of those, not every composed read of comments, and still sweeps the views it reached.
#[tokio::test]
async fn a_write_evicts_composed_reads_by_the_collections_it_reports() {
    let (adapter, cache) = adapter_and_cache();
    compose_comments_of(&cache, 1, "p1", None);
    compose_comments_of(&cache, 2, "p2", None);
    cache
        .put(3, Vec::new(), vec!["tv_comment".to_string()], None, None, None)
        .unwrap();

    write_comments(&adapter, &a_comment_under_p1(Some(json!({"Comment.fk_post": ["p1"]})))).await;

    assert!(cache.get(1).unwrap().is_none(), "p1's comments changed");
    assert!(cache.get(2).unwrap().is_some(), "p2's comments did not");
    assert!(cache.get(3).unwrap().is_none(), "a read of tv_comment itself is swept");
}

/// #1603: a report that captured no collections cannot say which lists changed: every
/// composed read of comments is swept, as before.
#[tokio::test]
async fn a_write_that_captured_no_collections_sweeps_every_composed_read_of_them() {
    let (adapter, cache) = adapter_and_cache();
    compose_comments_of(&cache, 1, "p1", None);
    compose_comments_of(&cache, 2, "p2", None);

    write_comments(&adapter, &a_comment_under_p1(None)).await;

    assert!(cache.get(1).unwrap().is_none());
    assert!(cache.get(2).unwrap().is_none());
}

/// #1603: a type named wholly (its rows past `max_entities`) stands for every collection of
/// it.
#[tokio::test]
async fn a_type_the_report_names_wholly_evicts_every_collection_of_it() {
    let (adapter, cache) = adapter_and_cache();
    compose_comments_of(&cache, 1, "p1", None);
    compose_comments_of(&cache, 2, "p2", None);
    let changed = fraiseql_db::ChangedRows::from_report(
        &json!({"invalidated_types": ["Comment"], "collections": {}}),
    )
    .unwrap();

    write_comments(&adapter, &changed).await;

    assert!(cache.get(1).unwrap().is_none());
    assert!(cache.get(2).unwrap().is_none());
}

/// #1603: a collection key that is not `<child type>.<column>` names nothing it can evict
/// by, so the write sweeps as one that captured no collections.
#[tokio::test]
async fn a_collection_that_names_no_column_sweeps_every_composed_read() {
    let (adapter, cache) = adapter_and_cache();
    compose_comments_of(&cache, 2, "p2", None);
    let mut changed = a_comment_under_p1(Some(json!({})));
    changed.collections.as_mut().unwrap().entry("Comment".to_string()).or_default();

    write_comments(&adapter, &changed).await;

    assert!(cache.get(2).unwrap().is_none());
}

/// #1603: the fence refuses a composed fill that a write's collection eviction reaches, and
/// only that one: a read of p2's comments in flight across a comment under p1 is stored.
#[tokio::test]
async fn a_composed_fill_straddling_a_write_is_refused_only_when_its_collection_changed() {
    let (adapter, cache) = adapter_and_cache();
    let fence = cache.invalidation_generation();

    write_comments(&adapter, &a_comment_under_p1(Some(json!({"Comment.fk_post": ["p1"]})))).await;
    compose_comments_of(&cache, 1, "p1", Some(fence));
    compose_comments_of(&cache, 2, "p2", Some(fence));

    assert!(cache.get(1).unwrap().is_none(), "p1's fill read the list before the comment");
    assert!(cache.get(2).unwrap().is_some(), "p2's fill is not stale");
}

// ── a function source ──────────────────────────────────────────────────────────

fn function_read(args: &[serde_json::Value]) -> crate::backend::ProjectionRequest<'_> {
    crate::backend::ProjectionRequest {
        source_args: Some(args),
        ..crate::backend::ProjectionRequest::new("app.fn_line_items")
    }
}

/// A function reads tables no mutation names it by, so nothing would ever evict it: it is
/// cached only when its query declared how stale it may be.
#[tokio::test]
async fn an_unannotated_function_source_is_not_cached_even_without_a_schema() {
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    let args = [json!("a1")];
    for _ in 0..2 {
        adapter.execute_with_projection_impl(&function_read(&args), &[]).await.unwrap();
    }
    assert_eq!(adapter.inner().call_count(), 2);
}

#[tokio::test]
async fn an_annotated_function_source_is_served_from_the_cache_for_the_same_arguments() {
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    )
    .with_view_ttl_overrides(HashMap::from([("app.fn_line_items".to_string(), 60)]));
    let args = [json!("a1")];
    for _ in 0..2 {
        adapter.execute_with_projection_impl(&function_read(&args), &[]).await.unwrap();
    }
    assert_eq!(adapter.inner().call_count(), 1);
}

#[tokio::test]
async fn a_cached_function_source_is_keyed_on_its_arguments() {
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    )
    .with_view_ttl_overrides(HashMap::from([("app.fn_line_items".to_string(), 60)]));
    for args in [[json!("ab"), json!("c")], [json!("a"), json!("bc")]] {
        adapter.execute_with_projection_impl(&function_read(&args), &[]).await.unwrap();
    }
    assert_eq!(adapter.inner().call_count(), 2, "two calls are two reads");
}

/// A function root's rows come from tables no write names it by: the views its query lists in
/// `additional_views` are what evicts it, as for a view.
#[tokio::test]
async fn a_cached_function_source_is_evicted_by_the_views_its_query_lists() {
    let mut root = crate::schema::QueryDefinition::new("lineItems", "LineItem")
        .returning_list()
        .with_sql_source_function("app.fn_line_items");
    root.cache_ttl_seconds = Some(60);
    root.additional_views = vec!["v_price".to_string()];
    let schema = CompiledSchema {
        queries: vec![root],
        ..Default::default()
    };
    let adapter = CachedDatabaseAdapter::new(
        MockAdapter::new(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    )
    .with_cache_metadata_from_schema(&schema);
    let args = [json!("a1")];
    adapter.execute_with_projection_impl(&function_read(&args), &[]).await.unwrap();
    adapter.invalidate_views(&[ViewName::from("v_price")]).unwrap();
    adapter.execute_with_projection_impl(&function_read(&args), &[]).await.unwrap();
    assert_eq!(adapter.inner().call_count(), 2, "a write to v_price re-reads the call");
}

// ── a read answered as text ────────────────────────────────────────────────────

/// Whether the wrapper hands a read of `v_user` to the inner adapter as text.
async fn answers_raw(adapter: &CachedDatabaseAdapter<MockAdapter>, routing: ReadRouting) -> bool {
    adapter
        .execute_with_projection_raw_with_session(
            &crate::backend::ProjectionRequest::new("v_user"),
            &[],
            routing,
        )
        .await
        .unwrap()
        .is_some()
}

fn wrapping(config: CacheConfig) -> CachedDatabaseAdapter<MockAdapter> {
    CachedDatabaseAdapter::new(MockAdapter::new(), QueryResultCache::new(config), "1.0.0".into())
}

/// The cache holds parsed rows. A read it may serve or store is never answered as text
/// past it: a hit would be bypassed and a miss would not populate it.
#[tokio::test]
async fn a_read_the_cache_may_serve_is_not_answered_as_text() {
    assert!(!answers_raw(&wrapping(CacheConfig::enabled()), ReadRouting::Any).await);
    let annotated = wrapping(CacheConfig::enabled())
        .with_view_ttl_overrides(std::iter::once(("v_user".to_string(), 60)).collect());
    assert!(!answers_raw(&annotated, ReadRouting::Any).await);
}

/// The wrapper sits in front of every server's adapter, cache on or off: a read the cache
/// would not touch reaches the inner adapter, or the raw path never runs in a server.
#[tokio::test]
async fn a_read_the_cache_would_not_touch_is_forwarded_as_text() {
    assert!(answers_raw(&wrapping(CacheConfig::disabled()), ReadRouting::Any).await);
    let other_view_annotated = wrapping(CacheConfig::enabled())
        .with_view_ttl_overrides(std::iter::once(("v_order".to_string(), 60)).collect());
    assert!(
        answers_raw(&other_view_annotated, ReadRouting::Any).await,
        "opt-in, not annotated"
    );
    assert!(
        answers_raw(&wrapping(CacheConfig::enabled()), ReadRouting::Primary).await,
        "a read that refused staleness bypasses the cache"
    );
}
