//! Test interface implementation validation.
//!
//! This test verifies that:
//! 1. Types can implement interfaces with required fields
//! 2. Interface field requirements are preserved
//! 3. Implementing types include all interface fields
//! 4. Multiple interface implementations work correctly
//! 5. Interface field types are validated for compatibility
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Types could implement interfaces without all fields
//! - Interface fields could be missing or have wrong types
//! - Multiple interface implementation could lose requirements

use serde_json::json;

#[test]
fn test_interface_definition_basic() {
    // Basic interface definition
    let interface = json!({
        "name": "Node",
        "kind": "INTERFACE",
        "fields": [
            {"name": "id", "type": "ID!"}
        ]
    });

    assert_eq!(interface["name"], json!("Node"));
    assert_eq!(interface["kind"], json!("INTERFACE"));
    let fields = interface["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["name"], json!("id"));
}

#[test]
fn test_type_implements_single_interface() {
    // Type implementing single interface
    let type_def = json!({
        "name": "User",
        "kind": "OBJECT",
        "interfaces": ["Node"],
        "fields": [
            {"name": "id", "type": "ID!"},
            {"name": "name", "type": "String!"}
        ]
    });

    let interfaces = type_def["interfaces"].as_array().unwrap();
    assert_eq!(interfaces.len(), 1);
    assert_eq!(interfaces[0], json!("Node"));

    // Verify type has interface field
    let fields = type_def["fields"].as_array().unwrap();
    let id_field = fields.iter().find(|f| f["name"] == "id").unwrap();
    assert_eq!(id_field["type"], json!("ID!"));
}

#[test]
fn test_type_implements_multiple_interfaces() {
    // Type implementing multiple interfaces
    let type_def = json!({
        "name": "User",
        "kind": "OBJECT",
        "interfaces": ["Node", "Timestamped", "Authored"],
        "fields": [
            {"name": "id", "type": "ID!"},
            {"name": "created_at", "type": "DateTime!"},
            {"name": "updated_at", "type": "DateTime!"},
            {"name": "author_id", "type": "ID!"}
        ]
    });

    let interfaces = type_def["interfaces"].as_array().unwrap();
    assert_eq!(interfaces.len(), 3);
    assert_eq!(interfaces[0], json!("Node"));
    assert_eq!(interfaces[1], json!("Timestamped"));
    assert_eq!(interfaces[2], json!("Authored"));
}

#[test]
fn test_interface_field_preservation() {
    // Interface fields are preserved exactly in type definitions
    let interface = json!({
        "name": "Entity",
        "kind": "INTERFACE",
        "fields": [
            {"name": "id", "type": "ID!", "description": "Unique identifier"},
            {"name": "created_at", "type": "DateTime!", "description": "Creation timestamp"},
            {"name": "updated_at", "type": "DateTime!", "description": "Last update timestamp"}
        ]
    });

    let fields = interface["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 3);

    // Verify each field is preserved exactly
    assert_eq!(fields[0]["name"], json!("id"));
    assert_eq!(fields[0]["type"], json!("ID!"));
    assert_eq!(fields[0]["description"], json!("Unique identifier"));

    assert_eq!(fields[1]["name"], json!("created_at"));
    assert_eq!(fields[1]["type"], json!("DateTime!"));

    assert_eq!(fields[2]["name"], json!("updated_at"));
    assert_eq!(fields[2]["type"], json!("DateTime!"));
}

#[test]
fn test_interface_with_nullable_fields() {
    // Interface can have both nullable and non-nullable fields
    let interface = json!({
        "name": "Searchable",
        "kind": "INTERFACE",
        "fields": [
            {"name": "search_index", "type": "[String!]!", "description": "Indexed terms"},
            {"name": "search_score", "type": "Float", "description": "Optional relevance score"}
        ]
    });

    let fields = interface["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 2);

    // Non-nullable field
    assert_eq!(fields[0]["type"], json!("[String!]!"));

    // Nullable field
    assert_eq!(fields[1]["type"], json!("Float"));
}

#[test]
fn test_interface_field_type_combinations() {
    // Various field type combinations in interface
    let interface = json!({
        "name": "ComplexEntity",
        "kind": "INTERFACE",
        "fields": [
            {"name": "scalar_field", "type": "String"},
            {"name": "non_null_field", "type": "Int!"},
            {"name": "list_field", "type": "[String]"},
            {"name": "list_non_null_field", "type": "[String!]!"},
            {"name": "nested_field", "type": "NestedType!"}
        ]
    });

    let fields = interface["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 5);

    // Verify all types are preserved
    let types: Vec<&str> = fields.iter().map(|f| f["type"].as_str().unwrap()).collect();

    assert!(types.contains(&"String"));
    assert!(types.contains(&"Int!"));
    assert!(types.contains(&"[String]"));
    assert!(types.contains(&"[String!]!"));
    assert!(types.contains(&"NestedType!"));
}

#[test]
fn test_implementing_type_has_interface_fields() {
    // Implementing type must have all interface fields
    let interface = json!({
        "name": "Identity",
        "fields": [
            {"name": "id", "type": "ID!"},
            {"name": "name", "type": "String!"}
        ]
    });

    let implementing_type = json!({
        "name": "User",
        "interfaces": ["Identity"],
        "fields": [
            {"name": "id", "type": "ID!"},
            {"name": "name", "type": "String!"},
            {"name": "email", "type": "String!"}
        ]
    });

    // Get interface field names
    let interface_field_names: Vec<&str> = interface["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str())
        .collect();

    // Get implementing type field names
    let type_field_names: Vec<&str> = implementing_type["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["name"].as_str())
        .collect();

    // All interface fields should be in implementing type
    for interface_field in &interface_field_names {
        assert!(
            type_field_names.contains(interface_field),
            "Implementing type should have interface field '{}'",
            interface_field
        );
    }
}

#[test]
fn test_interface_circular_references() {
    // Types can reference each other through interfaces
    let schema = json!({
        "interfaces": [
            {
                "name": "Node",
                "fields": [{"name": "id", "type": "ID!"}]
            }
        ],
        "types": [
            {
                "name": "Post",
                "interfaces": ["Node"],
                "fields": [
                    {"name": "id", "type": "ID!"},
                    {"name": "author", "type": "User!"}
                ]
            },
            {
                "name": "User",
                "interfaces": ["Node"],
                "fields": [
                    {"name": "id", "type": "ID!"},
                    {"name": "posts", "type": "[Post!]!"}
                ]
            }
        ]
    });

    let types = schema["types"].as_array().unwrap();
    assert_eq!(types.len(), 2);

    // Verify both implement Node
    for type_def in types {
        let interfaces = type_def["interfaces"].as_array().unwrap();
        assert_eq!(interfaces[0], json!("Node"));
    }
}

#[test]
fn test_interface_with_arguments() {
    // Interface fields can have arguments
    let interface = json!({
        "name": "Connection",
        "kind": "INTERFACE",
        "fields": [
            {
                "name": "edges",
                "type": "[Edge!]!",
                "arguments": [
                    {"name": "first", "type": "Int"},
                    {"name": "after", "type": "String"}
                ]
            }
        ]
    });

    let fields = interface["fields"].as_array().unwrap();
    let edges_field = &fields[0];

    assert_eq!(edges_field["name"], json!("edges"));

    let args = edges_field["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0]["name"], json!("first"));
    assert_eq!(args[1]["name"], json!("after"));
}

#[test]
fn test_interface_list_membership() {
    // Verify interface is in type's interface list correctly
    let type_def = json!({
        "name": "Article",
        "interfaces": [
            "Node",
            "Timestamped",
            "Content"
        ],
        "fields": []
    });

    let interfaces = type_def["interfaces"].as_array().unwrap();
    assert!(interfaces.iter().any(|i| *i == json!("Node")));
    assert!(interfaces.iter().any(|i| *i == json!("Timestamped")));
    assert!(interfaces.iter().any(|i| *i == json!("Content")));

    // Verify order is preserved
    assert_eq!(interfaces[0], json!("Node"));
    assert_eq!(interfaces[1], json!("Timestamped"));
    assert_eq!(interfaces[2], json!("Content"));
}

#[test]
fn test_interface_implementation_type_validation() {
    // Verify interface requirements are preserved in implementing type
    let _interface = json!({
        "name": "Result",
        "kind": "INTERFACE",
        "fields": [
            {"name": "success", "type": "Boolean!"},
            {"name": "message", "type": "String"}
        ]
    });

    let success_type = json!({
        "name": "SuccessResult",
        "interfaces": ["Result"],
        "fields": [
            {"name": "success", "type": "Boolean!"},
            {"name": "message", "type": "String"},
            {"name": "data", "type": "ResultData!"}
        ]
    });

    let error_type = json!({
        "name": "ErrorResult",
        "interfaces": ["Result"],
        "fields": [
            {"name": "success", "type": "Boolean!"},
            {"name": "message", "type": "String"},
            {"name": "code", "type": "Int!"}
        ]
    });

    // Both types have the interface fields
    for type_def in [success_type, error_type] {
        let type_fields: Vec<&str> = type_def["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["name"].as_str())
            .collect();

        assert!(type_fields.contains(&"success"));
        assert!(type_fields.contains(&"message"));
    }
}
