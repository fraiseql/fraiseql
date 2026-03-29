#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Database failure injection tests.
//!
//! Tests error paths when the database adapter returns various failure types.

use fraiseql_core::{db::DatabaseAdapter, error::FraiseQLError};
use fraiseql_test_utils::failing_adapter::{FailError, FailingAdapter};
use serde_json::json;

#[tokio::test]
async fn test_query_returns_database_error_on_injected_failure() {
    let adapter = FailingAdapter::new().fail_on_query(0);

    let result = adapter.execute_where_query("v_user", None, None, None, None).await;

    assert!(result.is_err(), "expected Err from injected failure, got: {result:?}");
    let err = result.unwrap_err();
    assert!(matches!(err, FraiseQLError::Database { .. }), "expected Database error, got: {err:?}");
    assert!(err.is_server_error());
}

#[tokio::test]
async fn test_second_query_fails_while_first_succeeds() {
    let adapter = FailingAdapter::new()
        .with_response("v_user", vec![fraiseql_core::db::types::JsonbValue::new(json!({"id": 1}))])
        .fail_on_query(1);

    // First query succeeds
    let result1 = adapter.execute_where_query("v_user", None, None, None, None).await;
    let rows = result1.unwrap_or_else(|e| panic!("expected Ok on first query: {e}"));
    assert_eq!(rows.len(), 1);

    // Second query fails
    let result2 = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert!(result2.is_err(), "expected Err on second query (fail_on_query(1)), got: {result2:?}");
}

#[tokio::test]
async fn test_timeout_error_contains_duration() {
    let adapter = FailingAdapter::new().fail_with_timeout(5000);

    let result = adapter.execute_where_query("v_user", None, None, None, None).await;

    let err = result.unwrap_err();
    match err {
        FraiseQLError::Timeout { timeout_ms, .. } => {
            assert_eq!(timeout_ms, 5000);
        },
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn test_connection_pool_error_is_retryable() {
    let adapter = FailingAdapter::new().fail_with_error(FailError::ConnectionPool {
        message: "pool exhausted".to_string(),
    });

    let result = adapter.execute_where_query("v_user", None, None, None, None).await;

    let err = result.unwrap_err();
    assert!(matches!(err, FraiseQLError::ConnectionPool { .. }));
    assert!(err.is_retryable());
}

#[tokio::test]
async fn test_health_check_failure() {
    let adapter = FailingAdapter::new().fail_health_check();

    let result = adapter.health_check().await;
    assert!(result.is_err(), "expected Err from failed health check, got: {result:?}");
    assert!(
        matches!(result.unwrap_err(), FraiseQLError::Database { .. }),
        "expected Database error from health check failure"
    );
}

#[tokio::test]
async fn test_query_log_records_all_attempts() {
    let adapter = FailingAdapter::new();

    let _ = adapter.execute_where_query("v_user", None, None, None, None).await;
    let _ = adapter.execute_where_query("v_post", None, None, None, None).await;
    let _ = adapter.execute_where_query("v_user", None, None, None, None).await;

    let log = adapter.recorded_queries();
    assert_eq!(log, vec!["v_user", "v_post", "v_user"]);
}

#[tokio::test]
async fn test_custom_error_propagation() {
    let adapter = FailingAdapter::new().fail_with_error(FailError::Cancelled {
        query_id: "q-123".to_string(),
        reason:   "client disconnect".to_string(),
    });

    let result = adapter.execute_where_query("v_user", None, None, None, None).await;

    let err = result.unwrap_err();
    match err {
        FraiseQLError::Cancelled { query_id, reason } => {
            assert_eq!(query_id, "q-123");
            assert_eq!(reason, "client disconnect");
        },
        other => panic!("expected Cancelled, got {other:?}"),
    }
}

#[tokio::test]
async fn test_error_classification_database_is_server_error() {
    let err = FraiseQLError::Database {
        message:   "connection refused".to_string(),
        sql_state: None,
    };
    assert!(err.is_server_error());
    assert!(!err.is_client_error());
    assert_eq!(err.status_code(), 500);
}

#[tokio::test]
async fn test_error_classification_timeout_is_retryable() {
    let err = FraiseQLError::Timeout {
        timeout_ms: 3000,
        query:      Some("SELECT 1".to_string()),
    };
    assert!(err.is_retryable());
    assert!(err.is_server_error());
    assert_eq!(err.status_code(), 408);
}

#[tokio::test]
async fn test_multiple_failures_then_recovery() {
    let adapter = FailingAdapter::new().fail_with_error(FailError::Database {
        message:   "temporary failure".to_string(),
        sql_state: None,
    });

    // Fails
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    assert!(result.is_err(), "expected Err from injected database failure, got: {result:?}");

    // Reset clears failure
    adapter.reset();

    // Succeeds after reset
    let result = adapter.execute_where_query("v_user", None, None, None, None).await;
    result.unwrap_or_else(|e| panic!("expected Ok after reset: {e}"));
}
