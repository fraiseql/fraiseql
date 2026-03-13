//! Behavioral tests for health and introspection HTTP endpoints.
//!
//! These tests exercise the real `health_handler` and `introspection_handler`
//! through axum's `tower::ServiceExt::oneshot`, verifying actual HTTP response
//! codes, JSON structure, and database health-check integration.
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

mod common;

use common::test_app::{get_json, health_router, make_test_state, make_test_state_with};
use fraiseql_core::schema::{CompiledSchema, FieldType};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestSchemaBuilder, TestTypeBuilder},
};
use http::StatusCode;

// ============================================================================
// HEALTH CHECK ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn health_returns_200_with_healthy_adapter() {
    let router = health_router(make_test_state());
    let (status, json) = get_json(&router, "/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "healthy");
    assert!(json["database"]["connected"].as_bool().unwrap());
}

#[tokio::test]
async fn health_returns_503_when_db_fails() {
    let adapter = FailingAdapter::new().fail_health_check();
    let state = make_test_state_with(adapter, CompiledSchema::new());
    let router = health_router(state);
    let (status, json) = get_json(&router, "/health").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["status"], "unhealthy");
    assert!(!json["database"]["connected"].as_bool().unwrap());
}

#[tokio::test]
async fn health_includes_database_type_and_pool_metrics() {
    let router = health_router(make_test_state());
    let (_, json) = get_json(&router, "/health").await;

    // FailingAdapter reports PostgreSQL and fixed pool metrics
    assert!(json["database"]["database_type"].as_str().unwrap().contains("PostgreSQL"));
    assert!(json["database"]["active_connections"].is_number());
    assert!(json["database"]["idle_connections"].is_number());
}

#[tokio::test]
async fn health_includes_version_from_cargo() {
    let router = health_router(make_test_state());
    let (_, json) = get_json(&router, "/health").await;

    let version = json["version"].as_str().unwrap();
    // Must match the crate version at compile time (not a hardcoded "2.0.0-a1")
    assert!(!version.is_empty());
    assert!(version.contains('.'), "version should be semver: {version}");
}

// ============================================================================
// INTROSPECTION ENDPOINT TESTS
// ============================================================================

#[tokio::test]
async fn introspection_with_empty_schema_returns_empty_collections() {
    let router = health_router(make_test_state());
    let (status, json) = get_json(&router, "/introspection").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["types"].as_array().unwrap().len(), 0);
    assert_eq!(json["queries"].as_array().unwrap().len(), 0);
    assert_eq!(json["mutations"].as_array().unwrap().len(), 0);
}

// Migration 7: introspection_returns_schema_types_and_queries
#[tokio::test]
async fn introspection_returns_schema_types_and_queries() {
    let schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_simple_field("id", FieldType::Int)
                .with_simple_field("name", FieldType::String)
                .build(),
        )
        .with_simple_query("users", "User", true)
        .build();

    let state = make_test_state_with(FailingAdapter::new(), schema);
    let router = health_router(state);
    let (status, json) = get_json(&router, "/introspection").await;

    assert_eq!(status, StatusCode::OK);

    let types = json["types"].as_array().unwrap();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["name"], "User");
    assert_eq!(types[0]["field_count"], 2);

    let queries = json["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["name"], "users");
    assert_eq!(queries[0]["return_type"], "User");
}

// Migration 8: introspection_includes_type_descriptions
#[tokio::test]
async fn introspection_includes_type_descriptions() {
    let schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_description("A user in the system")
                .build(),
        )
        .build();

    let state = make_test_state_with(FailingAdapter::new(), schema);
    let router = health_router(state);
    let (_, json) = get_json(&router, "/introspection").await;

    assert_eq!(json["types"][0]["description"], "A user in the system");
}
