#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Tests end-to-end query execution scenarios against **real PostgreSQL**:
//! - `data jsonb` tables seeded with sample data (the FraiseQL view shape)
//! - Query execution with field projection
//! - `ResultProjector` with projected data
//! - GraphQL response envelope generation
//! - Error handling and edge cases
//!
//! These run on the Dagger `integration --suite=postgres` leg (`--test '*'`
//! with `DATABASE_URL` bound) and skip cleanly when no Postgres is configured.

#![allow(clippy::used_underscore_binding, clippy::print_stderr)] // Reason: test helper results prefixed with _; skip notes to stderr
use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    error::FraiseQLError,
    runtime::ResultProjector,
};
use serde_json::json;

/// Sample user rows (the `data` payload of each `users` row).
fn users_seed() -> Vec<serde_json::Value> {
    vec![
        json!({
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Alice Johnson",
            "email": "alice@example.com",
            "status": "active",
            "created_at": "2024-01-15T10:00:00Z",
            "updated_at": "2024-01-15T10:00:00Z",
            "metadata": {
                "last_login": "2024-01-14T15:30:00Z",
                "login_count": 42
            }
        }),
        json!({
            "id": "223e4567-e89b-12d3-a456-426614174001",
            "name": "Bob Smith",
            "email": "bob@example.com",
            "status": "active",
            "created_at": "2024-01-10T09:30:00Z",
            "updated_at": "2024-01-14T11:00:00Z",
            "metadata": {
                "last_login": "2024-01-13T20:15:00Z",
                "login_count": 87
            }
        }),
        json!({
            "id": "323e4567-e89b-12d3-a456-426614174002",
            "name": "Carol Davis",
            "email": "carol@example.com",
            "status": "inactive",
            "created_at": "2023-12-01T14:45:00Z",
            "updated_at": "2024-01-05T08:20:00Z",
            "metadata": {
                "last_login": "2024-01-01T12:00:00Z",
                "login_count": 15
            }
        }),
    ]
}

/// Sample product rows (the `data` payload of each `products` row).
fn products_seed() -> Vec<serde_json::Value> {
    vec![
        json!({
            "id": "prod-001",
            "sku": "SKU-001",
            "name": "Product A",
            "price": 99.99,
            "stock": 150,
            "category": "Electronics",
            "available": true
        }),
        json!({
            "id": "prod-002",
            "sku": "SKU-002",
            "name": "Product B",
            "price": 149.99,
            "stock": 75,
            "category": "Electronics",
            "available": true
        }),
        json!({
            "id": "prod-003",
            "sku": "SKU-003",
            "name": "Product C",
            "price": 49.99,
            "stock": 0,
            "category": "Accessories",
            "available": false
        }),
    ]
}

/// (Re)provision `users` and `products` as `data jsonb` tables and seed them.
///
/// `execute_where_query(view, ...)` runs `SELECT data FROM "{view}"`, so each
/// table holds one `data jsonb` column per row. Returns `None` when no Postgres
/// is configured (`DATABASE_URL` unset and no local-testcontainers spawn) so the
/// caller skips on the non-DB preflight leg; the bound `Service` is returned so a
/// locally-spawned container, if any, is held for the test's lifetime.
async fn seed_users_products() -> Option<(fraiseql_test_support::Service, Arc<PostgresAdapter>)> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter =
        Arc::new(PostgresAdapter::new(pg.url()).await.expect("connect to harness postgres"));

    for (table, rows) in [("users", users_seed()), ("products", products_seed())] {
        adapter
            .execute_raw_query(&format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#))
            .await
            .expect("drop seed table");
        adapter
            .execute_raw_query(&format!(r#"CREATE TABLE "{table}" (data jsonb)"#))
            .await
            .expect("create seed table");
        for row in rows {
            let json = serde_json::to_string(&row).expect("serialize seed row");
            let escaped = json.replace('\'', "''");
            adapter
                .execute_raw_query(&format!(
                    r#"INSERT INTO "{table}" (data) VALUES ('{escaped}'::jsonb)"#
                ))
                .await
                .expect("seed row");
        }
    }

    Some((pg, adapter))
}

// ============================================================================
// Seed Data Tests
// ============================================================================

#[tokio::test]
async fn test_seed_data_users_available() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_seed_data_users_available: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_seed_data_products_available() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_seed_data_products_available: no postgres");
        return;
    };
    let results = adapter.execute_where_query("products", None, None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_seed_data_contains_correct_fields() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_seed_data_contains_correct_fields: no postgres");
        return;
    };
    let users = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    // Row order is not guaranteed without ORDER BY — find Alice by name.
    let alice = users
        .iter()
        .find(|u| u.as_value().get("name") == Some(&json!("Alice Johnson")))
        .expect("Alice Johnson seeded");
    let user_obj = alice.as_value();
    assert_eq!(user_obj.get("name"), Some(&json!("Alice Johnson")));
    assert_eq!(user_obj.get("email"), Some(&json!("alice@example.com")));
    assert_eq!(user_obj.get("status"), Some(&json!("active")));
}

// ============================================================================
// Query Execution Tests
// ============================================================================

#[tokio::test]
async fn test_query_execution_all_users() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_query_execution_all_users: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

#[tokio::test]
async fn test_query_execution_with_limit() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_query_execution_with_limit: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(2), None, None).await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_query_execution_products() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_query_execution_products: no postgres");
        return;
    };
    let results = adapter.execute_where_query("products", None, None, None, None).await.unwrap();

    assert_eq!(results.len(), 3);
}

// ============================================================================
// Result Projection Tests
// ============================================================================

#[tokio::test]
async fn test_result_projection_single_field() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_result_projection_single_field: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    // Project only id and name
    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&results, true).unwrap();

    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Verify each projected user only has the projected fields
    let first = &arr[0];
    assert!(first.get("id").is_some());
    assert!(first.get("name").is_some());
    assert_eq!(first.get("email"), None);
    assert_eq!(first.get("status"), None);
}

#[tokio::test]
async fn test_result_projection_multiple_fields() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_result_projection_multiple_fields: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    // Project id, name, and email
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "email".to_string()]);
    let projected = projector.project_results(&results, false).unwrap();

    assert!(projected.get("id").is_some());
    assert!(projected.get("name").is_some());
    assert!(projected.get("email").is_some());
    assert_eq!(projected.get("status"), None);
}

#[tokio::test]
async fn test_result_projection_products() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_result_projection_products: no postgres");
        return;
    };
    let results = adapter.execute_where_query("products", None, None, None, None).await.unwrap();

    // Project only name and price
    let projector = ResultProjector::new(vec!["name".to_string(), "price".to_string()]);
    let projected = projector.project_results(&results, true).unwrap();

    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Verify each product has only projected fields
    for item in arr {
        assert!(item.get("name").is_some());
        assert!(item.get("price").is_some());
        assert_eq!(item.get("sku"), None);
        assert_eq!(item.get("category"), None);
    }
}

// ============================================================================
// GraphQL Response Tests
// ============================================================================

#[tokio::test]
async fn test_graphql_response_data_envelope() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_graphql_response_data_envelope: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&results, false).unwrap();

    // Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "user");

    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    assert!(data.get("user").is_some());
}

#[tokio::test]
async fn test_graphql_response_with_typename() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_graphql_response_with_typename: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let _projected = projector.project_results(&results, false).unwrap();

    // Add __typename
    let with_typename = projector.add_typename_only(&results[0], "User").unwrap();

    assert_eq!(with_typename.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_graphql_response_list_with_typename() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_graphql_response_list_with_typename: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    let projector = ResultProjector::new(vec!["id".to_string()]);

    // Add __typename to a result
    let with_typename = projector.add_typename_only(&results[0], "User").unwrap();

    assert_eq!(with_typename.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_graphql_error_response() {
    // Pure error-envelope test — no database access.
    let error = FraiseQLError::Validation {
        message: "Invalid query field".to_string(),
        path:    Some("query.user.invalidField".to_string()),
    };

    let response = ResultProjector::wrap_error(&error);

    assert!(response.get("errors").is_some());
    assert_eq!(response.get("data"), None);

    let errors = response.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].get("message").is_some());
}

// ============================================================================
// Complete Pipeline Tests
// ============================================================================

#[tokio::test]
async fn test_complete_e2e_pipeline_single_user() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_complete_e2e_pipeline_single_user: no postgres");
        return;
    };
    // Step 1: Query database
    let db_results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    assert_eq!(db_results.len(), 1);

    // Step 2: Project fields
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "email".to_string()]);
    let projected = projector.project_results(&db_results, false).unwrap();

    // Verify projection worked
    assert!(projected.get("id").is_some());
    assert!(projected.get("name").is_some());
    assert!(projected.get("email").is_some());
    assert_eq!(projected.get("status"), None);

    // Step 3: Add __typename
    let with_typename = projector.add_typename_only(&db_results[0], "User").unwrap();

    // Step 4: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(with_typename, "user");

    // Verify complete response
    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    let user = data.get("user").unwrap();
    assert_eq!(user.get("__typename"), Some(&json!("User")));
}

#[tokio::test]
async fn test_complete_e2e_pipeline_user_list() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_complete_e2e_pipeline_user_list: no postgres");
        return;
    };
    // Step 1: Query database
    let db_results = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    assert_eq!(db_results.len(), 3);

    // Step 2: Project fields
    let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    let projected = projector.project_results(&db_results, true).unwrap();

    // Verify projection
    let arr = projected.as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Step 3: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "users");

    // Verify complete response
    assert!(response.get("data").is_some());
    let data = response.get("data").unwrap();
    let users = data.get("users").unwrap().as_array().unwrap();
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_complete_e2e_pipeline_products() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_complete_e2e_pipeline_products: no postgres");
        return;
    };
    // Step 1: Query database
    let db_results = adapter.execute_where_query("products", None, None, None, None).await.unwrap();

    // Step 2: Project fields
    let projector =
        ResultProjector::new(vec!["id".to_string(), "name".to_string(), "price".to_string()]);
    let projected = projector.project_results(&db_results, true).unwrap();

    let arr = projected.as_array().unwrap();
    for item in arr {
        assert!(item.get("id").is_some());
        assert!(item.get("name").is_some());
        assert!(item.get("price").is_some());
        assert_eq!(item.get("sku"), None);
    }

    // Step 3: Wrap in GraphQL envelope
    let response = ResultProjector::wrap_in_data_envelope(projected, "products");

    // Verify complete response
    assert!(response.get("data").is_some());
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_empty_projection_fields() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_empty_projection_fields: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    let projector = ResultProjector::new(vec![]);
    let _projected = projector.project_results(&results, false).unwrap();

    // With empty projection fields, should still return the data object
    assert!(_projected.is_object());
}

#[tokio::test]
async fn test_projection_nonexistent_fields() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_projection_nonexistent_fields: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(1), None, None).await.unwrap();

    let projector = ResultProjector::new(vec![
        "nonexistent_field".to_string(),
        "another_missing".to_string(),
    ]);
    let projected = projector.project_results(&results, false).unwrap();

    // Should return empty object for nonexistent fields
    assert!(projected.is_object());
    assert_eq!(projected.as_object().unwrap().len(), 0);
}

#[tokio::test]
async fn test_query_with_zero_limit() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_query_with_zero_limit: no postgres");
        return;
    };
    let results = adapter.execute_where_query("users", None, Some(0), None, None).await.unwrap();

    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn test_large_limit() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_large_limit: no postgres");
        return;
    };
    let results = adapter
        .execute_where_query("users", None, Some(1000), None, None)
        .await
        .unwrap();

    // Should return all 3 users even though we requested 1000
    assert_eq!(results.len(), 3);
}

// ============================================================================
// Data Integrity Tests
// ============================================================================

#[tokio::test]
async fn test_seed_data_not_mutated() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_seed_data_not_mutated: no postgres");
        return;
    };

    // Query multiple times
    let results1 = adapter.execute_where_query("users", None, None, None, None).await.unwrap();
    let results2 = adapter.execute_where_query("users", None, None, None, None).await.unwrap();

    assert_eq!(results1.len(), results2.len());

    // Two unordered SELECTs may differ in physical order — compare by id.
    let mut v1: Vec<serde_json::Value> = results1.iter().map(|r| r.as_value().clone()).collect();
    let mut v2: Vec<serde_json::Value> = results2.iter().map(|r| r.as_value().clone()).collect();
    v1.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    v2.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    assert_eq!(v1, v2);
}

#[tokio::test]
async fn test_different_tables_independent() {
    let Some((_pg, adapter)) = seed_users_products().await else {
        eprintln!("SKIP test_different_tables_independent: no postgres");
        return;
    };

    let users = adapter.execute_where_query("users", None, None, None, None).await.unwrap();
    let products = adapter.execute_where_query("products", None, None, None, None).await.unwrap();

    assert_eq!(users.len(), 3);
    assert_eq!(products.len(), 3);

    // Verify users have user fields
    assert!(users[0].as_value().get("email").is_some());

    // Verify products have product fields
    assert!(products[0].as_value().get("sku").is_some());
}
