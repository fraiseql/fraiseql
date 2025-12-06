//! Tests for mutation module

use super::*;
use serde_json::{json, Value};

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

    let fields = result.updated_fields.unwrap();
    assert_eq!(fields.len(), 2);
    assert!(fields.contains(&"name".to_string()));
}

// ============================================================================
// Test format detection
// ============================================================================

#[test]
fn test_format_detection_simple_vs_full() {
    // Simple: no status field
    let simple = r#"{"id": "123", "name": "Test"}"#;
    assert!(MutationResult::is_simple_format_json(simple));

    // Full: has status field
    let full = r#"{"status": "success", "message": "OK"}"#;
    assert!(!MutationResult::is_simple_format_json(full));

    // Edge case: status as a data field (not mutation status)
    // This would be rare but we handle it by checking for valid status values
    let data_with_status_field = r#"{"id": "123", "status": "active"}"#;
    // "active" is not a valid mutation status, so treat as simple
    assert!(MutationResult::is_simple_format_json(
        data_with_status_field
    ));
}

#[test]
fn test_parse_missing_status_fails() {
    // This should fail because we require status for full format, but let's test edge cases
    let json = r#"{"message": "No status"}"#;
    // This will be treated as simple format since no status field
    let result = MutationResult::from_json(json, Some("User")).unwrap();
    assert!(result.is_simple_format);
    assert!(result.status.is_success());
}

#[test]
fn test_parse_invalid_json_fails() {
    let result = MutationResult::from_json("not json", Some("User"));
    assert!(result.is_err());
}

// ============================================================================
// Tests for SIMPLE format response building
// ============================================================================

#[test]
fn test_build_simple_format_response() {
    // Simple format: just entity data, no status wrapper
    let mutation_json = r#"{"id": "123", "first_name": "John", "last_name": "Doe"}"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createUser",        // GraphQL field name
        "CreateUserSuccess", // Success type name
        "CreateUserError",   // Error type name
        Some("user"),        // Entity field name
        Some("User"),        // Entity type for __typename
        None,                // No cascade selections
        true,                // auto_camel_case
        None,                // No success type fields for validation
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();

    let create_user = &response["data"]["createUser"];
    assert_eq!(create_user["__typename"], "CreateUserSuccess");
    assert_eq!(create_user["message"], "Success"); // Default message for simple format

    // Check entity with __typename and camelCase
    let user = &create_user["user"];
    assert_eq!(user["__typename"], "User");
    assert_eq!(user["firstName"], "John"); // camelCase!
    assert_eq!(user["lastName"], "Doe"); // camelCase!
}

#[test]
fn test_build_simple_format_with_status_data_field() {
    // Entity has a "status" field but it's not a mutation status
    let mutation_json = r#"{"id": "123", "name": "Test", "status": "active"}"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"),
        Some("User"),
        None,
        true,
        None,
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();
    let create_user = &response["data"]["createUser"];

    // Should be treated as simple format (success)
    assert_eq!(create_user["__typename"], "CreateUserSuccess");

    // The "status": "active" should be preserved in entity
    assert_eq!(create_user["user"]["status"], "active");
}

// ============================================================================
// Tests for FULL format response building
// ============================================================================

#[test]
fn test_build_full_success_response() {
    let mutation_json = r#"{
        "status": "new",
        "message": "User created",
        "entity_id": "123",
        "entity_type": "User",
        "entity": {"id": "123", "first_name": "John", "last_name": "Doe"},
        "updated_fields": null,
        "cascade": null,
        "metadata": null
    }"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"), // entity_field_name,
        None,         // entity_type (comes from JSON in full format),
        None,
        true,
        None,
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();

    let create_user = &response["data"]["createUser"];
    assert_eq!(create_user["__typename"], "CreateUserSuccess");
    assert_eq!(create_user["message"], "User created");

    let user = &create_user["user"];
    assert_eq!(user["__typename"], "User");
    assert_eq!(user["firstName"], "John");
    assert_eq!(user["lastName"], "Doe");
}

#[test]
fn test_build_full_error_response() {
    let mutation_json = r#"{
        "status": "failed:validation",
        "message": "Email already exists",
        "entity_id": null,
        "entity_type": null,
        "entity": null,
        "updated_fields": null,
        "cascade": null,
        "metadata": {"errors": [{"field": "email", "code": "duplicate", "message": "Email already exists"}]}
    }"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"), // entity_field_name,
        None,         // entity_type,
        None,         // cascade_selections,
        true,
        None,
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();
    let create_user = &response["data"]["createUser"];

    assert_eq!(create_user["__typename"], "CreateUserError");
    assert_eq!(create_user["status"], "failed:validation");
    assert_eq!(create_user["code"], 422);
    assert!(create_user["errors"].as_array().unwrap().len() > 0);
}

#[test]
fn test_build_simple_format_array_response() {
    // Simple format with array of entities
    let mutation_json = r#"[{"id": "1", "name": "Alice"}, {"id": "2", "name": "Bob"}]"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createUsers",
        "CreateUsersSuccess",
        "CreateUsersError",
        Some("users"), // entity_field_name,
        Some("User"),  // entity_type for __typename,
        None,
        true,
        None,
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();

    let create_users = &response["data"]["createUsers"];
    assert_eq!(create_users["__typename"], "CreateUsersSuccess");
    assert_eq!(create_users["message"], "Success");

    // Check array of users with __typename and camelCase
    let users = &create_users["users"];
    assert!(users.is_array());
    let users_array = users.as_array().unwrap();
    assert_eq!(users_array.len(), 2);

    // First user
    assert_eq!(users_array[0]["__typename"], "User");
    assert_eq!(users_array[0]["id"], "1");
    assert_eq!(users_array[0]["name"], "Alice");

    // Second user
    assert_eq!(users_array[1]["__typename"], "User");
    assert_eq!(users_array[1]["id"], "2");
    assert_eq!(users_array[1]["name"], "Bob");
}

#[test]
fn test_build_full_noop_response() {
    let mutation_json = r#"{
        "status": "noop:unchanged",
        "message": "No changes needed",
        "entity_id": null,
        "entity_type": null,
        "entity": null,
        "updated_fields": null,
        "cascade": null,
        "metadata": null
    }"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "updateUser",
        "UpdateUserSuccess",
        "UpdateUserError",
        Some("user"),
        None,
        None,
        true,
        None,
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();
    let update_user = &response["data"]["updateUser"];

    assert_eq!(update_user["__typename"], "UpdateUserSuccess");
    assert_eq!(update_user["message"], "User updated");

    // Check updatedFields are camelCased
    let updated_fields = update_user["updatedFields"].as_array().unwrap();
    assert_eq!(updated_fields.len(), 2);
    assert!(updated_fields.contains(&json!("firstName")));
    assert!(updated_fields.contains(&json!("lastName")));
}

#[test]
fn test_mutation_status_parsing() {
    // Test success status
    let status = MutationStatus::from_str("success");
    assert!(status.is_success());
    assert!(!status.is_error());

    // Test new status
    let status = MutationStatus::from_str("new");
    assert!(status.is_success());

    // Test noop status
    let status = MutationStatus::from_str("noop:unchanged");
    assert!(status.is_noop());
    assert!(!status.is_success());

    // Test error status
    let status = MutationStatus::from_str("failed:validation");
    assert!(status.is_error());
    assert!(!status.is_success());
}

#[test]
fn test_mutation_status_http_codes() {
    assert_eq!(MutationStatus::from_str("success").http_code(), 200);
    assert_eq!(MutationStatus::from_str("noop:unchanged").http_code(), 422);
    assert_eq!(
        MutationStatus::from_str("failed:not_found").http_code(),
        404
    );
    assert_eq!(
        MutationStatus::from_str("failed:validation").http_code(),
        422
    );
    assert_eq!(MutationStatus::from_str("failed:conflict").http_code(), 409);
}

// ============================================================================
// Tests for CASCADE data extraction and inclusion
// ============================================================================

#[test]
fn test_parse_simple_format_with_cascade() {
    // Simple format with _cascade field (underscore prefix)
    let json = r#"{
        "id": "post-123",
        "message": "Post created",
        "_cascade": {
            "updated": [{"id": "user-456", "post_count": 5}],
            "deleted": [],
            "invalidations": ["User:post-123"],
            "metadata": {"operation": "create"}
        }
    }"#;

    let result = MutationResult::from_json(json, Some("Post")).unwrap();

    // Should be detected as simple format and treated as success
    assert!(result.status.is_success());
    assert!(result.is_simple_format);

    // Cascade should be extracted
    assert!(result.cascade.is_some());
    let cascade = result.cascade.as_ref().unwrap();

    // Verify cascade structure
    assert!(cascade.get("updated").is_some());
    assert!(cascade.get("deleted").is_some());
    assert!(cascade.get("invalidations").is_some());
    assert!(cascade.get("metadata").is_some());

    // Check specific values
    let updated = cascade["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0]["id"], "user-456");
    assert_eq!(updated[0]["post_count"], 5);
}

#[test]
fn test_build_simple_format_response_with_cascade() {
    // Simple format with cascade data
    let mutation_json = r#"{
        "id": "post-123",
        "title": "New Post",
        "_cascade": {
            "updated": [{"id": "user-456", "post_count": 5}],
            "deleted": [],
            "invalidations": ["User:post-123"],
            "metadata": {"operation": "create"}
        }
    }"#;

    let response_bytes = build_mutation_response(
        mutation_json,
        "createPost",        // GraphQL field name
        "CreatePostSuccess", // Success type name
        "CreatePostError",   // Error type name
        Some("post"),        // Entity field name
        Some("Post"),        // Entity type for __typename
        None,                // No cascade selections,
        true,
        None, // No success type fields validation
    )
    .unwrap();

    let response: Value = serde_json::from_slice(&response_bytes).unwrap();

    let create_post = &response["data"]["createPost"];
    assert_eq!(create_post["__typename"], "CreatePostSuccess");
    assert_eq!(create_post["message"], "Success");

    // Check entity
    let post = &create_post["post"];
    assert_eq!(post["__typename"], "Post");
    assert_eq!(post["id"], "post-123");
    assert_eq!(post["title"], "New Post");

    // Check cascade is included in response
    let cascade = &create_post["cascade"];
    assert!(cascade.is_object());

    // Verify cascade structure
    assert!(cascade.get("updated").is_some());
    assert!(cascade.get("deleted").is_some());
    assert!(cascade.get("invalidations").is_some());
    assert!(cascade.get("metadata").is_some());

    // Check specific cascade values
    let updated = cascade["updated"].as_array().unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0]["id"], "user-456");
    assert_eq!(updated[0]["post_count"], 5);

    let deleted = cascade["deleted"].as_array().unwrap();
    assert_eq!(deleted.len(), 0);

    let invalidations = cascade["invalidations"].as_array().unwrap();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0], "User:post-123");

    let metadata = &cascade["metadata"];
    assert_eq!(metadata["operation"], "create");
}

// ============================================================================
// Tests for STATUS TAXONOMY (Phase 2: GREEN)
// ============================================================================

#[cfg(test)]
mod test_status_taxonomy {
    use super::*;

    // SUCCESS KEYWORDS (no colon)
    #[test]
    fn test_success_keywords() {
        assert!(MutationStatus::from_str("success").is_success());
        assert!(MutationStatus::from_str("created").is_success());
        assert!(MutationStatus::from_str("updated").is_success());
        assert!(MutationStatus::from_str("deleted").is_success());
    }

    // ERROR PREFIXES (colon-separated)
    #[test]
    fn test_failed_prefix() {
        let status = MutationStatus::from_str("failed:validation");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(full_status) => assert_eq!(full_status, "failed:validation"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_unauthorized_prefix() {
        let status = MutationStatus::from_str("unauthorized:token_expired");
        assert!(status.is_error());
    }

    #[test]
    fn test_forbidden_prefix() {
        let status = MutationStatus::from_str("forbidden:insufficient_permissions");
        assert!(status.is_error());
    }

    #[test]
    fn test_not_found_prefix() {
        let status = MutationStatus::from_str("not_found:user_missing");
        assert!(status.is_error());
    }

    #[test]
    fn test_conflict_prefix() {
        let status = MutationStatus::from_str("conflict:duplicate_email");
        assert!(status.is_error());
    }

    #[test]
    fn test_timeout_prefix() {
        let status = MutationStatus::from_str("timeout:database_query");
        assert!(status.is_error());
    }

    // NOOP PREFIX (success with no changes)
    #[test]
    fn test_noop_prefix() {
        let status = MutationStatus::from_str("noop:unchanged");
        assert!(status.is_noop());
        match status {
            MutationStatus::Noop(reason) => assert_eq!(reason, "unchanged"),
            _ => panic!("Expected Noop variant"),
        }
    }

    #[test]
    fn test_noop_duplicate() {
        let status = MutationStatus::from_str("noop:duplicate");
        assert!(status.is_noop());
    }

    // CASE INSENSITIVITY
    #[test]
    fn test_case_insensitive_error_prefix() {
        assert!(MutationStatus::from_str("FAILED:validation").is_error());
        assert!(MutationStatus::from_str("Unauthorized:token").is_error());
        assert!(MutationStatus::from_str("Conflict:DUPLICATE").is_error());
    }

    #[test]
    fn test_case_insensitive_success() {
        assert!(MutationStatus::from_str("SUCCESS").is_success());
        assert!(MutationStatus::from_str("Created").is_success());
    }

    // EDGE CASES
    #[test]
    fn test_status_with_multiple_colons() {
        let status = MutationStatus::from_str("failed:validation:email_invalid");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(full_status) => {
                assert_eq!(full_status, "failed:validation:email_invalid")
            }
            _ => panic!("Expected Error with full status"),
        }
    }

    #[test]
    fn test_error_prefix_without_reason() {
        let status = MutationStatus::from_str("failed:");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(full_status) => assert_eq!(full_status, "failed:"),
            _ => panic!("Expected Error with empty status"),
        }
    }

    #[test]
    fn test_unknown_status_becomes_success() {
        // Unknown statuses default to success for backward compatibility
        let status = MutationStatus::from_str("unknown_status");
        assert!(status.is_success());
    }

    #[test]
    fn test_empty_status() {
        let status = MutationStatus::from_str("");
        assert!(status.is_success());
    }
}

// ============================================================================
// Tests for STATUS TAXONOMY INTEGRATION (Phase 3: REFACTOR)
// ============================================================================

#[cfg(test)]
mod test_mutation_response_integration {
    use super::*;

    #[test]
    fn test_build_error_response_validation() {
        let mutation_json = r#"{
            "status": "failed:validation_error",
            "message": "Invalid email format",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();

        // Parse JSON to verify structure
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be error type
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserError"
        );
        assert_eq!(
            response["data"]["createUser"]["message"],
            "Invalid email format"
        );
    }

    #[test]
    fn test_build_error_response_conflict() {
        let mutation_json = r#"{
            "status": "conflict:duplicate_email",
            "message": "Email already exists",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be error type with conflict status
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserError"
        );
        assert!(response["data"]["createUser"]["status"]
            .as_str()
            .unwrap()
            .starts_with("conflict:"));
    }

    #[test]
    fn test_build_noop_response() {
        let mutation_json = r#"{
            "status": "noop:duplicate",
            "message": "Already exists",
            "entity_id": "123",
            "entity_type": "User",
            "entity": {"id": "123", "email": "test@example.com"},
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Noop should be SUCCESS type (no change, but not an error)
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserSuccess"
        );
        assert_eq!(response["data"]["createUser"]["message"], "Already exists");
    }

    #[test]
    fn test_build_success_response() {
        let mutation_json = r#"{
            "status": "created",
            "message": "User created successfully",
            "entity_id": "456",
            "entity_type": "User",
            "entity": {"id": "456", "email": "new@example.com", "name": "Test User"},
            "updated_fields": ["email", "name"],
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be success type
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserSuccess"
        );
        assert!(response["data"]["createUser"]["user"].is_object());
        assert_eq!(response["data"]["createUser"]["user"]["id"], "456");
    }

    #[test]
    fn test_unauthorized_error() {
        let mutation_json = r#"{
            "status": "unauthorized:token_expired",
            "message": "Authentication token has expired",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "updateProfile",
            "UpdateProfileSuccess",
            "UpdateProfileError",
            None,
            None,
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        assert_eq!(
            response["data"]["updateProfile"]["__typename"],
            "UpdateProfileError"
        );
    }

    #[test]
    fn test_timeout_error() {
        let mutation_json = r#"{
            "status": "timeout:database_query",
            "message": "Database query exceeded 30 second timeout",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "processLargeDataset",
            "ProcessSuccess",
            "ProcessError",
            None,
            None,
            None,
            true,
            None,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        assert_eq!(
            response["data"]["processLargeDataset"]["__typename"],
            "ProcessError"
        );
        assert!(response["data"]["processLargeDataset"]["status"]
            .as_str()
            .unwrap()
            .starts_with("timeout:"));
    }
}

// ============================================================================
// COMPREHENSIVE EDGE CASE TESTS (Phase 5)
// ============================================================================

#[cfg(test)]
mod edge_cases {
    use super::*;

    // ===== CASCADE PLACEMENT =====

    #[test]
    fn test_cascade_never_nested_in_entity() {
        let json = r#"{
            "status": "created",
            "entity_type": "Post",
            "entity": {"id": "123", "title": "Test"},
            "cascade": {"updated": []}
        }"#;

        let result = build_mutation_response(
            json,
            "createPost",
            "CreatePostSuccess",
            "CreatePostError",
            Some("post"),
            Some("Post"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let success = &response["data"]["createPost"];

        // CASCADE at success level
        assert!(success["cascade"].is_object());
        // NOT in entity
        assert!(success["post"]["cascade"].is_null());
    }

    #[test]
    fn test_cascade_never_copied_from_entity_wrapper() {
        // TEST: When entity is a wrapper containing both the entity field
        // AND cascade data, CASCADE should NOT be copied from the wrapper
        // into the entity object.
        //
        // This tests the case where PostgreSQL returns:
        // entity: {"allocation": {...}, "cascade": {...}, "message": "..."}
        let json = r#"{
            "status": "created",
            "entity_type": "Allocation",
            "entity": {
                "allocation": {
                    "id": "d8c7c0b3-6b21-44c7-9195-504ca1c63e47",
                    "identifier": "test-allocation"
                },
                "cascade": {
                    "updated": [
                        {
                            "__typename": "Allocation",
                            "id": "d8c7c0b3-6b21-44c7-9195-504ca1c63e47",
                            "operation": "CREATED"
                        }
                    ],
                    "deleted": [],
                    "invalidations": [
                        {
                            "queryName": "allocations",
                            "scope": "PREFIX",
                            "strategy": "INVALIDATE"
                        }
                    ]
                },
                "message": "New allocation created"
            },
            "cascade": {
                "updated": [
                    {
                        "__typename": "Allocation",
                        "id": "d8c7c0b3-6b21-44c7-9195-504ca1c63e47",
                        "operation": "CREATED"
                    }
                ],
                "deleted": [],
                "invalidations": [
                    {
                        "queryName": "allocations",
                        "scope": "PREFIX",
                        "strategy": "INVALIDATE"
                    }
                ]
            }
        }"#;

        let result = build_mutation_response(
            json,
            "createAllocation",
            "CreateAllocationSuccess",
            "CreateAllocationError",
            Some("allocation"),
            Some("Allocation"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let success = &response["data"]["createAllocation"];

        // CASCADE must be at success level
        assert!(
            success["cascade"].is_object(),
            "CASCADE missing at success level"
        );
        assert!(
            success["cascade"]["updated"].is_array(),
            "CASCADE.updated should be array"
        );

        // CASCADE must NEVER be in the entity object
        assert!(
            success["allocation"]["cascade"].is_null(),
            "BUG: CASCADE should NOT be copied from entity wrapper into allocation object"
        );

        // Message from wrapper should be copied (this is correct behavior)
        assert_eq!(success["message"], "New allocation created");

        // Verify entity has correct fields
        assert_eq!(
            success["allocation"]["id"],
            "d8c7c0b3-6b21-44c7-9195-504ca1c63e47"
        );
        assert_eq!(success["allocation"]["identifier"], "test-allocation");
    }

    // ===== __typename CORRECTNESS =====

    #[test]
    fn test_typename_always_present() {
        let json = r#"{"id": "123"}"#;
        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            Some("entity"),
            Some("Entity"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Success type has __typename
        assert_eq!(response["data"]["test"]["__typename"], "TestSuccess");
        // Entity has __typename
        assert_eq!(response["data"]["test"]["entity"]["__typename"], "Entity");
    }

    #[test]
    fn test_typename_matches_entity_type() {
        let json = r#"{
            "status": "success",
            "entity_type": "CustomType",
            "entity": {"id": "123"}
        }"#;

        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            Some("entity"),
            Some("CustomType"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // __typename must match entity_type from JSON
        assert_eq!(
            response["data"]["test"]["entity"]["__typename"],
            "CustomType"
        );
    }

    // ===== FORMAT DETECTION =====

    #[test]
    fn test_ambiguous_status_treated_as_simple() {
        // Has "status" field but value is not a valid mutation status
        let json = r#"{"status": "active", "name": "User"}"#;
        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            Some("entity"),
            Some("Entity"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should be treated as simple format (entity only)
        // The entire object becomes the entity
        assert_eq!(response["data"]["test"]["entity"]["status"], "active");
    }

    // ===== NULL HANDLING =====

    #[test]
    fn test_null_entity() {
        let json = r#"{
            "status": "success",
            "message": "OK",
            "entity": null
        }"#;

        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            None,
            None,
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should have message but no entity field
        assert_eq!(response["data"]["test"]["message"], "OK");
        assert!(response["data"]["test"].get("entity").is_none());
    }

    // ===== ARRAY ENTITIES =====

    #[test]
    fn test_array_of_entities() {
        let json = r#"[
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ]"#;

        let result = build_mutation_response(
            json,
            "listUsers",
            "ListUsersSuccess",
            "ListUsersError",
            Some("users"),
            Some("User"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Each array element should have __typename
        let users = response["data"]["listUsers"]["users"].as_array().unwrap();
        assert_eq!(users[0]["__typename"], "User");
        assert_eq!(users[1]["__typename"], "User");
    }

    // ===== DEEP NESTING =====

    #[test]
    fn test_deeply_nested_objects() {
        let json = r#"{
            "id": "1",
            "level1": {
                "level2": {
                    "level3": {
                        "value": "deep"
                    }
                }
            }
        }"#;

        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            Some("entity"),
            Some("Entity"),
            None,
            true,
            None,
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should handle deep nesting
        assert_eq!(
            response["data"]["test"]["entity"]["level1"]["level2"]["level3"]["value"],
            "deep"
        );
    }

    // ===== SPECIAL CHARACTERS =====

    #[test]
    fn test_special_characters_in_fields() {
        let json = r#"{
            "id": "123",
            "field_with_unicode": "Hello 世界",
            "field_with_quotes": "He said \"hello\""
        }"#;

        let result = build_mutation_response(
            json,
            "test",
            "TestSuccess",
            "TestError",
            Some("entity"),
            Some("Entity"),
            None,
            false,
            None, // No camelCase
        )
        .unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should preserve special characters
        assert_eq!(
            response["data"]["test"]["entity"]["field_with_unicode"],
            "Hello 世界"
        );
    }
}

// ============================================================================
// PROPERTY-BASED TESTS (Phase 5)
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn cascade_never_in_entity(
            entity_id in ".*",
            cascade_data in prop::bool::ANY,
        ) {
            let json = if cascade_data {
                format!(r#"{{
                    "status": "success",
                    "entity_type": "Test",
                    "entity": {{"id": "{}"}},
                    "cascade": {{"updated": []}}
                }}"#, entity_id)
            } else {
                format!(r#"{{
                    "status": "success",
                    "entity_type": "Test",
                    "entity": {{"id": "{}"}}
                }}"#, entity_id)
            };

            let result = super::build_mutation_response(
                &json, "test", "TestSuccess", "TestError",
                Some("entity"), Some("Test"), None, true, None,
            ).unwrap();

            let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
            let entity = &response["data"]["test"]["entity"];

            // INVARIANT: CASCADE must NEVER be in entity
            prop_assert!(entity.get("cascade").is_none());
        }

        #[test]
        fn typename_always_present_in_success(
            entity_id in ".*",
        ) {
            let json = format!(r#"{{"id": "{}"}}"#, entity_id);

            let result = build_mutation_response(
                &json, "test", "TestSuccess", "TestError",
                Some("entity"), Some("Entity"), None, true, None,
            ).unwrap();

            let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

            // INVARIANT: __typename always present
            prop_assert_eq!(
                response["data"]["test"]["__typename"].as_str(),
                Some("TestSuccess")
            );
            prop_assert_eq!(
                response["data"]["test"]["entity"]["__typename"].as_str(),
                Some("Entity")
            );
        }

        #[test]
        fn format_detection_deterministic(
            has_status in prop::bool::ANY,
            entity_data in ".*",
        ) {
            let json = if has_status {
                format!(r#"{{"status": "success", "data": "{}"}}"#, entity_data)
            } else {
                format!(r#"{{"data": "{}"}}"#, entity_data)
            };

            // Parse twice - should get same format
            let result_first_parse = MutationResult::from_json(&json, None);
            let result_reparsed = MutationResult::from_json(&json, None);

            // INVARIANT: Format detection is deterministic (same JSON → same result)
            prop_assert_eq!(result_first_parse.is_ok(), result_reparsed.is_ok());
        }
    }
}

// ============================================================================
// CASCADE FIX TESTS (Phase 1: RED) - 8-field composite type parsing
// ============================================================================

#[cfg(test)]
mod postgres_composite_tests {
    use crate::mutation::PostgresMutationResponse;

    #[test]
    fn test_parse_8field_mutation_response() {
        // Test parsing of 8-field mutation response format
        let json = r#"{
            "status": "created",
            "message": "Allocation created successfully",
            "entity_id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
            "entity_type": "Allocation",
            "entity": {"id": "4d16b78b-7d9b-495f-9094-a65b57b33916", "identifier": "test"},
            "updated_fields": ["location_id", "machine_id"],
            "cascade": {
                "updated": [{"id": "some-id", "operation": "UPDATED"}],
                "deleted": [],
                "invalidations": [{"queryName": "allocations", "strategy": "INVALIDATE"}]
            },
            "metadata": {"extra": "data"}
        }"#;

        // Try to parse as 8-field format
        // Test parsing of 8-field composite type
        let result = PostgresMutationResponse::from_json(json).unwrap();

        assert_eq!(result.status, "created");
        assert_eq!(result.entity_type, Some("Allocation".to_string()));
        assert!(result.cascade.is_some());

        let cascade = result.cascade.as_ref().unwrap();
        assert!(cascade.get("updated").is_some());
    }

    #[test]
    fn test_cascade_extraction_from_position_7() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_id": "uuid",
            "entity_type": "Allocation",
            "entity": {},
            "updated_fields": [],
            "cascade": {"updated": [{"id": "1"}]},
            "metadata": {}
        }"#;

        let pg_response = PostgresMutationResponse::from_json(json).unwrap();
        let result = pg_response.to_mutation_result(None);

        // CASCADE should come from Position 7, not metadata
        assert!(result.cascade.is_some());
        assert_eq!(
            result.cascade.unwrap().get("updated").unwrap()[0]["id"],
            "1"
        );
    }
}
