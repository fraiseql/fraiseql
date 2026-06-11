#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use fraiseql_core::schema::NamingConvention;
use indexmap::IndexMap;

use crate::schema::{
    intermediate::{IntermediateQuery, IntermediateSchema, IntermediateType},
    validator::schema_validator::SchemaValidator,
};

#[test]
fn test_validate_empty_schema() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid());
}

#[test]
fn test_detect_unknown_return_type() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "UnknownType".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("users".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert_eq!(report.error_count(), 1);
    assert!(report.errors[0].message.contains("unknown type 'UnknownType'"));
}

#[test]
fn test_detect_duplicate_query_names() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "User".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![
            IntermediateQuery {
                name:              "users".to_string(),
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("users".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
            },
            IntermediateQuery {
                name:              "users".to_string(), // Duplicate!
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("users".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
            },
        ],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("Duplicate query name")));
}

#[test]
fn test_warning_for_query_without_sql_source() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "User".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        None, // Missing SQL source
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid()); // Still valid, just a warning
    assert_eq!(report.warning_count(), 1);
    assert!(report.errors[0].message.contains("no sql_source"));
}

#[test]
fn test_valid_observer() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "Order".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({
                "type": "webhook",
                "url": "https://example.com/orders"
            })],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid(), "Valid observer should pass validation");
    assert_eq!(report.error_count(), 0);
}

#[test]
fn test_observer_with_unknown_entity() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "UnknownEntity".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("unknown entity")));
}

#[test]
fn test_observer_with_invalid_event() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "Order".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INVALID_EVENT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid event")));
}

#[test]
fn test_observer_with_invalid_action_type() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "Order".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "invalid_action"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid type")));
}

#[test]
fn test_observer_with_invalid_retry_config() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "Order".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "invalid_strategy".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid backoff_strategy")));
}

#[test]
fn test_query_injection_in_sql_source_rejected() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "User".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("v_user\"; DROP TABLE users; --".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("valid SQL identifier")));
}

#[test]
fn test_query_schema_qualified_sql_source_passes() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                "User".to_string(),
            fields:              vec![],
            description:         None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            subscribable_tables: None,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("public.v_user".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    // Should only have the usual "no sql_source" warnings for other queries, not errors
    assert!(report.is_valid(), "Schema-qualified sql_source should be valid");
}

// ── schema_validator internal tests ────────────────────────────────────────

mod schema_validator_tests {
    use crate::schema::{
        intermediate::{
            IntermediateSchema,
            operations::{IntermediateArgument, IntermediateMutation, IntermediateQuery},
            types::{IntermediateField, IntermediateType},
        },
        validator::{
            ErrorSeverity,
            schema_validator::{SchemaValidator, extract_base_type},
        },
    };

    fn field(name: &str, ty: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     ty.to_string(),
            nullable:       false,
            description:    None,
            directives:     None,
            requires_scope: None,
            on_deny:        None,
            authorize:      None,
            hierarchy:      None,
        }
    }

    fn arg(name: &str, ty: &str) -> IntermediateArgument {
        IntermediateArgument {
            name:       name.to_string(),
            arg_type:   ty.to_string(),
            nullable:   false,
            default:    None,
            deprecated: None,
        }
    }

    fn minimal_schema() -> IntermediateSchema {
        let mut schema = IntermediateSchema::default();
        schema.types.push(IntermediateType {
            name: "Item".to_string(),
            fields: vec![field("id", "UUID")],
            ..Default::default()
        });
        schema
    }

    // ── extract_base_type unit tests ────────────────────────────────

    #[test]
    fn extract_base_type_strips_non_null_suffix() {
        assert_eq!(extract_base_type("Item!"), "Item");
        assert_eq!(extract_base_type("String!"), "String");
        assert_eq!(extract_base_type("Json!"), "Json");
    }

    #[test]
    fn extract_base_type_strips_list_brackets() {
        assert_eq!(extract_base_type("[User]"), "User");
        assert_eq!(extract_base_type("[User!]!"), "User");
        assert_eq!(extract_base_type("[String!]"), "String");
    }

    #[test]
    fn extract_base_type_passthrough() {
        assert_eq!(extract_base_type("String"), "String");
        assert_eq!(extract_base_type("Item"), "Item");
    }

    // ── Issue #151: ! suffix accepted in queries ────────────────────

    #[test]
    fn query_with_bang_suffixed_return_type_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "Item!".to_string(),
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "Item! should resolve to Item: {errors:?}");
    }

    #[test]
    fn query_arg_with_bang_suffix_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "Item".to_string(),
            arguments: vec![arg("id", "String!")],
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "String! should resolve to String: {errors:?}");
    }

    #[test]
    fn mutation_with_bang_suffixed_types_is_valid() {
        let mut schema = minimal_schema();
        schema.mutations.push(IntermediateMutation {
            name: "createItem".to_string(),
            return_type: "Item!".to_string(),
            arguments: vec![arg("name", "String!")],
            sql_source: Some("fn_create_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "Item! and String! should be valid: {errors:?}");
    }

    #[test]
    fn list_type_with_bang_is_valid() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "items".to_string(),
            return_type: "[Item!]!".to_string(),
            returns_list: true,
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(errors.is_empty(), "[Item!]! should resolve to Item: {errors:?}");
    }

    // ── Truly unknown types are still rejected ──────────────────────

    #[test]
    fn truly_unknown_type_still_rejected() {
        let mut schema = minimal_schema();
        schema.queries.push(IntermediateQuery {
            name: "item".to_string(),
            return_type: "NonExistent!".to_string(),
            sql_source: Some("v_item".to_string()),
            ..Default::default()
        });

        let report = SchemaValidator::validate(&schema).unwrap();
        let errors: Vec<_> =
            report.errors.iter().filter(|e| e.severity == ErrorSeverity::Error).collect();
        assert!(!errors.is_empty(), "NonExistent should still be rejected");
        assert!(
            errors[0].message.contains("NonExistent"),
            "error should name the base type, not 'NonExistent!': {}",
            errors[0].message
        );
        // Error message should show the base type, not the raw "NonExistent!"
        assert!(
            !errors[0].message.contains("NonExistent!"),
            "error should strip ! from type name: {}",
            errors[0].message
        );
    }
}

// ── sql_identifier tests ────────────────────────────────────────────────────

mod sql_identifier_tests {
    use crate::schema::validator::sql_identifier::validate_sql_identifier;

    #[test]
    fn test_valid_simple_identifier() {
        validate_sql_identifier("v_user", "sql_source", "Query.users")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_valid_schema_qualified_identifier() {
        validate_sql_identifier("public.v_user", "sql_source", "Query.users")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_empty_identifier_rejected() {
        let err = validate_sql_identifier("", "sql_source", "Query.users").unwrap_err();
        assert!(err.message.contains("must not be empty"));
    }

    #[test]
    fn test_identifier_exactly_63_bytes_accepted() {
        let ident = "a".repeat(63);
        validate_sql_identifier(&ident, "sql_source", "Query.x")
            .unwrap_or_else(|e| panic!("expected Ok: {e:?}"));
    }

    #[test]
    fn test_identifier_64_bytes_rejected() {
        let ident = "a".repeat(64);
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
        assert!(err.message.contains("63 bytes"));
    }

    #[test]
    fn test_schema_segment_64_bytes_rejected() {
        let schema_part = "a".repeat(64);
        let ident = format!("{schema_part}.v_user");
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
    }

    #[test]
    fn test_name_segment_64_bytes_rejected() {
        let name_part = "a".repeat(64);
        let ident = format!("public.{name_part}");
        let err = validate_sql_identifier(&ident, "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("exceeds the PostgreSQL maximum"));
    }

    #[test]
    fn test_valid_three_part_identifier() {
        assert!(validate_sql_identifier("catalog.schema.table", "sql_source", "Query.x").is_ok());
    }

    #[test]
    fn test_four_part_identifier_rejected() {
        let err = validate_sql_identifier("a.b.c.d", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_leading_dot_rejected() {
        let err = validate_sql_identifier(".foo", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_trailing_dot_rejected() {
        let err = validate_sql_identifier("foo.", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_double_dot_rejected() {
        let err = validate_sql_identifier("foo..bar", "sql_source", "Query.x").unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }

    #[test]
    fn test_injection_attempt_rejected() {
        let err = validate_sql_identifier("v_user; DROP TABLE users", "sql_source", "Query.users")
            .unwrap_err();
        assert!(err.message.contains("is not a valid SQL identifier"));
    }
}
