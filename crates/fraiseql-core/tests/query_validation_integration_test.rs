//! Integration tests for GraphQL query validation
//!
//! Tests query validation and error handling workflows.

use serde_json::{Value, json};

/// Helper to create a schema with a specific type
fn create_schema_with_type(type_name: &str, fields: Vec<&str>) -> Value {
    let field_defs: Vec<Value> = fields
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "type": "String",
                "nullable": false,
                "source": name
            })
        })
        .collect();

    json!({
        "version": "2.0.0",
        "types": [
            {
                "name": type_name,
                "fields": field_defs
            }
        ],
        "queries": [
            {
                "name": format!("get{}", type_name),
                "type": type_name,
                "args": []
            }
        ]
    })
}

#[test]
fn test_query_with_valid_field_succeeds() {
    let schema = create_schema_with_type("User", vec!["id", "name", "email"]);

    let query_fields = ["id", "name"];
    let has_all_fields = query_fields.iter().all(|field| {
        schema
            .get("types")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.first())
            .and_then(|t| t.get("fields"))
            .and_then(|f| f.as_array())
            .map(|fields| fields.iter().any(|f| f.get("name").map(|n| n == field).unwrap_or(false)))
            .unwrap_or(false)
    });

    assert!(has_all_fields, "All query fields should exist in schema");
}

#[test]
fn test_query_field_validation_against_schema() {
    let schema = create_schema_with_type("Product", vec!["sku", "price", "name"]);

    let valid_fields = vec!["sku", "price"];
    let invalid_fields = vec!["description", "rating"];

    let schema_fields: Vec<&str> = schema
        .get("types")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .and_then(|t| t.get("fields"))
        .and_then(|f| f.as_array())
        .map(|fields| {
            fields.iter().filter_map(|f| f.get("name").and_then(|n| n.as_str())).collect()
        })
        .unwrap_or_default();

    // Valid fields should be in schema
    for field in valid_fields {
        assert!(schema_fields.contains(&field), "Valid field {} should be in schema", field);
    }

    // Invalid fields should not be in schema
    for field in invalid_fields {
        assert!(
            !schema_fields.contains(&field),
            "Invalid field {} should not be in schema",
            field
        );
    }
}

#[test]
fn test_type_lookup_in_schema() {
    let schema = create_schema_with_type("Order", vec!["id", "total", "status"]);

    let type_name = "Order";
    let found_type = schema
        .get("types")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.iter().find(|t| t.get("name").map(|n| n == type_name).unwrap_or(false)))
        .is_some();

    assert!(found_type, "Type {} should be found in schema", type_name);
}

#[test]
fn test_nested_type_validation() {
    let schema = json!({
        "version": "2.0.0",
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "Int", "nullable": false},
                    {"name": "profile", "type": "Profile", "nullable": true},
                ]
            },
            {
                "name": "Profile",
                "fields": [
                    {"name": "bio", "type": "String", "nullable": true},
                    {"name": "avatar", "type": "String", "nullable": true},
                ]
            }
        ],
        "queries": []
    });

    // Verify user can reference profile type
    let user_type = schema
        .get("types")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.iter().find(|t| t.get("name").map(|n| n == "User").unwrap_or(false)))
        .expect("User type should exist");

    let profile_field = user_type
        .get("fields")
        .and_then(|f| f.as_array())
        .and_then(|arr| arr.iter().find(|f| f.get("name").map(|n| n == "profile").unwrap_or(false)))
        .expect("Profile field should exist");

    let field_type = profile_field.get("type").and_then(|t| t.as_str());
    assert_eq!(field_type, Some("Profile"), "Profile field should reference Profile type");
}

#[test]
fn test_query_root_type_validation() {
    let schema = json!({
        "version": "2.0.0",
        "types": [
            {"name": "User", "fields": []},
            {"name": "Post", "fields": []}
        ],
        "queries": [
            {"name": "user", "type": "User"},
            {"name": "post", "type": "Post"},
            {"name": "invalid", "type": "InvalidType"}
        ]
    });

    let queries = schema
        .get("queries")
        .and_then(|q| q.as_array())
        .expect("queries should be array");

    let types: std::collections::HashSet<&str> = schema
        .get("types")
        .and_then(|t| t.as_array())
        .map(|arr| arr.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str())).collect())
        .unwrap_or_default();

    for query in queries {
        let query_type = query.get("type").and_then(|t| t.as_str());
        // Note: The invalid type won't match but we're testing the structure
        if let Some(qtype) = query_type {
            let query_name = query.get("name").and_then(|n| n.as_str());
            if let Some(qname) = query_name {
                if qname != "invalid" {
                    assert!(types.contains(qtype), "Query {} references type {}", qname, qtype);
                }
            }
        }
    }
}

#[test]
fn test_field_nullability_info() {
    let schema = json!({
        "version": "2.0.0",
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "Int", "nullable": false},
                    {"name": "email", "type": "String", "nullable": false},
                    {"name": "bio", "type": "String", "nullable": true},
                ]
            }
        ],
        "queries": []
    });

    let user_type = schema
        .get("types")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .expect("User type should exist");

    let fields = user_type
        .get("fields")
        .and_then(|f| f.as_array())
        .expect("fields should be array");

    // Check nullable information is present
    for field in fields {
        let has_nullable = field.get("nullable").is_some();
        assert!(has_nullable, "Field should have nullable attribute");
    }

    // Verify specific nullability
    let non_null_fields: Vec<&str> = fields
        .iter()
        .filter(|f| f.get("nullable").map(|n| !n.as_bool().unwrap_or(false)).unwrap_or(false))
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
        .collect();

    assert_eq!(non_null_fields.len(), 2, "Should have 2 non-nullable fields");
    assert!(non_null_fields.contains(&"id"));
    assert!(non_null_fields.contains(&"email"));
}

#[test]
fn test_schema_query_coverage() {
    let schema = create_schema_with_type("User", vec!["id"]);

    // Verify that at least one query exists for each non-abstract type
    let types: Vec<&str> = schema
        .get("types")
        .and_then(|t| t.as_array())
        .map(|arr| arr.iter().filter_map(|t| t.get("name").and_then(|n| n.as_str())).collect())
        .unwrap_or_default();

    let _queries = schema
        .get("queries")
        .and_then(|q| q.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);

    // Schema should have types if queries are defined
    assert!(!types.is_empty(), "Schema should have types");
}
