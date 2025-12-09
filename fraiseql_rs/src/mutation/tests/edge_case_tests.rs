//! Edge Cases and Corner Cases Tests
//!
//! Tests for unusual scenarios:
//! - CASCADE edge cases (never copied from entity wrapper)
//! - __typename always present and matches entity_type
//! - Ambiguous status strings treated as simple format
//! - Null entity handling
//! - Array of entities
//! - Deeply nested objects
//! - Special characters in field names

use super::*;

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
