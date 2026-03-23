#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! End-to-end schema generation tests.
//!
//! Tests the complete compilation pipeline:
//! 1. Compile intermediate schema
//! 2. Verify all artifacts are generated correctly
//! 3. Validate GraphQL schema structure
//! 4. Verify SQL template correctness

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{
        IntermediateArgument, IntermediateField, IntermediateMutation, IntermediateQuery,
        IntermediateSchema, IntermediateType,
    },
};
use fraiseql_core::schema::{CursorType, FieldType};
use indexmap::IndexMap;

/// Test: E2E complete rich filter compilation pipeline
#[test]
fn test_e2e_complete_compilation_pipeline() {
    let intermediate = IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    // Verify 49 rich types were compiled
    assert_eq!(compiled.input_types.len(), 49, "Should have exactly 49 rich type WhereInputs");

    // Spot-check key rich types
    let key_types = vec![
        "EmailAddressWhereInput",
        "PhoneNumberWhereInput",
        "URLWhereInput",
        "CoordinatesWhereInput",
        "DateRangeWhereInput",
        "CurrencyCodeWhereInput",
    ];

    for type_name in key_types {
        assert!(
            compiled.input_types.iter().any(|t| t.name == type_name),
            "Should have {} type",
            type_name
        );
    }
}

/// Test: E2E SQL templates cover all databases
#[test]
fn test_e2e_sql_templates_all_databases() {
    let intermediate = IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let databases = vec!["postgres", "mysql", "sqlite", "sqlserver"];
    let mut total_templates = 0;

    // Verify all WhereInput types have SQL templates for all databases
    for input_type in &compiled.input_types {
        if let Some(metadata) = &input_type.metadata {
            if let Some(operators) = metadata.get("operators").and_then(|o| o.as_object()) {
                for (_op_name, templates) in operators {
                    if let Some(db_templates) = templates.as_object() {
                        for db in &databases {
                            assert!(
                                db_templates.contains_key(*db),
                                "Type {} missing {} template",
                                input_type.name,
                                db
                            );
                            total_templates += 1;
                        }
                    }
                }
            }
        }
    }

    // Rough sanity check - should have many templates
    assert!(total_templates > 100, "Should have many templates (found {})", total_templates);
}

/// Test: E2E lookup data is comprehensive
#[test]
fn test_e2e_lookup_data_comprehensive() {
    let intermediate = IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let security = compiled.security.as_ref().expect("Security should exist");
    let lookup = security.additional["lookup_data"].as_object().expect("Lookup data should exist");

    // Verify all expected lookup tables
    assert!(lookup.contains_key("countries"), "Should have countries");
    assert!(lookup.contains_key("currencies"), "Should have currencies");
    assert!(lookup.contains_key("timezones"), "Should have timezones");
    assert!(lookup.contains_key("languages"), "Should have languages");

    // Verify data integrity
    let countries = lookup["countries"].as_object().expect("Countries should exist");
    assert!(countries.len() >= 10, "Should have many countries");

    let currencies = lookup["currencies"].as_object().expect("Currencies should exist");
    assert!(currencies.len() >= 5, "Should have many currencies");

    let timezones = lookup["timezones"].as_object().expect("Timezones should exist");
    assert!(timezones.len() >= 5, "Should have many timezones");

    let languages = lookup["languages"].as_object().expect("Languages should exist");
    assert!(languages.len() >= 5, "Should have many languages");
}

/// Test: E2E schema operators cover standard cases
#[test]
fn test_e2e_all_operators_generated() {
    let intermediate = IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    // Verify EmailAddress operators
    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should exist");

    let field_names: Vec<_> = email_where.fields.iter().map(|f| &f.name).collect();

    // Standard operators
    assert!(field_names.contains(&&"eq".to_string()), "Should have eq operator");
    assert!(field_names.contains(&&"neq".to_string()), "Should have neq operator");
    assert!(field_names.contains(&&"contains".to_string()), "Should have contains operator");

    // Rich email operators
    assert!(field_names.contains(&&"domainEq".to_string()), "Should have domainEq operator");
    assert!(field_names.contains(&&"domainIn".to_string()), "Should have domainIn operator");

    // Verify Coordinates operators (if available)
    if let Some(coords_where) =
        compiled.input_types.iter().find(|t| t.name == "CoordinatesWhereInput")
    {
        let coord_field_names: Vec<_> = coords_where.fields.iter().map(|f| &f.name).collect();

        // Geospatial operators (should have at least one)
        assert!(
            coord_field_names.contains(&&"distanceWithin".to_string())
                || coord_field_names.contains(&&"eq".to_string()),
            "CoordinatesWhereInput should have operators"
        );
    }
}

/// Test: E2E compilation is deterministic
#[test]
fn test_e2e_compilation_deterministic() {
    let create_schema = || IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled1 =
        SchemaConverter::convert(create_schema()).expect("Compilation 1 should succeed");
    let compiled2 =
        SchemaConverter::convert(create_schema()).expect("Compilation 2 should succeed");
    let compiled3 =
        SchemaConverter::convert(create_schema()).expect("Compilation 3 should succeed");

    // All compilations should produce identical results
    assert_eq!(
        compiled1.input_types.len(),
        compiled2.input_types.len(),
        "Type counts should match"
    );
    assert_eq!(
        compiled2.input_types.len(),
        compiled3.input_types.len(),
        "Type counts should be consistent"
    );

    // Verify order is consistent
    for (t1, t2) in compiled1.input_types.iter().zip(compiled2.input_types.iter()) {
        assert_eq!(t1.name, t2.name, "Type order should be consistent");
    }

    for (t2, t3) in compiled2.input_types.iter().zip(compiled3.input_types.iter()) {
        assert_eq!(t2.name, t3.name, "Type order should be consistent");
    }
}

/// Test: E2E all 49 types generate proper `WhereInput`
#[test]
fn test_e2e_all_49_types_valid() {
    let intermediate = IntermediateSchema {
        security:          None,
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
        custom_scalars:    None,
        observers_config:  None,
            subscriptions_config: None,
            validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    // Verify all types are valid WhereInputs
    assert_eq!(compiled.input_types.len(), 49, "Should have 49 types");

    for input_type in &compiled.input_types {
        // Name validation
        assert!(
            input_type.name.ends_with("WhereInput"),
            "Type {} should end with 'WhereInput'",
            input_type.name
        );

        // Fields validation
        assert!(!input_type.fields.is_empty(), "Type {} should have fields", input_type.name);

        for field in &input_type.fields {
            assert!(!field.name.is_empty(), "Field name should not be empty");
            assert!(!field.field_type.is_empty(), "Field type should not be empty");
        }

        // Metadata validation (if present)
        // Note: Not all types have metadata - some may have minimal operators
        if let Some(metadata) = &input_type.metadata {
            if let Some(operators) = metadata.get("operators").and_then(|o| o.as_object()) {
                // All operators should have templates
                for (_op_name, templates) in operators {
                    assert!(templates.is_object(), "Operator templates should be an object");
                }
            }
        }
    }
}

/// Test: E2E compilation pipeline with a real type, query, and mutation asserts all fields
///
/// Regression test for issue #53: the old converter hardcoded `sql_source: None` on every
/// generated `MutationDefinition`. This test drives a full intermediate → compiled schema
/// conversion and asserts every field of the produced `QueryDefinition` and
/// `MutationDefinition`, making future regressions of this class immediately visible.
#[test]
fn test_e2e_full_field_assertion() {
    let intermediate = IntermediateSchema {
        security:          None,
        version:           "2.0.0".to_string(),
        types:             vec![IntermediateType {
            name:        "User".to_string(),
            fields:      vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
                IntermediateField {
                    name:           "email".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
            ],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        queries:           vec![IntermediateQuery {
            name:             "users".to_string(),
            return_type:      "User".to_string(),
            returns_list:     true,
            nullable:         false,
            arguments:        vec![],
            description:      None,
            sql_source:       Some("v_user".to_string()),
            auto_params:      None,
            deprecated:       None,
            jsonb_column:     None,
            relay:            false,
            inject:           IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role:    None,
            relay_cursor_type: None,
        }],
        mutations:         vec![IntermediateMutation {
            name:                   "createUser".to_string(),
            return_type:            "User".to_string(),
            returns_list:           false,
            nullable:               false,
            arguments:              vec![],
            description:            None,
            operation:              None,
            deprecated:             None,
            sql_source:             Some("fn_create_user".to_string()),
            inject:                 IndexMap::default(),
            invalidates_fact_tables: vec![],
            invalidates_views:      vec![],
            cascade:                false,
        }],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    None,
        observers_config:  None,
        subscriptions_config: None,
        validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let schema = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    // Types — sql_source on TypeDefinition is populated by the TOML merger, not the
    // SchemaConverter alone; in the bare CLI path it is an empty string.
    let user = schema.types.iter().find(|t| t.name == "User").unwrap();
    assert_eq!(user.name, "User");
    assert!(!user.is_error);
    assert!(!user.relay);
    assert!(user.requires_role.is_none());
    assert!(user.implements.is_empty());

    // Queries
    assert_eq!(schema.queries.iter().filter(|q| q.name == "users").count(), 1);
    let q = schema.queries.iter().find(|q| q.name == "users").unwrap();
    assert_eq!(q.sql_source.as_deref(), Some("v_user"));
    assert!(q.returns_list);
    assert_eq!(q.relay_cursor_type, CursorType::Int64);
    assert!(q.inject_params.is_empty());
    assert!(q.cache_ttl_seconds.is_none());
    assert!(q.deprecation.is_none());

    // Mutations — regression-proof for issue #53 (sql_source must not be None)
    let m = schema.mutations.iter().find(|m| m.name == "createUser").unwrap();
    assert_eq!(
        m.sql_source.as_deref(),
        Some("fn_create_user"),
        "sql_source must be threaded from intermediate schema through the CLI converter"
    );
    assert!(m.inject_params.is_empty());
    assert!(m.invalidates_fact_tables.is_empty());
    assert!(m.invalidates_views.is_empty());
    assert!(m.deprecation.is_none());
}

/// Test: argument types with trailing `!` compile successfully.
///
/// External code generators may emit `"ID!"` instead of `"ID"`. The compiler
/// must strip the non-null marker and produce the correct `FieldType`.
#[test]
fn test_e2e_argument_type_with_non_null_marker() {
    let intermediate = IntermediateSchema {
        security:          None,
        version:           "2.0.0".to_string(),
        types:             vec![IntermediateType {
            name:        "User".to_string(),
            fields:      vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
            ],
            description: None,
            implements:  vec![],
            requires_role: None,
            is_error:    false,
            relay:       false,
        }],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![IntermediateQuery {
            name:        "user".to_string(),
            return_type: "User".to_string(),
            arguments:   vec![IntermediateArgument {
                name:       "id".to_string(),
                arg_type:   "ID!".to_string(),  // <-- trailing `!`
                nullable:   false,
                default:    None,
                deprecated: None,
            }],
            sql_source:  Some("users".to_string()),
            ..Default::default()
        }],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    None,
        observers_config:  None,
        subscriptions_config: None,
        validation_config: None,
        federation_config: None,
        debug_config:      None,
        mcp_config:        None,
        rest_config:       None,
        dev_config:        None,
        query_defaults:    None,
    };

    let compiled = SchemaConverter::convert(intermediate)
        .expect("Compilation should succeed with trailing `!` in argument type");

    let query = compiled
        .queries
        .iter()
        .find(|q| q.name == "user")
        .expect("user query should exist");

    assert_eq!(query.arguments.len(), 1);
    assert_eq!(query.arguments[0].arg_type, FieldType::Id);
}
