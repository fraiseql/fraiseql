//! Integration tests for custom scalar compilation and execution.
//!
//! Tests the complete flow: SDK schema → Compiler → Runtime validation

#![allow(clippy::pedantic)]

use fraiseql_cli::schema::{IntermediateScalar, IntermediateSchema, SchemaConverter};
use fraiseql_core::validation::ValidationRule;

#[test]
#[allow(clippy::too_many_lines)] // Reason: integration test exercises full custom scalar pipeline in one flow
fn test_compile_schema_with_single_custom_scalar() {
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    Some(vec![IntermediateScalar {
            name:             "Email".to_string(),
            description:      Some("Valid email address".to_string()),
            specified_by_url: Some("https://tools.ietf.org/html/rfc5322".to_string()),
            validation_rules: vec![ValidationRule::Pattern {
                pattern: r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$".to_string(),
                message: Some("Invalid email format".to_string()),
            }],
            base_type:        Some("String".to_string()),
        }]),
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    // Verify custom scalar was registered
    assert!(compiled.custom_scalars.exists("Email"));

    // Retrieve and verify the scalar definition
    let scalar = compiled.custom_scalars.get("Email").expect("Failed to get scalar");
    assert_eq!(scalar.name, "Email");
    assert_eq!(scalar.description, Some("Valid email address".to_string()));
}

#[test]
fn test_compile_schema_with_multiple_custom_scalars() {
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    Some(vec![
            IntermediateScalar {
                name:             "Email".to_string(),
                description:      None,
                specified_by_url: None,
                validation_rules: vec![],
                base_type:        Some("String".to_string()),
            },
            IntermediateScalar {
                name:             "Phone".to_string(),
                description:      None,
                specified_by_url: None,
                validation_rules: vec![ValidationRule::Pattern {
                    pattern: r"^\+\d{10,14}$".to_string(),
                    message: Some("Invalid phone format".to_string()),
                }],
                base_type:        Some("String".to_string()),
            },
        ]),
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    // Verify both scalars are registered
    assert!(compiled.custom_scalars.exists("Email"));
    assert!(compiled.custom_scalars.exists("Phone"));

    // Get all scalars and verify count
    let all_scalars = compiled.custom_scalars.list_all();
    assert_eq!(all_scalars.len(), 2);
}

#[test]
fn test_custom_scalar_with_multiple_validation_rules() {
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    Some(vec![IntermediateScalar {
            name:             "Username".to_string(),
            description:      Some("Valid username".to_string()),
            specified_by_url: None,
            validation_rules: vec![
                ValidationRule::Length {
                    min: Some(3),
                    max: Some(20),
                },
                ValidationRule::Pattern {
                    pattern: r"^[a-zA-Z0-9_]+$".to_string(),
                    message: Some("Only alphanumeric and underscore allowed".to_string()),
                },
            ],
            base_type:        Some("String".to_string()),
        }]),
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    let scalar = compiled.custom_scalars.get("Username").expect("Failed to get scalar");
    assert_eq!(scalar.validation_rules.len(), 2);
}

#[test]
fn test_custom_scalar_preserves_all_metadata() {
    let url = "https://example.com/spec".to_string();
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    Some(vec![IntermediateScalar {
            name:             "CustomType".to_string(),
            description:      Some("A custom type".to_string()),
            specified_by_url: Some(url.clone()),
            validation_rules: vec![],
            base_type:        Some("Int".to_string()),
        }]),
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    let scalar = compiled.custom_scalars.get("CustomType").expect("Failed to get scalar");
    assert_eq!(scalar.name, "CustomType");
    assert_eq!(scalar.description, Some("A custom type".to_string()));
    assert_eq!(scalar.specified_by_url, Some(url));
    assert_eq!(scalar.base_type, Some("Int".to_string()));
}

#[test]
fn test_empty_custom_scalars_list() {
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    None, // No custom scalars
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    // Should have empty registry, not error
    let all_scalars = compiled.custom_scalars.list_all();
    assert!(all_scalars.is_empty());
}

#[test]
fn test_custom_scalar_with_no_validation_rules() {
    let schema = IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    Some(vec![IntermediateScalar {
            name:             "SimpleScalar".to_string(),
            description:      None,
            specified_by_url: None,
            validation_rules: vec![], // No rules
            base_type:        None,
        }]),
        security:          None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    let scalar = compiled.custom_scalars.get("SimpleScalar").expect("Failed to get scalar");
    assert!(scalar.validation_rules.is_empty());
}
