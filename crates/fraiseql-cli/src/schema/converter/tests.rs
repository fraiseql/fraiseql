#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

use fraiseql_core::schema::NamingConvention;
use indexmap::IndexMap;

use super::*;
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateAutoParams, IntermediateField, IntermediateQuery,
    IntermediateSchema, IntermediateType,
};

// ── #366 @subscribable threading ─────────────────────────────────────────────

#[test]
fn intermediate_type_subscribable_tables_defaults_none() {
    // A type authored before #366 has no `subscribable_tables` key.
    let json = r#"{ "name": "Post", "fields": [] }"#;
    let t: IntermediateType = serde_json::from_str(json).unwrap();
    assert!(t.subscribable_tables.is_none(), "absent subscribable_tables defaults to None");
}

#[test]
fn intermediate_type_reads_subscribable_tables() {
    let json = r#"{ "name": "Post", "fields": [], "subscribable_tables": ["tb_post"] }"#;
    let t: IntermediateType = serde_json::from_str(json).unwrap();
    assert_eq!(t.subscribable_tables, Some(vec!["tb_post".to_string()]));
}

#[test]
fn convert_aggregates_subscribable_tables_into_compiled() {
    let intermediate = IntermediateSchema {
        types: vec![
            IntermediateType {
                name: "Post".to_string(),
                subscribable_tables: Some(vec![
                    "tb_post".to_string(),
                    "public.tb_post_archive".to_string(),
                ]),
                ..Default::default()
            },
            // Not subscribable → excluded.
            IntermediateType {
                name: "Comment".to_string(),
                ..Default::default()
            },
            // Empty list → treated as not subscribable.
            IntermediateType {
                name: "Tag".to_string(),
                subscribable_tables: Some(vec![]),
                subscribable_pre_image: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(intermediate).expect("convert");
    assert_eq!(compiled.subscribable.len(), 1, "only Post is subscribable");
    assert_eq!(compiled.subscribable[0].entity_type, "Post");
    assert_eq!(
        compiled.subscribable[0].tables,
        vec!["tb_post".to_string(), "public.tb_post_archive".to_string()]
    );
    assert!(
        !compiled.subscribable[0].pre_image,
        "pre_image defaults off when subscribable_pre_image is unset"
    );
}

#[test]
fn convert_threads_subscribable_pre_image_into_compiled() {
    // changelog_pre_image out-of-band parity: subscribable_pre_image=true on a
    // @subscribable type threads into SubscribableEntity.pre_image, so the
    // generated capture triggers record OLD into object_data_before.
    let intermediate = IntermediateSchema {
        types: vec![IntermediateType {
            name: "Price".to_string(),
            subscribable_tables: Some(vec!["tb_price".to_string()]),
            subscribable_pre_image: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(intermediate).expect("convert");
    assert_eq!(compiled.subscribable.len(), 1);
    assert!(compiled.subscribable[0].pre_image, "subscribable_pre_image threads through");
}

// ── #507 type-level sql_source threading (federation extends entities) ────────

#[test]
fn intermediate_type_reads_type_level_sql_source() {
    // The authoring SDK emits a type-level `sql_source` for an owner-split
    // `extend type … @key` federation entity that has no local backing query.
    let json = r#"{ "name": "Organization", "fields": [], "sql_source": "v_organization" }"#;
    let t: IntermediateType = serde_json::from_str(json).unwrap();
    assert_eq!(t.sql_source.as_deref(), Some("v_organization"));
}

#[test]
fn intermediate_type_sql_source_defaults_none() {
    // The common case: an owned type binds its relation on the query that returns
    // it, so the type-level key is absent.
    let json = r#"{ "name": "User", "fields": [] }"#;
    let t: IntermediateType = serde_json::from_str(json).unwrap();
    assert!(t.sql_source.is_none(), "absent sql_source defaults to None");
}

#[test]
fn convert_threads_type_level_sql_source_into_compiled() {
    // An extends entity's type-level sql_source flows to TypeDefinition.sql_source
    // so the federation `_entities` resolver can source its backing relation when
    // no root query returns the type (#507). An owned type without one keeps the
    // historical empty type-level sql_source.
    let intermediate = IntermediateSchema {
        types: vec![
            IntermediateType {
                name: "Organization".to_string(),
                sql_source: Some("v_organization".to_string()),
                ..Default::default()
            },
            IntermediateType {
                name: "User".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(intermediate).expect("convert");
    let org = compiled
        .types
        .iter()
        .find(|t| t.name == "Organization")
        .expect("Organization type");
    assert_eq!(
        org.sql_source.as_str(),
        "v_organization",
        "type-level sql_source threads through to the compiled type"
    );
    assert_eq!(
        org.jsonb_column, "data",
        "an extends entity's fields default to the standard jsonb `data` column, symmetric \
         with the query path"
    );
    let user = compiled.types.iter().find(|t| t.name == "User").expect("User type");
    assert!(
        user.sql_source.as_str().is_empty(),
        "an owned type keeps the historical empty type-level sql_source"
    );
    assert!(
        user.jsonb_column.is_empty(),
        "an owned type keeps the historical empty type-level jsonb_column (optimizer heuristic \
         stays off, compiled output byte-identical)"
    );
}

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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    assert_eq!(compiled.types.len(), 0);
    assert_eq!(compiled.queries.len(), 0);
    assert_eq!(compiled.mutations.len(), 0);
}

#[test]
fn convert_field_maps_authorize_true() {
    let intermediate = IntermediateField {
        name:           "email".to_string(),
        field_type:     "String".to_string(),
        nullable:       true,
        description:    None,
        directives:     None,
        requires_scope: None,
        on_deny:        None,
        authorize:      Some(true),
        hierarchy:      None,
    };
    let compiled = SchemaConverter::convert_field(intermediate).unwrap();
    assert!(compiled.authorize, "authorize: Some(true) must compile to authorize == true");
}

#[test]
fn convert_field_authorize_absent_defaults_false() {
    let intermediate = IntermediateField {
        name:           "id".to_string(),
        field_type:     "Int".to_string(),
        nullable:       false,
        description:    None,
        directives:     None,
        requires_scope: None,
        on_deny:        None,
        authorize:      None,
        hierarchy:      None,
    };
    let compiled = SchemaConverter::convert_field(intermediate).unwrap();
    assert!(!compiled.authorize, "absent authorize must compile to authorize == false");
}

// ── #434: list field types must compile to FieldType::List ──────────────
//
// `parse_field_type` matched built-in scalar names and routed everything else —
// including the SDL list string "[Foo!]" — to FieldType::Object, so a list field
// (or list query argument) became Object("[Foo!]"), a single object whose type
// name does not exist, and projected at runtime as one null object, not a list.

#[test]
fn parse_field_type_unwraps_list_of_objects() {
    assert_eq!(
        SchemaConverter::parse_field_type("[Item!]").unwrap(),
        FieldType::List(Box::new(FieldType::Object("Item".to_string()))),
    );
}

#[test]
fn parse_field_type_unwraps_nullable_element_list() {
    assert_eq!(
        SchemaConverter::parse_field_type("[Item]").unwrap(),
        FieldType::List(Box::new(FieldType::Object("Item".to_string()))),
    );
}

#[test]
fn parse_field_type_unwraps_list_of_scalars() {
    assert_eq!(
        SchemaConverter::parse_field_type("[String!]").unwrap(),
        FieldType::List(Box::new(FieldType::String)),
    );
}

#[test]
fn parse_field_type_strips_trailing_nonnull() {
    assert_eq!(
        SchemaConverter::parse_field_type("Item!").unwrap(),
        FieldType::Object("Item".to_string()),
    );
    assert_eq!(SchemaConverter::parse_field_type("String!").unwrap(), FieldType::String);
}

#[test]
fn parse_field_type_nested_list() {
    assert_eq!(
        SchemaConverter::parse_field_type("[[Item!]!]").unwrap(),
        FieldType::List(Box::new(FieldType::List(Box::new(FieldType::Object("Item".to_string()))))),
    );
}

#[test]
fn parse_field_type_plain_scalar_and_object_unchanged() {
    assert_eq!(SchemaConverter::parse_field_type("String").unwrap(), FieldType::String);
    assert_eq!(
        SchemaConverter::parse_field_type("User").unwrap(),
        FieldType::Object("User".to_string()),
    );
}

#[test]
fn convert_field_list_type_compiles_to_list() {
    let intermediate = IntermediateField {
        name:           "items".to_string(),
        field_type:     "[Item!]".to_string(),
        nullable:       false,
        description:    None,
        directives:     None,
        requires_scope: None,
        on_deny:        None,
        authorize:      None,
        hierarchy:      None,
    };
    let compiled = SchemaConverter::convert_field(intermediate).unwrap();
    assert_eq!(
        compiled.field_type,
        FieldType::List(Box::new(FieldType::Object("Item".to_string()))),
        "a list field must compile to FieldType::List, not Object(\"[Item!]\")"
    );
}

#[test]
fn test_convert_type_with_fields() {
    let intermediate = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "Int".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
            ],
            description:            Some("User type".to_string()),
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "Item".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "Item".to_string(),
            sql_source:             None,
            fields:                 vec![],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![
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
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "Int".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
            ],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
                    nullable:    false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
    };

    let compiled = SchemaConverter::convert(intermediate).expect("test");
    // 1 user-defined input type + 49 rich type WhereInput types
    assert_eq!(compiled.input_types.len(), 50);

    // Find the UserFilter type (rich types are added at the end)
    let filter = compiled.input_types.iter().find(|t| t.name == "UserFilter").expect("test");
    assert_eq!(filter.name, "UserFilter");
    assert_eq!(filter.description, Some("User filter input".to_string()));
    assert_eq!(filter.fields.len(), 3);

    // Check name field — nullability must survive the conversion (#414).
    let name_field = filter.find_field("name").expect("test");
    assert_eq!(name_field.field_type, "String");
    assert!(!name_field.is_deprecated());
    assert!(
        !name_field.nullable,
        "non-null input field must compile to nullable=false (#414)"
    );
    assert!(name_field.is_required(), "non-null field with no default is required (#414)");

    // Check active field with default value
    let active_field = filter.find_field("active").expect("test");
    assert_eq!(active_field.field_type, "Boolean");
    assert_eq!(active_field.default_value, Some("true".to_string()));
    assert_eq!(active_field.description, Some("Filter by active status".to_string()));
    assert!(
        active_field.nullable,
        "nullable input field must compile to nullable=true (#414)"
    );
    assert!(!active_field.is_required(), "nullable field is not required (#414)");

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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
                authorize:      None,
                hierarchy:      None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
            ],
            description:            None,
            implements:             vec!["Node".to_string()],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
                authorize:      None,
                hierarchy:      None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![IntermediateField {
                name:           "id".to_string(),
                field_type:     "ID".to_string(),
                nullable:       false,
                description:    None,
                directives:     None,
                requires_scope: None,
                on_deny:        None,
                authorize:      None,
                hierarchy:      None,
            }],
            description:            None,
            implements:             vec!["UnknownInterface".to_string()],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "User".to_string(),
            sql_source:             None,
            fields:                 vec![
                // Missing the required 'id' field from Node interface!
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
            ],
            description:            None,
            implements:             vec!["Node".to_string()],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
                authorize:      None,
                hierarchy:      None,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
                name:                   "User".to_string(),
                sql_source:             None,
                fields:                 vec![IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                }],
                description:            None,
                implements:             vec![],
                requires_role:          None,
                is_error:               false,
                relay:                  false,
                subscribable_tables:    None,
                subscribable_pre_image: false,
            },
            IntermediateType {
                name:                   "Post".to_string(),
                sql_source:             None,
                fields:                 vec![IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                }],
                description:            None,
                implements:             vec![],
                requires_role:          None,
                is_error:               false,
                relay:                  false,
                subscribable_tables:    None,
                subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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
            name:                   "Employee".to_string(),
            sql_source:             None,
            fields:                 vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "salary".to_string(),
                    field_type:     "Float".to_string(),
                    nullable:       false,
                    description:    Some("Employee salary - protected field".to_string()),
                    directives:     None,
                    requires_scope: Some("read:Employee.salary".to_string()),
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
                IntermediateField {
                    name:           "ssn".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       true,
                    description:    Some("Social Security Number - highly protected".to_string()),
                    directives:     None,
                    requires_scope: Some("admin".to_string()),
                    on_deny:        None,
                    authorize:      None,
                    hierarchy:      None,
                },
            ],
            description:            None,
            implements:             vec![],
            requires_role:          None,
            is_error:               false,
            relay:                  false,
            subscribable_tables:    None,
            subscribable_pre_image: false,
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
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
        hierarchies_config:   None,
        changelog_config:     None,
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

// ── promote_input_type_args (#456 Option 2: IR honesty) ──────────────────────

#[test]
fn promote_input_type_args_rewrites_object_to_input() {
    use fraiseql_core::schema::{
        ArgumentDefinition, CompiledSchema, FieldType, InputObjectDefinition, MutationDefinition,
    };

    let arg = |arg_type| ArgumentDefinition {
        name: "input".to_string(),
        arg_type,
        nullable: false,
        default_value: None,
        description: None,
        deprecation: None,
    };

    let mut schema = CompiledSchema {
        input_types: vec![InputObjectDefinition {
            name:        "CreateOrderInput".to_string(),
            fields:      vec![],
            description: None,
            metadata:    None,
        }],
        ..Default::default()
    };
    // (1) arg naming an input type → Input
    let mut m1 = MutationDefinition::new("createOrder", "Order");
    m1.arguments = vec![arg(FieldType::Object("CreateOrderInput".to_string()))];
    // (2) arg naming an OUTPUT object type → stays Object
    let mut m2 = MutationDefinition::new("touchOrder", "Order");
    m2.arguments = vec![arg(FieldType::Object("Order".to_string()))];
    // (3) list of an input type → inner promoted to Input
    let mut m3 = MutationDefinition::new("bulkCreate", "Order");
    m3.arguments = vec![arg(FieldType::List(Box::new(FieldType::Object(
        "CreateOrderInput".to_string(),
    ))))];
    schema.mutations = vec![m1, m2, m3];

    SchemaConverter::promote_input_type_args(&mut schema);

    assert_eq!(
        schema.mutations[0].arguments[0].arg_type,
        FieldType::Input("CreateOrderInput".to_string()),
        "an Object naming a registered input type must become Input"
    );
    assert_eq!(
        schema.mutations[1].arguments[0].arg_type,
        FieldType::Object("Order".to_string()),
        "an Object naming an output type must stay Object"
    );
    assert_eq!(
        schema.mutations[2].arguments[0].arg_type,
        FieldType::List(Box::new(FieldType::Input("CreateOrderInput".to_string()))),
        "a list of an input type must promote its element to Input"
    );
}

// ── tenancy converter tests ─────────────────────────────────────────────────

mod tenancy_tests {
    use indexmap::IndexMap;

    use super::super::tenancy::{AnnotatedTypeIndex, validate_tenant_annotations};
    use crate::schema::intermediate::{
        IntermediateField, IntermediateMutation, IntermediateQuery, IntermediateSchema,
        IntermediateType, fragments::IntermediateAppliedDirective,
    };

    fn make_type(name: &str, fields: Vec<IntermediateField>) -> IntermediateType {
        IntermediateType {
            name: name.to_string(),
            sql_source: None,
            fields,
            description: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            subscribable_tables: None,
            subscribable_pre_image: false,
        }
    }

    fn make_field(name: &str, field_type: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     field_type.to_string(),
            nullable:       false,
            description:    None,
            directives:     None,
            requires_scope: None,
            on_deny:        None,
            authorize:      None,
            hierarchy:      None,
        }
    }

    fn make_tenant_id_field(name: &str) -> IntermediateField {
        IntermediateField {
            name:           name.to_string(),
            field_type:     "String".to_string(),
            nullable:       false,
            description:    None,
            directives:     Some(vec![IntermediateAppliedDirective {
                name:      "tenant_id".to_string(),
                arguments: None,
            }]),
            requires_scope: None,
            on_deny:        None,
            authorize:      None,
            hierarchy:      None,
        }
    }

    fn make_query(name: &str, return_type: &str) -> IntermediateQuery {
        IntermediateQuery {
            name: name.to_string(),
            return_type: return_type.to_string(),
            ..Default::default()
        }
    }

    fn make_mutation(name: &str, return_type: &str) -> IntermediateMutation {
        IntermediateMutation {
            name: name.to_string(),
            return_type: return_type.to_string(),
            ..Default::default()
        }
    }

    fn make_schema(
        types: Vec<IntermediateType>,
        queries: Vec<IntermediateQuery>,
        mutations: Vec<IntermediateMutation>,
    ) -> IntermediateSchema {
        IntermediateSchema {
            types,
            queries,
            mutations,
            ..Default::default()
        }
    }

    // ── AnnotatedTypeIndex ──────────────────────────────────────────────

    #[test]
    fn index_empty_when_no_annotations() {
        let types = vec![make_type("User", vec![make_field("id", "Int")])];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(!index.has_annotations());
    }

    #[test]
    fn index_detects_tenant_id_field() {
        let types = vec![make_type(
            "User",
            vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
        )];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(index.has_annotations());
        let fields = index.fields_for_type("User").unwrap();
        assert!(fields.contains("tenant_id"));
    }

    #[test]
    fn index_multiple_types_independently() {
        let types = vec![
            make_type("User", vec![make_tenant_id_field("tenant_id")]),
            make_type("Post", vec![make_field("id", "Int")]),
            make_type("Order", vec![make_tenant_id_field("org_id")]),
        ];
        let index = AnnotatedTypeIndex::build(&types);
        assert!(index.fields_for_type("User").is_some());
        assert!(index.fields_for_type("Post").is_none());
        assert!(index.fields_for_type("Order").is_some());
        assert!(index.fields_for_type("Order").unwrap().contains("org_id"));
    }

    // ── Auto-injection ──────────────────────────────────────────────────

    #[test]
    fn auto_injects_query_when_inject_empty() {
        let mut schema = make_schema(
            vec![make_type(
                "User",
                vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
            )],
            vec![make_query("getUser", "User")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert_eq!(schema.queries[0].inject.get("tenant_id"), Some(&"jwt:tenant_id".to_string()));
    }

    #[test]
    fn auto_injects_mutation_when_inject_empty() {
        let mut schema = make_schema(
            vec![make_type(
                "User",
                vec![make_field("id", "Int"), make_tenant_id_field("tenant_id")],
            )],
            vec![],
            vec![make_mutation("createUser", "User")],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert_eq!(schema.mutations[0].inject.get("tenant_id"), Some(&"jwt:tenant_id".to_string()));
    }

    // ── input_style threading (flatten default / explicit jsonb) ─────────

    /// An absent / unset `input_style` converts to the `Flatten` default.
    #[test]
    fn convert_mutation_defaults_input_style_to_flatten() {
        use fraiseql_core::schema::InputStyle;

        use crate::schema::SchemaConverter;
        let md = SchemaConverter::convert_mutation(make_mutation("createUser", "User")).unwrap();
        assert_eq!(md.input_style, InputStyle::Flatten);
    }

    /// `input_style = jsonb` threads through orthogonally to the real DML verb:
    /// the mutation keeps its `Insert` operation (so the Change Spine logs the
    /// true verb) while opting into single-JSONB input passing.
    #[test]
    fn convert_mutation_threads_jsonb_input_style_with_real_verb() {
        use fraiseql_core::schema::{InputStyle, MutationOperation};

        use crate::schema::SchemaConverter;
        let im = IntermediateMutation {
            operation: Some("INSERT".to_string()),
            sql_source: Some("create_user".to_string()),
            input_style: InputStyle::Jsonb,
            ..make_mutation("createUser", "User")
        };
        let md = SchemaConverter::convert_mutation(im).unwrap();
        assert_eq!(md.input_style, InputStyle::Jsonb);
        assert!(matches!(md.operation, MutationOperation::Insert { .. }));
    }

    /// The authoring JSON contract: lowercase `"flatten"`/`"jsonb"`, with an
    /// absent key defaulting to `flatten` (byte-identical to pre-`input_style`).
    #[test]
    fn intermediate_mutation_input_style_json_contract() {
        use fraiseql_core::schema::InputStyle;
        let absent: IntermediateMutation =
            serde_json::from_str(r#"{ "name": "m", "return_type": "R" }"#).unwrap();
        assert_eq!(absent.input_style, InputStyle::Flatten);
        let jsonb: IntermediateMutation =
            serde_json::from_str(r#"{ "name": "m", "return_type": "R", "input_style": "jsonb" }"#)
                .unwrap();
        assert_eq!(jsonb.input_style, InputStyle::Jsonb);
    }

    // ── changelog_pre_image threading (default off / explicit on) ─────────

    /// An absent / unset `changelog_pre_image` converts to `false` (after-image
    /// only — byte-identical to the behavior before the flag existed).
    #[test]
    fn convert_mutation_defaults_changelog_pre_image_to_false() {
        use crate::schema::SchemaConverter;
        let md = SchemaConverter::convert_mutation(make_mutation("createUser", "User")).unwrap();
        assert!(!md.changelog_pre_image);
    }

    /// `changelog_pre_image = true` threads through to the compiled mutation so the
    /// outbox CTE records the entity's pre-image into `object_data_before`.
    #[test]
    fn convert_mutation_threads_changelog_pre_image() {
        use crate::schema::SchemaConverter;
        let im = IntermediateMutation {
            changelog_pre_image: true,
            ..make_mutation("updatePrice", "Price")
        };
        let md = SchemaConverter::convert_mutation(im).unwrap();
        assert!(md.changelog_pre_image);
    }

    /// The authoring JSON contract: an absent key defaults to `false`; `true`
    /// threads through.
    #[test]
    fn intermediate_mutation_changelog_pre_image_json_contract() {
        let absent: IntermediateMutation =
            serde_json::from_str(r#"{ "name": "m", "return_type": "R" }"#).unwrap();
        assert!(!absent.changelog_pre_image);
        let on: IntermediateMutation = serde_json::from_str(
            r#"{ "name": "m", "return_type": "R", "changelog_pre_image": true }"#,
        )
        .unwrap();
        assert!(on.changelog_pre_image);
    }

    // ── cascade threading (default off / explicit on, gate decision 3) ────

    /// An absent / unset `cascade` converts to `false` (no typed cascade
    /// surface — byte-identical to the behavior before the flag existed).
    #[test]
    fn convert_mutation_defaults_cascade_to_false() {
        use crate::schema::SchemaConverter;
        let md = SchemaConverter::convert_mutation(make_mutation("createPost", "Post")).unwrap();
        assert!(!md.cascade);
    }

    /// `cascade = true` threads through to the compiled mutation so the runtime
    /// exposes and enforces the typed cascade field. Before this, the compiler
    /// silently dropped the SDK flag (eval finding 4).
    #[test]
    fn convert_mutation_threads_cascade() {
        use crate::schema::SchemaConverter;
        let im = IntermediateMutation {
            cascade: true,
            ..make_mutation("createPost", "Post")
        };
        let md = SchemaConverter::convert_mutation(im).unwrap();
        assert!(md.cascade);
    }

    /// The authoring JSON contract: an absent key defaults to `false`; `true`
    /// threads through. This is the key the Python/TS SDKs write for
    /// `@fraiseql.type(crud=True, cascade=True)`.
    #[test]
    fn intermediate_mutation_cascade_json_contract() {
        let absent: IntermediateMutation =
            serde_json::from_str(r#"{ "name": "m", "return_type": "R" }"#).unwrap();
        assert!(!absent.cascade);
        let on: IntermediateMutation =
            serde_json::from_str(r#"{ "name": "m", "return_type": "R", "cascade": true }"#)
                .unwrap();
        assert!(on.cascade);
    }

    // ── cascade type synthesis (typed payload-wrapper surface) ──

    /// A view-backed entity type with the given name (queryable — so it
    /// auto-implements `CascadeNode`).
    fn make_entity_type(name: &str) -> IntermediateType {
        IntermediateType {
            sql_source: Some(format!("v_{}", name.to_lowercase())),
            ..make_type(name, vec![make_field("id", "ID")])
        }
    }

    /// A `cascade = true` mutation synthesizes the spec-aligned typed surface: the
    /// `CascadeNode` interface (auto-implemented on every queryable entity), the
    /// `CascadeOperation` enum, the `UpdatedEntity`/`DeletedEntity`/`CascadeUpdates`
    /// envelope, and a `<Name>Payload { entity, cascade, updatedFields }` wrapper.
    #[test]
    fn cascade_synthesis_builds_typed_surface() {
        use fraiseql_core::schema::FieldType;

        use crate::schema::SchemaConverter;
        let create_post = IntermediateMutation {
            cascade: true,
            sql_source: Some("fn_create_post".to_string()),
            ..make_mutation("createPost", "Post")
        };
        let schema = make_schema(vec![make_entity_type("Post")], vec![], vec![create_post]);
        let compiled = SchemaConverter::convert(schema).expect("convert");

        // CascadeNode interface exists and the queryable entity implements it.
        assert!(compiled.interfaces.iter().any(|i| i.name == "CascadeNode"));
        let post = compiled.types.iter().find(|t| t.name.as_str() == "Post").unwrap();
        assert!(
            post.implements.iter().any(|i| i == "CascadeNode"),
            "Post implements CascadeNode"
        );

        // CascadeOperation enum with the spec's three values.
        let op = compiled.enums.iter().find(|e| e.name == "CascadeOperation").expect("enum");
        for v in ["CREATED", "UPDATED", "DELETED"] {
            assert!(op.has_value(v), "CascadeOperation has {v}");
        }

        // UpdatedEntity carries operation + a non-null typed entity; DeletedEntity
        // carries no entity body (a deleted row has nothing to project).
        let updated = compiled.types.iter().find(|t| t.name.as_str() == "UpdatedEntity").unwrap();
        assert!(updated.find_field("operation").is_some(), "UpdatedEntity.operation");
        let entity_field = updated.find_field("entity").expect("UpdatedEntity.entity");
        assert!(!entity_field.nullable, "UpdatedEntity.entity is non-null");
        let deleted = compiled.types.iter().find(|t| t.name.as_str() == "DeletedEntity").unwrap();
        assert!(deleted.find_field("entity").is_none(), "DeletedEntity has no entity body");
        assert!(deleted.find_field("deletedAt").is_some(), "DeletedEntity.deletedAt");

        // CascadeUpdates references the split entry types + a metadata envelope.
        let updates = compiled.types.iter().find(|t| t.name.as_str() == "CascadeUpdates").unwrap();
        let updated_field = updates.find_field("updated").expect("CascadeUpdates.updated");
        assert!(
            matches!(&updated_field.field_type, FieldType::List(inner)
                if matches!(inner.as_ref(), FieldType::Object(n) if n == "UpdatedEntity")),
            "updated: [UpdatedEntity!]!"
        );
        assert!(updates.find_field("metadata").is_some(), "CascadeUpdates.metadata");
        assert!(updates.find_field("invalidations").is_some(), "CascadeUpdates.invalidations");
        let meta = compiled.types.iter().find(|t| t.name.as_str() == "CascadeMetadata").unwrap();
        for f in ["timestamp", "depth", "affectedCount", "truncated"] {
            assert!(meta.find_field(f).is_some(), "CascadeMetadata.{f}");
        }
        // The invalidation surface (type + its two enums) is synthesized.
        assert!(compiled.types.iter().any(|t| t.name.as_str() == "QueryInvalidation"));
        assert!(compiled.enums.iter().any(|e| e.name == "InvalidationStrategy"));
        assert!(compiled.enums.iter().any(|e| e.name == "InvalidationScope"));

        // The mutation returns CreatePostPayload { entity, cascade, updatedFields }.
        let m = compiled.mutations.iter().find(|m| m.name == "createPost").unwrap();
        assert_eq!(m.return_type, "CreatePostPayload");
        let payload =
            compiled.types.iter().find(|t| t.name.as_str() == "CreatePostPayload").unwrap();
        assert!(payload.find_field("entity").is_some(), "payload has entity");
        assert!(payload.find_field("cascade").is_some(), "payload has cascade");
        assert!(payload.find_field("updatedFields").is_some(), "payload rehomes updatedFields");
    }

    /// No `cascade = true` mutation ⇒ the pass is inert: no cascade types/enum, no
    /// `implements` churn, mutation return type unchanged (byte-identical to a
    /// schema compiled before the pass existed).
    #[test]
    fn cascade_synthesis_inert_without_cascade_mutations() {
        use crate::schema::SchemaConverter;
        let create_post = IntermediateMutation {
            sql_source: Some("fn_create_post".to_string()),
            ..make_mutation("createPost", "Post")
        };
        let schema = make_schema(vec![make_entity_type("Post")], vec![], vec![create_post]);
        let compiled = SchemaConverter::convert(schema).expect("convert");

        assert!(!compiled.interfaces.iter().any(|i| i.name == "CascadeNode"));
        assert!(!compiled.types.iter().any(|t| t.name.as_str() == "CascadeUpdates"));
        assert!(!compiled.enums.iter().any(|e| e.name == "CascadeOperation"));
        let m = compiled.mutations.iter().find(|m| m.name == "createPost").unwrap();
        assert_eq!(m.return_type, "Post");
        let post = compiled.types.iter().find(|t| t.name.as_str() == "Post").unwrap();
        assert!(!post.implements.iter().any(|i| i == "CascadeNode"));
    }

    // ── Node `id: ID!` conformance enforcement (Option B: hard error) ──────

    /// A view-backed entity with a chosen `id` field type — for conformance tests.
    fn entity_with_id(name: &str, id_ty: &str) -> IntermediateType {
        IntermediateType {
            sql_source: Some(format!("v_{}", name.to_lowercase())),
            ..make_type(name, vec![make_field("id", id_ty)])
        }
    }

    /// A cascade mutation returning `entity` — the opt-in that triggers synthesis.
    fn cascade_mut(name: &str, entity: &str) -> IntermediateMutation {
        IntermediateMutation {
            cascade: true,
            sql_source: Some(format!("fn_{}", name.to_lowercase())),
            ..make_mutation(name, entity)
        }
    }

    /// The regression: a cascade entity whose `id` is `UUID` (not `ID`) must fail
    /// with a legible, actionable error at synthesis — not a swallowed validator
    /// bail. The message names the type, its actual id type, and the remedy.
    #[test]
    fn cascade_uuid_id_entity_fails_with_actionable_error() {
        use crate::schema::SchemaConverter;
        let schema = make_schema(
            vec![entity_with_id("Order", "UUID")],
            vec![],
            vec![cascade_mut("createOrder", "Order")],
        );
        let err = SchemaConverter::convert(schema).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("Order"), "names the offending type: {msg}");
        assert!(msg.contains("UUID"), "names the actual id type (not just 'missing'): {msg}");
        assert!(msg.contains("id: ID!"), "states the required shape: {msg}");
        assert!(msg.contains("Fix:"), "offers a remedy: {msg}");
    }

    /// A cascade entity with no `id` field at all (e.g. keyed on another column)
    /// fails the same way, with the "no `id` field" wording.
    #[test]
    fn cascade_missing_id_entity_fails_with_actionable_error() {
        use crate::schema::SchemaConverter;
        let order = IntermediateType {
            sql_source: Some("v_order".to_string()),
            ..make_type("Order", vec![make_field("orderNumber", "String")])
        };
        let schema = make_schema(vec![order], vec![], vec![cascade_mut("createOrder", "Order")]);
        let err = SchemaConverter::convert(schema).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("Order") && msg.contains("no `id` field"), "{msg}");
        assert!(msg.contains("Fix:"), "offers a remedy: {msg}");
    }

    /// One error lists *every* offender, so a developer fixes them in one pass
    /// rather than whack-a-mole.
    #[test]
    fn cascade_conformance_error_aggregates_all_offenders() {
        use crate::schema::SchemaConverter;
        let schema = make_schema(
            vec![
                entity_with_id("Order", "UUID"),
                entity_with_id("Invoice", "Int"),
            ],
            vec![],
            vec![cascade_mut("createOrder", "Order")],
        );
        let msg = format!("{:#}", SchemaConverter::convert(schema).unwrap_err());
        assert!(msg.contains("Order") && msg.contains("Invoice"), "lists both offenders: {msg}");
    }

    /// Happy-path guard: a conformant `id: ID!` cascade entity still synthesizes
    /// and implements `CascadeNode` (the fix must not over-correct).
    #[test]
    fn cascade_conformant_id_entity_still_implements_cascade_node() {
        use crate::schema::SchemaConverter;
        let schema = make_schema(
            vec![entity_with_id("Order", "ID")],
            vec![],
            vec![cascade_mut("createOrder", "Order")],
        );
        let compiled =
            SchemaConverter::convert(schema).expect("conformant cascade schema compiles");
        let order = compiled.types.iter().find(|t| t.name.as_str() == "Order").unwrap();
        assert!(order.implements.iter().any(|i| i == "CascadeNode"));
    }

    /// The identical latent defect in Relay `Node` injection is now a legible error
    /// too (previously a swallowed validator bail).
    #[test]
    fn relay_uuid_id_type_fails_with_actionable_error() {
        use crate::schema::SchemaConverter;
        let mut order = entity_with_id("Order", "UUID");
        order.relay = true;
        let schema = make_schema(vec![order], vec![make_query("orders", "Order")], vec![]);
        let msg = format!("{:#}", SchemaConverter::convert(schema).unwrap_err());
        assert!(msg.contains("Order") && msg.contains("relay"), "{msg}");
        assert!(msg.contains("Fix:"), "offers a remedy: {msg}");
    }

    /// Phase-2 soundness invariant: no synthesis pass may emit IR that `validate()`
    /// rejects with a swallowed bail. Across id types × interface-forcing features,
    /// `convert()` must either succeed or fail with a *legible* synthesis error
    /// (a `Fix:` remedy) — never leak the raw validator "…is missing field 'id'".
    #[test]
    fn synthesis_never_leaks_a_raw_validator_bail() {
        use crate::schema::SchemaConverter;

        let mut cases: Vec<(String, bool, IntermediateSchema)> = Vec::new();
        for (id_ty, expect_ok) in [("ID", true), ("UUID", false), ("Int", false)] {
            cases.push((
                format!("cascade+{id_ty}"),
                expect_ok,
                make_schema(
                    vec![entity_with_id("Order", id_ty)],
                    vec![],
                    vec![cascade_mut("createOrder", "Order")],
                ),
            ));
            let mut order = entity_with_id("Order", id_ty);
            order.relay = true;
            cases.push((
                format!("relay+{id_ty}"),
                expect_ok,
                make_schema(vec![order], vec![make_query("orders", "Order")], vec![]),
            ));
        }

        for (label, expect_ok, schema) in cases {
            match SchemaConverter::convert(schema) {
                Ok(_) => assert!(expect_ok, "{label} unexpectedly compiled"),
                Err(e) => {
                    let msg = format!("{e:#}");
                    assert!(!expect_ok, "{label} unexpectedly failed: {msg}");
                    assert!(msg.contains("Fix:"), "{label} error is not legible: {msg}");
                    assert!(
                        !msg.contains("but is missing field"),
                        "{label} leaked the raw validator bail: {msg}"
                    );
                },
            }
        }
    }

    // ── validate(): interfaces are valid return types (latent-gap fix) ─────

    /// An interface named `Node` with a single `id: ID!` field, for return-type tests.
    fn node_interface() -> crate::schema::intermediate::IntermediateInterface {
        crate::schema::intermediate::IntermediateInterface {
            name:        "Node".to_string(),
            fields:      vec![make_field("id", "ID")],
            description: None,
        }
    }

    /// A query may return an interface (narrowed via inline fragments). Previously
    /// interfaces weren't in the return-type registry, so this failed the reference
    /// check (with a `warn!`); now it converts.
    #[test]
    fn query_returning_an_interface_is_valid() {
        use crate::schema::SchemaConverter;
        let mut schema = make_schema(vec![], vec![make_query("node", "Node")], vec![]);
        schema.interfaces = vec![node_interface()];
        SchemaConverter::convert(schema).expect("a query may return an interface type");
    }

    /// Same for a mutation return type — previously a *silent* bail (no `warn!`).
    #[test]
    fn mutation_returning_an_interface_is_valid() {
        use crate::schema::SchemaConverter;
        let mut schema = make_schema(vec![], vec![], vec![make_mutation("promote", "Node")]);
        schema.interfaces = vec![node_interface()];
        SchemaConverter::convert(schema).expect("a mutation may return an interface type");
    }

    #[test]
    fn auto_inject_uses_custom_claim() {
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![make_query("getUser", "User")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "org_id").unwrap();
        assert_eq!(schema.queries[0].inject.get("tenant_id"), Some(&"jwt:org_id".to_string()));
    }

    // ── Existing inject accepted ────────────────────────────────────────

    #[test]
    fn existing_inject_with_tenant_field_accepted() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), "jwt:tenant_id".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![IntermediateQuery {
                name: "getUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
    }

    // ── Error: explicit inject missing tenant ───────────────────────────

    #[test]
    fn error_when_inject_overridden_without_tenant() {
        let mut inject = IndexMap::new();
        inject.insert("user_id".to_string(), "jwt:sub".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![IntermediateQuery {
                name: "getUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
            vec![],
        );
        let err = validate_tenant_annotations(&mut schema, "tenant_id").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("getUser"), "error should mention query name: {msg}");
        assert!(msg.contains("@tenant_id"), "error should mention directive: {msg}");
        assert!(msg.contains("tenant_id"), "error should mention field: {msg}");
    }

    #[test]
    fn error_mutation_inject_overridden_without_tenant() {
        let mut inject = IndexMap::new();
        inject.insert("user_id".to_string(), "jwt:sub".to_string());
        let mut schema = make_schema(
            vec![make_type("User", vec![make_tenant_id_field("tenant_id")])],
            vec![],
            vec![IntermediateMutation {
                name: "createUser".to_string(),
                return_type: "User".to_string(),
                inject,
                ..Default::default()
            }],
        );
        let err = validate_tenant_annotations(&mut schema, "tenant_id").unwrap_err();
        assert!(err.to_string().contains("createUser"));
    }

    // ── No-op for non-annotated types ───────────────────────────────────

    #[test]
    fn query_on_non_annotated_type_unchanged() {
        let mut schema = make_schema(
            vec![make_type("Post", vec![make_field("id", "Int")])],
            vec![make_query("getPosts", "Post")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
        assert!(schema.queries[0].inject.is_empty());
    }

    // ── Warning when no annotations ─────────────────────────────────────

    #[test]
    fn warning_when_no_tenant_id_annotations() {
        let mut schema = make_schema(
            vec![make_type("User", vec![make_field("id", "Int")])],
            vec![make_query("getUser", "User")],
            vec![],
        );
        validate_tenant_annotations(&mut schema, "tenant_id").unwrap();
    }
}

// ── converter types tests ───────────────────────────────────────────────────

mod types_tests {
    use super::super::SchemaConverter;

    #[test]
    fn test_is_safe_sql_identifier_simple() {
        assert!(SchemaConverter::is_safe_sql_identifier("v_user"));
    }

    #[test]
    fn test_is_safe_sql_identifier_schema_qualified() {
        assert!(SchemaConverter::is_safe_sql_identifier("public.v_user"));
    }

    #[test]
    fn test_is_safe_sql_identifier_three_part() {
        assert!(SchemaConverter::is_safe_sql_identifier("catalog.schema.table"));
    }

    #[test]
    fn test_is_safe_sql_identifier_empty_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier(""));
    }

    #[test]
    fn test_is_safe_sql_identifier_leading_dot_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier(".foo"));
    }

    #[test]
    fn test_is_safe_sql_identifier_trailing_dot_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier("foo."));
    }

    #[test]
    fn test_is_safe_sql_identifier_double_dot_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier("foo..bar"));
    }

    #[test]
    fn test_is_safe_sql_identifier_four_parts_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier("a.b.c.d"));
    }

    #[test]
    fn test_is_safe_sql_identifier_special_chars_rejected() {
        assert!(!SchemaConverter::is_safe_sql_identifier("v_user; DROP TABLE"));
    }
}

mod changelog_validation_tests {
    use fraiseql_core::schema::ChangelogConfig;
    use serde_json::json;

    use crate::schema::{converter::SchemaConverter, intermediate::IntermediateSchema};

    fn intermediate_with(
        changelog: Option<ChangelogConfig>,
        observers_config: Option<serde_json::Value>,
    ) -> IntermediateSchema {
        IntermediateSchema {
            version: "2.0.0".to_string(),
            changelog_config: changelog,
            observers_config,
            ..IntermediateSchema::default()
        }
    }

    #[test]
    fn changelog_expose_without_observers_is_rejected() {
        let intermediate = intermediate_with(
            Some(ChangelogConfig {
                expose: true,
                ..Default::default()
            }),
            None,
        );
        let err = SchemaConverter::convert(intermediate).unwrap_err();
        assert!(
            err.to_string().contains("[observers]"),
            "error should explain the observers prerequisite, got: {err}"
        );
    }

    #[test]
    fn changelog_expose_with_disabled_observers_is_rejected() {
        let intermediate = intermediate_with(
            Some(ChangelogConfig {
                expose: true,
                ..Default::default()
            }),
            Some(json!({ "enabled": false, "backend": "redis" })),
        );
        assert!(SchemaConverter::convert(intermediate).is_err());
    }

    #[test]
    fn changelog_expose_with_enabled_observers_is_allowed() {
        let intermediate = intermediate_with(
            Some(ChangelogConfig {
                expose: true,
                ..Default::default()
            }),
            Some(json!({ "enabled": true, "backend": "redis" })),
        );
        let compiled = SchemaConverter::convert(intermediate).expect("should convert");
        assert!(compiled.changelog.is_some());
        assert!(compiled.changelog.as_ref().unwrap().expose);

        // The injection ran (after rich filters, before validate) and validate()
        // accepted the schema-qualified view sql_source + the generated operations.
        assert!(compiled.types.iter().any(|t| t.name == "EntityChangeLog"));
        assert!(compiled.types.iter().any(|t| t.name == "TransportCheckpoint"));
        assert!(compiled.queries.iter().any(|q| q.name == "entity_change_logs"));
        assert!(compiled.queries.iter().any(|q| q.name == "transport_checkpoint"));
        assert!(compiled.mutations.iter().any(|m| m.name == "upsert_transport_checkpoint"));
    }

    #[test]
    fn changelog_disabled_does_not_require_observers() {
        // expose = false must never trip the observers gate.
        let intermediate = intermediate_with(
            Some(ChangelogConfig {
                expose: false,
                ..Default::default()
            }),
            None,
        );
        assert!(SchemaConverter::convert(intermediate).is_ok());
    }

    #[test]
    fn changelog_expose_on_federation_subgraph_warns_but_compiles() {
        // #497: exposing the change-log on a federation subgraph is the single-owner
        // pattern — allowed. The cross-subgraph collision can't be detected here (each
        // subgraph compiles alone), so the guardrail is a compile warning, never an
        // error; the owning subgraph legitimately sets both.
        let intermediate = IntermediateSchema {
            version: "2.0.0".to_string(),
            changelog_config: Some(ChangelogConfig {
                expose: true,
                ..Default::default()
            }),
            observers_config: Some(json!({ "enabled": true, "backend": "redis" })),
            federation_config: Some(json!({ "enabled": true })),
            ..IntermediateSchema::default()
        };
        let compiled =
            SchemaConverter::convert(intermediate).expect("federated expose warns, never errors");
        assert!(compiled.changelog.as_ref().unwrap().expose);
        assert!(compiled.federation.as_ref().unwrap().enabled);
        assert!(compiled.types.iter().any(|t| t.name == "EntityChangeLog"));
    }
}
