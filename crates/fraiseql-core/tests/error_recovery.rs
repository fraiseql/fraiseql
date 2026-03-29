#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Recovery scenario tests.
//!
//! Tests that the system can recover after transient failures.

use fraiseql_core::db::DatabaseAdapter;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::json;

#[tokio::test]
async fn test_recovery_after_transient_database_failure() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![fraiseql_core::db::types::JsonbValue::new(json!({"id": 1}))]);

    // Succeed first
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    result.unwrap_or_else(|e| panic!("expected Ok on first query: {e}"));

    // Inject failure
    adapter.set_fail_on_query(adapter.query_count());
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert!(result.is_err(), "expected Err after injected failure, got: {result:?}");

    // Reset and recover
    adapter.reset();
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_recovery_after_timeout() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![fraiseql_core::db::types::JsonbValue::new(json!({"id": 1}))]);

    // Inject timeout
    adapter.set_fail_with_timeout(5000);
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert!(
        matches!(result, Err(fraiseql_core::error::FraiseQLError::Timeout { .. })),
        "expected Timeout error, got: {result:?}"
    );

    // Reset and recover
    adapter.reset();
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    result.unwrap_or_else(|e| panic!("expected Ok after recovery from timeout: {e}"));
}

#[tokio::test]
async fn test_recovery_health_check_after_failure() {
    let adapter = FailingAdapter::new().fail_health_check();

    // Health check fails
    let hc = adapter.health_check().await;
    assert!(hc.is_err(), "expected health check to fail, got: {hc:?}");

    // Reset clears health check failure
    adapter.reset();

    // Health check succeeds
    adapter.health_check().await.unwrap_or_else(|e| panic!("expected health check to succeed after reset: {e}"));
}

#[tokio::test]
async fn test_adapter_state_independent_between_queries() {
    // Failure on query 0 should not affect query to a different view after reset
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![fraiseql_core::db::types::JsonbValue::new(json!({"id": 1}))])
        .with_response("v_post", vec![fraiseql_core::db::types::JsonbValue::new(json!({"id": 10}))])
        .fail_on_query(0);

    // First query fails (query 0)
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert!(result.is_err(), "expected Err on query 0, got: {result:?}");

    // Second query succeeds (query 1 != fail_on_query(0))
    let result = adapter.execute_where_query("v_post", None, None, None, None).await;
    let rows = result.unwrap_or_else(|e| panic!("expected Ok on query 1 (v_post): {e}"));
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_query_count_tracks_across_failures() {
    let adapter = FailingAdapter::new().fail_on_query(1);

    // Query 0 — succeeds
    let _ = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert_eq!(adapter.query_count(), 1);

    // Query 1 — fails
    let _ = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert_eq!(adapter.query_count(), 2); // Incremented even on failure

    // Query 2 — succeeds
    let _ = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert_eq!(adapter.query_count(), 3);
}
