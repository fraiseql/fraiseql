use indexmap::IndexMap;

use super::*;
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateAutoParams, IntermediateField, IntermediateQuery,
    IntermediateSchema, IntermediateType,
};

#[test]
fn test_convert_minimal_schema() {
    let intermediate = IntermediateSchema {
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.types.len(), 0);
    assert_eq!(compiled.queries.len(), 0);
    assert_eq!(compiled.mutations.len(), 0);
}

#[test]
fn test_convert_type_with_fields() {
    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "Int".to_string(),
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
            description:   Some("User type".to_string()),
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
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
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.types.len(), 1);
    assert_eq!(compiled.types[0].name, "User");
    assert_eq!(compiled.types[0].fields.len(), 2);
    assert_eq!(compiled.types[0].fields[0].field_type, FieldType::Int);
    assert_eq!(compiled.types[0].fields[1].field_type, FieldType::String);
}

#[test]
fn test_validate_unknown_type_reference() {
    let intermediate = IntermediateSchema {
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
            sql_source:        Some("v_user".to_string()),
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
        query_defaults:       None,
    };

    let result = SchemaConverter::convert(intermediate);
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.expect_err("test").to_string().contains("unknown type 'UnknownType'"));
}

#[test]
fn test_convert_query_with_arguments() {
    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
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
            arguments:         vec![IntermediateArgument {
                name:       "limit".to_string(),
                arg_type:   "Int".to_string(),
                nullable:   false,
                default:    Some(serde_json::json!(10)),
                deprecated: None,
            }],
            description:       Some("Get users".to_string()),
            sql_source:        Some("v_user".to_string()),
            auto_params:       Some(IntermediateAutoParams {
                limit:        Some(true),
                offset:       Some(true),
                where_clause: Some(false),
                order_by:     Some(false),
            }),
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.queries.len(), 1);
    assert_eq!(compiled.queries[0].arguments.len(), 1);
    assert_eq!(compiled.queries[0].arguments[0].arg_type, FieldType::Int);
    assert!(compiled.queries[0].auto_params.has_limit);
}

#[test]
fn test_list_query_without_auto_params_defaults_to_all() {
    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Item".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "items".to_string(),
            return_type:       "Item".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("v_item".to_string()),
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    let params = &compiled.queries[0].auto_params;
    assert!(params.has_limit);
    assert!(params.has_offset);
    assert!(params.has_where);
    assert!(params.has_order_by);
}

#[test]
fn test_single_item_query_without_auto_params_defaults_to_none() {
    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Item".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "item".to_string(),
            return_type:       "Item".to_string(),
            returns_list:      false,
            nullable:          true,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("v_item".to_string()),
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    let params = &compiled.queries[0].auto_params;
    assert!(!params.has_limit);
    assert!(!params.has_offset);
    assert!(!params.has_where);
    assert!(!params.has_order_by);
}

#[test]
fn test_convert_field_with_deprecated_directive() {
    use crate::schema::intermediate::IntermediateAppliedDirective;

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![
                IntermediateField {
                    name:           "oldId".to_string(),
                    field_type:     "Int".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     Some(vec![IntermediateAppliedDirective {
                        name:      "deprecated".to_string(),
                        arguments: Some(serde_json::json!({"reason": "Use 'id' instead"})),
                    }]),
                    requires_scope: None,
                    on_deny:        None,
                },
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "Int".to_string(),
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.types.len(), 1);
    assert_eq!(compiled.types[0].fields.len(), 2);

    // Check deprecated field
    let old_id_field = &compiled.types[0].fields[0];
    assert_eq!(old_id_field.name, "oldId");
    assert!(old_id_field.is_deprecated());
    assert_eq!(old_id_field.deprecation_reason(), Some("Use 'id' instead"));

    // Check non-deprecated field
    let id_field = &compiled.types[0].fields[1];
    assert_eq!(id_field.name, "id");
    assert!(!id_field.is_deprecated());
    assert_eq!(id_field.deprecation_reason(), None);
}

#[test]
fn test_convert_enum() {
    use crate::schema::intermediate::{
        IntermediateDeprecation, IntermediateEnum, IntermediateEnumValue,
    };

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![IntermediateEnum {
            name:        "OrderStatus".to_string(),
            values:      vec![
                IntermediateEnumValue {
                    name:        "PENDING".to_string(),
                    description: None,
                    deprecated:  None,
                },
                IntermediateEnumValue {
                    name:        "PROCESSING".to_string(),
                    description: Some("Currently being processed".to_string()),
                    deprecated:  None,
                },
                IntermediateEnumValue {
                    name:        "CANCELLED".to_string(),
                    description: None,
                    deprecated:  Some(IntermediateDeprecation {
                        reason: Some("Use VOIDED instead".to_string()),
                    }),
                },
            ],
            description: Some("Order status enum".to_string()),
        }],
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.enums.len(), 1);

    let status_enum = &compiled.enums[0];
    assert_eq!(status_enum.name, "OrderStatus");
    assert_eq!(status_enum.description, Some("Order status enum".to_string()));
    assert_eq!(status_enum.values.len(), 3);

    // Check PENDING value
    assert_eq!(status_enum.values[0].name, "PENDING");
    assert!(!status_enum.values[0].is_deprecated());

    // Check PROCESSING value with description
    assert_eq!(status_enum.values[1].name, "PROCESSING");
    assert_eq!(status_enum.values[1].description, Some("Currently being processed".to_string()));

    // Check CANCELLED deprecated value
    assert_eq!(status_enum.values[2].name, "CANCELLED");
    assert!(status_enum.values[2].is_deprecated());
}

#[test]
fn test_convert_input_object() {
    use crate::schema::intermediate::{
        IntermediateDeprecation, IntermediateInputField, IntermediateInputObject,
    };

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![IntermediateInputObject {
            name:        "UserFilter".to_string(),
            fields:      vec![
                IntermediateInputField {
                    name:        "name".to_string(),
                    field_type:  "String".to_string(),
                    nullable:    true,
                    description: None,
                    default:     None,
                    deprecated:  None,
                },
                IntermediateInputField {
                    name:        "active".to_string(),
                    field_type:  "Boolean".to_string(),
                    nullable:    true,
                    description: Some("Filter by active status".to_string()),
                    default:     Some(serde_json::json!(true)),
                    deprecated:  None,
                },
                IntermediateInputField {
                    name:        "oldField".to_string(),
                    field_type:  "String".to_string(),
                    nullable:    true,
                    description: None,
                    default:     None,
                    deprecated:  Some(IntermediateDeprecation {
                        reason: Some("Use newField instead".to_string()),
                    }),
                },
            ],
            description: Some("User filter input".to_string()),
        }],
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    // 1 user-defined input type + 49 rich type WhereInput types
    assert_eq!(compiled.input_types.len(), 50);

    // Find the UserFilter type (rich types are added at the end)
    let filter = compiled.input_types.iter().find(|t| t.name == "UserFilter").expect("test");
    assert_eq!(filter.name, "UserFilter");
    assert_eq!(filter.description, Some("User filter input".to_string()));
    assert_eq!(filter.fields.len(), 3);

    // Check name field
    let name_field = filter.find_field("name").expect("test");
    assert_eq!(name_field.field_type, "String");
    assert!(!name_field.is_deprecated());

    // Check active field with default value
    let active_field = filter.find_field("active").expect("test");
    assert_eq!(active_field.field_type, "Boolean");
    assert_eq!(active_field.default_value, Some("true".to_string()));
    assert_eq!(active_field.description, Some("Filter by active status".to_string()));

    // Check deprecated field
    let old_field = filter.find_field("oldField").expect("test");
    assert!(old_field.is_deprecated());
}

#[test]
fn test_rich_filter_types_generated() {
    let intermediate = IntermediateSchema {
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    // Should have 49 rich type WhereInput types
    assert_eq!(compiled.input_types.len(), 49);

    // Check that EmailAddressWhereInput exists
    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should be generated");

    // Should have standard operators (eq, neq, in, nin, contains, isnull) + rich operators
    assert!(email_where.fields.len() > 6);
    assert!(email_where.fields.iter().any(|f| f.name == "eq"));
    assert!(email_where.fields.iter().any(|f| f.name == "neq"));
    assert!(email_where.fields.iter().any(|f| f.name == "contains"));
    assert!(email_where.fields.iter().any(|f| f.name == "isnull"));

    // Check that VINWhereInput exists
    let vin_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "VINWhereInput")
        .expect("VINWhereInput should be generated");

    assert!(vin_where.fields.len() > 6);
    assert!(vin_where.fields.iter().any(|f| f.name == "eq"));
}

#[test]
fn test_rich_filter_types_have_sql_templates() {
    let intermediate = IntermediateSchema {
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    // Check that EmailAddressWhereInput has SQL template metadata
    let email_where = compiled
        .input_types
        .iter()
        .find(|t| t.name == "EmailAddressWhereInput")
        .expect("EmailAddressWhereInput should be generated");

    // Verify metadata exists and contains operators
    assert!(
        email_where.metadata.is_some(),
        "Metadata should exist for EmailAddressWhereInput"
    );
    let metadata = email_where.metadata.as_ref().expect("test");
    assert!(
        metadata.get("operators").is_some(),
        "Operators should be in metadata: {metadata:?}"
    );

    let operators = metadata["operators"].as_object().expect("test");
    // Should have templates for email-specific operators
    assert!(!operators.is_empty(), "Operators map should not be empty: {operators:?}");
    assert!(
        operators.contains_key("domainEq"),
        "Missing domainEq in operators: {:?}",
        operators.keys().collect::<Vec<_>>()
    );

    // Verify domainEq has templates for all 4 databases
    let email_domain_eq = operators["domainEq"].as_object().expect("test");
    assert!(email_domain_eq.contains_key("postgres"));
    assert!(email_domain_eq.contains_key("mysql"));
    assert!(email_domain_eq.contains_key("sqlite"));
    assert!(email_domain_eq.contains_key("sqlserver"));

    // Verify PostgreSQL template is correct
    let postgres_template = email_domain_eq["postgres"].as_str().expect("test");
    assert!(postgres_template.contains("SPLIT_PART"));
    assert!(postgres_template.contains("$field"));
}

#[test]
fn test_lookup_data_embedded_in_schema() {
    let intermediate = IntermediateSchema {
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    // Verify lookup data is embedded in schema.security
    assert!(compiled.security.is_some(), "Security section should exist");
    let security = compiled.security.as_ref().expect("test");
    assert!(
        security.additional.contains_key("lookup_data"),
        "Lookup data should be in security section"
    );

    let lookup_data = security.additional["lookup_data"].as_object().expect("test");

    // Verify all lookup tables are present
    assert!(lookup_data.contains_key("countries"), "Countries lookup should be present");
    assert!(lookup_data.contains_key("currencies"), "Currencies lookup should be present");
    assert!(lookup_data.contains_key("timezones"), "Timezones lookup should be present");
    assert!(lookup_data.contains_key("languages"), "Languages lookup should be present");

    // Verify countries data
    let countries = lookup_data["countries"].as_object().expect("test");
    assert!(countries.contains_key("US"), "US should be in countries");
    assert!(countries.contains_key("FR"), "France should be in countries");
    assert!(countries.contains_key("GB"), "UK should be in countries");

    // Verify US data
    let us = countries["US"].as_object().expect("test");
    assert_eq!(us["continent"].as_str().expect("test"), "North America");
    assert!(!us["in_eu"].as_bool().expect("test"));

    // Verify France is EU and Schengen
    let fr = countries["FR"].as_object().expect("test");
    assert!(fr["in_eu"].as_bool().expect("test"));
    assert!(fr["in_schengen"].as_bool().expect("test"));

    // Verify currencies data
    let currencies = lookup_data["currencies"].as_object().expect("test");
    assert!(currencies.contains_key("USD"));
    assert!(currencies.contains_key("EUR"));
    let usd = currencies["USD"].as_object().expect("test");
    assert_eq!(usd["symbol"].as_str().expect("test"), "$");
    assert_eq!(usd["decimal_places"].as_i64().expect("test"), 2);

    // Verify timezones data
    let timezones = lookup_data["timezones"].as_object().expect("test");
    assert!(timezones.contains_key("UTC"));
    assert!(timezones.contains_key("EST"));
    let est = timezones["EST"].as_object().expect("test");
    assert_eq!(est["offset_minutes"].as_i64().expect("test"), -300);
    assert!(est["has_dst"].as_bool().expect("test"));
}

#[test]
fn test_convert_interface() {
    use crate::schema::intermediate::{IntermediateField, IntermediateInterface};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![IntermediateInterface {
            name:        "Node".to_string(),
            fields:      vec![IntermediateField {
                name:           "id".to_string(),
                field_type:     "ID".to_string(),
                nullable:       false,
                description:    None,
                directives:     None,
                requires_scope: None,
                on_deny:        None,
            }],
            description: Some("An object with a globally unique ID".to_string()),
        }],
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.interfaces.len(), 1);

    let interface = &compiled.interfaces[0];
    assert_eq!(interface.name, "Node");
    assert_eq!(interface.description, Some("An object with a globally unique ID".to_string()));
    assert_eq!(interface.fields.len(), 1);
    assert_eq!(interface.fields[0].name, "id");
    assert_eq!(interface.fields[0].field_type, FieldType::Id);
}

#[test]
fn test_convert_type_implements_interface() {
    use crate::schema::intermediate::{IntermediateField, IntermediateInterface, IntermediateType};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![
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
            description:   None,
            implements:    vec!["Node".to_string()],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![IntermediateInterface {
            name:        "Node".to_string(),
            fields:      vec![IntermediateField {
                name:           "id".to_string(),
                field_type:     "ID".to_string(),
                nullable:       false,
                description:    None,
                directives:     None,
                requires_scope: None,
                on_deny:        None,
            }],
            description: None,
        }],
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    // Check type implements interface
    assert_eq!(compiled.types.len(), 1);
    assert_eq!(compiled.types[0].implements, vec!["Node"]);

    // Check interface exists
    assert_eq!(compiled.interfaces.len(), 1);
    assert_eq!(compiled.interfaces[0].name, "Node");
}

#[test]
fn test_validate_unknown_interface() {
    use crate::schema::intermediate::{IntermediateField, IntermediateType};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![IntermediateField {
                name:           "id".to_string(),
                field_type:     "ID".to_string(),
                nullable:       false,
                description:    None,
                directives:     None,
                requires_scope: None,
                on_deny:        None,
            }],
            description:   None,
            implements:    vec!["UnknownInterface".to_string()],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![], // No interface defined!
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
        query_defaults:       None,
    };

    let result = SchemaConverter::convert(intermediate);
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.expect_err("test").to_string().contains("unknown interface"));
}

#[test]
fn test_validate_missing_interface_field() {
    use crate::schema::intermediate::{IntermediateField, IntermediateInterface, IntermediateType};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![
                // Missing the required 'id' field from Node interface!
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
            description:   None,
            implements:    vec!["Node".to_string()],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![IntermediateInterface {
            name:        "Node".to_string(),
            fields:      vec![IntermediateField {
                name:           "id".to_string(),
                field_type:     "ID".to_string(),
                nullable:       false,
                description:    None,
                directives:     None,
                requires_scope: None,
                on_deny:        None,
            }],
            description: None,
        }],
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
        query_defaults:       None,
    };

    let result = SchemaConverter::convert(intermediate);
    assert!(result.is_err(), "expected Err, got: {result:?}");
    assert!(result.expect_err("test").to_string().contains("missing field 'id'"));
}

#[test]
fn test_convert_union() {
    use crate::schema::intermediate::{IntermediateField, IntermediateType, IntermediateUnion};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![
            IntermediateType {
                name:          "User".to_string(),
                fields:        vec![IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                }],
                description:   None,
                implements:    vec![],
                requires_role: None,
                is_error:      false,
                relay:         false,
            },
            IntermediateType {
                name:          "Post".to_string(),
                fields:        vec![IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                }],
                description:   None,
                implements:    vec![],
                requires_role: None,
                is_error:      false,
                relay:         false,
            },
        ],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![IntermediateUnion {
            name:         "SearchResult".to_string(),
            member_types: vec!["User".to_string(), "Post".to_string()],
            description:  Some("Result from a search query".to_string()),
        }],
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
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    // Check union exists
    assert_eq!(compiled.unions.len(), 1);
    let union_def = &compiled.unions[0];
    assert_eq!(union_def.name, "SearchResult");
    assert_eq!(union_def.member_types, vec!["User", "Post"]);
    assert_eq!(union_def.description, Some("Result from a search query".to_string()));
}

#[test]
fn test_convert_field_requires_scope() {
    use crate::schema::intermediate::{IntermediateField, IntermediateType};

    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Employee".to_string(),
            fields:        vec![
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
                IntermediateField {
                    name:           "salary".to_string(),
                    field_type:     "Float".to_string(),
                    nullable:       false,
                    description:    Some("Employee salary - protected field".to_string()),
                    directives:     None,
                    requires_scope: Some("read:Employee.salary".to_string()),
                    on_deny:        None,
                },
                IntermediateField {
                    name:           "ssn".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       true,
                    description:    Some("Social Security Number - highly protected".to_string()),
                    directives:     None,
                    requires_scope: Some("admin".to_string()),
                    on_deny:        None,
                },
            ],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
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
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        query_defaults:       None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");

    assert_eq!(compiled.types.len(), 1);
    let employee_type = &compiled.types[0];
    assert_eq!(employee_type.name, "Employee");
    assert_eq!(employee_type.fields.len(), 4);

    // id field - no scope required
    assert_eq!(employee_type.fields[0].name, "id");
    assert!(employee_type.fields[0].requires_scope.is_none());

    // name field - no scope required
    assert_eq!(employee_type.fields[1].name, "name");
    assert!(employee_type.fields[1].requires_scope.is_none());

    // salary field - requires specific scope
    assert_eq!(employee_type.fields[2].name, "salary");
    assert_eq!(employee_type.fields[2].requires_scope, Some("read:Employee.salary".to_string()));

    // ssn field - requires admin scope
    assert_eq!(employee_type.fields[3].name, "ssn");
    assert_eq!(employee_type.fields[3].requires_scope, Some("admin".to_string()));
}
