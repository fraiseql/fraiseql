#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Cache failure and edge case tests.
//!
//! Tests cache behavior when the underlying database adapter fails,
//! and verifies cache isolation across schema versions and views.

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{DatabaseAdapter, WhereClause, WhereOperator, types::JsonbValue},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;

fn make_user_data() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(json!({"id": 1, "name": "Alice"})),
        JsonbValue::new(json!({"id": 2, "name": "Bob"})),
    ]
}

#[tokio::test]
async fn test_cache_miss_hits_database() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string());

    let result = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(cached.inner().query_count(), 1);
}

#[tokio::test]
async fn test_cache_hit_skips_database() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string());

    // First call — cache miss
    let _ = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached.inner().query_count(), 1);

    // Second call — cache hit
    let result = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(cached.inner().query_count(), 1); // Still 1
}

#[tokio::test]
async fn test_cache_miss_with_different_where_clause() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string());

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

    let _ = cached
        .execute_where_query("v_user", Some(&where1), None, None, None)
        .await
        .unwrap();
    assert_eq!(cached.inner().query_count(), 1);

    let _ = cached
        .execute_where_query("v_user", Some(&where2), None, None, None)
        .await
        .unwrap();
    assert_eq!(cached.inner().query_count(), 2); // Different where = cache miss
}

#[tokio::test]
async fn test_database_error_not_cached() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data()).fail_on_query(0);
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string());

    // First call fails
    let result = cached.execute_where_query("v_user", None, None, None, None).await;
    assert!(result.is_err());

    // Reset the failure — next call should hit the adapter again (error was NOT cached)
    cached.inner().reset();

    let result = cached.execute_where_query("v_user", None, None, None, None).await;
    assert!(result.is_ok());
    // query_count is 1 because reset() zeroed it, then we made 1 successful call
    assert_eq!(cached.inner().query_count(), 1);
}

#[tokio::test]
async fn test_cache_with_schema_version_isolation() {
    // Two adapters with same cache config but different schema versions
    // should produce different cache keys (no cross-version hits)
    let adapter_v1 = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache_v1 = QueryResultCache::new(CacheConfig::enabled());
    let cached_v1 = CachedDatabaseAdapter::new(adapter_v1, cache_v1, "1.0.0".to_string());

    let _ = cached_v1.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached_v1.inner().query_count(), 1);

    // Verify cache hit works within same version
    let _ = cached_v1.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached_v1.inner().query_count(), 1); // Cache hit

    // Different schema version — separate adapter, separate cache
    let adapter_v2 = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache_v2 = QueryResultCache::new(CacheConfig::enabled());
    let cached_v2 = CachedDatabaseAdapter::new(adapter_v2, cache_v2, "2.0.0".to_string());

    let _ = cached_v2.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached_v2.inner().query_count(), 1); // Cache miss — fresh cache
}

#[tokio::test]
async fn test_invalidate_view_forces_cache_miss() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string());

    // Populate cache
    let _ = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached.inner().query_count(), 1);

    // Cache hit
    let _ = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached.inner().query_count(), 1);

    // Invalidate
    let invalidated = cached.invalidate_views(&["v_user".to_string()]).unwrap();
    assert_eq!(invalidated, 1);

    // Must hit adapter again
    let _ = cached.execute_where_query("v_user", None, None, None, None).await.unwrap();
    assert_eq!(cached.inner().query_count(), 2);
}
