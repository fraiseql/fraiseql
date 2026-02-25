//! Cache concurrency stress tests.
//!
//! Verifies that `CachedDatabaseAdapter` handles concurrent access correctly:
//! cache stampede mitigation, safe concurrent invalidation, view isolation,
//! and data consistency across concurrent readers.

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{DatabaseAdapter, WhereClause, WhereOperator, types::JsonbValue},
};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;
use tokio::sync::Barrier;

fn make_user_data() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(json!({"id": 1, "name": "Alice"})),
        JsonbValue::new(json!({"id": 2, "name": "Bob"})),
    ]
}

fn make_post_data() -> Vec<JsonbValue> {
    vec![JsonbValue::new(json!({"id": 1, "title": "Hello"}))]
}

#[tokio::test]
async fn test_cache_stampede_limited_database_hits() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string()));
    let barrier = Arc::new(Barrier::new(50));
    let where_clause = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let cached = Arc::clone(&cached);
        let barrier = Arc::clone(&barrier);
        let wc = where_clause.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            cached.execute_where_query("v_user", Some(&wc), None, None).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    // Cache should absorb most requests — far fewer than 50 DB hits
    let db_hits = cached.inner().query_count();
    assert!(
        db_hits < 50,
        "cache should absorb stampede: got {db_hits} DB hits for 50 concurrent requests"
    );
}

#[tokio::test]
async fn test_concurrent_reads_and_invalidation_no_deadlock() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::default());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string()));

    // Warm the cache
    cached.execute_where_query("v_user", None, None, None).await.unwrap();

    let mut handles = Vec::with_capacity(11);

    // 10 reader tasks, each making 100 queries
    for _ in 0..10 {
        let cached = Arc::clone(&cached);
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let result = cached.execute_where_query("v_user", None, None, None).await;
                assert!(result.is_ok());
            }
        }));
    }

    // 1 invalidator task
    {
        let cached = Arc::clone(&cached);
        handles.push(tokio::spawn(async move {
            for _ in 0..10 {
                let _ = cached.invalidate_views(&["v_user".to_string()]);
                tokio::task::yield_now().await;
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_queries_different_views_independent() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", make_user_data())
        .with_response("v_post", make_post_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string()));
    let where_clause = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };

    // Warm both caches
    cached.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
    cached.execute_where_query("v_post", Some(&where_clause), None, None).await.unwrap();
    let count_after_warm = cached.inner().query_count();
    assert_eq!(count_after_warm, 2);

    // Concurrent reads to both views
    let mut handles = Vec::with_capacity(20);
    for i in 0..20 {
        let cached = Arc::clone(&cached);
        let view = if i % 2 == 0 { "v_user" } else { "v_post" };
        let wc = where_clause.clone();
        handles.push(tokio::spawn(async move {
            cached.execute_where_query(view, Some(&wc), None, None).await
        }));
    }

    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    // Invalidate v_user only
    cached.invalidate_views(&["v_user".to_string()]).unwrap();

    // v_post should still be cached (no additional DB hit)
    let count_before = cached.inner().query_count();
    cached.execute_where_query("v_post", Some(&where_clause), None, None).await.unwrap();
    assert_eq!(
        cached.inner().query_count(),
        count_before,
        "v_post cache should be unaffected by v_user invalidation"
    );

    // v_user should miss cache (causes a DB hit)
    cached.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
    assert_eq!(cached.inner().query_count(), count_before + 1);
}

#[tokio::test]
async fn test_concurrent_cache_hits_return_consistent_data() {
    let adapter = FailingAdapter::new().with_response("v_user", make_user_data());
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "1.0.0".to_string()));
    let where_clause = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };

    // Warm the cache
    let expected =
        cached.execute_where_query("v_user", Some(&where_clause), None, None).await.unwrap();
    let expected_json: Vec<String> =
        expected.iter().map(|v| serde_json::to_string(v.as_value()).unwrap()).collect();

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let cached = Arc::clone(&cached);
        let expected_json = expected_json.clone();
        let wc = where_clause.clone();
        handles.push(tokio::spawn(async move {
            let result =
                cached.execute_where_query("v_user", Some(&wc), None, None).await.unwrap();
            let result_json: Vec<String> =
                result.iter().map(|v| serde_json::to_string(v.as_value()).unwrap()).collect();
            assert_eq!(result_json, expected_json, "cached data must be consistent");
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // All reads should have been cache hits
    assert_eq!(cached.inner().query_count(), 1);
}
