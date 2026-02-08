//! End-to-end schema generation tests.
//!
//! Tests the complete compilation pipeline:
//! 1. Compile intermediate schema
//! 2. Verify all artifacts are generated correctly
//! 3. Validate GraphQL schema structure
//! 4. Verify SQL template correctness

use fraiseql_cli::schema::{SchemaConverter, intermediate::IntermediateSchema};

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
    };

    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    let security = compiled.security.as_ref().expect("Security should exist");
    let lookup = security["lookup_data"].as_object().expect("Lookup data should exist");

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
