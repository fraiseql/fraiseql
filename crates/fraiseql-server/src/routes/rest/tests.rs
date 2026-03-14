use std::collections::HashMap;

use fraiseql_core::schema::{ArgumentDefinition, FieldType};

use super::translator::{RestOutcome, build_graphql_request, classify_response, extract_data_field};

// ── translator tests ──────────────────────────────────────────────────────────

fn id_arg(name: &str) -> ArgumentDefinition {
    ArgumentDefinition::new(name, FieldType::Id)
}

fn string_arg(name: &str) -> ArgumentDefinition {
    ArgumentDefinition::new(name, FieldType::String)
}

fn nullable_string_arg(name: &str) -> ArgumentDefinition {
    let mut arg = ArgumentDefinition::new(name, FieldType::String);
    arg.nullable = true;
    arg
}

#[test]
fn test_build_graphql_request_no_args_no_fields() {
    let req = build_graphql_request(
        "query",
        "list_users",
        &[],
        &[],
        &HashMap::new(),
        &HashMap::new(),
        None,
    );
    assert!(req.query.contains("query"));
    assert!(req.query.contains("list_users"));
    assert!(req.query.contains("__typename"));
    assert!(req.variables.is_none());
}

#[test]
fn test_build_graphql_request_with_path_param() {
    let mut path_params = HashMap::new();
    path_params.insert("id".to_string(), "42".to_string());

    let req = build_graphql_request(
        "query",
        "get_user",
        &[id_arg("id")],
        &["id".to_string(), "name".to_string()],
        &path_params,
        &HashMap::new(),
        None,
    );

    assert!(req.query.contains("$id: ID!"));
    assert!(req.query.contains("id: $id"));
    assert!(req.query.contains("id\n    name"));
    let vars = req.variables.unwrap();
    assert_eq!(vars["id"], "42");
}

#[test]
fn test_build_graphql_request_mutation() {
    let body = serde_json::json!({"name": "Alice", "email": "alice@example.com"});
    let req = build_graphql_request(
        "mutation",
        "create_user",
        &[string_arg("name"), string_arg("email")],
        &["id".to_string(), "name".to_string()],
        &HashMap::new(),
        &HashMap::new(),
        Some(&body),
    );

    assert!(req.query.starts_with("mutation"));
    assert!(req.query.contains("create_user"));
    assert!(req.query.contains("$name: String!"));
    assert!(req.query.contains("$email: String!"));
    let vars = req.variables.unwrap();
    assert_eq!(vars["name"], "Alice");
    assert_eq!(vars["email"], "alice@example.com");
}

#[test]
fn test_build_graphql_request_nullable_arg_omitted_when_absent() {
    let req = build_graphql_request(
        "query",
        "search_users",
        &[nullable_string_arg("filter")],
        &["id".to_string()],
        &HashMap::new(),
        &HashMap::new(),
        None,
    );

    // filter is nullable and absent — should not appear as a variable
    assert!(!req.query.contains("$filter"));
    assert!(req.variables.is_none());
}

#[test]
fn test_build_graphql_request_path_overrides_query_param() {
    let mut path_params = HashMap::new();
    path_params.insert("id".to_string(), "path_value".to_string());

    let mut query_params = HashMap::new();
    query_params.insert("id".to_string(), "query_value".to_string());

    let req = build_graphql_request(
        "query",
        "get_user",
        &[id_arg("id")],
        &["id".to_string()],
        &path_params,
        &query_params,
        None,
    );

    let vars = req.variables.unwrap();
    // Path params take precedence
    assert_eq!(vars["id"], "path_value");
}

#[test]
fn test_extract_data_field_returns_data_slice() {
    let json = r#"{"data":{"get_user":{"id":"1","name":"Alice"}}}"#;
    let val = extract_data_field(json, "get_user");
    assert_eq!(val["id"], "1");
    assert_eq!(val["name"], "Alice");
}

#[test]
fn test_extract_data_field_missing_returns_full_response() {
    let json = r#"{"errors":[{"message":"Not found"}]}"#;
    let val = extract_data_field(json, "get_user");
    assert!(val.get("errors").is_some());
}

#[test]
fn test_extract_data_field_invalid_json_returns_string() {
    let val = extract_data_field("not json", "get_user");
    assert_eq!(val, serde_json::Value::String("not json".to_string()));
}

// ── classify_response edge case tests ─────────────────────────────────────────

#[test]
fn test_partial_response_returns_partial_outcome() {
    let json = r#"{
        "data": {"users": [{"id": "1", "name": "Alice"}]},
        "errors": [{"message": "Permission denied for field 'name' on User:2", "path": ["users", 1, "name"]}]
    }"#;
    let outcome = classify_response(json, "users", true);
    assert!(matches!(outcome, RestOutcome::Partial { .. }));
    if let RestOutcome::Partial { data, errors } = outcome {
        assert!(data.as_array().is_some());
        assert!(errors.as_array().is_some());
    }
}

#[test]
fn test_null_data_unauthenticated_error_returns_401() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Not authenticated", "extensions": {"code": "UNAUTHENTICATED"}}]
    }"#;
    let outcome = classify_response(json, "me", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 401, .. }));
}

#[test]
fn test_null_data_forbidden_error_returns_403() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Access denied", "extensions": {"code": "FORBIDDEN"}}]
    }"#;
    let outcome = classify_response(json, "get_secret", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 403, .. }));
}

#[test]
fn test_null_data_validation_error_returns_400() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Invalid input", "extensions": {"code": "VALIDATION_ERROR"}}]
    }"#;
    let outcome = classify_response(json, "create_user", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 400, .. }));
}

#[test]
fn test_null_data_rate_limited_returns_429() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Rate limit exceeded", "extensions": {"code": "RATE_LIMITED"}}]
    }"#;
    let outcome = classify_response(json, "search", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 429, .. }));
}

#[test]
fn test_null_data_generic_error_returns_500() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Database connection failed", "extensions": {"code": "INTERNAL_ERROR"}}]
    }"#;
    let outcome = classify_response(json, "get_user", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 500, .. }));
}

#[test]
fn test_null_data_no_extension_code_returns_500() {
    let json = r#"{
        "data": null,
        "errors": [{"message": "Something went wrong"}]
    }"#;
    let outcome = classify_response(json, "get_user", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 500, .. }));
}

#[test]
fn test_empty_list_returns_ok_empty_array() {
    let json = r#"{"data": {"users": []}, "errors": null}"#;
    let outcome = classify_response(json, "users", true);
    assert!(matches!(&outcome, RestOutcome::Ok(v) if v.as_array().is_some_and(|a| a.is_empty())));
}

#[test]
fn test_non_empty_list_returns_ok_with_data() {
    let json = r#"{"data": {"users": [{"id": "1"}]}, "errors": null}"#;
    let outcome = classify_response(json, "users", true);
    assert!(matches!(&outcome, RestOutcome::Ok(v) if v.as_array().is_some_and(|a| a.len() == 1)));
}

#[test]
fn test_single_item_null_returns_not_found() {
    let json = r#"{"data": {"get_user": null}, "errors": null}"#;
    let outcome = classify_response(json, "get_user", false);
    assert!(matches!(outcome, RestOutcome::NotFound));
}

#[test]
fn test_single_item_present_returns_ok() {
    let json = r#"{"data": {"get_user": {"id": "1", "name": "Alice"}}, "errors": null}"#;
    let outcome = classify_response(json, "get_user", false);
    assert!(matches!(&outcome, RestOutcome::Ok(v) if v["id"] == "1"));
}

#[test]
fn test_unparseable_response_returns_500() {
    let outcome = classify_response("not valid json", "get_user", false);
    assert!(matches!(outcome, RestOutcome::Failure { status: 500, .. }));
}

// ── path conversion tests ─────────────────────────────────────────────────────

#[test]
fn test_to_axum_path_no_params() {
    assert_eq!(to_axum_path_test("/users"), "/users");
}

#[test]
fn test_to_axum_path_single_param() {
    // Axum 0.7+ uses {param} syntax — schema paths are already valid axum patterns.
    assert_eq!(to_axum_path_test("/users/{id}"), "/users/{id}");
}

#[test]
fn test_to_axum_path_multiple_params() {
    assert_eq!(
        to_axum_path_test("/orgs/{org_id}/users/{user_id}"),
        "/orgs/{org_id}/users/{user_id}"
    );
}

#[test]
fn test_to_axum_path_trailing_slash() {
    assert_eq!(to_axum_path_test("/users/{id}/"), "/users/{id}/");
}

// Helper: delegates to the production function (identity for axum 0.7+ {param} syntax).
fn to_axum_path_test(s: &str) -> String {
    super::router::to_axum_path(s)
}
