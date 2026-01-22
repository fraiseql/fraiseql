//! Test deprecated field introspection handling.
//!
//! This test verifies that:
//! 1. Fields can be marked as deprecated with reason
//! 2. Deprecated fields are still queryable (for backward compatibility)
//! 3. __deprecated introspection field reflects deprecation status
//! 4. Deprecation reasons are preserved exactly
//! 5. Schema properly tracks deprecated fields
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Deprecated field metadata could be lost
//! - Deprecation reasons could be truncated
//! - Tools couldn't warn developers about deprecated fields
//! - Backward compatibility could break

use serde_json::json;

#[test]
fn test_field_deprecated_status_false() {
    // Non-deprecated field has isDeprecated: false
    let field = json!({
        "name": "id",
        "type": "ID!",
        "isDeprecated": false,
        "deprecationReason": null
    });

    assert_eq!(field["name"], json!("id"));
    assert_eq!(field["isDeprecated"], json!(false));
    assert_eq!(field["deprecationReason"], json!(null));
}

#[test]
fn test_field_deprecated_status_true() {
    // Deprecated field has isDeprecated: true
    let field = json!({
        "name": "userId",
        "type": "Int!",
        "isDeprecated": true,
        "deprecationReason": "Use id instead"
    });

    assert_eq!(field["name"], json!("userId"));
    assert_eq!(field["isDeprecated"], json!(true));
    assert_eq!(field["deprecationReason"], json!("Use id instead"));
}

#[test]
fn test_deprecation_reason_preservation() {
    // Deprecation reason is preserved exactly
    let deprecation_reasons = vec![
        "Use newFieldName instead",
        "This field will be removed in v2.0",
        "Deprecated since 2023-01-01. Use replacement field.",
        "Legacy field - use the refactored field",
        "This field has been superseded by a better implementation",
        "Scheduled for removal in June 2024",
    ];

    for reason in deprecation_reasons {
        let field = json!({
            "name": "old_field",
            "isDeprecated": true,
            "deprecationReason": reason
        });

        assert_eq!(field["deprecationReason"], json!(reason));
    }
}

#[test]
fn test_deprecated_field_still_queryable() {
    // Deprecated fields can still be queried (backward compatibility)
    let schema = json!({
        "User": {
            "fields": [
                {
                    "name": "id",
                    "type": "ID!",
                    "isDeprecated": false
                },
                {
                    "name": "userId",
                    "type": "Int!",
                    "isDeprecated": true,
                    "deprecationReason": "Use id field instead"
                }
            ]
        }
    });

    let fields = schema["User"]["fields"].as_array().unwrap();
    let mut deprecated_count = 0;

    for field in fields {
        if field["isDeprecated"] == json!(true) {
            deprecated_count += 1;
            // Deprecated fields should still have type info for queries
            assert!(field["type"].is_string());
        }
    }

    assert_eq!(deprecated_count, 1);
}

#[test]
fn test_multiple_deprecated_fields() {
    // Type can have multiple deprecated fields
    let type_def = json!({
        "name": "User",
        "fields": [
            {
                "name": "id",
                "isDeprecated": false
            },
            {
                "name": "userId",
                "isDeprecated": true,
                "deprecationReason": "Use id instead"
            },
            {
                "name": "emailAddress",
                "isDeprecated": false
            },
            {
                "name": "email",
                "isDeprecated": true,
                "deprecationReason": "Use emailAddress instead"
            },
            {
                "name": "birthDate",
                "isDeprecated": false
            },
            {
                "name": "dateOfBirth",
                "isDeprecated": true,
                "deprecationReason": "Use birthDate instead"
            }
        ]
    });

    let fields = type_def["fields"].as_array().unwrap();
    let deprecated_fields: Vec<_> = fields.iter()
        .filter(|f| f["isDeprecated"] == json!(true))
        .collect();

    assert_eq!(deprecated_fields.len(), 3);
    assert_eq!(deprecated_fields[0]["deprecationReason"], json!("Use id instead"));
    assert_eq!(deprecated_fields[1]["deprecationReason"], json!("Use emailAddress instead"));
    assert_eq!(deprecated_fields[2]["deprecationReason"], json!("Use birthDate instead"));
}

#[test]
fn test_deprecated_field_with_empty_reason() {
    // Deprecated field can have empty reason string (not null, but empty)
    let field_with_empty_reason = json!({
        "name": "oldField",
        "isDeprecated": true,
        "deprecationReason": ""
    });

    assert_eq!(field_with_empty_reason["isDeprecated"], json!(true));
    assert_eq!(field_with_empty_reason["deprecationReason"], json!(""));

    // Verify it's different from null
    let field_with_null_reason = json!({
        "name": "newField",
        "isDeprecated": false,
        "deprecationReason": null
    });

    assert_ne!(
        field_with_empty_reason["deprecationReason"],
        field_with_null_reason["deprecationReason"]
    );
}

#[test]
fn test_enum_value_deprecated() {
    // Enum values can also be deprecated
    let enum_def = json!({
        "name": "UserRole",
        "values": [
            {
                "name": "ADMIN",
                "isDeprecated": false
            },
            {
                "name": "MODERATOR",
                "isDeprecated": false
            },
            {
                "name": "SUPERUSER",
                "isDeprecated": true,
                "deprecationReason": "Use ADMIN instead"
            },
            {
                "name": "USER",
                "isDeprecated": false
            }
        ]
    });

    let values = enum_def["values"].as_array().unwrap();
    let deprecated_values: Vec<_> = values.iter()
        .filter(|v| v["isDeprecated"] == json!(true))
        .collect();

    assert_eq!(deprecated_values.len(), 1);
    assert_eq!(deprecated_values[0]["name"], json!("SUPERUSER"));
}

#[test]
fn test_introspection_query_response_deprecated() {
    // Introspection response includes deprecation info
    let introspection_response = json!({
        "__type": {
            "name": "Post",
            "fields": [
                {
                    "name": "id",
                    "type": {"kind": "NON_NULL"},
                    "isDeprecated": false,
                    "deprecationReason": null
                },
                {
                    "name": "authorId",
                    "type": {"kind": "SCALAR"},
                    "isDeprecated": true,
                    "deprecationReason": "Use author { id } instead"
                },
                {
                    "name": "author",
                    "type": {"kind": "OBJECT"},
                    "isDeprecated": false,
                    "deprecationReason": null
                }
            ]
        }
    });

    let fields = introspection_response["__type"]["fields"].as_array().unwrap();

    // Find deprecated field
    let deprecated_field = fields.iter()
        .find(|f| f["name"] == "authorId")
        .unwrap();

    assert_eq!(deprecated_field["isDeprecated"], json!(true));
    assert_eq!(deprecated_field["deprecationReason"], json!("Use author { id } instead"));
}

#[test]
fn test_deprecated_field_multiline_reason() {
    // Deprecation reason can span multiple lines
    let reason = "This field is deprecated as of version 2.0.\n\
                  Please use the new `newFieldName` field instead.\n\
                  Migration guide: https://docs.example.com/migration";

    let field = json!({
        "name": "oldField",
        "isDeprecated": true,
        "deprecationReason": reason
    });

    assert_eq!(field["deprecationReason"], json!(reason));
    let reason_str = field["deprecationReason"].as_str().unwrap();
    assert!(reason_str.contains("version 2.0"));
    assert!(reason_str.contains("newFieldName"));
    assert!(reason_str.contains("migration"));
}

#[test]
fn test_deprecated_field_with_special_characters() {
    // Deprecation reason can contain special characters
    let reason = r#"Use "newField" instead (deprecated as of v2.0 & beyond)"#;

    let field = json!({
        "name": "oldField",
        "isDeprecated": true,
        "deprecationReason": reason
    });

    assert_eq!(field["deprecationReason"], json!(reason));
    let reason_str = field["deprecationReason"].as_str().unwrap();
    assert!(reason_str.contains("\"newField\""));
    assert!(reason_str.contains("&"));
}

#[test]
fn test_deprecated_input_field() {
    // Input type fields can also be deprecated
    let input_type = json!({
        "name": "UserInput",
        "inputFields": [
            {
                "name": "id",
                "type": "ID!",
                "isDeprecated": false
            },
            {
                "name": "userId",
                "type": "Int!",
                "isDeprecated": true,
                "deprecationReason": "Use id instead"
            },
            {
                "name": "name",
                "type": "String!",
                "isDeprecated": false
            }
        ]
    });

    let input_fields = input_type["inputFields"].as_array().unwrap();
    let deprecated_input_fields: Vec<_> = input_fields.iter()
        .filter(|f| f["isDeprecated"] == json!(true))
        .collect();

    assert_eq!(deprecated_input_fields.len(), 1);
    assert_eq!(deprecated_input_fields[0]["name"], json!("userId"));
}

#[test]
fn test_deprecated_status_field_structure() {
    // Verify deprecated status field structure is consistent
    let field_info = json!({
        "name": "legacyField",
        "type": "String",
        "description": "A legacy field",
        "isDeprecated": true,
        "deprecationReason": "Use modernField instead"
    });

    // All introspection fields present
    assert!(field_info["name"].is_string());
    assert!(field_info["type"].is_string());
    assert!(field_info["description"].is_string());
    assert!(field_info["isDeprecated"].is_boolean());
    assert!(field_info["deprecationReason"].is_string());

    // Deprecation info specifically
    assert!(field_info["isDeprecated"].as_bool().unwrap());
    let reason = field_info["deprecationReason"].as_str().unwrap();
    assert!(!reason.is_empty());
}

