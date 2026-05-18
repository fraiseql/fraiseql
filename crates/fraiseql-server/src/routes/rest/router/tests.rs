//! Tests for the `router` module.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use axum::http::StatusCode;
use fraiseql_core::schema::{FieldType, MutationDefinition, MutationOperation, RestConfig};
use fraiseql_test_utils::schema_builder::{TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder};

use super::{
    helpers::{error_response, parse_query_pairs, strip_base_path, to_axum_path},
    *,
};

// ---------------------------------------------------------------------------
// helpers tests
// ---------------------------------------------------------------------------

#[test]
fn to_axum_path_simple() {
    assert_eq!(to_axum_path("/rest/v1", "/users"), "/rest/v1/users");
}

#[test]
fn to_axum_path_with_param() {
    assert_eq!(to_axum_path("/rest/v1", "/users/{id}"), "/rest/v1/users/{id}");
}

#[test]
fn to_axum_path_multiple_params() {
    assert_eq!(
        to_axum_path("/rest/v1", "/users/{uid}/posts/{pid}"),
        "/rest/v1/users/{uid}/posts/{pid}"
    );
}

#[test]
fn strip_base_path_exact() {
    assert_eq!(strip_base_path("/rest/v1", "/rest/v1"), "/");
}

#[test]
fn strip_base_path_with_route() {
    assert_eq!(strip_base_path("/rest/v1", "/rest/v1/users"), "/users");
}

#[test]
fn strip_base_path_no_match() {
    assert_eq!(strip_base_path("/rest/v1", "/api/users"), "/api/users");
}

#[test]
fn parse_query_pairs_single() {
    let pairs = parse_query_pairs("key=value");
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0], ("key".to_string(), "value".to_string()));
}

#[test]
fn parse_query_pairs_multiple() {
    let pairs = parse_query_pairs("key1=value1&key2=value2");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0], ("key1".to_string(), "value1".to_string()));
    assert_eq!(pairs[1], ("key2".to_string(), "value2".to_string()));
}

#[test]
fn parse_query_pairs_url_encoded() {
    let pairs = parse_query_pairs("name=John%20Doe");
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0], ("name".to_string(), "John Doe".to_string()));
}

#[test]
fn parse_query_pairs_no_value() {
    let pairs = parse_query_pairs("flag");
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0], ("flag".to_string(), String::new()));
}

#[test]
fn error_response_structure() {
    let resp = error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", "Invalid input");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(resp.headers().get("content-type").unwrap(), "application/json");
}

// ---------------------------------------------------------------------------
// mod tests (router construction)
// ---------------------------------------------------------------------------

fn mutation(name: &str, op: MutationOperation) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "User");
    m.operation = op;
    m.sql_source = Some(format!("fn_{name}"));
    m
}

/// Build a minimal schema with REST enabled and one resource (`users`).
fn schema_with_rest() -> fraiseql_core::schema::CompiledSchema {
    let table = "users".to_string();
    let mut schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_simple_query("user", "User", false)
        .with_mutation(mutation(
            "create_user",
            MutationOperation::Insert {
                table: table.clone(),
            },
        ))
        .with_mutation(mutation(
            "update_user",
            MutationOperation::Update {
                table: table.clone(),
            },
        ))
        .with_mutation(mutation("delete_user", MutationOperation::Delete { table }))
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
                .build(),
        )
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });

    schema
}

/// Build a schema with REST disabled.
fn schema_with_rest_disabled() -> fraiseql_core::schema::CompiledSchema {
    let mut schema = schema_with_rest();
    schema.rest_config = Some(RestConfig {
        enabled: false,
        ..RestConfig::default()
    });
    schema
}

/// Build a schema with no REST config at all.
fn schema_without_rest() -> fraiseql_core::schema::CompiledSchema {
    TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                .build(),
        )
        .build()
}

fn make_app_state(
    schema: fraiseql_core::schema::CompiledSchema,
) -> AppState<fraiseql_test_utils::failing_adapter::FailingAdapter> {
    let adapter = Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::default());
    let executor = Arc::new(fraiseql_core::runtime::Executor::new(schema, adapter));
    AppState::new(executor)
}

// -----------------------------------------------------------------------
// rest_query_router function tests
// -----------------------------------------------------------------------

#[test]
fn rest_query_router_returns_none_when_no_config() {
    let state = make_app_state(schema_without_rest());
    assert!(rest_query_router(&state, false).is_none());
}

#[test]
fn rest_query_router_returns_none_when_disabled() {
    let state = make_app_state(schema_with_rest_disabled());
    assert!(rest_query_router(&state, false).is_none());
}

#[test]
fn rest_query_router_returns_some_when_enabled() {
    let state = make_app_state(schema_with_rest());
    assert!(rest_query_router(&state, false).is_some());
}

// -----------------------------------------------------------------------
// rest_router function tests
// -----------------------------------------------------------------------

#[test]
fn rest_router_returns_none_when_no_config() {
    let state = make_app_state(schema_without_rest());
    assert!(rest_router(&state, false).is_none());
}

#[test]
fn rest_router_returns_none_when_disabled() {
    let state = make_app_state(schema_with_rest_disabled());
    assert!(rest_router(&state, false).is_none());
}

#[test]
fn rest_router_returns_some_when_enabled() {
    let state = make_app_state(schema_with_rest());
    assert!(rest_router(&state, false).is_some());
}

#[test]
fn rest_router_custom_base_path() {
    let mut schema = schema_with_rest();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        path: "/api/rest".to_string(),
        ..RestConfig::default()
    });
    let state = make_app_state(schema);
    // Should succeed — custom path doesn't prevent creation.
    assert!(rest_router(&state, false).is_some());
}

// -----------------------------------------------------------------------
// Path conversion tests
// -----------------------------------------------------------------------

#[test]
fn to_axum_path_collection() {
    let result = to_axum_path("/rest/v1", "/users");
    assert_eq!(result, "/rest/v1/users");
}

#[test]
fn to_axum_path_single_resource() {
    let result = to_axum_path("/rest/v1", "/users/{id}");
    assert_eq!(result, "/rest/v1/users/{id}");
}

#[test]
fn to_axum_path_action() {
    let result = to_axum_path("/rest/v1", "/users/{id}/archive");
    assert_eq!(result, "/rest/v1/users/{id}/archive");
}

#[test]
fn to_axum_path_trailing_slash_base() {
    let result = to_axum_path("/rest/v1/", "/users");
    assert_eq!(result, "/rest/v1/users");
}

#[test]
fn strip_base_path_normal() {
    let result = strip_base_path("/rest/v1", "/rest/v1/users");
    assert_eq!(result, "/users");
}

#[test]
fn strip_base_path_with_id() {
    let result = strip_base_path("/rest/v1", "/rest/v1/users/123");
    assert_eq!(result, "/users/123");
}

#[test]
fn strip_base_path_root() {
    let result = strip_base_path("/rest/v1", "/rest/v1");
    assert_eq!(result, "/");
}

#[test]
fn parse_query_pairs_empty() {
    let result = parse_query_pairs("");
    assert!(result.is_empty());
}

#[test]
fn parse_query_pairs_simple() {
    let result = parse_query_pairs("limit=10&offset=0");
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("limit".to_string(), "10".to_string()));
    assert_eq!(result[1], ("offset".to_string(), "0".to_string()));
}

#[test]
fn parse_query_pairs_encoded() {
    let result = parse_query_pairs("name%5Bicontains%5D=alice");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], ("name[icontains]".to_string(), "alice".to_string()));
}
