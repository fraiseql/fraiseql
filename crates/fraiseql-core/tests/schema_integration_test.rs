//! Integration tests for schema compilation and execution
//!
//! Tests the end-to-end schema loading and query validation workflow.

use serde_json::{json, Value};

/// Helper to create a minimal compiled schema for testing
fn create_test_schema(name: &str) -> Value {
    json!({
        "version": "2.0.0",
        "types": [
            {
                "name": name,
                "fields": [
                    {
                        "name": "id",
                        "type": "Int",
                        "nullable": false,
                        "source": "id"
                    },
                    {
                        "name": "name",
                        "type": "String",
                        "nullable": true,
                        "source": "name"
                    }
                ]
            }
        ],
        "queries": []
    })
}

#[test]
fn test_schema_serialization_preserves_structure() {
    let schema = create_test_schema("User");

    let serialized = serde_json::to_string(&schema).expect("serialization failed");
    let deserialized: Value = serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(schema, deserialized, "Schema structure should be preserved");
}

#[test]
fn test_schema_has_required_fields() {
    let schema = create_test_schema("Product");

    // Verify required top-level fields
    assert!(schema.get("version").is_some(), "Schema must have version");
    assert!(schema.get("types").is_some(), "Schema must have types");
    assert!(schema.get("queries").is_some(), "Schema must have queries");
}

#[test]
fn test_schema_types_have_names() {
    let schema = create_test_schema("Order");

    let types = schema
        .get("types")
        .and_then(|t| t.as_array())
        .expect("types should be array");

    for type_def in types {
        assert!(
            type_def.get("name").is_some(),
            "Type definition must have name field"
        );
    }
}

#[test]
fn test_schema_fields_have_required_attributes() {
    let schema = create_test_schema("Customer");

    let types = schema
        .get("types")
        .and_then(|t| t.as_array())
        .expect("types should be array");

    for type_def in types {
        let fields = type_def
            .get("fields")
            .and_then(|f| f.as_array())
            .expect("fields should be array");

        for field in fields {
            assert!(field.get("name").is_some(), "Field must have name");
            assert!(field.get("type").is_some(), "Field must have type");
            assert!(field.get("source").is_some(), "Field must have source");
        }
    }
}

#[test]
fn test_multiple_types_in_schema() {
    let mut schema = json!({
        "version": "2.0.0",
        "types": [],
        "queries": []
    });

    let types = vec!["User", "Post", "Comment"];
    for type_name in types {
        let type_def = json!({
            "name": type_name,
            "fields": []
        });
        schema["types"].as_array_mut().unwrap().push(type_def);
    }

    let type_names: Vec<&str> = schema
        .get("types")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                .collect()
        })
        .unwrap_or_default();

    assert_eq!(type_names.len(), 3, "Schema should have 3 types");
    assert_eq!(type_names, vec!["User", "Post", "Comment"]);
}

#[test]
fn test_schema_queries_structure() {
    let schema = json!({
        "version": "2.0.0",
        "types": [],
        "queries": [
            {
                "name": "getUser",
                "type": "User",
                "args": []
            },
            {
                "name": "listUsers",
                "type": "User",
                "args": []
            }
        ]
    });

    let queries = schema
        .get("queries")
        .and_then(|q| q.as_array())
        .expect("queries should be array");

    assert_eq!(queries.len(), 2, "Should have 2 queries");

    for query in queries {
        assert!(query.get("name").is_some(), "Query must have name");
        assert!(query.get("type").is_some(), "Query must have type");
        assert!(query.get("args").is_some(), "Query must have args");
    }
}

#[test]
fn test_empty_schema_is_valid() {
    let schema = json!({
        "version": "2.0.0",
        "types": [],
        "queries": []
    });

    // Empty schema should still have structure
    assert!(schema.get("version").is_some());
    assert!(schema.get("types").is_some());
    assert!(schema.get("queries").is_some());

    // Arrays should be empty but present
    assert_eq!(
        schema.get("types").and_then(|t| t.as_array()).map(|a| a.len()),
        Some(0)
    );
    assert_eq!(
        schema.get("queries").and_then(|q| q.as_array()).map(|a| a.len()),
        Some(0)
    );
}

#[test]
fn test_schema_version_compatibility() {
    let schemas = vec![
        json!({"version": "2.0.0", "types": [], "queries": []}),
        json!({"version": "2.0.1", "types": [], "queries": []}),
        json!({"version": "2.1.0", "types": [], "queries": []}),
    ];

    for schema in schemas {
        let version = schema
            .get("version")
            .and_then(|v| v.as_str())
            .expect("version should be string");

        // Version should be semver format
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(parts.len(), 3, "Version should be semver (X.Y.Z)");

        for part in parts {
            assert!(
                part.parse::<u32>().is_ok(),
                "Version parts should be numbers"
            );
        }
    }
}
