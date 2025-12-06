//! Tests for schema-aware JSON transformation
//!
//! These tests verify that the transformer correctly:
//! 1. Resolves nested object types using the schema registry
//! 2. Injects correct __typename for nested objects
//! 3. Handles list types (arrays of objects)
//! 4. Handles null values and edge cases

use serde_json::json;

/// Helper to create test schema registry
fn create_test_schema() -> fraiseql_rs::schema_registry::SchemaRegistry {
    let schema_json = r#"{
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "Assignment": {
                "fields": {
                    "id": {
                        "type_name": "UUID",
                        "is_nested_object": false,
                        "is_list": false
                    },
                    "equipment": {
                        "type_name": "Equipment",
                        "is_nested_object": true,
                        "is_list": false
                    },
                    "equipments": {
                        "type_name": "Equipment",
                        "is_nested_object": true,
                        "is_list": true
                    }
                }
            },
            "Equipment": {
                "fields": {
                    "id": {
                        "type_name": "UUID",
                        "is_nested_object": false,
                        "is_list": false
                    },
                    "name": {
                        "type_name": "String",
                        "is_nested_object": false,
                        "is_list": false
                    },
                    "is_active": {
                        "type_name": "Boolean",
                        "is_nested_object": false,
                        "is_list": false
                    }
                }
            }
        }
    }"#;

    fraiseql_rs::schema_registry::SchemaRegistry::from_json(schema_json).unwrap()
}

/// RED PHASE TEST 1: Nested object should have correct __typename
#[test]
fn test_transform_nested_object_with_correct_typename() {
    let registry = create_test_schema();

    // Input: Assignment with nested Equipment (from PostgreSQL JSONB)
    let input = json!({
        "id": "assignment-1",
        "equipment": {
            "id": "equipment-2",
            "name": "Device",
            "is_active": true
        }
    });

    // Transform with schema awareness
    // This function doesn't exist yet - RED PHASE (test should fail)
    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    // Expected: Nested equipment has correct __typename = "Equipment" (not "Assignment")
    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "equipment": {
            "__typename": "Equipment",
            "id": "equipment-2",
            "name": "Device",
            "isActive": true
        }
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 2: Deeply nested objects
#[test]
fn test_transform_deeply_nested_objects() {
    let schema_json = r#"{
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "Organization": {
                "fields": {
                    "id": {"type_name": "UUID", "is_nested_object": false, "is_list": false},
                    "department": {"type_name": "Department", "is_nested_object": true, "is_list": false}
                }
            },
            "Department": {
                "fields": {
                    "id": {"type_name": "UUID", "is_nested_object": false, "is_list": false},
                    "team": {"type_name": "Team", "is_nested_object": true, "is_list": false}
                }
            },
            "Team": {
                "fields": {
                    "id": {"type_name": "UUID", "is_nested_object": false, "is_list": false},
                    "name": {"type_name": "String", "is_nested_object": false, "is_list": false}
                }
            }
        }
    }"#;

    let registry = fraiseql_rs::schema_registry::SchemaRegistry::from_json(schema_json).unwrap();

    let input = json!({
        "id": "org-1",
        "department": {
            "id": "dept-1",
            "team": {
                "id": "team-1",
                "name": "Engineering"
            }
        }
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Organization", &registry);

    let expected = json!({
        "__typename": "Organization",
        "id": "org-1",
        "department": {
            "__typename": "Department",
            "id": "dept-1",
            "team": {
                "__typename": "Team",
                "id": "team-1",
                "name": "Engineering"
            }
        }
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 3: Array of nested objects (list type)
#[test]
fn test_transform_array_of_nested_objects() {
    let registry = create_test_schema();

    let input = json!({
        "id": "assignment-1",
        "equipments": [
            {"id": "eq-1", "name": "Device 1", "is_active": true},
            {"id": "eq-2", "name": "Device 2", "is_active": false}
        ]
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "equipments": [
            {"__typename": "Equipment", "id": "eq-1", "name": "Device 1", "isActive": true},
            {"__typename": "Equipment", "id": "eq-2", "name": "Device 2", "isActive": false}
        ]
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 4: Null nested object should pass through as null
#[test]
fn test_transform_null_nested_field() {
    let registry = create_test_schema();

    let input = json!({
        "id": "assignment-1",
        "equipment": null
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "equipment": null
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 5: Empty array should pass through
#[test]
fn test_transform_empty_array() {
    let registry = create_test_schema();

    let input = json!({
        "id": "assignment-1",
        "equipments": []
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "equipments": []
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 6: Array with null items should preserve nulls
#[test]
fn test_transform_array_with_null_items() {
    let registry = create_test_schema();

    let input = json!({
        "id": "assignment-1",
        "equipments": [
            {"id": "eq-1", "name": "Device 1", "is_active": true},
            null,
            {"id": "eq-2", "name": "Device 2", "is_active": false}
        ]
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "equipments": [
            {"__typename": "Equipment", "id": "eq-1", "name": "Device 1", "isActive": true},
            null,
            {"__typename": "Equipment", "id": "eq-2", "name": "Device 2", "isActive": false}
        ]
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 7: Missing type in schema - graceful degradation
#[test]
fn test_transform_with_missing_schema_type() {
    let registry = create_test_schema();

    // Input has a field type not in schema
    let input = json!({
        "id": "assignment-1",
        "unknown_field": {
            "id": "unknown-1",
            "name": "Unknown"
        }
    });

    // Should still transform (without __typename for unknown field)
    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    // Expected: Root type has __typename, unknown field passes through
    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1",
        "unknownField": {
            "id": "unknown-1",
            "name": "Unknown"
        }
    });

    assert_eq!(result, expected);
}

/// RED PHASE TEST 8: Performance - no overhead for scalar-only objects
#[test]
fn test_transform_scalar_only_object() {
    let registry = create_test_schema();

    let input = json!({
        "id": "assignment-1"
    });

    let result =
        fraiseql_rs::json_transform::transform_with_schema(&input, "Assignment", &registry);

    let expected = json!({
        "__typename": "Assignment",
        "id": "assignment-1"
    });

    assert_eq!(result, expected);
}
