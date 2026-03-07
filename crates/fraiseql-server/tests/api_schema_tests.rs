//! Behavioral tests for schema export API endpoints.
//!
//! Exercises the real `export_sdl_handler` and `export_json_handler`
//! through axum's `tower::ServiceExt::oneshot`.
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics use usize/u64→f64 for reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are small and bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are small and bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables prefixed with _ by convention
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures follow test patterns

mod common;

use common::test_app::{api_router, get_json, get_text, make_populated_test_state};
use http::StatusCode;

// ============================================================================
// SDL EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_graphql_endpoint_returns_sdl_text() {
    let router = api_router(make_populated_test_state());
    let (status, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert_eq!(status, StatusCode::OK);
    assert!(sdl.contains("type Query"), "SDL should contain type Query, got: {sdl}");
}

#[tokio::test]
async fn schema_graphql_sdl_contains_types_and_mutations() {
    let router = api_router(make_populated_test_state());
    let (_, sdl) = get_text(&router, "/api/v1/schema.graphql").await;

    assert!(sdl.contains("type User"), "SDL should contain User type");
    assert!(sdl.contains("type Mutation"), "SDL should contain Mutation type");
}

// ============================================================================
// JSON SCHEMA EXPORT ENDPOINT
// ============================================================================

#[tokio::test]
async fn schema_json_endpoint_returns_structured_schema() {
    let router = api_router(make_populated_test_state());
    let (status, json) = get_json(&router, "/api/v1/schema.json").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "success");
    assert!(json["data"]["schema"]["types"].is_array());
    // CompiledSchema exposes queries[], not a query_type field.
    assert!(json["data"]["schema"]["queries"].is_array());
}

#[tokio::test]
async fn schema_json_types_have_fields() {
    let router = api_router(make_populated_test_state());
    let (_, json) = get_json(&router, "/api/v1/schema.json").await;

    let types = json["data"]["schema"]["types"].as_array().unwrap();
    assert!(!types.is_empty());

    // types[] contains user-defined object types (e.g. User), not the synthetic Query root.
    let user_type = types.iter().find(|t| t["name"] == "User");
    assert!(user_type.is_some(), "Should have a User type");
    assert!(user_type.unwrap()["fields"].is_array());
}
