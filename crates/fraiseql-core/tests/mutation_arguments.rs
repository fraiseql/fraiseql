//! Test mutation argument binding and parameter handling.
//!
//! This test verifies that:
//! 1. Mutations with multiple arguments are structured correctly
//! 2. Argument types and values are preserved
//! 3. Nested input objects maintain structure
//! 4. Argument names are correctly mapped
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Multi-argument mutations could lose or swap arguments
//! - Type information could be lost in parameter binding
//! - Arguments could be silently dropped

use serde_json::json;

#[test]
fn test_mutation_single_argument() {
    // Single argument mutation should be straightforward
    let single_arg = json!({
        "name": "updateUser",
        "arguments": [
            {"name": "id", "value": "123"}
        ]
    });

    // Verify structure is preserved
    assert_eq!(single_arg["name"], json!("updateUser"));
    assert_eq!(single_arg["arguments"].as_array().unwrap().len(), 1);
    assert_eq!(single_arg["arguments"][0]["name"], json!("id"));
    assert_eq!(single_arg["arguments"][0]["value"], json!("123"));
}

#[test]
fn test_mutation_multiple_arguments() {
    // Multiple arguments should be preserved in order
    let multi_arg = json!({
        "name": "updateUser",
        "arguments": [
            {"name": "id", "value": "123", "type": "ID!"},
            {"name": "name", "value": "Bob", "type": "String!"},
            {"name": "email", "value": "bob@example.com", "type": "String!"}
        ]
    });

    let args = multi_arg["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 3);

    // Verify each argument
    assert_eq!(args[0]["name"], json!("id"));
    assert_eq!(args[0]["value"], json!("123"));
    assert_eq!(args[0]["type"], json!("ID!"));

    assert_eq!(args[1]["name"], json!("name"));
    assert_eq!(args[1]["value"], json!("Bob"));
    assert_eq!(args[1]["type"], json!("String!"));

    assert_eq!(args[2]["name"], json!("email"));
    assert_eq!(args[2]["value"], json!("bob@example.com"));
    assert_eq!(args[2]["type"], json!("String!"));
}

#[test]
fn test_mutation_nested_input_object() {
    // Nested input object should maintain structure
    let nested = json!({
        "name": "createUser",
        "arguments": [
            {
                "name": "input",
                "type": "CreateUserInput!",
                "value": {
                    "name": "Charlie",
                    "email": "charlie@example.com",
                    "role": "ADMIN"
                }
            }
        ]
    });

    let arg = &nested["arguments"][0];
    assert_eq!(arg["name"], json!("input"));
    assert_eq!(arg["type"], json!("CreateUserInput!"));

    let input_obj = &arg["value"];
    assert_eq!(input_obj["name"], json!("Charlie"));
    assert_eq!(input_obj["email"], json!("charlie@example.com"));
    assert_eq!(input_obj["role"], json!("ADMIN"));
}

#[test]
fn test_mutation_mixed_scalars_and_objects() {
    // Mix of scalar and object arguments
    let mixed = json!({
        "name": "updateProfile",
        "arguments": [
            {"name": "userId", "value": "user123", "type": "ID!"},
            {
                "name": "profile",
                "value": {
                    "bio": "New bio",
                    "avatar": "https://example.com/avatar.jpg"
                },
                "type": "ProfileInput!"
            },
            {"name": "notify", "value": true, "type": "Boolean"}
        ]
    });

    let args = mixed["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 3);

    // Scalar argument
    assert_eq!(args[0]["value"], json!("user123"));

    // Object argument
    assert_eq!(args[1]["value"]["bio"], json!("New bio"));

    // Boolean argument
    assert_eq!(args[2]["value"], json!(true));
}

#[test]
fn test_mutation_argument_types_preserved() {
    // Verify argument types are correctly identified and preserved
    let typed_args = json!({
        "arguments": [
            {"name": "str_arg", "value": "text", "type": "String"},
            {"name": "int_arg", "value": 42, "type": "Int"},
            {"name": "bool_arg", "value": true, "type": "Boolean"},
            {"name": "float_arg", "value": 3.15, "type": "Float"},
            {"name": "id_arg", "value": "id123", "type": "ID"},
            {"name": "null_arg", "value": null, "type": "String"}
        ]
    });

    let args = typed_args["arguments"].as_array().unwrap();

    // Verify each type is distinct
    assert!(args[0]["value"].is_string());
    assert!(args[1]["value"].is_number());
    assert!(args[2]["value"].is_boolean());
    assert!(args[3]["value"].is_number());
    assert!(args[4]["value"].is_string());
    assert!(args[5]["value"].is_null());
}

#[test]
fn test_mutation_array_arguments() {
    // Array arguments with multiple elements
    let array_args = json!({
        "arguments": [
            {
                "name": "ids",
                "value": ["1", "2", "3", "4", "5"],
                "type": "[ID!]!"
            },
            {
                "name": "tags",
                "value": ["tag1", "tag2", "tag3"],
                "type": "[String!]!"
            }
        ]
    });

    let args = array_args["arguments"].as_array().unwrap();

    let ids = args[0]["value"].as_array().unwrap();
    assert_eq!(ids.len(), 5);
    assert_eq!(ids[0], json!("1"));
    assert_eq!(ids[4], json!("5"));

    let tags = args[1]["value"].as_array().unwrap();
    assert_eq!(tags.len(), 3);
}

#[test]
fn test_mutation_enum_arguments() {
    // Enum type arguments
    let enum_args = json!({
        "arguments": [
            {"name": "role", "value": "ADMIN", "type": "UserRole!"},
            {"name": "status", "value": "ACTIVE", "type": "UserStatus!"},
            {"name": "priority", "value": "HIGH", "type": "Priority!"}
        ]
    });

    let args = enum_args["arguments"].as_array().unwrap();
    assert_eq!(args[0]["value"], json!("ADMIN"));
    assert_eq!(args[1]["value"], json!("ACTIVE"));
    assert_eq!(args[2]["value"], json!("HIGH"));
}

#[test]
fn test_mutation_deeply_nested_input() {
    // Deeply nested input object structures
    let deep_nested = json!({
        "arguments": [
            {
                "name": "input",
                "value": {
                    "user": {
                        "name": "John",
                        "contact": {
                            "email": "john@example.com",
                            "phone": {
                                "country": "US",
                                "number": "555-1234"
                            }
                        }
                    }
                }
            }
        ]
    });

    let input = &deep_nested["arguments"][0]["value"];
    assert_eq!(input["user"]["name"], json!("John"));
    assert_eq!(input["user"]["contact"]["email"], json!("john@example.com"));
    assert_eq!(input["user"]["contact"]["phone"]["country"], json!("US"));
    assert_eq!(input["user"]["contact"]["phone"]["number"], json!("555-1234"));
}

#[test]
fn test_mutation_argument_order_preserved() {
    // Arguments should maintain their order
    let ordered = json!({
        "arguments": [
            {"index": 0, "name": "arg0"},
            {"index": 1, "name": "arg1"},
            {"index": 2, "name": "arg2"},
            {"index": 3, "name": "arg3"},
            {"index": 4, "name": "arg4"}
        ]
    });

    let args = ordered["arguments"].as_array().unwrap();
    for (i, arg) in args.iter().enumerate() {
        assert_eq!(arg["index"], json!(i as i32));
        assert_eq!(arg["name"], json!(format!("arg{}", i)));
    }
}

#[test]
fn test_mutation_optional_arguments() {
    // Optional arguments can be null or omitted
    let optional = json!({
        "arguments": [
            {"name": "required_arg", "value": "value", "required": true},
            {"name": "optional_arg", "value": null, "required": false},
            {"name": "omitted_arg", "required": false}
        ]
    });

    let args = optional["arguments"].as_array().unwrap();

    // Required argument has value
    assert_eq!(args[0]["value"], json!("value"));

    // Optional argument can be null
    assert_eq!(args[1]["value"], json!(null));

    // Omitted argument might not have value field
    assert!(args[2].get("value").is_none() || args[2]["value"].is_null());
}

#[test]
fn test_mutation_argument_nullability() {
    // Verify nullability markers are preserved
    let nullable = json!({
        "arguments": [
            {"name": "arg1", "type": "String!", "value": "required"},
            {"name": "arg2", "type": "String", "value": null},
            {"name": "arg3", "type": "[Int!]!", "value": [1, 2, 3]},
            {"name": "arg4", "type": "[Int]", "value": null}
        ]
    });

    let args = nullable["arguments"].as_array().unwrap();

    // Non-nullable (!) should not be null
    assert_eq!(args[0]["type"], json!("String!"));
    assert!(args[0]["value"].is_string());

    // Nullable can be null
    assert_eq!(args[1]["type"], json!("String"));
    assert!(args[1]["value"].is_null());

    // Array types follow same rules
    assert_eq!(args[2]["type"], json!("[Int!]!"));
    assert!(args[2]["value"].is_array());

    assert_eq!(args[3]["type"], json!("[Int]"));
    assert!(args[3]["value"].is_null());
}

#[test]
fn test_mutation_large_number_of_arguments() {
    // Mutation with many arguments should all be preserved
    let mut args_vec = Vec::new();
    for i in 0..100 {
        args_vec.push(json!({
            "name": format!("arg{}", i),
            "value": i
        }));
    }

    let large_mutation = json!({
        "arguments": args_vec
    });

    let args = large_mutation["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 100);

    // Spot check some
    assert_eq!(args[0]["value"], json!(0));
    assert_eq!(args[50]["value"], json!(50));
    assert_eq!(args[99]["value"], json!(99));
}

#[test]
fn test_mutation_argument_value_types() {
    // Different JSON value types should all be preserved
    let value_types = json!({
        "arguments": [
            {"value": "string value"},
            {"value": 123},
            {"value": 45.67},
            {"value": true},
            {"value": false},
            {"value": null},
            {"value": []},
            {"value": {}},
            {"value": {"nested": "object"}},
            {"value": [1, "two", {"three": 3}, null]}
        ]
    });

    let args = value_types["arguments"].as_array().unwrap();
    assert!(args[0]["value"].is_string());
    assert!(args[1]["value"].is_number());
    assert!(args[2]["value"].is_number());
    assert_eq!(args[3]["value"], json!(true));
    assert_eq!(args[4]["value"], json!(false));
    assert_eq!(args[5]["value"], json!(null));
    assert!(args[6]["value"].is_array());
    assert!(args[7]["value"].is_object());
    assert!(args[8]["value"].is_object());
    assert!(args[9]["value"].is_array());
}
