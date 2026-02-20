#![allow(missing_docs)]

//! Integration tests for CachedDatabaseAdapter with a real PostgreSQL backend.
//!
//! Validates cache behavior (hit/miss/invalidation) against a real database,
//! ensuring the cache layer works correctly with real query results.

mod common;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::DatabaseAdapter,
};

#[tokio::test]
async fn cache_hit_avoids_second_real_query() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::with_max_entries(100));
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // First call — cache miss, hits real DB
    let r1 = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();

    // Second identical call — cache hit
    let r2 = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();

    assert_eq!(r1.len(), r2.len());

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.hits, 1, "second call should be a cache hit");
    assert_eq!(metrics.misses, 1, "first call should be a cache miss");
}

#[tokio::test]
async fn cache_invalidation_forces_refetch() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::with_max_entries(100));
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Populate cache
    let _ = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();

    // Invalidate
    let evicted = cached.invalidate_views(&["test.v_user".to_string()]).unwrap();
    assert!(evicted > 0, "should have evicted at least one entry");

    // Query again — should miss
    let _ = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2, "post-invalidation call should miss");
    assert_eq!(metrics.hits, 0, "no hits expected after invalidation");
}

#[tokio::test]
async fn different_views_cache_independently() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::with_max_entries(100));
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query two different views
    let _ = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();
    let _ = cached.execute_where_query("test.v_project", None, None, None).await.unwrap();

    // Repeat both — should hit
    let _ = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();
    let _ = cached.execute_where_query("test.v_project", None, None, None).await.unwrap();

    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2);
    assert_eq!(metrics.hits, 2);
}

#[tokio::test]
async fn different_query_params_cache_independently() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::with_max_entries(100));
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query with limit 1
    let r1 = cached.execute_where_query("test.v_project", None, Some(1), None).await.unwrap();

    // Query with limit 2 — different cache key
    let r2 = cached.execute_where_query("test.v_project", None, Some(2), None).await.unwrap();

    // Both should be misses (different params = different cache keys)
    let metrics = cached.cache().metrics().unwrap();
    assert_eq!(metrics.misses, 2, "different limits should produce different cache keys");
    assert_eq!(metrics.hits, 0);

    assert!(r1.len() <= 1);
    assert!(r2.len() <= 2);
}

#[tokio::test]
async fn disabled_cache_always_hits_database() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let cache = QueryResultCache::new(CacheConfig::disabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "v1".to_string());

    // Query twice
    let r1 = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();
    let r2 = cached.execute_where_query("test.v_user", None, None, None).await.unwrap();

    // Both should return data (from DB each time)
    assert_eq!(r1.len(), r2.len());

    let metrics = cached.cache().metrics().unwrap();
    // With caching disabled, no hits
    assert_eq!(metrics.hits, 0);
}
