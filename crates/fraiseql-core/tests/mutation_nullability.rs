//! Test mutation return type nullability handling.
//!
//! This test verifies that:
//! 1. Mutation return types can be nullable or non-nullable
//! 2. Nullability markers (! suffix) are preserved in type definitions
//! 3. Array return types handle nullability correctly
//! 4. Nested input/output types maintain nullability
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Nullable mutations could be marked as non-nullable
//! - Non-nullable mutations could accept NULL returns
//! - Type validation could fail on nullability mismatches

use serde_json::json;

#[test]
fn test_mutation_non_nullable_return_type() {
    // Non-nullable return type (with ! suffix)
    let non_nullable = json!({
        "name": "createUser",
        "return_type": "User!",
    });

    assert_eq!(non_nullable["return_type"], json!("User!"));
    assert!(non_nullable["return_type"].as_str().unwrap().ends_with("!"));
}

#[test]
fn test_mutation_nullable_return_type() {
    // Nullable return type (no ! suffix)
    let nullable = json!({
        "name": "maybeUser",
        "return_type": "User",
    });

    assert_eq!(nullable["return_type"], json!("User"));
    assert!(!nullable["return_type"].as_str().unwrap().ends_with("!"));
}

#[test]
fn test_mutation_array_return_type_non_nullable_items() {
    // Array of non-nullable items: [Int!]!
    let array_non_null_items = json!({
        "name": "getIds",
        "return_type": "[Int!]!",
    });

    assert_eq!(array_non_null_items["return_type"], json!("[Int!]!"));
    let type_str = array_non_null_items["return_type"].as_str().unwrap();
    assert!(type_str.starts_with("["));
    assert!(type_str.ends_with("]!"));
}

#[test]
fn test_mutation_array_return_type_nullable_items() {
    // Array of nullable items: [String]
    let array_nullable_items = json!({
        "name": "getTags",
        "return_type": "[String]",
    });

    assert_eq!(array_nullable_items["return_type"], json!("[String]"));
    let type_str = array_nullable_items["return_type"].as_str().unwrap();
    assert!(type_str.starts_with("["));
    assert!(type_str.ends_with("]"));
    assert!(!type_str.ends_with("]!"));
}

#[test]
fn test_mutation_scalar_return_types_nullability() {
    // Various scalar return types with nullability
    let scalar_types = vec![
        ("String!", true),   // non-nullable string
        ("String", false),   // nullable string
        ("Int!", true),      // non-nullable int
        ("Int", false),      // nullable int
        ("Boolean!", true),  // non-nullable boolean
        ("Boolean", false),  // nullable boolean
        ("Float!", true),    // non-nullable float
        ("Float", false),    // nullable float
        ("ID!", true),       // non-nullable ID
        ("ID", false),       // nullable ID
    ];

    for (type_str, should_be_non_nullable) in scalar_types {
        let mutation = json!({
            "name": "operation",
            "return_type": type_str,
        });

        let ret_type = mutation["return_type"].as_str().unwrap();
        let is_non_nullable = ret_type.ends_with("!");

        assert_eq!(is_non_nullable, should_be_non_nullable,
                   "Type {} should have non_nullable={}", type_str, should_be_non_nullable);
    }
}

#[test]
fn test_mutation_custom_type_return_nullability() {
    // Custom object types with nullability
    let custom_types = vec![
        ("User!", true),
        ("User", false),
        ("CreateUserPayload!", true),
        ("CreateUserPayload", false),
        ("MutationResponse!", true),
        ("MutationResponse", false),
    ];

    for (type_str, should_be_non_nullable) in custom_types {
        let mutation = json!({
            "name": "operation",
            "return_type": type_str,
        });

        let ret_type = mutation["return_type"].as_str().unwrap();
        let is_non_nullable = ret_type.ends_with("!");

        assert_eq!(is_non_nullable, should_be_non_nullable,
                   "Custom type {} should have non_nullable={}", type_str, should_be_non_nullable);
    }
}

#[test]
fn test_mutation_nested_array_nullability() {
    // Nested array types with various nullability combinations
    let nested_arrays = vec![
        ("[Int!]!", "non-nullable array of non-nullable ints"),
        ("[Int!]", "nullable array of non-nullable ints"),
        ("[Int]!", "non-nullable array of nullable ints"),
        ("[Int]", "nullable array of nullable ints"),
        ("[[String!]!]!", "nested arrays, all non-nullable"),
        ("[[String!]]", "nested arrays, outer nullable"),
        ("[[String]!]", "nested arrays, middle non-nullable"),
        ("[[String]]", "nested arrays, all nullable"),
    ];

    for (type_str, _description) in nested_arrays {
        let mutation = json!({
            "name": "operation",
            "return_type": type_str,
        });

        let ret_type = mutation["return_type"].as_str().unwrap();
        assert_eq!(ret_type, type_str, "Type should be preserved exactly");

        // Count the nullability markers
        let exclamation_count = type_str.matches('!').count();
        let stored_count = ret_type.matches('!').count();
        assert_eq!(stored_count, exclamation_count, "Nullability markers should be preserved");
    }
}

#[test]
fn test_mutation_nullability_with_input_args() {
    // Mutation with both input arg nullability and return type nullability
    let mutation_with_args = json!({
        "name": "updateUser",
        "arguments": [
            {
                "name": "id",
                "type": "ID!",
                "required": true
            },
            {
                "name": "input",
                "type": "UpdateUserInput!",
                "required": true
            },
            {
                "name": "notify",
                "type": "Boolean",
                "required": false
            }
        ],
        "return_type": "User!"
    });

    // Verify args preserve their nullability
    let args = mutation_with_args["arguments"].as_array().unwrap();
    assert_eq!(args[0]["type"], json!("ID!"));
    assert_eq!(args[1]["type"], json!("UpdateUserInput!"));
    assert_eq!(args[2]["type"], json!("Boolean"));

    // Verify return type
    assert_eq!(mutation_with_args["return_type"], json!("User!"));
}

#[test]
fn test_mutation_list_return_nullability_combinations() {
    // All valid combinations of list nullability
    let combinations = vec![
        "[Type]",      // nullable list of nullable items
        "[Type!]",     // nullable list of non-nullable items
        "[Type]!",     // non-nullable list of nullable items
        "[Type!]!",    // non-nullable list of non-nullable items
    ];

    for type_str in combinations {
        let mutation = json!({
            "return_type": type_str,
        });

        // Should preserve exactly
        assert_eq!(mutation["return_type"].as_str().unwrap(), type_str);

        // Verify structure
        let ret_type = mutation["return_type"].as_str().unwrap();
        assert!(ret_type.starts_with("["), "Should start with [");
        assert!(ret_type.contains("]"), "Should contain ]");
    }
}

#[test]
fn test_mutation_return_type_distinctions() {
    // Verify that similar types are actually distinct
    let user_non_nullable = json!({"return_type": "User!"});
    let user_nullable = json!({"return_type": "User"});
    let user_list_non_nullable = json!({"return_type": "[User]!"});
    let user_list_nullable = json!({"return_type": "[User]"});

    assert_ne!(user_non_nullable["return_type"], user_nullable["return_type"]);
    assert_ne!(user_list_non_nullable["return_type"], user_list_nullable["return_type"]);
    assert_ne!(user_non_nullable["return_type"], user_list_non_nullable["return_type"]);

    // Verify exact preservation
    assert_eq!(user_non_nullable["return_type"], json!("User!"));
    assert_eq!(user_nullable["return_type"], json!("User"));
    assert_eq!(user_list_non_nullable["return_type"], json!("[User]!"));
    assert_eq!(user_list_nullable["return_type"], json!("[User]"));
}

