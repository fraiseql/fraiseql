#![allow(clippy::unwrap_used)]

use serde_json::json;

use super::*;

#[test]
fn test_no_graphql_errors_success() {
    let response = json!({"data": {"user": {"id": 1}}});
    assert_no_graphql_errors(&response);
}

#[test]
fn test_no_graphql_errors_empty_errors_array() {
    let response = json!({"data": {}, "errors": []});
    assert_no_graphql_errors(&response);
}

#[test]
#[should_panic(expected = "Expected no GraphQL errors")]
fn test_no_graphql_errors_fails() {
    let response = json!({"errors": [{"message": "error"}]});
    assert_no_graphql_errors(&response);
}

#[test]
fn test_has_data_success() {
    let response = json!({"data": {"user": {"id": 1}}});
    let data = assert_has_data(&response);
    assert_eq!(data["user"]["id"], 1);
}

#[test]
#[should_panic(expected = "should have 'data' field")]
fn test_has_data_fails() {
    let response = json!({"errors": [{"message": "error"}]});
    assert_has_data(&response);
}

#[test]
fn test_graphql_success_passes_on_no_errors() {
    let response = json!({"data": {"users": [{"id": 1}]}});
    assert_graphql_success(&response);
}

#[test]
#[should_panic(expected = "Expected no GraphQL errors")]
fn test_graphql_success_fails_on_errors() {
    let response = json!({"errors": [{"message": "field not found"}]});
    assert_graphql_success(&response);
}

#[test]
fn test_graphql_error_contains_match() {
    let response = json!({"errors": [{"message": "Field 'id' not found"}]});
    assert_graphql_error_contains(&response, "not found");
}

#[test]
fn test_graphql_error_contains_partial_match() {
    let response = json!({"errors": [
        {"message": "Rate limit exceeded"},
        {"message": "Retry after 60s"}
    ]});
    assert_graphql_error_contains(&response, "Rate limit");
}

#[test]
#[should_panic(expected = "Expected an error containing")]
fn test_graphql_error_contains_no_match() {
    let response = json!({"errors": [{"message": "Database error"}]});
    assert_graphql_error_contains(&response, "not found");
}

#[test]
#[should_panic(expected = "errors' array")]
fn test_graphql_error_contains_no_errors_key() {
    let response = json!({"data": {}});
    assert_graphql_error_contains(&response, "error");
}

#[test]
#[should_panic(expected = "errors array is empty")]
fn test_graphql_error_contains_empty_errors() {
    let response = json!({"errors": []});
    assert_graphql_error_contains(&response, "error");
}

#[test]
fn test_graphql_error_code_match() {
    let response =
        json!({"errors": [{"message": "Not allowed", "extensions": {"code": "FORBIDDEN"}}]});
    assert_graphql_error_code(&response, "FORBIDDEN");
}

#[test]
#[should_panic(expected = "Expected an error with extension code")]
fn test_graphql_error_code_no_match() {
    let response =
        json!({"errors": [{"message": "Not allowed", "extensions": {"code": "FORBIDDEN"}}]});
    assert_graphql_error_code(&response, "UNAUTHENTICATED");
}

#[test]
#[should_panic(expected = "errors' array")]
fn test_graphql_error_code_no_errors() {
    let response = json!({"data": {}});
    assert_graphql_error_code(&response, "FORBIDDEN");
}

#[test]
fn test_assert_field_path_simple() {
    let response = json!({"data": {"user": {"id": 42}}});
    assert_field_path(&response, "data.user.id", &json!(42));
}

#[test]
#[should_panic(expected = "segment 'missing' not found in path")]
fn test_assert_field_path_missing_segment() {
    let response = json!({"data": {}});
    assert_field_path(&response, "data.missing.field", &json!("x"));
}

#[test]
#[should_panic(expected = "at path 'data.user.name'")]
fn test_assert_field_path_value_mismatch() {
    let response = json!({"data": {"user": {"name": "alice"}}});
    assert_field_path(&response, "data.user.name", &json!("bob"));
}
