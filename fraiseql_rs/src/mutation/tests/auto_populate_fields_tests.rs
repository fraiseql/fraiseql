//! Tests for auto-populated fields in mutation responses

use crate::mutation::{MutationResult, MutationStatus};
use crate::mutation::response_builder::build_success_response;
use serde_json::{json, Value};

#[test]
fn test_success_response_has_status_field() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("Operation completed".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "name": "Test User"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,  // auto_camel_case
        None,  // success_type_fields
        None,  // cascade_selections
    ).expect("Failed to build response");

    // Verify
    let obj = response.as_object().expect("Response should be object");

    // Check status field exists
    assert!(obj.contains_key("status"), "Response missing 'status' field");

    // Check status value
    let status = obj.get("status").expect("status field should exist");
    assert_eq!(status.as_str(), Some("success"), "status should be 'success'");
}

#[test]
fn test_success_response_has_errors_field() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("Operation completed".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "name": "Test User"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    // Verify
    let obj = response.as_object().expect("Response should be object");

    // Check errors field exists
    assert!(obj.contains_key("errors"), "Response missing 'errors' field");

    // Check errors is empty array
    let errors = obj.get("errors").expect("errors field should exist");
    let errors_array = errors.as_array().expect("errors should be array");
    assert_eq!(errors_array.len(), 0, "errors array should be empty for success");
}

#[test]
fn test_success_response_all_standard_fields() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success:created".to_string()),
        message: Some("User created successfully".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "email": "test@example.com"})),
        updated_fields: Some(vec!["email".to_string(), "name".to_string()]),
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    // Verify all standard fields present
    let obj = response.as_object().expect("Response should be object");

    assert!(obj.contains_key("__typename"), "Missing __typename");
    assert!(obj.contains_key("id"), "Missing id");
    assert!(obj.contains_key("message"), "Missing message");
    assert!(obj.contains_key("status"), "Missing status");
    assert!(obj.contains_key("errors"), "Missing errors");
    assert!(obj.contains_key("user"), "Missing user entity");
    assert!(obj.contains_key("updatedFields"), "Missing updatedFields");

    // Verify values
    assert_eq!(obj.get("__typename").unwrap().as_str(), Some("CreateUserSuccess"));
    assert_eq!(obj.get("status").unwrap().as_str(), Some("success:created"));
    assert_eq!(obj.get("message").unwrap().as_str(), Some("User created successfully"));

    let errors = obj.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 0, "Success should have empty errors array");
}

#[test]
fn test_success_status_preserves_detail() {
    // Test that status detail is preserved (e.g., "success:created")
    let result = MutationResult {
        status: MutationStatus::Success("success:updated".to_string()),
        message: Some("Updated".to_string()),
        entity_id: Some("abc-123".to_string()),
        entity_type: Some("Post".to_string()),
        entity: Some(json!({"id": "abc-123"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    let response = build_success_response(
        &result,
        "UpdatePostSuccess",
        Some("post"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    let obj = response.as_object().unwrap();
    let status = obj.get("status").unwrap().as_str().unwrap();

    assert_eq!(status, "success:updated", "Status detail should be preserved");
}

#[test]
fn test_success_fields_order() {
    // Verify fields appear in expected order for consistent API
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("OK".to_string()),
        entity_id: Some("123".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    let obj = response.as_object().unwrap();
    let keys: Vec<&String> = obj.keys().collect();

    // Check that standard fields come before entity field
    let typename_idx = keys.iter().position(|&k| k == "__typename").unwrap();
    let id_idx = keys.iter().position(|&k| k == "id").unwrap();
    let message_idx = keys.iter().position(|&k| k == "message").unwrap();
    let status_idx = keys.iter().position(|&k| k == "status").unwrap();
    let errors_idx = keys.iter().position(|&k| k == "errors").unwrap();
    let user_idx = keys.iter().position(|&k| k == "user").unwrap();

    // Verify ordering
    assert!(typename_idx < id_idx, "__typename should come before id");
    assert!(id_idx < message_idx, "id should come before message");
    assert!(message_idx < status_idx, "message should come before status");
    assert!(status_idx < errors_idx, "status should come before errors");
    assert!(errors_idx < user_idx, "errors should come before entity");
}

