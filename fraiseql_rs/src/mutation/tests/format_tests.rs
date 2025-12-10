//! Format parsing and response building tests
//!
//! Tests for:
//! - Simple format (entity JSONB only, no status)
//! - Full format (mutation_response with status field)
//! - Response building for both formats
//! - CASCADE integration
//! - Format detection

use super::*;

// ============================================================================
// Tests for SIMPLE format (just entity JSONB, no status field)
// ============================================================================

#[test]
fn test_parse_simple_format() {
    // Simple format: just entity data, no status/message wrapper
    let json = r#"{"id": "123", "first_name": "John", "email": "john@example.com"}"#;

    let result = MutationResult::from_json(json, Some("User")).unwrap();

    // Should be detected as simple format and treated as success
    assert!(result.status.is_success());
    assert!(result.is_simple_format);
    assert!(result.entity.is_some());

    // Entity should be the whole JSON
    let entity = result.entity.as_ref().unwrap();
    assert_eq!(entity["id"], "123");
    assert_eq!(entity["first_name"], "John");
}

#[test]
fn test_parse_simple_format_array() {
    // Simple format can also be an array of entities
    let json = r#"[{"id": "1", "name": "A"}, {"id": "2", "name": "B"}]"#;

    let result = MutationResult::from_json(json, Some("User")).unwrap();

    assert!(result.is_simple_format);
    assert!(result.entity.is_some());
}

// ============================================================================
// Tests for FULL format (mutation_response with status field)
// ============================================================================

#[test]
fn test_parse_full_success_result() {
    let json = r#"{
        "status": "new",
        "message": "User created",
        "entity_id": "550e8400-e29b-41d4-a716-446655440000",
        "entity_type": "User",
        "entity": {"id": "123", "first_name": "John"},
        "updated_fields": null,
        "cascade": null,
        "metadata": null
    }"#;

    let result = MutationResult::from_json(json, Some("User")).unwrap();

    assert!(!result.is_simple_format); // Not simple - has status
    assert!(result.status.is_success());
    assert_eq!(result.message, "User created");
    assert_eq!(result.entity_type, Some("User".to_string()));
    assert!(result.entity.is_some());
}

#[test]
fn test_parse_full_error_result() {
    let json = r#"{
        "status": "failed:validation",
        "message": "Email already exists",
        "entity_id": null,
        "entity_type": null,
        "entity": null,
        "updated_fields": null,
        "cascade": null,
        "metadata": {"errors": [{"field": "email", "code": "duplicate"}]}
    }"#;

    let result = MutationResult::from_json(json, None).unwrap();

    assert!(!result.is_simple_format);
    assert!(result.status.is_error());
    assert_eq!(result.message, "Email already exists");
    assert!(result.errors().is_some());
}

#[test]
fn test_parse_full_with_updated_fields() {
    let json = r#"{
        "status": "updated",
        "message": "User updated",
        "entity_id": "123",
        "entity_type": "User",
        "entity": {"id": "123"},
        "updated_fields": ["name", "email"],
        "cascade": null,
        "metadata": null
    }"#;

    let result = MutationResult::from_json(json, None).unwrap();

    assert!(result.status.is_success());
    assert_eq!(result.updated_fields, Some(vec!["name".to_string(), "email".to_string()]));
}

// ============================================================================
// Format detection tests
// ============================================================================

#[test]
fn test_format_detection_simple_vs_full() {
    // Full format: has "status" field
    let full = r#"{"status": "new", "entity": {"id": "1"}}"#;
    let result = MutationResult::from_json(full, None).unwrap();
    assert!(!result.is_simple_format);

    // Simple format: no "status" field, just entity data
    let simple = r#"{"id": "1", "name": "test"}"#;
    let result = MutationResult::from_json(simple, None).unwrap();
    assert!(result.is_simple_format);
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_parse_missing_status_fails() {
    // Empty JSON should fail
    let json = r#"{}"#;
    let result = MutationResult::from_json(json, None);
    assert!(result.is_err());
}

#[test]
fn test_parse_invalid_json_fails() {
    let json = r#"not valid json"#;
    let result = MutationResult::from_json(json, None);
    assert!(result.is_err());
}

// ============================================================================
// Response building tests - Simple format
// ============================================================================

#[test]
fn test_build_simple_format_response() {
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "User created".to_string(),
        entity_id: None,
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "first_name": "John"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: true,
    };

    let response = build_graphql_response(
        &result,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"),
        Some("User"),
        true,
        None,
        None,
    ).unwrap();

    let data = &response["data"]["createUser"];
    assert_eq!(data["__typename"], "CreateUserSuccess");
    assert_eq!(data["user"]["id"], "123");
    assert_eq!(data["user"]["firstName"], "John");
}

#[test]
fn test_build_simple_format_with_status_data_field() {
    // When simple format has "status" in entity, it should be renamed to "statusData"
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Task created".to_string(),
        entity_id: None,
        entity_type: Some("Task".to_string()),
        entity: Some(json!({"id": "1", "status": "pending"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: true,
    };

    let response = build_graphql_response(
        &result,
        "createTask",
        "CreateTaskSuccess",
        "CreateTaskError",
        Some("task"),
        Some("Task"),
        true,
        None,
        None,
    ).unwrap();

    let data = &response["data"]["createTask"];
    assert_eq!(data["task"]["statusData"], "pending");
}

// ============================================================================
// Response building tests - Full format
// ============================================================================

#[test]
fn test_build_full_success_response() {
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "User created".to_string(),
        entity_id: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "first_name": "John"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: false,
    };

    let response = build_graphql_response(
        &result,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"),
        Some("User"),
        true,
        None,
        None,
    ).unwrap();

    let data = &response["data"]["createUser"];
    assert_eq!(data["__typename"], "CreateUserSuccess");
    assert_eq!(data["code"], 201); // New
    assert_eq!(data["message"], "User created");
    assert_eq!(data["user"]["id"], "123");
}

#[test]
fn test_build_full_error_response() {
    let result = MutationResult {
        status: MutationStatus::Error("failed:validation".to_string()),
        message: "Email already exists".to_string(),
        entity_id: None,
        entity_type: None,
        entity: None,
        updated_fields: None,
        cascade: None,
        metadata: Some(json!({"errors": [{"field": "email"}]})),
        is_simple_format: false,
    };

    let response = build_graphql_response(
        &result,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        None,
        None,
        true,
        None,
        None,
    ).unwrap();

    let data = &response["data"]["createUser"];
    assert_eq!(data["__typename"], "CreateUserError");
    assert_eq!(data["code"], 400); // Failed
    assert_eq!(data["message"], "Email already exists");
}

// ============================================================================
// Array response tests
// ============================================================================

#[test]
fn test_build_simple_format_array_response() {
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Users created".to_string(),
        entity_id: None,
        entity_type: Some("User".to_string()),
        entity: Some(json!([
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ])),
        updated_fields: None,
        cascade: None,
        metadata: None,
        is_simple_format: true,
    };

    let response = build_graphql_response(
        &result,
        "createUsers",
        "CreateUsersSuccess",
        "CreateUsersError",
        Some("users"),
        Some("User"),
        true,
        None,
        None,
    ).unwrap();

    let data = &response["data"]["createUsers"];
    assert_eq!(data["users"][0]["id"], "1");
    assert_eq!(data["users"][1]["id"], "2");
}

// ============================================================================
// CASCADE integration tests
// ============================================================================

#[test]
fn test_parse_simple_format_with_cascade() {
    // Simple format with CASCADE selections - CASCADE data goes in top-level
    let json = r#"{
        "id": "123",
        "first_name": "John",
        "posts": [{"id": "1", "title": "Hello"}]
    }"#;

    let result = MutationResult::from_json(json, Some("User")).unwrap();

    assert!(result.is_simple_format);
    assert!(result.entity.is_some());
    // CASCADE relationships should be in entity
    let entity = result.entity.as_ref().unwrap();
    assert!(entity.get("posts").is_some());
}

#[test]
fn test_build_simple_format_response_with_cascade() {
    let result = MutationResult {
        status: MutationStatus::Success("updated".to_string()),
        message: "Post updated".to_string(),
        entity_id: None,
        entity_type: Some("Post".to_string()),
        entity: Some(json!({
            "id": "post-123",
            "title": "Updated",
            "comments": [{"id": "1", "text": "Nice"}]
        })),
        updated_fields: Some(vec!["title".to_string()]),
        cascade: Some(json!({
            "updated": [
                {"entity_id": "user-1", "entity_type": "User", "fields": ["post_count"]}
            ],
            "deleted": [],
            "invalidations": ["User:post-123"],
            "metadata": {"operation": "create"}
        })),
        metadata: None,
        is_simple_format: true,
    };

    let response = build_graphql_response(
        &result,
        "updatePost",
        "UpdatePostSuccess",
        "UpdatePostError",
        Some("post"),
        Some("Post"),
        true,
        None,  // success_type_fields
        Some(r#"{"cascade": true}"#),  // cascade_selections
    ).unwrap();

    let data = &response["data"]["updatePost"];
    assert_eq!(data["post"]["id"], "post-123");
    assert_eq!(data["post"]["title"], "Updated");

    // Verify CASCADE structure
    let cascade = &data["cascade"];
    assert!(cascade.is_object());

    let updated = cascade["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0]["entityId"], "user-1");
    assert_eq!(updated[0]["entityType"], "User");
    assert_eq!(updated[0]["fields"][0], "postCount");
    assert_eq!(updated[0]["post_count"], 5);

    let deleted = cascade["deleted"].as_array().unwrap();
    assert_eq!(deleted.len(), 0);

    let invalidations = cascade["invalidations"].as_array().unwrap();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0], "User:post-123");

    let metadata = &cascade["metadata"];
    assert_eq!(metadata["operation"], "create");
}
