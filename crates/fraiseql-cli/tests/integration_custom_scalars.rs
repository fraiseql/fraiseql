//! Integration tests for custom scalar compilation.
//!
//! Tests the flow: SDK schema → compiler → compiled artifact.
//!
//! **Not** "→ runtime validation", which the module doc used to claim. There is no such
//! leg: `CompiledSchema.custom_scalars` is `#[serde(skip)]`, so the registry these tests
//! inspect is dropped when the schema is written to `schema.compiled.json`, and nothing
//! in `fraiseql-server` reads scalar rules back. A `ValidationRule` declared on a scalar
//! was never enforced anywhere, and these tests asserted it into the registry and stopped
//! one step short of the serialization that discards it.
//!
//! The compiler now refuses a scalar that declares `validation_rules` rather than accept a
//! constraint it cannot honour, so the cases below assert the refusal and the cases that
//! declare no rules assert that the *declaration* still works — which is the half that
//! does something, since it makes the name known to the compiler.

#![allow(clippy::pedantic)]

use fraiseql_cli::schema::{IntermediateScalar, IntermediateSchema, SchemaConverter};
use fraiseql_core::{
    schema::NamingConvention,
    validation::{CompiledPattern, ValidationRule},
};

#[test]
#[allow(clippy::too_many_lines)] // Reason: integration test exercises full custom scalar pipeline in one flow
fn test_compile_schema_with_single_custom_scalar() {
    let schema = IntermediateSchema {
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       Some(vec![IntermediateScalar {
            name:             "Email".to_string(),
            description:      Some("Valid email address".to_string()),
            specified_by_url: Some("https://tools.ietf.org/html/rfc5322".to_string()),
            validation_rules: vec![ValidationRule::Pattern {
                pattern: CompiledPattern::new(r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$")
                    .expect("valid regex"),
                message: Some("Invalid email format".to_string()),
            }],
            base_type:        Some("String".to_string()),
        }]),
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let error = SchemaConverter::convert(schema)
        .expect_err("a scalar declaring a pattern no artifact carries must be refused");
    let message = error.to_string();

    assert!(message.contains("Email"), "the diagnostic must name the scalar: {message}");
    assert!(
        message.contains("validation_rules"),
        "the diagnostic must name the offending key: {message}"
    );
}

#[test]
fn test_compile_schema_with_multiple_custom_scalars() {
    let schema = IntermediateSchema {
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       Some(vec![
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
                    pattern: CompiledPattern::new(r"^\+\d{10,14}$").expect("valid regex"),
                    message: Some("Invalid phone format".to_string()),
                }],
                base_type:        Some("String".to_string()),
            },
        ]),
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    // `Email` declares no rules and `Phone` declares a pattern. One unenforceable
    // declaration is enough to refuse the schema — the alternative is a compile that
    // succeeds while silently honouring one scalar's constraint and not the other's.
    let error = SchemaConverter::convert(schema)
        .expect_err("a schema containing a rule-declaring scalar must be refused");

    assert!(
        error.to_string().contains("Phone"),
        "the diagnostic must name the offending scalar, not the clean one: {error}"
    );
}

#[test]
fn test_custom_scalar_with_multiple_validation_rules() {
    let schema = IntermediateSchema {
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       Some(vec![IntermediateScalar {
            name:             "Username".to_string(),
            description:      Some("Valid username".to_string()),
            specified_by_url: None,
            validation_rules: vec![
                ValidationRule::Length {
                    min: Some(3),
                    max: Some(20),
                },
                ValidationRule::Pattern {
                    pattern: CompiledPattern::new(r"^[a-zA-Z0-9_]+$").expect("valid regex"),
                    message: Some("Only alphanumeric and underscore allowed".to_string()),
                },
            ],
            base_type:        Some("String".to_string()),
        }]),
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    // Two rules are refused for the same reason one is: neither reaches a runtime.
    let error = SchemaConverter::convert(schema)
        .expect_err("a scalar declaring rules no artifact carries must be refused");

    assert!(
        error.to_string().contains("Username"),
        "the diagnostic must name the scalar: {error}"
    );
}

#[test]
fn test_custom_scalar_preserves_all_metadata() {
    let url = "https://example.com/spec".to_string();
    let schema = IntermediateSchema {
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       Some(vec![IntermediateScalar {
            name:             "CustomType".to_string(),
            description:      Some("A custom type".to_string()),
            specified_by_url: Some(url.clone()),
            validation_rules: vec![],
            base_type:        Some("Int".to_string()),
        }]),
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       None, // No custom scalars
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    // Should have empty registry, not error
    let all_scalars = compiled.custom_scalars.list_all();
    assert!(all_scalars.is_empty());
}

#[test]
fn test_custom_scalar_with_no_validation_rules() {
    let schema = IntermediateSchema {
        grpc_config:       None,
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

        sources:              None,
        custom_scalars:       Some(vec![IntermediateScalar {
            name:             "SimpleScalar".to_string(),
            description:      None,
            specified_by_url: None,
            validation_rules: vec![], // No rules
            base_type:        None,
        }]),
        security:             None,
        auth:                 None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        inject_defaults:      None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let compiled = SchemaConverter::convert(schema).expect("Failed to convert schema");

    let scalar = compiled.custom_scalars.get("SimpleScalar").expect("Failed to get scalar");
    assert!(scalar.validation_rules.is_empty());
}
