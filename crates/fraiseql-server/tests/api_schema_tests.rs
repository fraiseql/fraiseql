//! Behavioral tests for schema export API endpoints.
//!
//! Exercises the real `export_sdl_handler` and `export_json_handler`
//! through axum's `tower::ServiceExt::oneshot`.
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** none
//! **Parallelism:** safe

mod common;

use common::test_app::{api_router, get_json, get_text, make_test_state};
use http::StatusCode;

// ============================================================================
// SDL EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_graphql_endpoint_returns_sdl_text() {
    let router = api_router(make_test_state());
    let (status, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert_eq!(status, StatusCode::OK);
    assert!(sdl.contains("type Query"), "SDL should contain type Query, got: {sdl}");
}

#[tokio::test]
async fn schema_graphql_sdl_contains_types_and_mutations() {
    let router = api_router(make_test_state());
    let (_, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert!(sdl.contains("type User"), "SDL should contain User type");
    assert!(sdl.contains("type Mutation"), "SDL should contain Mutation type");
}

// ============================================================================
// JSON SCHEMA EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_json_endpoint_returns_structured_schema() {
    let router = api_router(make_test_state());
    let (status, json) = get_json(&router, "/api/v1/schema.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");
    assert!(json["data"]["schema"]["types"].is_array());
    assert_eq!(json["data"]["schema"]["query_type"], "Query");
}

#[tokio::test]
async fn schema_json_types_have_fields() {
    let router = api_router(make_test_state());
    let (_, json) = get_json(&router, "/api/v1/schema.json").await;

    let types = json["data"]["schema"]["types"].as_array().unwrap();
    assert!(!types.is_empty());

    let query_type = types.iter().find(|t| t["name"] == "Query");
    assert!(query_type.is_some(), "Should have a Query type");
    assert!(query_type.unwrap()["fields"].is_array());
}
