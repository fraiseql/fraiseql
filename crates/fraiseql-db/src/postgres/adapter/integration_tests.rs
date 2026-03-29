//! PostgreSQL integration tests.
#![allow(clippy::unwrap_used)]
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

const TEST_DB_URL: &str =
    "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql";

// Helper to create test adapter
async fn create_test_adapter() -> PostgresAdapter {
    PostgresAdapter::new(TEST_DB_URL)
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
    let adapter = PostgresAdapter::with_pool_size(TEST_DB_URL, 5)
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

    assert!(result.is_err());
    match result {
        Err(FraiseQLError::Database { .. }) => (),
        _ => panic!("Expected Database error"),
    }
}

#[tokio::test]
async fn test_invalid_connection_string() {
    let result =
        PostgresAdapter::new("postgresql://invalid:invalid@localhost:9999/nonexistent").await;

    assert!(result.is_err());
    match result {
        Err(FraiseQLError::ConnectionPool { .. }) => (),
        _ => panic!("Expected ConnectionPool error"),
    }
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
