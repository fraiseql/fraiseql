//! Integration tests for rich filter compiler.
//!
//! Tests the complete pipeline:
//! 1. Parse schema.json with rich scalar types
//! 2. Compile to schema.compiled.json
//! 3. Verify GraphQL types generated
//! 4. Verify SQL templates embedded
//! 5. Verify lookup data present

use fraiseql_cli::schema::{converter::SchemaConverter, intermediate::IntermediateSchema};

/// Test: Complete rich filter compilation pipeline
#[test]
fn test_rich_filter_compilation_pipeline() {
    // 1. Build minimal intermediate schema
    // Rich types are auto-generated, so we just need an empty schema
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

    // 2. Compile to schema
    let compiled = SchemaConverter::convert(intermediate).expect("Compilation should succeed");

    // 3. Verify GraphQL WhereInput types were generated
    assert!(
        compiled.input_types.iter().any(|t| t.name == "EmailAddressWhereInput"),
        "EmailAddressWhereInput should be generated"
    );
    assert!(
        compiled.input_types.iter().any(|t| t.name == "CoordinatesWhereInput"),
        "CoordinatesWhereInput should be generated"
    );

    // 4. Verify SQL templates are embedded
    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should exist");

    assert!(email_where.metadata.is_some(), "EmailAddressWhereInput should have metadata");
    let metadata = email_where.metadata.as_ref().unwrap();
    assert!(metadata.get("operators").is_some(), "Metadata should contain operators");

    let operators = metadata["operators"].as_object().unwrap();
    assert!(operators.contains_key("domainEq"), "Should have domainEq operator template");

    // Verify templates for all databases
    let domain_eq = operators["domainEq"].as_object().unwrap();
    for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
        assert!(domain_eq.contains_key(*db), "Should have {} template for domainEq", db);
    }

    // 5. Verify lookup data is present
    assert!(compiled.security.is_some(), "Security section should exist for lookup data");
    let security = compiled.security.as_ref().unwrap();
    assert!(security.get("lookup_data").is_some(), "Lookup data should be embedded");

    let lookup = security["lookup_data"].as_object().unwrap();
    assert!(lookup.contains_key("countries"), "Countries lookup should be present");
    assert!(lookup.contains_key("currencies"), "Currencies lookup should be present");
    assert!(lookup.contains_key("timezones"), "Timezones lookup should be present");
    assert!(lookup.contains_key("languages"), "Languages lookup should be present");
}

/// Test: All 49 rich types generate `WhereInput`
#[test]
fn test_all_rich_types_generate_where_input() {
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

    // Check that all 49 rich types generated WhereInput
    assert_eq!(
        compiled.input_types.len(),
        49,
        "Should have 49 rich type WhereInput definitions"
    );

    // Sample of important types
    let expected_types = vec![
        "EmailAddressWhereInput",
        "PhoneNumberWhereInput",
        "URLWhereInput",
        "VINWhereInput",
        "IBANWhereInput",
        "CountryCodeWhereInput",
        "CoordinatesWhereInput",
        "DateRangeWhereInput",
        "DurationWhereInput",
        "CurrencyCodeWhereInput",
    ];

    for expected in expected_types {
        assert!(
            compiled.input_types.iter().any(|t| t.name == expected),
            "Should have {} type",
            expected
        );
    }
}

/// Test: `WhereInput` types have correct fields
#[test]
fn test_where_input_fields_include_standard_operators() {
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

    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should exist");

    // Check standard operators present
    let field_names: Vec<_> = email_where.fields.iter().map(|f| &f.name).collect();
    assert!(field_names.contains(&&"eq".to_string()), "Should have eq operator");
    assert!(field_names.contains(&&"neq".to_string()), "Should have neq operator");
    assert!(field_names.contains(&&"contains".to_string()), "Should have contains operator");
    assert!(field_names.contains(&&"isnull".to_string()), "Should have isnull operator");

    // Check rich operators for email
    assert!(field_names.contains(&&"domainEq".to_string()), "Should have domainEq operator");
    assert!(field_names.contains(&&"domainIn".to_string()), "Should have domainIn operator");
    assert!(
        field_names.contains(&&"domainEndswith".to_string()),
        "Should have domainEndswith operator"
    );

    // Total fields should be reasonable (6 standard + 4 email specific)
    assert!(email_where.fields.len() >= 10, "Should have at least 10 fields");
}

/// Test: SQL templates have correct database coverage
#[test]
fn test_sql_templates_cover_all_databases() {
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

    // Check multiple types for database coverage
    let types_to_check = vec![
        "EmailAddressWhereInput",
        "VINWhereInput",
        "CoordinatesWhereInput",
    ];

    for type_name in types_to_check {
        let where_input = compiled
            .input_types
            .iter()
            .find(|t| t.name == type_name)
            .unwrap_or_else(|| panic!("{} should exist", type_name));

        let metadata = where_input
            .metadata
            .as_ref()
            .unwrap_or_else(|| panic!("{} should have metadata", type_name));
        let operators = metadata["operators"].as_object().unwrap();

        // Each operator should have templates for all 4 databases
        for (op_name, templates) in operators {
            let db_templates = templates.as_object().unwrap();
            for db in &["postgres", "mysql", "sqlite", "sqlserver"] {
                assert!(
                    db_templates.contains_key(*db),
                    "{}/{} should have {} template",
                    type_name,
                    op_name,
                    db
                );
            }
        }
    }
}

/// Test: Lookup data integrity
#[test]
fn test_lookup_data_integrity() {
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

    let security = compiled.security.as_ref().expect("Security section should exist");
    let lookup = security["lookup_data"].as_object().expect("Lookup data should be an object");

    // Verify countries
    let countries = lookup["countries"].as_object().expect("Countries should exist");
    assert!(countries.len() >= 10, "Should have at least 10 countries");

    // Check sample countries have required fields
    if let Some(us) = countries.get("US") {
        let us_obj = us.as_object().unwrap();
        assert!(us_obj.contains_key("continent"), "US should have continent");
        assert!(us_obj.contains_key("in_eu"), "US should have in_eu");
        assert!(us_obj.contains_key("in_schengen"), "US should have in_schengen");
    }

    // Verify currencies
    let currencies = lookup["currencies"].as_object().expect("Currencies should exist");
    assert!(currencies.len() >= 5, "Should have at least 5 currencies");

    // Check sample currency has required fields
    if let Some(usd) = currencies.get("USD") {
        let usd_obj = usd.as_object().unwrap();
        assert!(usd_obj.contains_key("symbol"), "USD should have symbol");
        assert!(usd_obj.contains_key("decimal_places"), "USD should have decimal_places");
    }

    // Verify timezones
    let timezones = lookup["timezones"].as_object().expect("Timezones should exist");
    assert!(timezones.len() >= 5, "Should have at least 5 timezones");

    // Verify languages
    let languages = lookup["languages"].as_object().expect("Languages should exist");
    assert!(languages.len() >= 5, "Should have at least 5 languages");
}

/// Test: Schema is valid after compilation
#[test]
fn test_compiled_schema_is_valid() {
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

    // Verify basic structure
    assert!(
        !compiled.types.is_empty() || !compiled.input_types.is_empty(),
        "Compiled schema should have types or input types"
    );
    assert!(!compiled.input_types.is_empty(), "Should have input types");

    // Verify all WhereInput types have valid structure
    for where_input in &compiled.input_types {
        assert!(!where_input.name.is_empty(), "Type name should not be empty");
        assert!(
            where_input.name.ends_with("WhereInput"),
            "Type {} should end with 'WhereInput'",
            where_input.name
        );
        assert!(!where_input.fields.is_empty(), "Type {} should have fields", where_input.name);

        // All fields should have valid names and types
        for field in &where_input.fields {
            assert!(!field.name.is_empty(), "Field name should not be empty");
            assert!(!field.field_type.is_empty(), "Field type should not be empty");
        }
    }
}
