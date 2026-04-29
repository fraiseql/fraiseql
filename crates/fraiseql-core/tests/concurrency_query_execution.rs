#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Concurrent query execution stress tests.
//!
//! Verifies that the `FailingAdapter` (and by extension, the `DatabaseAdapter` trait)
//! handles concurrent access correctly: shared state remains consistent, failures
//! are isolated, and no deadlocks occur under contention.

use std::sync::Arc;

use fraiseql_core::db::{DatabaseAdapter, types::JsonbValue};
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
async fn test_100_concurrent_reads_all_succeed() {
    let adapter = Arc::new(FailingAdapter::new().with_response("v_user", make_user_data()));

    let mut handles = Vec::with_capacity(100);
    for _ in 0..100 {
        let adapter = Arc::clone(&adapter);
        handles.push(tokio::spawn(async move {
            adapter.execute_where_query("v_user", None, None, None, None, &[]).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        let rows = result.unwrap_or_else(|e| panic!("expected Ok from concurrent read: {e}"));
        assert_eq!(rows.len(), 2);
    }

    assert_eq!(adapter.query_count(), 100);
}

#[tokio::test]
async fn test_concurrent_reads_to_different_views() {
    let adapter = Arc::new(
        FailingAdapter::new()
            .with_response("v_user", make_user_data())
            .with_response("v_post", make_post_data()),
    );

    let mut handles = Vec::with_capacity(100);

    for i in 0..100 {
        let adapter = Arc::clone(&adapter);
        let view = if i < 50 { "v_user" } else { "v_post" };
        handles.push(tokio::spawn(async move {
            adapter.execute_where_query(view, None, None, None, None, &[]).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        result.unwrap_or_else(|e| panic!("expected Ok from concurrent view read: {e}"));
    }

    assert_eq!(adapter.query_count(), 100);

    let queries = adapter.recorded_queries();
    let user_count = queries.iter().filter(|q| *q == "v_user").count();
    let post_count = queries.iter().filter(|q| *q == "v_post").count();
    assert_eq!(user_count, 50);
    assert_eq!(post_count, 50);
}

#[tokio::test]
async fn test_concurrent_queries_with_single_failure() {
    let adapter = Arc::new(
        FailingAdapter::new()
            .with_response("v_user", make_user_data())
            .fail_on_query(50),
    );

    let mut handles = Vec::with_capacity(100);
    for _ in 0..100 {
        let adapter = Arc::clone(&adapter);
        handles.push(tokio::spawn(async move {
            adapter.execute_where_query("v_user", None, None, None, None, &[]).await
        }));
    }

    let mut successes = 0u64;
    let mut failures = 0u64;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    assert_eq!(failures, 1, "exactly one query should fail");
    assert_eq!(successes, 99, "remaining queries should succeed");
    assert_eq!(adapter.query_count(), 100);
}

#[tokio::test]
async fn test_barrier_synchronized_concurrent_queries() {
    let adapter = Arc::new(FailingAdapter::new().with_response("v_user", make_user_data()));
    let barrier = Arc::new(Barrier::new(50));

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let adapter = Arc::clone(&adapter);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            adapter.execute_where_query("v_user", None, None, None, None, &[]).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        result.unwrap_or_else(|e| panic!("expected Ok from barrier-synchronized query: {e}"));
    }

    assert_eq!(adapter.query_count(), 50);
}

#[tokio::test]
async fn test_concurrent_health_checks() {
    let adapter = Arc::new(FailingAdapter::new());

    let mut handles = Vec::with_capacity(100);
    for _ in 0..100 {
        let adapter = Arc::clone(&adapter);
        handles.push(tokio::spawn(async move { adapter.health_check().await }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        result.unwrap_or_else(|e| panic!("expected Ok from concurrent health check: {e}"));
    }
}
