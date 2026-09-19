#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use indexmap::IndexMap;

use crate::schema::{
    intermediate::{IntermediateQuery, IntermediateSchema, IntermediateType},
    validator::schema_validator::SchemaValidator,
};

#[test]
fn test_validate_empty_schema() {
    let schema = IntermediateSchema::default();

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid());
}

#[test]
fn test_detect_unknown_return_type() {
    let schema = IntermediateSchema {
        queries: vec![IntermediateQuery {
            function: None,

            requires_actor:    Vec::new(),
            count:             false,
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
            read_routing:      fraiseql_core::db::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
            rest:              None,
            rest_stream:       false,
            pagination_order:  None,
        }],
        ..Default::default()
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert_eq!(report.error_count(), 1);
    assert!(report.errors[0].message.contains("unknown type 'UnknownType'"));
}

#[test]
fn test_detect_duplicate_query_names() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        queries: vec![
            IntermediateQuery {
                function: None,

                requires_actor:    Vec::new(),
                count:             false,
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
                read_routing:      fraiseql_core::db::types::ReadRouting::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
                rest:              None,
                rest_stream:       false,
                pagination_order:  None,
            },
            IntermediateQuery {
                function: None,

                requires_actor:    Vec::new(),
                count:             false,
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
                read_routing:      fraiseql_core::db::types::ReadRouting::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
                rest:              None,
                rest_stream:       false,
                pagination_order:  None,
            },
        ],
        ..Default::default()
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("Duplicate query name")));
}

#[test]
fn test_warning_for_query_without_sql_source() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        queries: vec![IntermediateQuery {
            function: None,

            requires_actor:    Vec::new(),
            count:             false,
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
            read_routing:      fraiseql_core::db::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
            rest:              None,
            rest_stream:       false,
            pagination_order:  None,
        }],
        ..Default::default()
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
        types: vec![IntermediateType {
            name:                   "Order".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        observers: Some(vec![IntermediateObserver {
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
        ..Default::default()
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
        observers: Some(vec![IntermediateObserver {
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
        ..Default::default()
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
        types: vec![IntermediateType {
            name:                   "Order".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        observers: Some(vec![IntermediateObserver {
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
        ..Default::default()
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
        types: vec![IntermediateType {
            name:                   "Order".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        observers: Some(vec![IntermediateObserver {
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
        ..Default::default()
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
        types: vec![IntermediateType {
            name:                   "Order".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        observers: Some(vec![IntermediateObserver {
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
        ..Default::default()
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid backoff_strategy")));
}

#[test]
fn test_query_injection_in_sql_source_rejected() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        queries: vec![IntermediateQuery {
            function: None,

            requires_actor:    Vec::new(),
            count:             false,
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
            read_routing:      fraiseql_core::db::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
            rest:              None,
            rest_stream:       false,
            pagination_order:  None,
        }],
        ..Default::default()
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("valid SQL identifier")));
}

#[test]
fn test_query_schema_qualified_sql_source_passes() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            is_input:               false,
            relay:                  false,
            embedded:               false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
            inject_params:          indexmap::IndexMap::new(),
            relationships:          Vec::new(),
        }],
        queries: vec![IntermediateQuery {
            function: None,

            requires_actor:    Vec::new(),
            count:             false,
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
            read_routing:      fraiseql_core::db::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
            rest:              None,
            rest_stream:       false,
            pagination_order:  None,
        }],
        ..Default::default()
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
            advanced_types::{IntermediateInterface, IntermediateUnion},
            operations::{IntermediateArgument, IntermediateMutation, IntermediateQuery},
            types::{IntermediateEnum, IntermediateField, IntermediateType},
        },
        validator::{
            ErrorSeverity,
            schema_validator::{SchemaValidator, extract_base_type},
        },
    };

    fn field(name: &str, ty: &str) -> IntermediateField {
        IntermediateField {
            function: None,

            deprecated:      None,
            vector_config:   None,
            vector_distance: None,
            name:            name.to_string(),
            field_type:      ty.to_string(),
            nullable:        false,
            description:     None,
            directives:      None,
            requires_scope:  None,
            on_deny:         None,
            authorize:       None,
            hierarchy:       None,
        }
    }

    fn arg(name: &str, ty: &str) -> IntermediateArgument {
        IntermediateArgument {
            name:        name.to_string(),
            arg_type:    ty.to_string(),
            nullable:    false,
            default:     None,
            description: None,
            deprecated:  None,
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

    // ── #1358: a mutation's return type must be composite ───────────

    fn mutation(return_type: &str) -> IntermediateMutation {
        IntermediateMutation {
            name: "act".to_string(),
            return_type: return_type.to_string(),
            sql_source: Some("fn_act".to_string()),
            ..Default::default()
        }
    }

    fn errors_of(schema: &IntermediateSchema) -> Vec<String> {
        SchemaValidator::validate(schema)
            .unwrap()
            .errors
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Error)
            .map(|e| e.message.clone())
            .collect()
    }

    /// An enum is a *leaf* type, so § 5.3.3 (#1357) rightly lets
    /// `mutation { act }` through with no selection set — and an empty selection
    /// set is the permissive shape at the projector. The result was the whole
    /// stored entity object under an enum-typed field, with no field
    /// authorization. The envelope a mutation is projected out of is
    /// entity-shaped, so a leaf return type has no meaning under it; refused here
    /// rather than taught to the runtime.
    #[test]
    fn a_mutation_returning_an_enum_is_refused() {
        let mut schema = minimal_schema();
        schema.enums.push(IntermediateEnum {
            name:        "Status".to_string(),
            values:      vec![],
            description: None,
        });
        schema.mutations.push(mutation("Status"));

        let errors = errors_of(&schema);
        assert_eq!(errors.len(), 1, "expected exactly one error: {errors:?}");
        assert!(
            errors[0].contains("not a composite type"),
            "must say why, not merely that it is wrong: {errors:?}"
        );
        assert!(errors[0].contains("Status"), "must name the type: {errors:?}");
    }

    /// The wider half of the same hole. `type_names` registers the built-in
    /// scalars because every *other* position takes them, so `Boolean` resolved
    /// and the mutation compiled.
    #[test]
    fn a_mutation_returning_a_builtin_scalar_is_refused() {
        let mut schema = minimal_schema();
        schema.mutations.push(mutation("Boolean"));

        let errors = errors_of(&schema);
        assert_eq!(errors.len(), 1, "expected exactly one error: {errors:?}");
        assert!(
            errors[0].contains("not a composite type"),
            "a scalar return type is refused for the same reason as an enum: {errors:?}"
        );
    }

    /// The list wrapper is stripped first, so `[Boolean!]!` is adjudicated as
    /// `Boolean` — a gate that only saw the bare spelling would miss it.
    #[test]
    fn a_mutation_returning_a_list_of_scalars_is_refused() {
        let mut schema = minimal_schema();
        schema.mutations.push(mutation("[Boolean!]!"));

        let errors = errors_of(&schema);
        assert_eq!(errors.len(), 1, "expected exactly one error: {errors:?}");
        assert!(errors[0].contains("not a composite type"), "{errors:?}");
    }

    /// Control: an object return type — the ordinary shape — still compiles.
    #[test]
    fn a_mutation_returning_an_object_is_valid() {
        let mut schema = minimal_schema();
        schema.mutations.push(mutation("Item"));
        assert!(errors_of(&schema).is_empty());
    }

    /// Control: a union of success and error variants is the shape #212,
    /// #450/#451 and #698 produce, and is the one this rule must not reject.
    #[test]
    fn a_mutation_returning_a_union_is_valid() {
        let mut schema = minimal_schema();
        schema.unions.push(IntermediateUnion {
            name:         "ActResult".to_string(),
            member_types: vec!["Item".to_string()],
            description:  None,
        });
        schema.mutations.push(mutation("ActResult"));
        assert!(errors_of(&schema).is_empty(), "{:?}", errors_of(&schema));
    }

    /// Control: an interface is composite too (GraphQL § 3.7).
    #[test]
    fn a_mutation_returning_an_interface_is_valid() {
        let mut schema = minimal_schema();
        schema.interfaces.push(IntermediateInterface {
            name:        "Node".to_string(),
            fields:      vec![field("id", "UUID")],
            description: None,
        });
        schema.mutations.push(mutation("Node"));
        assert!(errors_of(&schema).is_empty(), "{:?}", errors_of(&schema));
    }

    /// The two refusals stay distinguishable: a name the schema does not carry at
    /// all is still "unknown type", with its did-you-mean suggestion, rather than
    /// being absorbed into the composite message.
    #[test]
    fn a_mutation_returning_an_unresolvable_name_still_reports_unknown_type() {
        let mut schema = minimal_schema();
        schema.mutations.push(mutation("Nonesuch"));

        let errors = errors_of(&schema);
        assert_eq!(errors.len(), 1, "expected exactly one error: {errors:?}");
        assert!(errors[0].contains("unknown type"), "{errors:?}");
        assert!(!errors[0].contains("not a composite type"), "{errors:?}");
    }

    /// The rule is mutation-only. A *query* may return a leaf — a count, a flag —
    /// and its result is not projected out of the mutation envelope.
    #[test]
    fn a_query_returning_an_enum_is_unaffected() {
        let mut schema = minimal_schema();
        schema.enums.push(IntermediateEnum {
            name:        "Status".to_string(),
            values:      vec![],
            description: None,
        });
        schema.queries.push(IntermediateQuery {
            name: "status".to_string(),
            return_type: "Status".to_string(),
            sql_source: Some("v_status".to_string()),
            ..Default::default()
        });
        assert!(errors_of(&schema).is_empty(), "{:?}", errors_of(&schema));
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

// ── #573 scheduled ingress source validation ────────────────────────────────

#[test]
fn valid_source_passes_validation() {
    use fraiseql_core::schema::SourceDefinition;
    let schema = IntermediateSchema {
        sources: Some(vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid(), "a well-formed source validates: {:?}", report.errors);
}

#[test]
fn invalid_cron_schedule_is_rejected() {
    use fraiseql_core::schema::SourceDefinition;
    let schema = IntermediateSchema {
        sources: Some(vec![SourceDefinition::new("orders", "not-a-cron", "pollOrders")]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(
        report.errors.iter().any(|e| e.path.contains("schedule")),
        "the bad cron schedule is reported"
    );
}

#[test]
fn duplicate_source_cursor_is_rejected() {
    use fraiseql_core::schema::SourceDefinition;
    // Two sources sharing a cursor name would clobber each other's watermark.
    let schema = IntermediateSchema {
        sources: Some(vec![
            SourceDefinition::new("a", "*/5 * * * *", "pollA").with_cursor("shared"),
            SourceDefinition::new("b", "*/5 * * * *", "pollB").with_cursor("shared"),
        ]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(
        report.errors.iter().any(|e| e.path.contains("cursor")),
        "the shared cursor is reported"
    );
}

#[test]
fn duplicate_source_name_is_rejected() {
    use fraiseql_core::schema::SourceDefinition;
    let schema = IntermediateSchema {
        sources: Some(vec![
            SourceDefinition::new("orders", "*/5 * * * *", "pollA"),
            SourceDefinition::new("orders", "0 * * * *", "pollB"),
        ]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("Duplicate source name")));
}

#[test]
fn run_as_without_authority_warns_but_is_valid() {
    use fraiseql_core::schema::{RunAs, SourceDefinition};
    // A run_as that grants neither roles nor scopes is fail-closed: the source can
    // write nothing. That is a valid (deny-by-default) configuration, but almost
    // always a mistake — surface it as a warning, not an error.
    let schema = IntermediateSchema {
        sources: Some(vec![
            SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")
                .with_run_as(RunAs::default()),
        ]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid(), "a fail-closed run_as is valid, only warned");
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.path.contains("run_as") && e.message.contains("no authority")),
        "the no-authority run_as is warned"
    );
}

#[test]
fn run_as_with_blank_grant_is_rejected() {
    use fraiseql_core::schema::{RunAs, SourceDefinition};

    use crate::schema::validator::types::ErrorSeverity;
    // A blank role/scope/tenant string is a config error, not a silent no-op.
    let schema = IntermediateSchema {
        sources: Some(vec![
            SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_run_as(RunAs {
                roles:  vec!["  ".to_string()],
                scopes: vec![],
                tenant: None,
            }),
        ]),
        ..Default::default()
    };
    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.path.contains("run_as") && e.severity == ErrorSeverity::Error),
        "the blank role is rejected"
    );
}
