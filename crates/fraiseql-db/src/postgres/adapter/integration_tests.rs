//! PostgreSQL integration tests.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::items_after_statements)]
#![allow(clippy::panic)] // Reason: test code, panics acceptable
//! These tests require a running PostgreSQL database with test data.
//!
//! ## Running the tests
//!
//! ```bash
//! # Start test database
//! docker compose -f docker-compose.test.yml up -d postgres-test
//!
//! # Run tests with the test-postgres feature
//! cargo test -p fraiseql-core --features test-postgres db::postgres::adapter::tests
//!
//! # Or run all tests including ignored ones (legacy method)
//! cargo test -p fraiseql-core -- --ignored
//!
//! # Stop test database
//! docker compose -f docker-compose.test.yml down
//! ```

use fraiseql_error::FraiseQLError;
use serde_json::json;

use super::*;
use crate::{WhereClause, WhereOperator, traits::DatabaseAdapter, types::DatabaseType};

// Test DB URL from the `fraiseql_test_support` env-URL harness (`DATABASE_URL`), so this
// suite runs against a Dagger-bound service (local == CI) instead of a hardcoded host.
fn test_db_url() -> String {
    fraiseql_test_support::database_url()
}

// Helper to create test adapter
async fn create_test_adapter() -> PostgresAdapter {
    PostgresAdapter::new(&test_db_url())
        .await
        .expect("Failed to create test adapter - is PostgreSQL running? Use: docker compose -f docker-compose.test.yml up -d postgres-test")
}

// ========================================================================
// Connection & Adapter Tests
// ========================================================================

#[tokio::test]
async fn test_adapter_creation() {
    let adapter = create_test_adapter().await;
    let metrics = adapter.pool_metrics();
    assert!(metrics.total_connections > 0);
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
}

#[tokio::test]
async fn test_adapter_with_custom_pool_size() {
    let adapter = PostgresAdapter::with_pool_size(&test_db_url(), 5)
        .await
        .expect("Failed to create adapter");

    // Pool starts with 1 connection and grows on demand up to max_size
    let metrics = adapter.pool_metrics();
    assert!(metrics.total_connections >= 1, "Pool should have at least 1 connection");
    assert!(metrics.total_connections <= 5, "Pool should not exceed max_size of 5");
}

#[tokio::test]
async fn test_health_check() {
    let adapter = create_test_adapter().await;
    adapter.health_check().await.expect("Health check failed");
}

#[tokio::test]
async fn test_pool_metrics() {
    let adapter = create_test_adapter().await;
    let metrics = adapter.pool_metrics();

    assert!(metrics.total_connections > 0);
    assert!(metrics.idle_connections <= metrics.total_connections);
    assert_eq!(metrics.active_connections, metrics.total_connections - metrics.idle_connections);
}

// ========================================================================
// Simple Query Tests (No WHERE Clause)
// ========================================================================

#[tokio::test]
async fn test_query_all_users() {
    let adapter = create_test_adapter().await;

    let results = adapter
        .execute_where_query("v_user", None, None, None, None)
        .await
        .expect("Failed to query users");

    assert_eq!(results.len(), 5, "Should have 5 test users");

    // Verify JSONB structure
    let first_user = results[0].as_value();
    assert!(first_user.get("id").is_some());
    assert!(first_user.get("email").is_some());
    assert!(first_user.get("name").is_some());
}

#[tokio::test]
async fn test_query_all_posts() {
    let adapter = create_test_adapter().await;

    let results = adapter
        .execute_where_query("v_post", None, None, None, None)
        .await
        .expect("Failed to query posts");

    assert_eq!(results.len(), 4, "Should have 4 test posts");

    // Verify nested author object
    let first_post = results[0].as_value();
    assert!(first_post.get("author").is_some());
    assert!(first_post["author"].get("name").is_some());
}

// ========================================================================
// WHERE Clause Tests - Comparison Operators
// ========================================================================

#[tokio::test]
async fn test_where_eq() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice@example.com"),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_value()["email"], "alice@example.com");
}

#[tokio::test]
async fn test_where_neq() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["role".to_string()],
        operator: WhereOperator::Neq,
        value:    json!("user"),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    // Should return admin and moderator (not regular users)
    assert!(results.len() >= 2);
    for result in &results {
        assert_ne!(result.as_value()["role"], "user");
    }
}

#[tokio::test]
async fn test_where_gt() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: WhereOperator::Gt,
        value:    json!(30),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert!(!results.is_empty(), "Should return at least one result");
    assert_eq!(results.len(), 1, "Should return exactly 1 user (Charlie with age 35)");

    for result in &results {
        let age = result.as_value()["age"].as_i64().unwrap();
        assert!(age > 30, "Age should be > 30, but got {}", age);
    }
}

#[tokio::test]
async fn test_where_gte() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: WhereOperator::Gte,
        value:    json!(30),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    for result in &results {
        let age = result.as_value()["age"].as_i64().unwrap();
        assert!(age >= 30);
    }
}

// ========================================================================
// WHERE Clause Tests - String Operators
// ========================================================================

#[tokio::test]
async fn test_where_icontains() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["email".to_string()],
        operator: WhereOperator::Icontains,
        value:    json!("example.com"),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert!(results.len() >= 3);
    for result in &results {
        let email = result.as_value()["email"].as_str().unwrap();
        assert!(email.to_lowercase().contains("example.com"));
    }
}

#[tokio::test]
async fn test_where_startswith() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: WhereOperator::Startswith,
        value:    json!("Alice"),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 1);
    assert!(results[0].as_value()["name"].as_str().unwrap().starts_with("Alice"));
}

// ========================================================================
// WHERE Clause Tests - Logical Operators
// ========================================================================

#[tokio::test]
async fn test_where_and() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        },
        WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gte,
            value:    json!(25),
        },
    ]);

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    for result in &results {
        assert_eq!(result.as_value()["active"], true);
        let age = result.as_value()["age"].as_i64().unwrap();
        assert!(age >= 25);
    }
}

#[tokio::test]
async fn test_where_or() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Or(vec![
        WhereClause::Field {
            path:     vec!["role".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("admin"),
        },
        WhereClause::Field {
            path:     vec!["role".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("moderator"),
        },
    ]);

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert!(results.len() >= 2);
    for result in &results {
        let role = result.as_value()["role"].as_str().unwrap();
        assert!(role == "admin" || role == "moderator");
    }
}

#[tokio::test]
async fn test_where_not() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Not(Box::new(WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    }));

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    for result in &results {
        assert_ne!(result.as_value()["active"], json!(true));
    }
}

// ========================================================================
// WHERE Clause Tests - Array Operators
// ========================================================================

#[tokio::test]
async fn test_where_in() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["role".to_string()],
        operator: WhereOperator::In,
        value:    json!(["admin", "moderator"]),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert!(results.len() >= 2);
    for result in &results {
        let role = result.as_value()["role"].as_str().unwrap();
        assert!(role == "admin" || role == "moderator");
    }
}

// ========================================================================
// Pagination Tests
// ========================================================================

#[tokio::test]
async fn test_limit() {
    let adapter = create_test_adapter().await;

    let results = adapter
        .execute_where_query("v_user", None, Some(2), None, None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_offset() {
    let adapter = create_test_adapter().await;

    let results_all = adapter
        .execute_where_query("v_user", None, None, None, None)
        .await
        .expect("Failed to execute query");

    let results_offset = adapter
        .execute_where_query("v_user", None, None, Some(2), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results_offset.len(), results_all.len() - 2);
}

#[tokio::test]
async fn test_limit_and_offset() {
    let adapter = create_test_adapter().await;

    let results = adapter
        .execute_where_query("v_user", None, Some(2), Some(1), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 2);
}

// ========================================================================
// Nested Object Tests
// ========================================================================

#[tokio::test]
async fn test_nested_object_query() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["metadata".to_string(), "city".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("Paris"),
    };

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    assert!(!results.is_empty());
    for result in &results {
        assert_eq!(result.as_value()["metadata"]["city"], "Paris");
    }
}

// ========================================================================
// Complex Query Tests
// ========================================================================

#[tokio::test]
async fn test_complex_nested_where() {
    let adapter = create_test_adapter().await;

    // (active = true) AND ((role = 'admin') OR (age >= 30))
    let where_clause = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        },
        WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(30),
            },
        ]),
    ]);

    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), None, None, None)
        .await
        .expect("Failed to execute query");

    for result in &results {
        assert_eq!(result.as_value()["active"], true);
        let role = result.as_value()["role"].as_str().unwrap();
        let age = result.as_value()["age"].as_i64().unwrap();
        assert!(role == "admin" || age >= 30);
    }
}

// ========================================================================
// Error Handling Tests
// ========================================================================

#[tokio::test]
async fn test_invalid_view_name() {
    let adapter = create_test_adapter().await;

    let result = adapter.execute_where_query("v_nonexistent", None, None, None, None).await;

    assert!(
        matches!(result, Err(FraiseQLError::Database { .. })),
        "expected Database error for nonexistent view, got: {result:?}"
    );
}

#[tokio::test]
async fn test_invalid_connection_string() {
    let result =
        PostgresAdapter::new("postgresql://invalid:invalid@localhost:9999/nonexistent").await;

    assert!(
        matches!(result, Err(FraiseQLError::ConnectionPool { .. })),
        "expected ConnectionPool error for invalid connection string, got: {result:?}"
    );
}

// ========================================================================
// Parameterized Query Tests (LIMIT/OFFSET with parameters)
// ========================================================================

#[tokio::test]
async fn test_parameterized_limit_only() {
    let adapter = create_test_adapter().await;

    // Test that LIMIT is parameterized (not interpolated)
    let results = adapter
        .execute_where_query("v_user", None, Some(3), None, None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), 3, "Should return exactly 3 results with parameterized LIMIT");
}

#[tokio::test]
async fn test_parameterized_offset_only() {
    let adapter = create_test_adapter().await;

    let results_all = adapter
        .execute_where_query("v_user", None, None, None, None)
        .await
        .expect("Failed to execute query");

    let offset_val = 1;
    let results_offset = adapter
        .execute_where_query("v_user", None, None, Some(offset_val), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results_offset.len(), results_all.len() - offset_val as usize);
}

#[tokio::test]
async fn test_parameterized_limit_and_offset() {
    let adapter = create_test_adapter().await;

    // Query with both LIMIT and OFFSET parameterized
    let limit_val = 2;
    let offset_val = 1;
    let results = adapter
        .execute_where_query("v_user", None, Some(limit_val), Some(offset_val), None)
        .await
        .expect("Failed to execute query");

    assert_eq!(results.len(), limit_val as usize, "Should return exactly {} results", limit_val);
}

#[tokio::test]
async fn test_parameterized_limit_with_where_clause() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };

    // Parameterized LIMIT with WHERE clause
    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), Some(2), None, None)
        .await
        .expect("Failed to execute query");

    assert!(results.len() <= 2);
    for result in &results {
        assert_eq!(result.as_value()["active"], true);
    }
}

#[tokio::test]
async fn test_parameterized_limit_and_offset_with_where_clause() {
    let adapter = create_test_adapter().await;

    let where_clause = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };

    // Parameterized LIMIT and OFFSET with WHERE clause
    let results = adapter
        .execute_where_query("v_user", Some(&where_clause), Some(2), Some(1), None)
        .await
        .expect("Failed to execute query");

    assert!(results.len() <= 2);
    for result in &results {
        assert_eq!(result.as_value()["active"], true);
    }
}

// ========================================================================
// Mutation Timing Tests
// ========================================================================

#[tokio::test]
async fn test_mutation_timing_disabled_by_default() {
    let adapter = create_test_adapter().await;
    assert!(!adapter.mutation_timing_enabled());
}

#[tokio::test]
async fn test_mutation_timing_with_builder() {
    let adapter = create_test_adapter().await.with_mutation_timing("fraiseql.started_at");
    assert!(adapter.mutation_timing_enabled());
}

#[tokio::test]
async fn test_mutation_timing_custom_variable() {
    let adapter = create_test_adapter().await.with_mutation_timing("app.custom_timer");
    assert!(adapter.mutation_timing_enabled());
}

#[tokio::test]
async fn test_execute_function_call_with_timing_disabled() {
    let adapter = create_test_adapter().await;

    // Calling a nonexistent function should produce a Database error,
    // not a timing-related error — verifying the non-timing path is taken.
    let result = adapter.execute_function_call("fn_nonexistent", &[]).await;
    assert!(
        matches!(result, Err(FraiseQLError::Database { .. })),
        "expected Database error for nonexistent function (timing disabled), got: {result:?}"
    );
}

#[tokio::test]
async fn test_execute_function_call_with_timing_enabled() {
    let adapter = create_test_adapter().await.with_mutation_timing("fraiseql.started_at");

    // Calling a nonexistent function should still produce a Database error,
    // but the timing transaction wrapping should not cause a different error type.
    let result = adapter.execute_function_call("fn_nonexistent", &[]).await;
    assert!(
        matches!(result, Err(FraiseQLError::Database { .. })),
        "expected Database error for nonexistent function (timing enabled), got: {result:?}"
    );
}

// #413: proves the live tokio-postgres driver populates `sql_state` with the exact
// SQLSTATE class the HTTP error-mapper keys on (class 22 = client-input data
// exception). The unit tests cover the 22xxx/23xxx → 400 classification; this closes
// the gap that the *premise* (the class actually arrives) holds against real Postgres.
#[tokio::test]
async fn execute_raw_query_surfaces_sqlstate_22p02_for_malformed_cast() {
    let adapter = create_test_adapter().await;
    let result = adapter.execute_raw_query("SELECT 'not-a-uuid'::uuid").await;
    match result {
        Err(FraiseQLError::Database { sql_state, .. }) => {
            assert_eq!(
                sql_state.as_deref(),
                Some("22P02"),
                "a malformed uuid cast must surface SQLSTATE 22P02 (class 22 → HTTP 400)"
            );
        },
        other => panic!("expected Database error carrying sql_state, got: {other:?}"),
    }
}

// #888: `tokio_postgres::Error`'s `Display` for a server-side failure is the literal
// string `db error` — the half that names the relation, column or constraint lives on
// the `DbError` behind `as_db_error()`. These two drive real Postgres failures through
// the two query paths the adapter exposes and require the server's own text.

#[tokio::test]
async fn execute_raw_query_names_the_missing_relation() {
    let adapter = create_test_adapter().await;
    let result = adapter.execute_raw_query("SELECT data FROM v_p09_no_such_relation").await;
    match result {
        Err(FraiseQLError::Database { message, sql_state }) => {
            assert_eq!(sql_state.as_deref(), Some("42P01"), "expected undefined_table SQLSTATE");
            assert!(
                message.contains("v_p09_no_such_relation"),
                "the error must name the relation Postgres reported as missing, \
                 not just `db error`; got: {message}"
            );
        },
        other => panic!("expected Database error for a missing relation, got: {other:?}"),
    }
}

#[tokio::test]
async fn parameterized_aggregate_names_the_missing_relation() {
    let adapter = create_test_adapter().await;
    let result = adapter
        .execute_parameterized_aggregate(
            "SELECT count(*) FROM tb_p09_no_such_relation WHERE data->>'k' = $1",
            &[json!("v")],
        )
        .await;
    match result {
        Err(FraiseQLError::Database { message, sql_state }) => {
            assert_eq!(sql_state.as_deref(), Some("42P01"), "expected undefined_table SQLSTATE");
            assert!(
                message.contains("tb_p09_no_such_relation"),
                "the aggregate path must carry Postgres' own message too; got: {message}"
            );
        },
        other => panic!("expected Database error for a missing relation, got: {other:?}"),
    }
}

// ========================================================================
// Pool Pre-warming Tests (Issue #183)
// ========================================================================

#[tokio::test]
async fn pool_prewarms_to_min_size() {
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      5,
            max_size:      20,
            timeout_secs:  None,
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: None,
        },
    )
    .await
    .expect("adapter should be created");

    let metrics = adapter.pool_metrics();
    assert!(
        metrics.idle_connections >= 5,
        "expected >=5 idle connections after pre-warm, got {}",
        metrics.idle_connections
    );
}

#[tokio::test]
async fn pool_prewarm_zero_min_size_creates_one_connection() {
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      10,
            timeout_secs:  None,
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: None,
        },
    )
    .await
    .expect("adapter should be created");

    let metrics = adapter.pool_metrics();
    assert_eq!(
        metrics.idle_connections, 1,
        "expected exactly 1 idle connection with min_size=0"
    );
}

#[tokio::test]
async fn pool_prewarm_min_capped_at_max() {
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      100,
            max_size:      3,
            timeout_secs:  None,
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: None,
        },
    )
    .await
    .expect("adapter should not panic when min_size > max_size");

    let metrics = adapter.pool_metrics();
    assert!(
        metrics.idle_connections <= 3,
        "idle connections ({}) should not exceed max_size (3)",
        metrics.idle_connections
    );
}

#[tokio::test]
async fn pool_timeout_causes_fast_failure_when_exhausted() {
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      1,
            max_size:      1,
            timeout_secs:  Some(1),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: None,
        },
    )
    .await
    .expect("adapter created");

    // Hold the single connection so the pool is exhausted.
    let _held = adapter.pool().get().await.expect("first get ok");

    let start = std::time::Instant::now();
    let result = adapter.acquire_connection_with_retry().await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "should fail when pool exhausted");
    // Should fail within ~2s (1s timeout + no retry for Timeout errors).
    assert!(elapsed.as_secs() < 3, "timeout should fail fast, took {}s", elapsed.as_secs());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("timeout") || err_msg.contains("busy"),
        "error should mention exhaustion, got: {err_msg}"
    );
}

#[tokio::test]
async fn acquire_does_not_retry_on_timeout_error() {
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      1,
            max_size:      1,
            timeout_secs:  Some(1),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: None,
        },
    )
    .await
    .expect("adapter created");

    let _hold = adapter.pool().get().await.unwrap();

    let err = adapter.acquire_connection_with_retry().await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("timeout") || msg.contains("busy"),
        "error should mention exhaustion, got: {msg}"
    );
}

// ========================================================================
// Panic surface: NULL / non-JSONB `data` column (audit H34)
// ========================================================================

// A backing view that projects a NULL `data` cell (e.g. an unmatched LEFT JOIN)
// must surface a `FraiseQLError::Database`, not panic the request task via
// `Row::get` (the PostgreSQL adapter was the drifted outlier — every other
// backend degraded to JSON null).
#[tokio::test]
async fn execute_raw_null_data_column_errors_instead_of_panicking() {
    let adapter = create_test_adapter().await;
    let result = adapter.execute_raw("SELECT NULL::jsonb AS data", &[], ReadRouting::Any).await;
    match result {
        Err(FraiseQLError::Database { message, .. }) => {
            assert!(
                message.contains("NULL `data`"),
                "expected a NULL-data Database error naming the column, got: {message}"
            );
        },
        other => panic!("expected FraiseQLError::Database for NULL data, got: {other:?}"),
    }
}

// A `data` column that is not JSONB must surface a Database error, not panic on
// a `Row::get` type mismatch.
#[tokio::test]
async fn execute_raw_non_jsonb_data_column_errors_instead_of_panicking() {
    let adapter = create_test_adapter().await;
    let result = adapter.execute_raw("SELECT 42 AS data", &[], ReadRouting::Any).await;
    assert!(
        matches!(result, Err(FraiseQLError::Database { .. })),
        "expected FraiseQLError::Database for a non-JSONB data column, got: {result:?}"
    );
}

// ── H35: row_to_map must not null NUMERIC / UUID / timestamp columns ──────────

// The headline drift bug: a `SUM(numeric)` aggregate (and any raw NUMERIC, UUID,
// or timestamp column) fell through `row_to_map`'s type ladder to `Null`. The
// query below uses only literals so it is seed-independent.
#[tokio::test]
async fn row_to_map_renders_numeric_uuid_and_timestamp() {
    let adapter = create_test_adapter().await;
    let rows = adapter
        .execute_raw_query(
            "SELECT \
               SUM(v)::numeric                              AS revenue, \
               '550e8400-e29b-41d4-a716-446655440000'::uuid AS uid, \
               '2024-01-15T10:30:00Z'::timestamptz          AS ts \
             FROM (VALUES (100.50::numeric), (200.25::numeric)) t(v)",
        )
        .await
        .expect("query failed");
    let row = &rows[0];

    // NUMERIC aggregate is a JSON number, not null.
    assert_eq!(row["revenue"], json!(300.75), "NUMERIC SUM must not be null");
    // UUID renders as its canonical string, not null.
    assert_eq!(
        row["uid"],
        json!("550e8400-e29b-41d4-a716-446655440000"),
        "UUID must not be null"
    );
    // timestamptz renders as ISO 8601 text, not null.
    assert!(
        row["ts"].as_str().is_some_and(|s| s.starts_with("2024-01-15")),
        "timestamptz must render as ISO text, got: {:?}",
        row["ts"]
    );
}

// A `SMALLINT`/`int2` column must decode to a JSON number, not null. The
// headline symptom: `app.mutation_response.http_status` is `SMALLINT`, so a
// failed mutation's `MutationError.httpStatus` arrived absent because the value
// fell through `row_to_map`'s integer ladder (which started at `i32`/`int4`) to
// `Null`. Literal cast keeps the fixture seed-independent.
#[tokio::test]
async fn row_to_map_renders_smallint() {
    let adapter = create_test_adapter().await;
    let rows = adapter
        .execute_raw_query("SELECT 404::int2 AS http_status")
        .await
        .expect("query failed");
    let row = &rows[0];

    assert_eq!(row["http_status"], json!(404), "SMALLINT/int2 must not be null");
}

// A PostgreSQL `ENUM` column must decode to its text label, not null. The headline
// symptom: `app.mutation_response.error_class` is the `app.mutation_error_class`
// ENUM, so a failed mutation's `error_class` fell through `row_to_map`'s type ladder
// (`String: FromSql` rejects a custom enum OID) to `Null`, and the parser then
// rejected the whole row with "succeeded=false requires error_class" — the typed
// error arm was never reached (#472).
#[tokio::test]
async fn row_to_map_renders_enum() {
    let adapter = create_test_adapter().await;
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE row_to_map_enum_t AS ENUM ('not_found', 'conflict'); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .expect("create enum type failed");
    let rows = adapter
        .execute_raw_query("SELECT 'not_found'::row_to_map_enum_t AS c_enum")
        .await
        .expect("query failed");

    assert_eq!(
        rows[0]["c_enum"],
        json!("not_found"),
        "ENUM column must decode to its text label, not null"
    );
}

// Cross-type conformance: one table of (SQL expression → expected JSON), so the
// next time a type drifts back to a silent null a shared test fails. Each column
// is a literal cast, keeping the fixture seed-independent.
#[tokio::test]
async fn row_to_map_type_conformance() {
    let adapter = create_test_adapter().await;
    let rows = adapter
        .execute_raw_query(
            "SELECT \
               7::int2                                      AS c_smallint, \
               1::int4                                      AS c_int, \
               9000000000::int8                             AS c_bigint, \
               1.5::float8                                  AS c_float, \
               true                                         AS c_bool, \
               'hi'::text                                   AS c_text, \
               42.42::numeric                               AS c_numeric, \
               '11111111-2222-3333-4444-555555555555'::uuid AS c_uuid, \
               '2030-06-13T08:00:00Z'::timestamptz          AS c_tstz, \
               '2030-06-13 08:00:00'::timestamp             AS c_ts, \
               '2030-06-13'::date                           AS c_date, \
               ARRAY['a','b']::text[]                       AS c_textarr, \
               '{\"k\":1}'::jsonb                            AS c_jsonb, \
               NULL::text                                   AS c_null",
        )
        .await
        .expect("query failed");
    let r = &rows[0];

    assert_eq!(r["c_smallint"], json!(7));
    assert_eq!(r["c_int"], json!(1));
    assert_eq!(r["c_bigint"], json!(9_000_000_000i64));
    assert_eq!(r["c_float"], json!(1.5));
    assert_eq!(r["c_bool"], json!(true));
    assert_eq!(r["c_text"], json!("hi"));
    assert_eq!(r["c_numeric"], json!(42.42));
    assert_eq!(r["c_uuid"], json!("11111111-2222-3333-4444-555555555555"));
    assert!(r["c_tstz"].as_str().is_some_and(|s| s.starts_with("2030-06-13")));
    assert!(r["c_ts"].as_str().is_some_and(|s| s.starts_with("2030-06-13")));
    assert_eq!(r["c_date"], json!("2030-06-13"));
    assert_eq!(r["c_textarr"], json!(["a", "b"]));
    assert_eq!(r["c_jsonb"], json!({"k": 1}));
    assert_eq!(r["c_null"], serde_json::Value::Null);
}

// ── #980: NUMERIC values rust_decimal could not represent must not be null ────

// The old NUMERIC branch decoded through `rust_decimal::Decimal`, whose 96-bit
// mantissa caps at 28-29 significant digits and which has no NaN or Infinity —
// so a wider value, a tiny exponent, or a NaN failed `try_get` and fell through
// the type ladder to `Null`, silently.
#[tokio::test]
async fn row_to_map_renders_numeric_beyond_decimal_range_and_nan() {
    let adapter = create_test_adapter().await;
    let rows = adapter
        .execute_raw_query(
            "SELECT \
               12345678901234567890123456789012345678901234567890.5::numeric AS wide, \
               1e-40::numeric      AS tiny, \
               'NaN'::numeric      AS nan, \
               'Infinity'::numeric AS inf",
        )
        .await
        .expect("query failed");
    let r = &rows[0];

    assert!(!r["wide"].is_null(), "a 50-digit NUMERIC must not decode to null");
    assert!(!r["tiny"].is_null(), "1e-40::numeric must not decode to null");
    assert_eq!(r["nan"], json!("NaN"), "NUMERIC NaN must decode as the text PostgreSQL prints");
    assert_eq!(r["inf"], json!("Infinity"), "NUMERIC Infinity must decode as text, not null");
}

// ── #980: the NUMERIC decoder must agree with PostgreSQL's own rendering ──────

// Differential property test: for every corpus value, decode the binary NUMERIC
// through `PgNumericText` and compare against `value::text` computed by the
// same server in the same round trip. PostgreSQL is the reference
// implementation — hand-rolled formatting on money-shaped data is this repo's
// recurring defect class (#719, #832, #833), so the decoder is proven against
// the engine rather than against our own expectations.
#[tokio::test]
async fn numeric_decode_matches_postgres_own_text_rendering() {
    // Deterministic corpus: curated edge shapes plus seeded pseudo-random
    // values, so a failure names a reproducible input.
    let mut corpus: Vec<String> = [
        "0",
        "0.000",
        "-0.000",
        "0.1",
        "-0.1",
        "1",
        "-1",
        "9999",
        "10000",
        "10001",
        "99999999",
        "1.5",
        "1.500",
        "1.0001",
        "123.4567",
        "0.00001",
        "0.000000001",
        "0.9999",
        "9999.9999",
        "10000.0001",
        "10000000001",
        "1.00000001",
        "123.000",
        "007.5",
        "1e10",
        "1e-10",
        "1.5e30",
        "2e-40",
        "-12345678901234567890123456789012345678901234567890.09876543210987654321",
        "99999999999999999999999999999999999999999999999999999999999999999999999999999999",
        "0.00000000000000000000000000000000000000000000000000000000000000000000000000000001",
        "NaN",
        "Infinity",
        "-Infinity",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    corpus.extend(pseudo_random_decimal_strings(1500));

    let (client, connection) = tokio_postgres::connect(&test_db_url(), tokio_postgres::NoTls)
        .await
        .expect("failed to connect for the NUMERIC differential test");
    let connection_task = tokio::spawn(connection);

    // `$1::text::numeric` (not `$1::numeric`) so the parameter binds as text and
    // the cast happens server-side — binding a bare `$1::numeric` would make the
    // driver infer a NUMERIC parameter and reject the string.
    let stmt = client
        .prepare("SELECT $1::text::numeric AS bin, ($1::text::numeric)::text AS txt")
        .await
        .expect("prepare failed");

    for input in &corpus {
        let row = client
            .query_one(&stmt, &[input])
            .await
            .unwrap_or_else(|e| panic!("query failed for input {input}: {e}"));
        let ours: super::numeric::PgNumericText = row
            .try_get("bin")
            .unwrap_or_else(|e| panic!("decode failed for input {input}: {e}"));
        let oracle: String = row.try_get("txt").expect("text column");
        assert_eq!(
            ours.0, oracle,
            "decoder disagrees with PostgreSQL's own text rendering for input {input}"
        );
    }

    connection_task.abort();
}

// Seeded xorshift64 generator of decimal strings spanning the interesting
// space: 0-45 integer digits, 0-45 fraction digits, both signs, leading and
// trailing zeros occurring naturally. Deterministic so every CI failure is
// reproducible from the assertion message alone.
fn pseudo_random_decimal_strings(count: usize) -> Vec<String> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let int_len = usize::try_from(next() % 46).unwrap();
        let frac_len = usize::try_from(next() % 46).unwrap();
        if int_len == 0 && frac_len == 0 {
            continue;
        }
        let mut s = String::new();
        if next() % 2 == 0 {
            s.push('-');
        }
        if int_len == 0 {
            s.push('0');
        } else {
            for _ in 0..int_len {
                s.push(char::from(b'0' + u8::try_from(next() % 10).unwrap()));
            }
        }
        if frac_len > 0 {
            s.push('.');
            for _ in 0..frac_len {
                s.push(char::from(b'0' + u8::try_from(next() % 10).unwrap()));
            }
        }
        out.push(s);
    }
    out
}

// ── #832 (PostgreSQL half): relay ORDER BY must use the storage key ─────────
//
// The offset path renders ORDER BY through `render_order_by_columns`, which
// converts the camelCase GraphQL field name to its snake_case JSONB storage
// key and applies the declared type's cast. The relay path hand-built
// `data->>'{c.field}'` from the RAW field name instead, so `orderBy:
// {createdAt: DESC}` extracted a key that does not exist: NULL for every row,
// all rows tie, and the page came back in primary-key order with the requested
// sort silently dropped. It also skipped the cast, so a numeric field sorted
// lexicographically ("9" after "10").

/// Seeds a table whose JSONB `data` carries snake_case keys — the convention
/// the projection generator, the WHERE parser and the offset ORDER BY renderer
/// all use — and returns the view name.
async fn setup_relay_order_fixture(adapter: &PostgresAdapter, suffix: &str) -> String {
    let table = format!("tb_relay_order_{suffix}");
    let view = format!("v_relay_order_{suffix}");
    // `execute_raw_query` sends one statement per call (simple query protocol
    // rejects the multi-statement form here), so issue them individually.
    for stmt in [
        format!("DROP VIEW IF EXISTS {view}"),
        format!("DROP TABLE IF EXISTS {table}"),
        format!("CREATE TABLE {table} (pk BIGSERIAL PRIMARY KEY, data JSONB NOT NULL)"),
        format!(
            "INSERT INTO {table} (data) VALUES \
             ('{{\"id\":\"a\",\"created_at\":\"2024-01-01\",\"amount\":9}}'), \
             ('{{\"id\":\"b\",\"created_at\":\"2024-03-01\",\"amount\":100}}'), \
             ('{{\"id\":\"c\",\"created_at\":\"2024-02-01\",\"amount\":10}}')"
        ),
        format!("CREATE VIEW {view} AS SELECT pk, data FROM {table}"),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("apply relay fixture DDL");
    }
    view
}

async fn drop_relay_order_fixture(adapter: &PostgresAdapter, suffix: &str) {
    let _ = adapter
        .execute_raw_query(&format!("DROP VIEW IF EXISTS v_relay_order_{suffix}"))
        .await;
    let _ = adapter
        .execute_raw_query(&format!("DROP TABLE IF EXISTS tb_relay_order_{suffix}"))
        .await;
}

#[tokio::test]
async fn relay_order_by_camel_case_field_reaches_the_snake_case_storage_key() {
    use crate::{OrderByClause, OrderDirection, traits::RelayDatabaseAdapter};

    let adapter = create_test_adapter().await;
    let view = setup_relay_order_fixture(&adapter, "camel").await;

    // GraphQL sends the camelCase name; the JSONB key is `created_at`.
    let clauses = [OrderByClause::new(
        "createdAt".to_string(),
        OrderDirection::Desc,
    )];
    let page = adapter
        .execute_relay_page(&view, "pk", None, None, 10, true, None, Some(&clauses), false)
        .await
        .expect("relay page");

    let ids: Vec<String> = page
        .rows
        .iter()
        .map(|r| r.data["id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["b", "c", "a"],
        "newest-first by created_at; a camelCase orderBy must not be silently dropped"
    );
    drop_relay_order_fixture(&adapter, "camel").await;
}

#[tokio::test]
async fn relay_order_by_numeric_field_sorts_numerically_not_lexicographically() {
    use crate::{OrderByClause, OrderDirection, traits::RelayDatabaseAdapter};

    let adapter = create_test_adapter().await;
    let view = setup_relay_order_fixture(&adapter, "numeric").await;

    // Declared Numeric → the cast must be applied, or "10" sorts before "9".
    let mut clause = OrderByClause::new("amount".to_string(), OrderDirection::Asc);
    clause.field_type = crate::types::sql_hints::ScalarFieldType::Numeric;
    let page = adapter
        .execute_relay_page(&view, "pk", None, None, 10, true, None, Some(&[clause]), false)
        .await
        .expect("relay page");

    let amounts: Vec<i64> = page
        .rows
        .iter()
        .map(|r| r.data["amount"].as_i64().unwrap_or_default())
        .collect();
    assert_eq!(
        amounts,
        vec![9, 10, 100],
        "a Numeric orderBy must sort numerically; lexicographic order would be 10, 100, 9"
    );
    drop_relay_order_fixture(&adapter, "numeric").await;
}

// ========================================================================
// Read replica routing (#407)
// ========================================================================
//
// A real streaming replica is not available in the test rig, so a second,
// independent database stands in for a replica with unbounded lag: whatever is
// seeded there at setup is all a replica-routed read can ever see. That makes
// routing *observable*: a row marker says which database served the read, and a
// write that appears in a subsequent read can only have come from the primary.

/// Derive a URL for `db` on the same server as `base` (the rig URL).
fn url_for_db(base: &str, db: &str) -> String {
    assert!(!base.contains('?'), "rig URL is expected to carry no query string");
    let slash = base.rfind('/').expect("connection URL has a path segment");
    format!("{}/{db}", &base[..slash])
}

/// Admin connection to `url` for fixture DDL (simple-query protocol, so
/// multi-statement batches are fine).
async fn rr_admin_connect(url: &str) -> tokio_postgres::Client {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
        .await
        .expect("admin connection to test server");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::warn!(error = %e, "rr admin connection error");
        }
    });
    client
}

/// Create the primary/replica database pair for one test, with a marker row
/// telling us which database served a read. Returns (primary_url, replica_url).
async fn rr_fixture(test_tag: &str) -> (String, String) {
    let base = test_db_url();
    let admin = rr_admin_connect(&base).await;

    let primary_db = format!("rr407_{test_tag}_primary");
    let replica_db = format!("rr407_{test_tag}_replica");
    for db in [&primary_db, &replica_db] {
        admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .await
            .expect("drop scratch db");
        admin
            .batch_execute(&format!("CREATE DATABASE {db}"))
            .await
            .expect("create scratch db");
    }

    let primary_url = url_for_db(&base, &primary_db);
    let replica_url = url_for_db(&base, &replica_db);

    let primary = rr_admin_connect(&primary_url).await;
    primary
        .batch_execute(
            "CREATE TABLE tb_rr_item (id int PRIMARY KEY, label text NOT NULL);
             INSERT INTO tb_rr_item VALUES (1, 'from-primary');
             CREATE VIEW v_rr_item AS
               SELECT jsonb_build_object('id', id, 'label', label) AS data FROM tb_rr_item;
             CREATE FUNCTION fn_rr_insert(p jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
             BEGIN
               INSERT INTO tb_rr_item (id, label) VALUES ((p->>'id')::int, p->>'label');
               RETURN jsonb_build_object('status', 'ok');
             END $$;",
        )
        .await
        .expect("primary fixture DDL");

    let replica = rr_admin_connect(&replica_url).await;
    replica
        .batch_execute(
            "CREATE TABLE tb_rr_item (id int PRIMARY KEY, label text NOT NULL);
             INSERT INTO tb_rr_item VALUES (1, 'from-replica');
             CREATE VIEW v_rr_item AS
               SELECT jsonb_build_object('id', id, 'label', label) AS data FROM tb_rr_item;",
        )
        .await
        .expect("replica fixture DDL");

    (primary_url, replica_url)
}

/// Build an adapter over the fixture pair with the given pin window.
async fn rr_adapter(
    primary_url: &str,
    replica_url: &str,
    pin_after_write: std::time::Duration,
) -> PostgresAdapter {
    PostgresAdapter::with_pool_config(
        primary_url,
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls: vec![replica_url.to_string()],
                pin_after_write,
                // No staleness budget: these tests assert *where* a read is
                // served, and the stand-in replica reports no lag at all.
                max_lag: None,
                health_probe_interval: std::time::Duration::from_secs(1),
            }),
        },
    )
    .await
    .expect("replica-routed adapter should boot")
}

/// The label of the single `id = 1` row `v_rr_item` serves — the marker for
/// which database answered.
async fn rr_read_marker(adapter: &PostgresAdapter) -> String {
    let rows = adapter
        .execute_where_query(
            "v_rr_item",
            Some(&WhereClause::Field {
                path:     vec!["id".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(1),
            }),
            None,
            None,
            None,
        )
        .await
        .expect("marker read");
    assert_eq!(rows.len(), 1, "exactly one marker row expected");
    rows[0].as_value()["label"].as_str().expect("label marker").to_string()
}

#[tokio::test]
async fn reads_route_to_replica_when_no_recent_write() {
    let (primary_url, replica_url) = rr_fixture("route").await;
    let adapter = rr_adapter(&primary_url, &replica_url, std::time::Duration::from_secs(30)).await;

    assert_eq!(
        rr_read_marker(&adapter).await,
        "from-replica",
        "with replicas configured and no writes yet, a compiled read must be served by \
         the replica"
    );
}

#[tokio::test]
async fn writes_route_to_primary_with_replicas_configured() {
    let (primary_url, replica_url) = rr_fixture("writes").await;
    let adapter = rr_adapter(&primary_url, &replica_url, std::time::Duration::from_secs(30)).await;

    adapter
        .execute_function_call("fn_rr_insert", &[json!({"id": 2, "label": "written"})])
        .await
        .expect("mutation through the replica-routed adapter");

    // The write must exist on the primary and must NOT have touched the replica.
    let primary = rr_admin_connect(&primary_url).await;
    let n: i64 = primary
        .query_one("SELECT count(*) FROM tb_rr_item WHERE id = 2", &[])
        .await
        .expect("primary count")
        .get(0);
    assert_eq!(n, 1, "the mutation must land on the primary");

    let replica = rr_admin_connect(&replica_url).await;
    let n: i64 = replica
        .query_one("SELECT count(*) FROM tb_rr_item WHERE id = 2", &[])
        .await
        .expect("replica count")
        .get(0);
    assert_eq!(n, 0, "the mutation must never execute on a replica");
}

#[tokio::test]
async fn read_after_write_cannot_serve_the_stale_replica_row() {
    let (primary_url, replica_url) = rr_fixture("raw").await;
    let adapter = rr_adapter(&primary_url, &replica_url, std::time::Duration::from_secs(30)).await;

    // Sanity: before the write, reads are replica-served (the routing is live —
    // without this, a broken router that always reads the primary would pass
    // the assertion below for the wrong reason).
    assert_eq!(rr_read_marker(&adapter).await, "from-replica");

    adapter
        .execute_function_call("fn_rr_insert", &[json!({"id": 2, "label": "own-write"})])
        .await
        .expect("mutation");

    // The replica never receives the write (it simulates unbounded lag), so
    // this row can only come back if the post-write pin routed the read to the
    // primary: the read-your-writes guarantee.
    let rows = adapter
        .execute_where_query(
            "v_rr_item",
            Some(&WhereClause::Field {
                path:     vec!["id".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(2),
            }),
            None,
            None,
            None,
        )
        .await
        .expect("read-after-write");
    assert_eq!(
        rows.len(),
        1,
        "a client must read its own write immediately after a mutation; an empty result \
         means the read was served by the lagging replica"
    );
    assert_eq!(rows[0].as_value()["label"], json!("own-write"));
}

#[tokio::test]
async fn reads_return_to_the_replica_after_the_pin_expires() {
    let (primary_url, replica_url) = rr_fixture("expiry").await;
    let adapter =
        rr_adapter(&primary_url, &replica_url, std::time::Duration::from_millis(300)).await;

    adapter
        .execute_function_call("fn_rr_insert", &[json!({"id": 2, "label": "written"})])
        .await
        .expect("mutation");

    // Inside the window: pinned to primary.
    assert_eq!(rr_read_marker(&adapter).await, "from-primary");

    // Sleeping past a fixed lower bound is deterministic for expiry (the pin can
    // only be *shorter* than the sleep, never longer).
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    assert_eq!(
        rr_read_marker(&adapter).await,
        "from-replica",
        "the primary pin must expire so read load returns to the replicas"
    );
}

#[tokio::test]
async fn replica_pool_carries_the_tenant_search_path() {
    let (primary_url, replica_url) = rr_fixture("sp").await;

    // A same-named probe view in a tenant schema on BOTH databases; only the
    // search path decides which schema resolves, and only the routing decides
    // which database. The view also projects the *established*
    // (`pg_settings.reset_val`) search path — the #809 guard: a session-level
    // SET cannot fake it.
    let probe_ddl = |origin: &str| {
        format!(
            "CREATE SCHEMA tenant_rr;
             CREATE VIEW tenant_rr.v_rr_probe AS
               SELECT jsonb_build_object(
                 'origin', '{origin}',
                 'established_path',
                 (SELECT reset_val FROM pg_settings WHERE name = 'search_path')
               ) AS data;"
        )
    };
    rr_admin_connect(&primary_url)
        .await
        .batch_execute(&probe_ddl("primary"))
        .await
        .unwrap();
    rr_admin_connect(&replica_url)
        .await
        .batch_execute(&probe_ddl("replica"))
        .await
        .unwrap();

    let adapter = PostgresAdapter::with_pool_config(
        &primary_url,
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   Some(SearchPath::new(["tenant_rr", "public"]).unwrap()),
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls:                  vec![replica_url.clone()],
                pin_after_write:       std::time::Duration::from_secs(30),
                max_lag:               None,
                health_probe_interval: std::time::Duration::from_secs(1),
            }),
        },
    )
    .await
    .expect("adapter with search path + replica");

    let rows = adapter
        .execute_where_query("v_rr_probe", None, None, None, None)
        .await
        .expect("probe read resolves through the search path");
    assert_eq!(rows.len(), 1);
    let probe = rows[0].as_value().clone();
    assert_eq!(probe["origin"], json!("replica"), "an unpinned read must be replica-served");
    let established = probe["established_path"].as_str().unwrap_or_default();
    assert!(
        established.contains("tenant_rr"),
        "the replica pool's connections must carry the tenant search path in their \
         ESTABLISHED (startup) settings, not a session SET; got: {established}"
    );
}

#[tokio::test]
async fn unreachable_replica_refuses_to_boot() {
    let (primary_url, _replica_url) = rr_fixture("boot").await;

    let result = PostgresAdapter::with_pool_config(
        &primary_url,
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(2),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                // Port 9 (discard) on loopback: reliably connection-refused.
                urls:                  vec![
                    "postgres://nobody:nothing@127.0.0.1:9/nowhere".to_string(),
                ],
                pin_after_write:       std::time::Duration::from_secs(5),
                max_lag:               None,
                health_probe_interval: std::time::Duration::from_secs(1),
            }),
        },
    )
    .await;

    match result {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("unreachable at boot"),
                "the refusal must say which replica failed and why; got: {msg}"
            );
        },
        Ok(_) => panic!(
            "an adapter with an unreachable configured replica must refuse to boot, not \
             silently serve every read from the primary"
        ),
    }
}

#[tokio::test]
async fn empty_replica_url_list_is_refused() {
    let (primary_url, _replica_url) = rr_fixture("empty").await;

    let result = PostgresAdapter::with_pool_config(
        &primary_url,
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(2),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls:                  vec![],
                pin_after_write:       std::time::Duration::from_secs(5),
                max_lag:               None,
                health_probe_interval: std::time::Duration::from_secs(1),
            }),
        },
    )
    .await;

    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "an empty replica URL list is an inert configuration and must be refused loudly"
    );
}

// ========================================================================
// Bounded staleness against a REAL streaming standby (#957)
// ========================================================================
//
// Everything above stands a second independent database in for a replica. That
// shape is enough to observe *which* server answered, but it reports no
// replication lag at all — `pg_is_in_recovery()` is false there, so
// `pg_last_wal_replay_lsn()` is NULL — and a staleness budget proven against it
// would be proven against a server that can never be stale.
//
// These tests run against `STANDBY_DATABASE_URL`: a real streaming standby of
// the rig's primary (`postgres-standby-test` locally, `pgStandbyService` in
// Dagger). Lag is induced deterministically with `pg_wal_replay_pause()`, which
// stops replay while the walreceiver keeps streaming — exactly the shape of a
// standby that has fallen behind.
//
// The discriminator is `pg_is_in_recovery()` evaluated *inside the view*: a
// physical standby is byte-identical to its primary once caught up, so a marker
// row cannot say which server answered, but the recovery flag always can.

/// URL of the real streaming standby. Panics when unset — see
/// [`fraiseql_test_support::standby_database_url`] for why this must not skip.
fn standby_db_url() -> String {
    fraiseql_test_support::standby_database_url()
}

/// Create this test's probe view on the primary and wait for the standby to
/// replay it. Returns an admin client on the standby.
async fn bs_fixture(tag: &str) -> tokio_postgres::Client {
    let primary = rr_admin_connect(&test_db_url()).await;
    let standby = rr_admin_connect(&standby_db_url()).await;

    // A previous run that panicked between pause and resume would leave replay
    // stopped, and every later assertion would then read a standby frozen for
    // reasons of its own. Resuming is a no-op when nothing is paused.
    standby
        .simple_query("SELECT pg_wal_replay_resume()")
        .await
        .expect("resume replay");

    primary
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS v_rr957_{tag};
             DROP TABLE IF EXISTS tb_rr957_{tag};
             CREATE TABLE tb_rr957_{tag} (id int PRIMARY KEY, label text NOT NULL);
             INSERT INTO tb_rr957_{tag} VALUES (1, 'seed');
             CREATE VIEW v_rr957_{tag} AS
               SELECT jsonb_build_object(
                 'id', 1,
                 -- Evaluated by whichever server runs the query: this is the
                 -- only reliable way to tell a physical standby from its
                 -- primary, since their contents are identical when caught up.
                 'origin', CASE WHEN pg_is_in_recovery() THEN 'replica' ELSE 'primary' END,
                 'rows', (SELECT count(*) FROM tb_rr957_{tag})
               ) AS data;"
        ))
        .await
        .expect("primary fixture DDL");

    bs_wait_caught_up(&standby).await;
    standby
}

/// Drop this test's fixture from the primary and make sure replay is running.
async fn bs_cleanup(tag: &str, standby: &tokio_postgres::Client) {
    standby
        .simple_query("SELECT pg_wal_replay_resume()")
        .await
        .expect("resume replay");
    rr_admin_connect(&test_db_url())
        .await
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS v_rr957_{tag}; DROP TABLE IF EXISTS tb_rr957_{tag};"
        ))
        .await
        .expect("fixture cleanup");
}

/// Block until the standby has replayed everything it has received.
async fn bs_wait_caught_up(standby: &tokio_postgres::Client) {
    for _ in 0..200 {
        let row = standby
            .query_one(
                "SELECT pg_last_wal_replay_lsn() IS NOT NULL \
                   AND pg_last_wal_receive_lsn() = pg_last_wal_replay_lsn()",
                &[],
            )
            .await
            .expect("standby lsn read");
        if row.get::<_, bool>(0) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("standby never caught up with the primary");
}

/// Adapter over primary + the real standby, with the given staleness budget.
async fn bs_adapter(
    max_lag: Option<std::time::Duration>,
    probe_interval: std::time::Duration,
) -> PostgresAdapter {
    PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls: vec![standby_db_url()],
                // The fixtures are written through admin connections rather than
                // the mutation pipeline, so no pin is ever armed; zero states
                // that read-your-writes is not what these tests exercise.
                pin_after_write: std::time::Duration::ZERO,
                max_lag,
                health_probe_interval: probe_interval,
            }),
        },
    )
    .await
    .expect("adapter over the real standby should boot")
}

/// Which server answered a read through `adapter`: `"primary"` or `"replica"`.
///
/// Only valid while the replica is genuinely in recovery — see
/// [`bs_read_label`], which the failover test uses instead.
async fn bs_read_origin(adapter: &PostgresAdapter, tag: &str) -> String {
    bs_probe_field(adapter, tag, "origin").await
}

/// The stored `label` a read through `adapter` came back with — the discriminator
/// that still works once a replica has been promoted and the two databases can
/// hold different values.
async fn bs_read_label(adapter: &PostgresAdapter, tag: &str) -> String {
    bs_probe_field(adapter, tag, "label").await
}

async fn bs_probe_field(adapter: &PostgresAdapter, tag: &str, field: &str) -> String {
    let rows = adapter
        .execute_where_query(&format!("v_rr957_{tag}"), None, None, None, None)
        .await
        .expect("probe read");
    assert_eq!(rows.len(), 1, "the probe view yields exactly one row");
    rows[0].as_value()[field].as_str().expect("probe marker").to_string()
}

#[tokio::test]
async fn a_caught_up_standby_serves_reads_inside_the_lag_budget() {
    let tag = "inside";
    let standby = bs_fixture(tag).await;
    let adapter =
        bs_adapter(Some(std::time::Duration::from_secs(5)), std::time::Duration::from_millis(100))
            .await;

    // Nothing is eligible until a probe has produced a reading — "not measured
    // yet" is not "zero lag" — so the first reads legitimately land on the
    // primary. Wait for the gate to open rather than assuming a cadence.
    let mut origin = String::new();
    for _ in 0..100 {
        origin = bs_read_origin(&adapter, tag).await;
        if origin == "replica" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    assert_eq!(
        origin, "replica",
        "a standby that has replayed everything it received reports zero lag, and must \
         therefore serve reads while a staleness budget is configured"
    );
    bs_cleanup(tag, &standby).await;
}

#[tokio::test]
async fn a_standby_past_the_lag_budget_stops_serving_reads() {
    let tag = "past";
    let standby = bs_fixture(tag).await;
    let budget = std::time::Duration::from_millis(600);
    let adapter = bs_adapter(Some(budget), std::time::Duration::from_millis(100)).await;

    // Sanity, and the half that makes the assertion below mean something: while
    // the standby is caught up it serves the read. Without this, an adapter that
    // had simply stopped using replicas would pass the over-budget assertion for
    // entirely the wrong reason.
    let mut caught_up_origin = String::new();
    for _ in 0..100 {
        caught_up_origin = bs_read_origin(&adapter, tag).await;
        if caught_up_origin == "replica" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(caught_up_origin, "replica", "the caught-up standby must serve reads first");

    // Stop replay and keep the primary writing: the walreceiver goes on
    // streaming, so the standby holds a consistent — and steadily older —
    // snapshot. This is a real lagging standby, not a mocked LSN.
    standby
        .simple_query("SELECT pg_wal_replay_pause()")
        .await
        .expect("pause replay");
    let primary = rr_admin_connect(&test_db_url()).await;
    primary
        .batch_execute(&format!("INSERT INTO tb_rr957_{tag} VALUES (2, 'after-pause');"))
        .await
        .expect("write past the pause");

    // Wait for the standby to fall outside the budget, then assert the routing.
    // Polling for the condition rather than sleeping a guessed interval keeps
    // this deterministic: lag grows with wall-clock while replay is stopped.
    let mut origin = String::new();
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        origin = bs_read_origin(&adapter, tag).await;
        if origin == "primary" {
            break;
        }
    }

    assert_eq!(
        origin, "primary",
        "a standby whose measured replay lag exceeds max_lag must stop serving reads; \
         serving them anyway is the bounded-staleness guarantee being reported and not kept"
    );

    // And the point of it all: the read that moved to the primary sees the row
    // the standby has not replayed.
    let rows = adapter
        .execute_where_query(&format!("v_rr957_{tag}"), None, None, None, None)
        .await
        .expect("probe read");
    assert_eq!(
        rows[0].as_value()["rows"],
        json!(2),
        "the primary-served read must see the write the lagging standby is missing"
    );

    bs_cleanup(tag, &standby).await;
}

#[tokio::test]
async fn a_replica_whose_lag_cannot_be_measured_is_never_eligible() {
    // The plain-database stand-in is exactly the state a promoted replica is in:
    // reachable, queryable, and outside recovery, so `pg_last_wal_replay_lsn()`
    // is NULL and there is no lag to compare against a budget. Unknown staleness
    // must route to the primary — reading it as zero is the fail-open.
    let (primary_url, replica_url) = rr_fixture("unmeasurable").await;
    let adapter = PostgresAdapter::with_pool_config(
        &primary_url,
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls:                  vec![replica_url.clone()],
                pin_after_write:       std::time::Duration::ZERO,
                max_lag:               Some(std::time::Duration::from_secs(60)),
                health_probe_interval: std::time::Duration::from_millis(100),
            }),
        },
    )
    .await
    .expect("adapter with an unmeasurable replica still boots");

    // Generous: a budget of 60 s against a replica that would report zero lag if
    // it could is the most favourable case an implementation reading "unknown"
    // as "fine" could hope for.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(
        rr_read_marker(&adapter).await,
        "from-primary",
        "a replica that cannot report replication lag must never satisfy a staleness \
         budget, however generous the budget is"
    );
}

/// Read `v_rr957_<tag>` through `adapter` with an explicit per-query routing.
async fn bs_read_origin_routed(
    adapter: &PostgresAdapter,
    tag: &str,
    routing: crate::types::ReadRouting,
) -> String {
    let rows = adapter
        .execute_where_query_arc_with_session(
            &format!("v_rr957_{tag}"),
            None,
            None,
            None,
            None,
            &[],
            routing,
        )
        .await
        .expect("routed probe read");
    assert_eq!(rows.len(), 1, "the probe view yields exactly one row");
    rows[0].as_value()["origin"].as_str().expect("origin marker").to_string()
}

#[tokio::test]
async fn a_query_routed_to_the_primary_never_reads_a_replica() {
    let tag = "routing";
    let standby = bs_fixture(tag).await;
    // No server-wide budget and no pin: everything about this test comes from the
    // per-query annotation, so a passing assertion cannot be some other rule's doing.
    let adapter = bs_adapter(None, std::time::Duration::from_millis(100)).await;

    // The control: with `any`, the standby serves the read. Without this half, an
    // adapter that had simply stopped using replicas would satisfy the assertion
    // below for entirely the wrong reason.
    assert_eq!(
        bs_read_origin_routed(&adapter, tag, crate::types::ReadRouting::Any).await,
        "replica",
        "with no routing override, an unpinned read is replica-served"
    );

    // Same adapter, same view, same instant — only the annotation differs.
    assert_eq!(
        bs_read_origin_routed(&adapter, tag, crate::types::ReadRouting::Primary).await,
        "primary",
        "read_routing = primary must keep the query off replicas entirely; a replica \
         cannot promise freshness, which is the only reason to ask for the primary"
    );

    bs_cleanup(tag, &standby).await;
}

#[tokio::test]
async fn a_query_routed_to_a_replica_ignores_the_read_your_writes_pin() {
    let tag = "pin";
    let standby = bs_fixture(tag).await;
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls:                  vec![standby_db_url()],
                // Long enough that nothing expires during the test.
                pin_after_write:       std::time::Duration::from_secs(300),
                max_lag:               None,
                health_probe_interval: std::time::Duration::from_millis(100),
            }),
        },
    )
    .await
    .expect("pinning adapter should boot");

    // Arm the pin the way the mutation pipeline does.
    adapter.mark_write();

    assert_eq!(
        bs_read_origin_routed(&adapter, tag, crate::types::ReadRouting::Any).await,
        "primary",
        "inside the pin window an unannotated read is primary-served — read-your-writes"
    );
    assert_eq!(
        bs_read_origin_routed(
            &adapter,
            tag,
            crate::types::ReadRouting::Replica { max_lag_ms: None }
        )
        .await,
        "replica",
        "read_routing = replica opts out of the pin: the pin exists so a client reads \
         its own writes, and this query has declared that is not what it is for"
    );

    bs_cleanup(tag, &standby).await;
}

#[tokio::test]
async fn a_per_query_lag_budget_overrides_the_servers() {
    let tag = "budget";
    let standby = bs_fixture(tag).await;
    // Server-wide budget generous enough that it alone would keep the standby in
    // rotation; the per-query budget is what has to exclude it.
    let adapter = bs_adapter(
        Some(std::time::Duration::from_secs(300)),
        std::time::Duration::from_millis(100),
    )
    .await;

    let mut origin = String::new();
    for _ in 0..100 {
        origin = bs_read_origin(&adapter, tag).await;
        if origin == "replica" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(origin, "replica", "under the server's budget, the standby serves reads");

    // Stop replay and write, so the standby holds a measurably older snapshot —
    // still far inside the server's 300 s budget.
    standby
        .simple_query("SELECT pg_wal_replay_pause()")
        .await
        .expect("pause replay");
    rr_admin_connect(&test_db_url())
        .await
        .batch_execute(&format!("INSERT INTO tb_rr957_{tag} VALUES (2, 'after-pause');"))
        .await
        .expect("write past the pause");

    // A query that declares it tolerates only 300 ms must fall to the primary long
    // before the server's budget would have excluded anything.
    let tight = crate::types::ReadRouting::Replica {
        max_lag_ms: Some(300),
    };
    let mut routed = String::new();
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        routed = bs_read_origin_routed(&adapter, tag, tight).await;
        if routed == "primary" {
            break;
        }
    }
    assert_eq!(
        routed, "primary",
        "a per-query max_lag_ms must be enforced against the measured lag; ignoring it \
         in favour of the server's looser budget is the override being accepted and \
         dropped"
    );

    // And the server's own budget still holds for everyone else — proving the
    // tighter number came from the annotation and not from a global change.
    assert_eq!(
        bs_read_origin(&adapter, tag).await,
        "replica",
        "an unannotated query keeps the server's budget, which this lag is well inside"
    );

    bs_cleanup(tag, &standby).await;
}

#[tokio::test]
async fn a_replica_promoted_by_a_failover_stops_serving_reads() {
    // Runs against its own standby — `FAILOVER_STANDBY_DATABASE_URL` — because
    // `pg_promote()` is one-way. Sharing the bounded-staleness standby would make
    // those tests' meaning depend on which one libtest ran first.
    let failover_url = fraiseql_test_support::db::failover_standby_database_url();
    let tag = "failover";

    let primary = rr_admin_connect(&test_db_url()).await;
    // `pg_is_in_recovery()` cannot be the discriminator here, the way it is for
    // the bounded-staleness tests: after promotion it is false on the *replica*
    // too, so a read served by the promoted server would report "primary" and
    // the assertion would pass while doing exactly the wrong thing. The label is
    // a stored value, and once promoted the replica is writable — so the test
    // can give the two servers genuinely different contents, which is the
    // divergence the promotion causes in the first place.
    primary
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS v_rr957_{tag};
             DROP TABLE IF EXISTS tb_rr957_{tag};
             CREATE TABLE tb_rr957_{tag} (id int PRIMARY KEY, label text NOT NULL);
             INSERT INTO tb_rr957_{tag} VALUES (1, 'shared');
             CREATE VIEW v_rr957_{tag} AS
               SELECT jsonb_build_object(
                 'id', 1,
                 'origin', CASE WHEN pg_is_in_recovery() THEN 'replica' ELSE 'primary' END,
                 'label', (SELECT label FROM tb_rr957_{tag} WHERE id = 1)
               ) AS data;"
        ))
        .await
        .expect("primary fixture DDL");

    let standby = rr_admin_connect(&failover_url).await;
    // `pg_promote()` is one-way, so a second run in the same rig finds a plain
    // writable server and would otherwise fail somewhere far from the cause.
    let in_recovery: bool = standby
        .query_one("SELECT pg_is_in_recovery()", &[])
        .await
        .expect("failover standby recovery state")
        .get(0);
    assert!(
        in_recovery,
        "the failover standby is not in recovery — a previous run of this test already \
         promoted it, and promotion is one-way. Re-clone it with `make db-failover-reset` \
         (CI legs start from a fresh container and never hit this)."
    );
    bs_wait_caught_up(&standby).await;

    // No staleness budget: the point is that a promoted replica stops serving
    // whether or not one is configured. With `max_lag` set, "no measurable lag"
    // would exclude it anyway and the test would prove nothing about promotion.
    let adapter = PostgresAdapter::with_pool_config(
        &test_db_url(),
        PoolPrewarmConfig {
            min_size:      0,
            max_size:      5,
            timeout_secs:  Some(10),
            search_path:   None,
            tls:           PostgresTlsConfig::default(),
            read_replicas: Some(crate::postgres::ReadReplicaConfig {
                urls:                  vec![failover_url.clone()],
                pin_after_write:       std::time::Duration::ZERO,
                max_lag:               None,
                health_probe_interval: std::time::Duration::from_millis(100),
            }),
        },
    )
    .await
    .expect("adapter over the failover standby should boot");

    assert_eq!(
        bs_read_origin(&adapter, tag).await,
        "replica",
        "while it is still a standby, the replica serves reads"
    );

    // A real failover, not a simulated one.
    standby
        .simple_query("SELECT pg_promote(wait => true)")
        .await
        .expect("promote the standby");
    for _ in 0..100 {
        let row = standby.query_one("SELECT pg_is_in_recovery()", &[]).await.expect("recovery");
        if !row.get::<_, bool>(0) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Diverge the two databases, which is now possible precisely because the
    // promoted server accepts writes. Reading `promoted-replica` back can only
    // mean the read was served by it.
    standby
        .batch_execute(&format!(
            "UPDATE tb_rr957_{tag} SET label = 'promoted-replica' WHERE id = 1;"
        ))
        .await
        .expect("write to the promoted replica");

    // The boot health check ran once, before any of this. Only a periodic probe
    // can notice, and noticing has to change the routing: reads from a promoted
    // server diverge from the primary in *both* directions, which is not a
    // staleness a budget could bound.
    let mut label = String::new();
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        label = bs_read_label(&adapter, tag).await;
        if label == "shared" {
            break;
        }
    }
    assert_eq!(
        label, "shared",
        "a replica promoted out of recovery by a failover must stop serving reads; reading \
         `promoted-replica` back means the server answering is a now-independent database, \
         which is the failure a boot-time health check structurally cannot catch"
    );

    // The promoted standby's slot is now inactive and would retain WAL on the
    // primary for the rest of the run.
    primary
        .simple_query("SELECT pg_drop_replication_slot('fraiseql_failover')")
        .await
        .expect("drop the orphaned replication slot");
    primary
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS v_rr957_{tag}; DROP TABLE IF EXISTS tb_rr957_{tag};"
        ))
        .await
        .expect("fixture cleanup");
}
