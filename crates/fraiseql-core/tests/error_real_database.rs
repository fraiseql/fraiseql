#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Integration tests for real database error paths using testcontainers.
//!
//! These tests verify error handling against a real PostgreSQL instance,
//! ensuring our error types correctly capture real driver behavior.

mod common;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    error::FraiseQLError,
};

// ---------------------------------------------------------------------------
// Connection failures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connection_to_invalid_host_returns_connection_pool_error() {
    let result = PostgresAdapter::new("postgres://localhost:19999/nonexistent").await;
    let err = result.err().expect("expected connection to fail");
    assert!(
        matches!(err, FraiseQLError::ConnectionPool { .. }),
        "expected ConnectionPool, got {err:?}"
    );
}

#[tokio::test]
async fn connection_with_wrong_credentials_returns_error() {
    let container = common::testcontainer::get_test_container().await;
    let bad_url = format!("postgres://wrong_user:wrong_pass@127.0.0.1:{}/testdb", container.port);
    let result = PostgresAdapter::new(&bad_url).await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Query execution failures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn raw_query_with_syntax_error_returns_database_error() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let result = adapter.execute_raw_query("SELCT * FORM nonexistent").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Database { .. }),
        "expected Database error, got {err:?}"
    );
    if let FraiseQLError::Database { sql_state, .. } = &err {
        assert!(sql_state.is_some(), "SQL syntax errors should include sql_state");
    }
}

#[tokio::test]
async fn query_on_nonexistent_view_returns_database_error() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let result = adapter.execute_where_query("v_does_not_exist", None, None, None, None).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Database { .. }),
        "expected Database error, got {err:?}"
    );
}

#[tokio::test]
async fn query_timeout_via_statement_timeout() {
    let adapter = common::testcontainer::get_test_adapter().await;

    // Set a tight statement_timeout then run a slow query
    let _ = adapter.execute_raw_query("SET statement_timeout = '10ms'").await;
    let result = adapter.execute_raw_query("SELECT pg_sleep(5)").await;
    assert!(result.is_err());
    let err = result.unwrap_err();

    // PostgreSQL cancellation due to statement_timeout: SQL state "57014"
    if let FraiseQLError::Database { sql_state, .. } = &err {
        assert_eq!(
            sql_state.as_deref(),
            Some("57014"),
            "statement_timeout should produce SQL state 57014, got {sql_state:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Constraint violations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn duplicate_primary_key_returns_sql_state_23505() {
    let adapter = common::testcontainer::get_test_adapter().await;

    // Use a fixed UUID to trigger a duplicate
    let fixed_id = "00000000-0000-0000-0000-000000000001";
    let insert = format!(
        "INSERT INTO test.tb_project (id, data) VALUES ('{fixed_id}', '{{\"name\": \"dup test\"}}')"
    );

    // First insert should succeed (or already exist from a prior run — ignore)
    let _ = adapter.execute_raw_query(&insert).await;

    // Second insert with the same PK must fail
    let result = adapter.execute_raw_query(&insert).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    if let FraiseQLError::Database { sql_state, .. } = &err {
        assert_eq!(
            sql_state.as_deref(),
            Some("23505"),
            "unique violation should produce SQL state 23505, got {sql_state:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Pool behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pool_size_one_with_concurrent_queries_does_not_hang() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = PostgresAdapter::with_pool_size(&container.connection_string(), 1)
        .await
        .unwrap();

    // Run two queries concurrently on a pool of 1 — second must wait
    let a1 = adapter.clone();
    let a2 = adapter.clone();

    let (r1, r2) =
        tokio::join!(a1.execute_raw_query("SELECT 1 AS v"), a2.execute_raw_query("SELECT 2 AS v"),);

    // Both should complete (deadpool queues the second)
    assert!(r1.is_ok(), "first query failed: {r1:?}");
    assert!(r2.is_ok(), "second query failed: {r2:?}");
}

// ---------------------------------------------------------------------------
// Health check and pool metrics against real DB
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_check_succeeds_with_running_database() {
    let adapter = common::testcontainer::get_test_adapter().await;
    assert!(adapter.health_check().await.is_ok());
}

#[tokio::test]
async fn pool_metrics_reflect_real_state() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let metrics = adapter.pool_metrics();
    assert!(
        metrics.total_connections > 0,
        "expected at least 1 connection, got {}",
        metrics.total_connections
    );
}

// ---------------------------------------------------------------------------
// Successful queries for sanity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn query_seeded_view_returns_data() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let results = adapter.execute_where_query("test.v_user", None, None, None, None).await.unwrap();
    assert!(!results.is_empty(), "seeded v_user should return rows");
}

#[tokio::test]
async fn query_with_limit_respects_limit() {
    let adapter = common::testcontainer::get_test_adapter().await;
    let results = adapter
        .execute_where_query("test.v_project", None, Some(2), None, None)
        .await
        .unwrap();
    assert!(results.len() <= 2, "limit 2 should return at most 2 rows");
}
