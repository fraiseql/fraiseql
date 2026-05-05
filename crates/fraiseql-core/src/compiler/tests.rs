//! Tests for the `compiler` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

// ---------------------------------------------------------------------------
// mod.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod compiler_tests {
    use crate::{
        compiler::fact_table::{DimensionColumn, FactTableMetadata},
        schema::CompiledSchema,
    };

    #[test]
    fn test_compiled_schema_fact_table_operations() {
        let mut schema = CompiledSchema::new();

        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        };

        schema.add_fact_table("tf_sales".to_string(), metadata.clone());

        assert!(schema.has_fact_tables());

        let tables = schema.list_fact_tables();
        assert_eq!(tables.len(), 1);
        assert!(tables.contains(&"tf_sales"));

        let retrieved = schema.get_fact_table("tf_sales");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &metadata);

        assert!(schema.get_fact_table("tf_nonexistent").is_none());
    }
}

// ---------------------------------------------------------------------------
// ir.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod ir_tests {
    use super::super::ir::{
        AutoParams, IRArgument, IRField, IRMutation, IRQuery, IRScalar, IRType, MutationOperation,
        AuthoringIR,
    };
    use crate::validation::rules::ValidationRule;

    #[test]
    fn test_authoring_ir_new() {
        let ir = AuthoringIR::new();
        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
        assert!(ir.mutations.is_empty());
        assert!(ir.subscriptions.is_empty());
    }

    #[test]
    fn test_authoring_ir_with_scalars() {
        let mut ir = AuthoringIR::new();

        // Add custom scalar
        ir.scalars.push(IRScalar::new("Email".to_string()));
        ir.scalars.push(IRScalar::new("ISBN".to_string()));

        assert_eq!(ir.scalars.len(), 2);
        assert_eq!(ir.scalars[0].name, "Email");
        assert_eq!(ir.scalars[1].name, "ISBN");
    }

    #[test]
    fn test_ir_type() {
        let ir_type = IRType {
            name:        "User".to_string(),
            fields:      vec![IRField {
                name:        "id".to_string(),
                field_type:  "Int!".to_string(),
                nullable:    false,
                description: None,
                sql_column:  Some("id".to_string()),
            }],
            sql_source:  Some("v_user".to_string()),
            description: Some("User type".to_string()),
        };

        assert_eq!(ir_type.name, "User");
        assert_eq!(ir_type.fields.len(), 1);
        assert_eq!(ir_type.sql_source, Some("v_user".to_string()));
    }

    #[test]
    fn test_ir_query() {
        let query = IRQuery {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams {
                has_where: true,
                has_limit: true,
                ..Default::default()
            },
        };

        assert_eq!(query.name, "users");
        assert!(query.returns_list);
        assert!(query.auto_params.has_where);
        assert!(query.auto_params.has_limit);
    }

    #[test]
    fn test_ir_mutation() {
        let mutation = IRMutation {
            name:        "createUser".to_string(),
            return_type: "User".to_string(),
            nullable:    false,
            arguments:   vec![IRArgument {
                name:          "input".to_string(),
                arg_type:      "CreateUserInput!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            description: None,
            operation:   MutationOperation::Create,
        };

        assert_eq!(mutation.name, "createUser");
        assert_eq!(mutation.operation, MutationOperation::Create);
        assert_eq!(mutation.arguments.len(), 1);
    }

    #[test]
    fn test_auto_params_default() {
        let params = AutoParams::default();
        assert!(!params.has_where);
        assert!(!params.has_order_by);
        assert!(!params.has_limit);
        assert!(!params.has_offset);
    }

    #[test]
    fn test_mutation_operations() {
        assert_eq!(MutationOperation::Create, MutationOperation::Create);
        assert_ne!(MutationOperation::Create, MutationOperation::Update);
    }

    #[test]
    fn test_ir_scalar_new() {
        let scalar = IRScalar::new("Email".to_string());

        assert_eq!(scalar.name, "Email");
        assert_eq!(scalar.description, None);
        assert_eq!(scalar.specified_by_url, None);
        assert_eq!(scalar.validation_rules.len(), 0);
        assert_eq!(scalar.base_type, None);
    }

    #[test]
    fn test_ir_scalar_with_all_fields() {
        let scalar = IRScalar {
            name:             "Email".to_string(),
            description:      Some("Valid email address".to_string()),
            specified_by_url: Some("https://html.spec.whatwg.org/".to_string()),
            validation_rules: vec![ValidationRule::Pattern {
                pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
                message: Some("Invalid email format".to_string()),
            }],
            base_type:        Some("String".to_string()),
        };

        assert_eq!(scalar.name, "Email");
        assert_eq!(scalar.description, Some("Valid email address".to_string()));
        assert_eq!(scalar.specified_by_url, Some("https://html.spec.whatwg.org/".to_string()));
        assert_eq!(scalar.validation_rules.len(), 1);
        assert_eq!(scalar.base_type, Some("String".to_string()));
    }

    #[test]
    fn test_ir_scalar_serialization() {
        let scalar = IRScalar {
            name:             "ISBN".to_string(),
            description:      Some("International Standard Book Number".to_string()),
            specified_by_url: Some("https://www.isbn-international.org/".to_string()),
            validation_rules: vec![],
            base_type:        None,
        };

        // Serialize to JSON
        let json = serde_json::to_value(&scalar).expect("Should serialize");

        // Verify structure
        assert_eq!(json["name"], "ISBN");
        assert_eq!(json["description"], "International Standard Book Number");
        assert_eq!(json["specified_by_url"], "https://www.isbn-international.org/");
        assert_eq!(json["validation_rules"], serde_json::json!([]));
    }

    #[test]
    fn test_ir_scalar_deserialization() {
        let json = serde_json::json!({
            "name": "PhoneNumber",
            "description": "Valid phone number",
            "specified_by_url": null,
            "validation_rules": [],
            "base_type": "String"
        });

        let scalar: IRScalar = serde_json::from_value(json).expect("Should deserialize");

        assert_eq!(scalar.name, "PhoneNumber");
        assert_eq!(scalar.description, Some("Valid phone number".to_string()));
        assert_eq!(scalar.specified_by_url, None);
        assert_eq!(scalar.validation_rules.len(), 0);
        assert_eq!(scalar.base_type, Some("String".to_string()));
    }

    #[test]
    fn test_ir_scalar_equality() {
        let scalar1 = IRScalar {
            name:             "UUID".to_string(),
            description:      Some("Universal Unique Identifier".to_string()),
            specified_by_url: None,
            validation_rules: vec![],
            base_type:        None,
        };

        let scalar2 = IRScalar {
            name:             "UUID".to_string(),
            description:      Some("Universal Unique Identifier".to_string()),
            specified_by_url: None,
            validation_rules: vec![],
            base_type:        None,
        };

        assert_eq!(scalar1, scalar2);
    }
}

// ---------------------------------------------------------------------------
// parser.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod parser_tests {
    use super::super::ir::MutationOperation;
    use super::super::parser::SchemaParser;
    use crate::error::FraiseQLError;
    use crate::schema::GraphQLValue;

    #[test]
    fn test_parse_empty_schema() {
        let parser = SchemaParser::new();
        let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
        let ir = parser.parse(json).unwrap();

        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
        assert!(ir.mutations.is_empty());
        assert!(ir.subscriptions.is_empty());
    }

    #[test]
    fn test_parse_minimal_schema() {
        let parser = SchemaParser::new();
        let json = r"{}";
        let ir = parser.parse(json).unwrap();

        assert!(ir.types.is_empty());
        assert!(ir.queries.is_empty());
    }

    #[test]
    fn test_parse_type_with_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "types": [{
                "name": "User",
                "fields": [
                    {"name": "id", "type": "Int!", "nullable": false},
                    {"name": "name", "type": "String!", "nullable": false}
                ],
                "sql_source": "v_user"
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.types.len(), 1);
        assert_eq!(ir.types[0].name, "User");
        assert_eq!(ir.types[0].fields.len(), 2);
        assert_eq!(ir.types[0].sql_source, Some("v_user".to_string()));
    }

    #[test]
    fn test_parse_query_with_auto_params() {
        let parser = SchemaParser::new();
        let json = r#"{
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
                "nullable": false,
                "sql_source": "v_user",
                "auto_params": {
                    "has_where": true,
                    "has_limit": true
                }
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.queries.len(), 1);
        assert_eq!(ir.queries[0].name, "users");
        assert!(ir.queries[0].returns_list);
        assert!(ir.queries[0].auto_params.has_where);
        assert!(ir.queries[0].auto_params.has_limit);
    }

    #[test]
    fn test_parse_mutation() {
        let parser = SchemaParser::new();
        let json = r#"{
            "mutations": [{
                "name": "createUser",
                "return_type": "User",
                "nullable": false,
                "operation": "create",
                "arguments": [
                    {"name": "input", "type": "CreateUserInput!", "nullable": false}
                ]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.mutations.len(), 1);
        assert_eq!(ir.mutations[0].name, "createUser");
        assert_eq!(ir.mutations[0].operation, MutationOperation::Create);
        assert_eq!(ir.mutations[0].arguments.len(), 1);
    }

    #[test]
    fn test_parse_mutation_operation_case_insensitive() {
        // Test that operation strings are accepted in any case (lowercase, uppercase, mixed)
        let parser = SchemaParser::new();

        let cases: &[(&str, MutationOperation)] = &[
            ("create", MutationOperation::Create),
            ("CREATE", MutationOperation::Create),
            ("Create", MutationOperation::Create),
            ("update", MutationOperation::Update),
            ("UPDATE", MutationOperation::Update),
            ("delete", MutationOperation::Delete),
            ("DELETE", MutationOperation::Delete),
            ("custom", MutationOperation::Custom),
            ("CUSTOM", MutationOperation::Custom),
        ];

        for (op_str, expected) in cases {
            let json = format!(
                r#"{{"mutations": [{{"name": "m", "return_type": "T", "nullable": false, "operation": "{op_str}", "arguments": []}}]}}"#
            );
            let ir = parser.parse(&json).unwrap_or_else(|e| {
                panic!("Expected parse to succeed for operation {op_str:?}, got error: {e}")
            });
            assert_eq!(
                ir.mutations[0].operation, *expected,
                "operation {op_str:?} should map to {expected:?}"
            );
        }
    }

    #[test]
    fn test_parse_mutation_operation_missing_defaults_to_custom() {
        let parser = SchemaParser::new();
        let json = r#"{"mutations": [{"name": "m", "return_type": "T", "nullable": false, "arguments": []}]}"#;
        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.mutations[0].operation, MutationOperation::Custom);
    }

    #[test]
    fn test_parse_mutation_operation_typo_returns_error() {
        let parser = SchemaParser::new();
        let invalid_ops = &["creat", "CREAT", "updaet", "delet", "FUNCTION", "insert"];
        for op in invalid_ops {
            let json = format!(
                r#"{{"mutations": [{{"name": "m", "return_type": "T", "nullable": false, "operation": "{op}", "arguments": []}}]}}"#
            );
            let result = parser.parse(&json);
            assert!(
                result.is_err(),
                "Expected parse error for unknown operation {op:?}, but got Ok"
            );
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("unknown operation"),
                "Error for {op:?} should mention 'unknown operation', got: {err}"
            );
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let parser = SchemaParser::new();
        let json = "not valid json";
        let result = parser.parse(json);
        assert!(
            matches!(result, Err(FraiseQLError::Parse { .. })),
            "expected Parse error for invalid JSON, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_missing_required_field() {
        let parser = SchemaParser::new();
        let json = r#"{
            "types": [{
                "fields": []
            }]
        }"#;
        let result = parser.parse(json);
        assert!(
            matches!(result, Err(FraiseQLError::Parse { .. })),
            "expected Parse error for missing required field, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_interface_basic() {
        let parser = SchemaParser::new();
        let json = r#"{
            "interfaces": [{
                "name": "Node",
                "fields": [
                    {"name": "id", "type": "ID!", "nullable": false}
                ]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.interfaces.len(), 1);
        assert_eq!(ir.interfaces[0].name, "Node");
        assert_eq!(ir.interfaces[0].fields.len(), 1);
        assert_eq!(ir.interfaces[0].fields[0].name, "id");
    }

    #[test]
    fn test_parse_interface_with_multiple_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "interfaces": [{
                "name": "Timestamped",
                "fields": [
                    {"name": "createdAt", "type": "String!", "nullable": false},
                    {"name": "updatedAt", "type": "String!", "nullable": false}
                ],
                "description": "Records creation and update times"
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.interfaces[0].fields.len(), 2);
        assert_eq!(
            ir.interfaces[0].description,
            Some("Records creation and update times".to_string())
        );
    }

    #[test]
    fn test_parse_interface_with_empty_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "interfaces": [{
                "name": "Empty",
                "fields": []
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.interfaces.len(), 1);
        assert_eq!(ir.interfaces[0].fields.len(), 0);
    }

    #[test]
    fn test_parse_multiple_interfaces() {
        let parser = SchemaParser::new();
        let json = r#"{
            "interfaces": [
                {"name": "Node", "fields": []},
                {"name": "Auditable", "fields": []},
                {"name": "Publishable", "fields": []}
            ]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.interfaces.len(), 3);
        assert_eq!(ir.interfaces[0].name, "Node");
        assert_eq!(ir.interfaces[1].name, "Auditable");
        assert_eq!(ir.interfaces[2].name, "Publishable");
    }

    #[test]
    fn test_parse_interface_missing_name() {
        let parser = SchemaParser::new();
        let json = r#"{"interfaces": [{"fields": []}]}"#;
        let result = parser.parse(json);
        assert!(
            matches!(result, Err(FraiseQLError::Parse { .. })),
            "expected Parse error for interface missing name, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_union_basic() {
        let parser = SchemaParser::new();
        let json = r#"{
            "unions": [{
                "name": "SearchResult",
                "types": ["User", "Post"]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.unions.len(), 1);
        assert_eq!(ir.unions[0].name, "SearchResult");
        assert_eq!(ir.unions[0].types.len(), 2);
        assert_eq!(ir.unions[0].types[0], "User");
        assert_eq!(ir.unions[0].types[1], "Post");
    }

    #[test]
    fn test_parse_union_single_type() {
        let parser = SchemaParser::new();
        let json = r#"{
            "unions": [{
                "name": "Result",
                "types": ["Error"]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.unions[0].types.len(), 1);
        assert_eq!(ir.unions[0].types[0], "Error");
    }

    #[test]
    fn test_parse_union_with_description() {
        let parser = SchemaParser::new();
        let json = r#"{
            "unions": [{
                "name": "SearchResult",
                "types": ["User", "Post", "Comment"],
                "description": "Results from search"
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.unions[0].description, Some("Results from search".to_string()));
        assert_eq!(ir.unions[0].types.len(), 3);
    }

    #[test]
    fn test_parse_multiple_unions() {
        let parser = SchemaParser::new();
        let json = r#"{
            "unions": [
                {"name": "SearchResult", "types": ["User", "Post"]},
                {"name": "Error", "types": ["ValidationError", "NotFoundError"]},
                {"name": "Response", "types": ["Success", "Error"]}
            ]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.unions.len(), 3);
        assert_eq!(ir.unions[0].name, "SearchResult");
        assert_eq!(ir.unions[1].name, "Error");
        assert_eq!(ir.unions[2].name, "Response");
    }

    #[test]
    fn test_parse_union_missing_name() {
        let parser = SchemaParser::new();
        let json = r#"{"unions": [{"types": []}]}"#;
        let result = parser.parse(json);
        assert!(
            matches!(result, Err(FraiseQLError::Parse { .. })),
            "expected Parse error for union missing name, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_union_empty_types() {
        let parser = SchemaParser::new();
        let json = r#"{
            "unions": [{
                "name": "Empty",
                "types": []
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.unions[0].types.len(), 0);
    }

    #[test]
    fn test_parse_input_type_basic() {
        let parser = SchemaParser::new();
        let json = r#"{
            "input_types": [{
                "name": "UserInput",
                "fields": [
                    {"name": "name", "type": "String!", "nullable": false}
                ]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.input_types.len(), 1);
        assert_eq!(ir.input_types[0].name, "UserInput");
        assert_eq!(ir.input_types[0].fields.len(), 1);
        assert_eq!(ir.input_types[0].fields[0].name, "name");
    }

    #[test]
    fn test_parse_input_type_with_multiple_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "input_types": [{
                "name": "CreateUserInput",
                "fields": [
                    {"name": "name", "type": "String!", "nullable": false},
                    {"name": "email", "type": "String!", "nullable": false},
                    {"name": "age", "type": "Int", "nullable": true}
                ],
                "description": "Input for creating users"
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.input_types[0].fields.len(), 3);
        assert!(!ir.input_types[0].fields[0].nullable);
        assert!(ir.input_types[0].fields[2].nullable);
        assert_eq!(ir.input_types[0].description, Some("Input for creating users".to_string()));
    }

    #[test]
    fn test_parse_input_field_with_default_value() {
        let parser = SchemaParser::new();
        let json = r#"{
            "input_types": [{
                "name": "QueryInput",
                "fields": [
                    {"name": "limit", "type": "Int", "nullable": true, "default_value": 10},
                    {"name": "active", "type": "Boolean", "nullable": true, "default_value": true}
                ]
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.input_types[0].fields[0].default_value, Some(GraphQLValue::Int(10)));
        assert_eq!(ir.input_types[0].fields[1].default_value, Some(GraphQLValue::Boolean(true)));
    }

    #[test]
    fn test_parse_input_type_with_empty_fields() {
        let parser = SchemaParser::new();
        let json = r#"{
            "input_types": [{
                "name": "EmptyInput",
                "fields": []
            }]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.input_types[0].fields.len(), 0);
    }

    #[test]
    fn test_parse_multiple_input_types() {
        let parser = SchemaParser::new();
        let json = r#"{
            "input_types": [
                {"name": "UserInput", "fields": []},
                {"name": "PostInput", "fields": []},
                {"name": "FilterInput", "fields": []}
            ]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.input_types.len(), 3);
        assert_eq!(ir.input_types[0].name, "UserInput");
        assert_eq!(ir.input_types[1].name, "PostInput");
        assert_eq!(ir.input_types[2].name, "FilterInput");
    }

    #[test]
    fn test_parse_input_type_missing_name() {
        let parser = SchemaParser::new();
        let json = r#"{"input_types": [{"fields": []}]}"#;
        let result = parser.parse(json);
        assert!(
            matches!(result, Err(FraiseQLError::Parse { .. })),
            "expected Parse error for input type missing name, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_complete_schema_with_all_features() {
        let parser = SchemaParser::new();
        let json = r#"{
            "types": [{"name": "User", "fields": []}],
            "interfaces": [{"name": "Node", "fields": []}],
            "unions": [{"name": "SearchResult", "types": ["User"]}],
            "input_types": [{"name": "UserInput", "fields": []}],
            "queries": [{"name": "users", "return_type": "User", "returns_list": true}],
            "mutations": [{"name": "createUser", "return_type": "User", "operation": "create"}]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.types.len(), 1);
        assert_eq!(ir.interfaces.len(), 1);
        assert_eq!(ir.unions.len(), 1);
        assert_eq!(ir.input_types.len(), 1);
        assert_eq!(ir.queries.len(), 1);
        assert_eq!(ir.mutations.len(), 1);
    }

    #[test]
    fn test_parse_scalars() {
        let parser = SchemaParser::new();
        let json = r#"{
            "scalars": [
                {
                    "name": "Email",
                    "description": "Valid email address",
                    "specified_by_url": "https://html.spec.whatwg.org/",
                    "validation_rules": [],
                    "base_type": null
                },
                {
                    "name": "ISBN",
                    "description": "International Standard Book Number",
                    "specified_by_url": null,
                    "validation_rules": [],
                    "base_type": null
                }
            ]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.scalars.len(), 2);
        assert_eq!(ir.scalars[0].name, "Email");
        assert_eq!(ir.scalars[0].description, Some("Valid email address".to_string()));
        assert_eq!(
            ir.scalars[0].specified_by_url,
            Some("https://html.spec.whatwg.org/".to_string())
        );
        assert_eq!(ir.scalars[1].name, "ISBN");
    }

    #[test]
    fn test_parse_schema_with_scalars_and_types() {
        let parser = SchemaParser::new();
        let json = r#"{
            "scalars": [{"name": "Email", "description": null, "specified_by_url": null, "validation_rules": [], "base_type": null}],
            "types": [{"name": "User", "fields": []}],
            "queries": [{"name": "users", "return_type": "User", "returns_list": true}]
        }"#;

        let ir = parser.parse(json).unwrap();
        assert_eq!(ir.scalars.len(), 1);
        assert_eq!(ir.types.len(), 1);
        assert_eq!(ir.queries.len(), 1);
    }
}

// ---------------------------------------------------------------------------
// validator.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod validator_tests {
    use super::super::ir::{AutoParams, IRArgument, IRField, IRQuery, IRType};
    use super::super::validator::SchemaValidator;
    use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata, MeasureColumn, SqlType};
    use crate::compiler::ir::AuthoringIR;
    use crate::compiler::validator::extract_base_type;
    use crate::error::FraiseQLError;

    #[test]
    fn test_validator_new() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate new IR should succeed: {e}"));
    }

    #[test]
    fn test_validate_empty_ir() {
        let validator = SchemaValidator::new();
        let ir = AuthoringIR::new();
        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate empty IR should succeed: {e}"));
    }

    fn make_fact_table(measures: Vec<MeasureColumn>, dim_name: &str) -> FactTableMetadata {
        FactTableMetadata {
            table_name: String::new(),
            measures,
            dimensions: DimensionColumn {
                name:  dim_name.to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions: vec![],
        }
    }

    #[test]
    fn test_validate_fact_table_with_valid_metadata() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "tf_sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "data",
            ),
        );
        validator.validate(ir).unwrap_or_else(|e| {
            panic!("validate fact table with valid metadata should succeed: {e}")
        });
    }

    #[test]
    fn test_validate_fact_table_invalid_prefix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "data",
            ),
        );
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must start with 'tf_' prefix")),
            "expected Validation error about tf_ prefix, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_fact_table_empty_measures() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert("tf_sales".to_string(), make_fact_table(vec![], "data"));
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have at least one measure")),
            "expected Validation error about empty measures, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_fact_table_dimensions_missing_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();
        ir.fact_tables.insert(
            "tf_sales".to_string(),
            make_fact_table(
                vec![MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }],
                "",
            ),
        );
        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("dimensions missing 'name' field")),
            "expected Validation error about missing dimensions name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_aggregate_type_missing_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesAggregate".to_string(),
            fields:      vec![IRField {
                name:        "revenue_sum".to_string(),
                field_type:  "Float".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have a 'count' field")),
            "expected Validation error about missing count field, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_aggregate_type_with_count() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesAggregate".to_string(),
            fields:      vec![
                IRField {
                    name:        "count".to_string(),
                    field_type:  "Int!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "revenue_sum".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate aggregate type with count should succeed: {e}"));
    }

    #[test]
    fn test_validate_group_by_input_invalid_field_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesGroupByInput".to_string(),
            fields:      vec![IRField {
                name:        "category".to_string(),
                field_type:  "String".to_string(), // Should be Boolean
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must be Boolean")),
            "expected Validation error about Boolean requirement, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_group_by_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesGroupByInput".to_string(),
            fields:      vec![IRField {
                name:        "category".to_string(),
                field_type:  "Boolean".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        validator.validate(ir).unwrap_or_else(|e| {
            panic!("validate group by input with Boolean fields should succeed: {e}")
        });
    }

    #[test]
    fn test_validate_having_input_invalid_suffix() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesHavingInput".to_string(),
            fields:      vec![IRField {
                name:        "count".to_string(), // Missing operator suffix
                field_type:  "Int".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("must have operator suffix")),
            "expected Validation error about operator suffix, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_having_input_valid() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "SalesHavingInput".to_string(),
            fields:      vec![
                IRField {
                    name:        "count_gt".to_string(),
                    field_type:  "Int".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "revenue_sum_gte".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator.validate(ir).unwrap_or_else(|e| {
            panic!("validate having input with valid suffixes should succeed: {e}")
        });
    }

    // =========================================================================
    // Type and Query Validation Tests
    // =========================================================================

    #[test]
    fn test_extract_base_type() {
        assert_eq!(extract_base_type("String"), "String");
        assert_eq!(extract_base_type("String!"), "String");
        assert_eq!(extract_base_type("[String]"), "String");
        assert_eq!(extract_base_type("[String!]"), "String");
        assert_eq!(extract_base_type("[String!]!"), "String");
        assert_eq!(extract_base_type("  User  "), "User");
    }

    #[test]
    fn test_validate_type_with_valid_references() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "name".to_string(),
                    field_type:  "String!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  Some("v_user".to_string()),
            description: None,
        });

        // Define Post type that references User
        ir.types.push(IRType {
            name:        "Post".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "author".to_string(),
                    field_type:  "User".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  Some("v_post".to_string()),
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate type with valid references should succeed: {e}"));
    }

    #[test]
    fn test_validate_type_with_invalid_reference() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        "Post".to_string(),
            fields:      vec![IRField {
                name:        "author".to_string(),
                field_type:  "NonExistentType".to_string(),
                nullable:    true,
                description: None,
                sql_column:  None,
            }],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("references unknown type") && message.contains("NonExistentType")),
            "expected Validation error about unknown type reference, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_type_empty_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.types.push(IRType {
            name:        String::new(),
            fields:      vec![],
            sql_source:  None,
            description: None,
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("name cannot be empty")),
            "expected Validation error about empty type name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_query_with_valid_return_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![IRField {
                name:        "id".to_string(),
                field_type:  "ID!".to_string(),
                nullable:    false,
                description: None,
                sql_column:  None,
            }],
            sql_source:  Some("v_user".to_string()),
            description: None,
        });

        // Define query that returns User
        ir.queries.push(IRQuery {
            name:         "user".to_string(),
            return_type:  "User".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![IRArgument {
                name:          "id".to_string(),
                arg_type:      "ID!".to_string(),
                nullable:      false,
                default_value: None,
                description:   None,
            }],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams::default(),
        });

        validator.validate(ir).unwrap_or_else(|e| {
            panic!("validate query with valid return type should succeed: {e}")
        });
    }

    #[test]
    fn test_validate_query_with_invalid_return_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.queries.push(IRQuery {
            name:         "unknownQuery".to_string(),
            return_type:  "NonExistentType".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("returns unknown type") && message.contains("NonExistentType")),
            "expected Validation error about unknown return type, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_query_with_scalar_return_type() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Query returning scalar type (no custom type needed)
        ir.queries.push(IRQuery {
            name:         "serverTime".to_string(),
            return_type:  "DateTime".to_string(),
            returns_list: false,
            nullable:     false,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        validator.validate(ir).unwrap_or_else(|e| {
            panic!("validate query with scalar return type should succeed: {e}")
        });
    }

    #[test]
    fn test_validate_query_empty_name() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        ir.queries.push(IRQuery {
            name:         String::new(),
            return_type:  "String".to_string(),
            returns_list: false,
            nullable:     true,
            arguments:    vec![],
            sql_source:   None,
            description:  None,
            auto_params:  AutoParams::default(),
        });

        let result = validator.validate(ir);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("Query name cannot be empty")),
            "expected Validation error about empty query name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_list_type_references() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Define User type
        ir.types.push(IRType {
            name:        "User".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID!".to_string(),
                    nullable:    false,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "friends".to_string(),
                    field_type:  "[User!]".to_string(), // List of Users
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("validate list type references should succeed: {e}"));
    }

    #[test]
    fn test_validate_builtin_scalar_types() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Test all builtin scalars are recognized in type fields
        ir.types.push(IRType {
            name:        "TestType".to_string(),
            fields:      vec![
                IRField {
                    name:        "id".to_string(),
                    field_type:  "ID".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "name".to_string(),
                    field_type:  "String".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "age".to_string(),
                    field_type:  "Int".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "rating".to_string(),
                    field_type:  "Float".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "active".to_string(),
                    field_type:  "Boolean".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "created".to_string(),
                    field_type:  "DateTime".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "uid".to_string(),
                    field_type:  "UUID".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("all builtin scalars should be recognized: {e}"));
    }

    #[test]
    fn test_validate_rich_scalar_types() {
        let validator = SchemaValidator::new();
        let mut ir = AuthoringIR::new();

        // Test some rich scalars are recognized
        ir.types.push(IRType {
            name:        "Contact".to_string(),
            fields:      vec![
                IRField {
                    name:        "email".to_string(),
                    field_type:  "Email".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "phone".to_string(),
                    field_type:  "PhoneNumber".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "url".to_string(),
                    field_type:  "URL".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
                IRField {
                    name:        "ip".to_string(),
                    field_type:  "IPAddress".to_string(),
                    nullable:    true,
                    description: None,
                    sql_column:  None,
                },
            ],
            sql_source:  None,
            description: None,
        });

        validator
            .validate(ir)
            .unwrap_or_else(|e| panic!("rich scalars should be recognized: {e}"));
    }
}

// ---------------------------------------------------------------------------
// enum_validator.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod enum_validator_tests {
    use super::super::enum_validator::EnumValidator;
    use super::super::ir::{IREnum, IREnumValue};
    use crate::error::FraiseQLError;

    #[test]
    fn test_parse_simple_enum() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"name": "ACTIVE"},
                    {"name": "INACTIVE"}
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse simple enum should succeed: {e}"));
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "Status");
        assert_eq!(enums[0].values.len(), 2);
    }

    #[test]
    fn test_parse_enum_with_description() {
        let json = serde_json::json!([
            {
                "name": "UserStatus",
                "description": "User account status",
                "values": [
                    {
                        "name": "ACTIVE",
                        "description": "User is active"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse enum with description should succeed: {e}"));
        assert_eq!(enums[0].description, Some("User account status".to_string()));
        assert_eq!(enums[0].values[0].description, Some("User is active".to_string()));
    }

    #[test]
    fn test_parse_enum_with_deprecation() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {
                        "name": "OLD_STATUS",
                        "deprecationReason": "Use NEW_STATUS instead"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse enum with deprecation should succeed: {e}"));
        assert_eq!(
            enums[0].values[0].deprecation_reason,
            Some("Use NEW_STATUS instead".to_string())
        );
    }

    #[test]
    fn test_parse_multiple_enums() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [{"name": "ACTIVE"}]
            },
            {
                "name": "Priority",
                "values": [{"name": "HIGH"}, {"name": "LOW"}]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse multiple enums should succeed: {e}"));
        assert_eq!(enums.len(), 2);
    }

    #[test]
    fn test_enum_not_array() {
        let json = serde_json::json!({"name": "Status"});
        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for non-array enums, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_missing_name() {
        let json = serde_json::json!([
            {
                "values": [{"name": "ACTIVE"}]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing enum name, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_missing_values() {
        let json = serde_json::json!([
            {
                "name": "Status"
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing values field, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_empty_values() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": []
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty values, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_duplicate_values() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"name": "ACTIVE"},
                    {"name": "ACTIVE"}
                ]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for duplicate values, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_value_missing_name() {
        let json = serde_json::json!([
            {
                "name": "Status",
                "values": [
                    {"description": "Active status"}
                ]
            }
        ]);

        let result = EnumValidator::parse_enums(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing value name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_name_valid() {
        EnumValidator::validate_enum_name("Status")
            .unwrap_or_else(|e| panic!("'Status' should be valid: {e}"));
        EnumValidator::validate_enum_name("UserStatus")
            .unwrap_or_else(|e| panic!("'UserStatus' should be valid: {e}"));
        EnumValidator::validate_enum_name("Status2")
            .unwrap_or_else(|e| panic!("'Status2' should be valid: {e}"));
    }

    #[test]
    fn test_validate_enum_name_invalid_start() {
        let result = EnumValidator::validate_enum_name("2Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for name starting with digit, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_name_invalid_chars() {
        let result1 = EnumValidator::validate_enum_name("Status-Type");
        assert!(
            matches!(result1, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for hyphen in name, got: {result1:?}"
        );
        let result2 = EnumValidator::validate_enum_name("Status Type");
        assert!(
            matches!(result2, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for space in name, got: {result2:?}"
        );
    }

    #[test]
    fn test_validate_enum_value_valid() {
        EnumValidator::validate_enum_value_name("ACTIVE", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE' should be valid: {e}"));
        EnumValidator::validate_enum_value_name("ACTIVE_STATUS", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE_STATUS' should be valid: {e}"));
        EnumValidator::validate_enum_value_name("ACTIVE_STATUS_2", "Status")
            .unwrap_or_else(|e| panic!("'ACTIVE_STATUS_2' should be valid: {e}"));
    }

    #[test]
    fn test_validate_enum_value_invalid_lowercase() {
        let result = EnumValidator::validate_enum_value_name("Active", "Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for lowercase value name, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_enum_value_invalid_start_underscore() {
        let result = EnumValidator::validate_enum_value_name("_ACTIVE", "Status");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for underscore-prefixed value, got: {result:?}"
        );
    }

    #[test]
    fn test_enum_name_empty() {
        let result = EnumValidator::validate_enum_name("");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty enum name, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_complex_enum_scenario() {
        let json = serde_json::json!([
            {
                "name": "OrderStatus",
                "description": "Order processing status",
                "values": [
                    {
                        "name": "PENDING",
                        "description": "Order awaiting processing"
                    },
                    {
                        "name": "PROCESSING",
                        "description": "Order is being processed"
                    },
                    {
                        "name": "COMPLETED",
                        "description": "Order has been completed"
                    },
                    {
                        "name": "CANCELLED",
                        "description": "Order was cancelled",
                        "deprecationReason": "Use VOID instead"
                    }
                ]
            }
        ]);

        let enums = EnumValidator::parse_enums(&json)
            .unwrap_or_else(|e| panic!("parse complex enum scenario should succeed: {e}"));
        assert_eq!(enums[0].name, "OrderStatus");
        assert_eq!(enums[0].values.len(), 4);
        assert!(enums[0].values[3].deprecation_reason.is_some());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let enum_val = IREnum {
            name:        "Status".to_string(),
            values:      vec![IREnumValue {
                name:               "ACTIVE".to_string(),
                description:        Some("Active status".to_string()),
                deprecation_reason: None,
            }],
            description: Some("Status enum".to_string()),
        };

        let json = serde_json::to_string(&enum_val).expect("serialize should work");
        let restored: IREnum = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.name, enum_val.name);
        assert_eq!(restored.values.len(), enum_val.values.len());
    }
}

// ---------------------------------------------------------------------------
// aggregate_types.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod aggregate_types_tests {
    use super::super::aggregate_types::{
        AggregateFieldKind, AggregateTypeGenerator, GroupByFieldKind, TemporalBucket,
    };
    use crate::compiler::fact_table::{DimensionColumn, MeasureColumn, SqlType, FactTableMetadata};

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_extract_type_name() {
        assert_eq!(AggregateTypeGenerator::extract_type_name("tf_sales").unwrap(), "Sales");
        assert_eq!(
            AggregateTypeGenerator::extract_type_name("tf_api_requests").unwrap(),
            "ApiRequests"
        );
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(AggregateTypeGenerator::to_pascal_case("sales"), "Sales");
        assert_eq!(AggregateTypeGenerator::to_pascal_case("api_requests"), "ApiRequests");
        assert_eq!(AggregateTypeGenerator::to_pascal_case("user_sessions"), "UserSessions");
    }

    #[test]
    fn test_generate_aggregate_type() {
        let metadata = create_test_metadata();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        assert_eq!(aggregate_type.name, "SalesAggregate");

        // Should have: count + (revenue: sum, avg, min, max) + (quantity: sum, avg, min, max) = 9
        // fields
        assert_eq!(aggregate_type.fields.len(), 9);

        // Check count field
        assert_eq!(aggregate_type.fields[0].name, "count");
        assert_eq!(aggregate_type.fields[0].field_type, "Int");
        assert!(!aggregate_type.fields[0].nullable);

        // Check revenue aggregates
        let revenue_sum = aggregate_type.fields.iter().find(|f| f.name == "revenue_sum").unwrap();
        assert_eq!(revenue_sum.field_type, "Float");
        assert!(revenue_sum.nullable);
    }

    #[test]
    fn test_generate_with_statistical() {
        let metadata = create_test_metadata();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, true).unwrap();

        // Should have additional stddev and variance for each measure
        // count + (revenue: 6) + (quantity: 6) = 13 fields
        assert_eq!(aggregate_type.fields.len(), 13);

        // Check statistical functions
        assert!(aggregate_type.fields.iter().any(|f| f.name == "revenue_stddev"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "revenue_variance"));
    }

    #[test]
    fn test_generate_having_input() {
        let metadata = create_test_metadata();
        let (_, _, having_input) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        assert_eq!(having_input.name, "SalesHaving");

        // Should have: count (6 operators) + revenue (4 functions × 6 operators) + quantity (4
        // functions × 6 operators) = 6 + 24 + 24 = 54 fields
        assert_eq!(having_input.fields.len(), 54);

        // Check count HAVING fields
        assert!(having_input.fields.iter().any(|f| f.name == "count_gt"));
        assert!(having_input.fields.iter().any(|f| f.name == "count_eq"));

        // Check measure HAVING fields
        assert!(having_input.fields.iter().any(|f| f.name == "revenue_sum_gt"));
        assert!(having_input.fields.iter().any(|f| f.name == "revenue_avg_gte"));
    }

    #[test]
    fn test_sql_type_to_graphql() {
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Int), "Int");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Decimal), "Float");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Text), "String");
        assert_eq!(AggregateTypeGenerator::sql_type_to_graphql(&SqlType::Uuid), "ID");
    }

    // ===========================================================================
    // Dimension Fields Tests
    // ===========================================================================

    fn create_metadata_with_dimensions() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![
                    DimensionPath {
                        name:      "category".to_string(),
                        json_path: "dimensions->>'category'".to_string(),
                        data_type: "string".to_string(),
                    },
                    DimensionPath {
                        name:      "region".to_string(),
                        json_path: "dimensions->>'region'".to_string(),
                        data_type: "string".to_string(),
                    },
                    DimensionPath {
                        name:      "priority".to_string(),
                        json_path: "dimensions->>'priority'".to_string(),
                        data_type: "integer".to_string(),
                    },
                ],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_generate_with_dimension_fields() {
        let metadata = create_metadata_with_dimensions();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Check aggregate type has dimension fields
        let category_field = aggregate_type.fields.iter().find(|f| f.name == "category");
        assert!(category_field.is_some());
        let category = category_field.unwrap();
        assert_eq!(category.field_type, "String");
        assert!(category.nullable);
        assert!(
            matches!(&category.kind, AggregateFieldKind::Dimension { path } if path == "dimensions->>'category'")
        );

        // Check integer dimension type
        let priority_field = aggregate_type.fields.iter().find(|f| f.name == "priority");
        assert!(priority_field.is_some());
        assert_eq!(priority_field.unwrap().field_type, "Int");

        // Check group_by has dimension fields
        assert!(group_by.fields.iter().any(|f| f.name == "category"));
        assert!(group_by.fields.iter().any(|f| f.name == "region"));
        assert!(group_by.fields.iter().any(|f| f.name == "priority"));
    }

    #[test]
    fn test_group_by_dimension_field_kind() {
        let metadata = create_metadata_with_dimensions();
        let (_, group_by, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        let category = group_by.fields.iter().find(|f| f.name == "category").unwrap();
        assert!(
            matches!(&category.kind, GroupByFieldKind::Dimension { path } if path == "dimensions->>'category'")
        );
    }

    // ===========================================================================
    // Calendar Dimension / Temporal Bucket Tests
    // ===========================================================================

    fn create_metadata_with_calendar_dimensions() -> FactTableMetadata {
        use crate::compiler::fact_table::{CalendarBucket, CalendarDimension, CalendarGranularity};

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![CalendarDimension {
                source_column: "occurred_at".to_string(),
                granularities: vec![CalendarGranularity {
                    column_name: "date_info".to_string(),
                    buckets:     vec![
                        CalendarBucket {
                            json_key:    "date".to_string(),
                            bucket_type: TemporalBucket::Day,
                            data_type:   "date".to_string(),
                        },
                        CalendarBucket {
                            json_key:    "month".to_string(),
                            bucket_type: TemporalBucket::Month,
                            data_type:   "integer".to_string(),
                        },
                        CalendarBucket {
                            json_key:    "year".to_string(),
                            bucket_type: TemporalBucket::Year,
                            data_type:   "integer".to_string(),
                        },
                    ],
                }],
            }],
        }
    }

    #[test]
    fn test_generate_with_calendar_dimensions() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Check aggregate type has temporal bucket fields
        let day_field = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_day");
        assert!(day_field.is_some());
        let day = day_field.unwrap();
        assert_eq!(day.field_type, "String"); // Date type maps to String
        assert!(day.nullable);
        assert!(matches!(&day.kind, AggregateFieldKind::TemporalBucket { column, bucket }
            if column == "date_info" && *bucket == TemporalBucket::Day));

        // Check integer bucket type
        let month_field = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_month");
        assert!(month_field.is_some());
        assert_eq!(month_field.unwrap().field_type, "Int");

        // Check group_by has temporal bucket fields
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_year"));
    }

    #[test]
    fn test_group_by_temporal_bucket_field_kind() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (_, group_by, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        let month = group_by.fields.iter().find(|f| f.name == "occurred_at_month").unwrap();
        assert!(matches!(&month.kind, GroupByFieldKind::TemporalBucket { column, bucket }
            if column == "date_info" && *bucket == TemporalBucket::Month));
    }

    // ===========================================================================
    // Fallback Temporal Buckets (from timestamp filter columns)
    // ===========================================================================

    fn create_metadata_with_timestamp_filter() -> FactTableMetadata {
        use crate::compiler::fact_table::FilterColumn;

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![], // No calendar dimensions
        }
    }

    #[test]
    fn test_generate_fallback_temporal_buckets() {
        let metadata = create_metadata_with_timestamp_filter();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should have fallback temporal buckets: day, week, month, year
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_week"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_year"));

        // All fallback buckets should be String type (from DATE_TRUNC)
        let day = aggregate_type.fields.iter().find(|f| f.name == "occurred_at_day").unwrap();
        assert_eq!(day.field_type, "String");

        // Group by should also have temporal buckets
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_day"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_week"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_year"));
    }

    #[test]
    fn test_no_fallback_when_calendar_dimensions_exist() {
        let metadata = create_metadata_with_calendar_dimensions();
        let (aggregate_type, _, _) = AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should only have calendar dimension buckets, not DATE_TRUNC fallbacks
        // Calendar dimensions have: day, month, year
        // Should NOT have: week (not in our calendar dimension)
        assert!(!aggregate_type.fields.iter().any(|f| f.name == "occurred_at_week"));
    }

    // ===========================================================================
    // Helper Function Tests
    // ===========================================================================

    #[test]
    fn test_dimension_type_to_graphql() {
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("string"), "String");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("integer"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("int"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("number"), "Int");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("float"), "Float");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("decimal"), "Float");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("boolean"), "Boolean");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("date"), "String");
        assert_eq!(AggregateTypeGenerator::dimension_type_to_graphql("unknown"), "String");
    }

    #[test]
    fn test_calendar_bucket_to_graphql() {
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("integer"), "Int");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("int"), "Int");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("date"), "String");
        assert_eq!(AggregateTypeGenerator::calendar_bucket_to_graphql("unknown"), "String");
    }

    // ===========================================================================
    // Combined Dimensions and Calendar Tests
    // ===========================================================================

    fn create_metadata_with_dimensions_and_calendar() -> FactTableMetadata {
        use crate::compiler::fact_table::{
            CalendarBucket, CalendarDimension, CalendarGranularity, DimensionPath,
        };

        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "dimensions->>'category'".to_string(),
                    data_type: "string".to_string(),
                }],
            },
            denormalized_filters: vec![],
            calendar_dimensions:  vec![CalendarDimension {
                source_column: "occurred_at".to_string(),
                granularities: vec![CalendarGranularity {
                    column_name: "date_info".to_string(),
                    buckets:     vec![CalendarBucket {
                        json_key:    "month".to_string(),
                        bucket_type: TemporalBucket::Month,
                        data_type:   "integer".to_string(),
                    }],
                }],
            }],
        }
    }

    #[test]
    fn test_generate_with_both_dimensions_and_calendar() {
        let metadata = create_metadata_with_dimensions_and_calendar();
        let (aggregate_type, group_by, _) =
            AggregateTypeGenerator::generate(&metadata, false).unwrap();

        // Should have both dimension fields and temporal bucket fields
        assert!(aggregate_type.fields.iter().any(|f| f.name == "category"));
        assert!(aggregate_type.fields.iter().any(|f| f.name == "occurred_at_month"));

        assert!(group_by.fields.iter().any(|f| f.name == "category"));
        assert!(group_by.fields.iter().any(|f| f.name == "occurred_at_month"));
    }
}

// ---------------------------------------------------------------------------
// window_allowlist.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod window_allowlist_tests {
    use super::super::window_allowlist::WindowAllowlist;
    use crate::compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    };
    use crate::error::FraiseQLError;

    fn test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "units".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![DimensionPath {
                    name:      "category".to_string(),
                    json_path: "dimensions->>'category'".to_string(),
                    data_type: "text".to_string(),
                }],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_measure_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("revenue", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_filter_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("occurred_at", "ORDER BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_dimension_short_name_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("category", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_dimension_full_json_path_accepted() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        al.validate("dimensions->>'category'", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok: {e}"));
    }

    #[test]
    fn test_unknown_field_rejected() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        assert!(
            matches!(
                al.validate("secret_column", "PARTITION BY"),
                Err(FraiseQLError::Validation { .. })
            ),
            "expected Validation error for unknown field"
        );
    }

    #[test]
    fn test_sql_injection_payloads_rejected() {
        let al = WindowAllowlist::from_metadata(&test_metadata());
        let payloads = [
            "'; DROP TABLE users; --",
            "1 UNION SELECT * FROM secrets",
            "field; DELETE FROM logs",
            "x\x00y",
            "field' OR '1'='1",
            "revenue--",
            "revenue UNION SELECT password FROM admin",
        ];
        for payload in &payloads {
            assert!(
                al.validate(payload, "PARTITION BY").is_err(),
                "Should reject payload: {payload}"
            );
        }
    }

    #[test]
    fn test_empty_allowlist_accepts_anything() {
        // When metadata has no known fields, allowlist is empty and validation is
        // skipped (character-level validation in the planner still applies).
        let al = WindowAllowlist::default();
        assert!(al.is_empty());
        al.validate("any_field", "PARTITION BY")
            .unwrap_or_else(|e| panic!("expected Ok for empty allowlist: {e}"));
        al.validate("'; DROP TABLE users; --", "ORDER BY")
            .unwrap_or_else(|e| panic!("expected Ok for empty allowlist: {e}"));
    }
}

// ---------------------------------------------------------------------------
// aggregation.rs tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod aggregation_tests {
    use super::super::aggregation::{
        AggregateExpression, AggregateSelection, AggregationPlanner, AggregationRequest,
        GroupByExpression, GroupBySelection, HavingCondition, OrderByClause, OrderDirection,
    };
    use crate::compiler::aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket};
    use crate::compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    };
    use crate::error::FraiseQLError;

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name:     "customer_id".to_string(),
                    sql_type: SqlType::Uuid,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed:  true,
                },
            ],
            calendar_dimensions:  vec![],
        }
    }

    #[test]
    fn test_plan_simple_aggregation() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![],
            aggregates:   vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
            ],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.aggregate_expressions.len(), 2);
        assert!(matches!(plan.aggregate_expressions[0], AggregateExpression::Count { .. }));
        assert!(matches!(
            plan.aggregate_expressions[1],
            AggregateExpression::MeasureAggregate { .. }
        ));
    }

    #[test]
    fn test_plan_with_group_by() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![
                GroupBySelection::Dimension {
                    path:  "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "occurred_at".to_string(),
                    bucket: TemporalBucket::Day,
                    alias:  "occurred_at_day".to_string(),
                },
            ],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.group_by_expressions.len(), 2);
        assert!(matches!(plan.group_by_expressions[0], GroupByExpression::JsonbPath { .. }));
        assert!(matches!(plan.group_by_expressions[1], GroupByExpression::TemporalBucket { .. }));
    }

    #[test]
    fn test_plan_with_having() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![AggregateSelection::MeasureAggregate {
                measure:  "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "revenue_sum".to_string(),
            }],
            having:       vec![HavingCondition {
                aggregate: AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
                operator:  HavingOperator::Gt,
                value:     serde_json::json!(1000),
            }],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = AggregationPlanner::plan(request, metadata).unwrap();

        assert_eq!(plan.having_conditions.len(), 1);
        assert_eq!(plan.having_conditions[0].operator, HavingOperator::Gt);
    }

    #[test]
    fn test_validate_invalid_measure() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![],
            aggregates:   vec![AggregateSelection::MeasureAggregate {
                measure:  "nonexistent".to_string(),
                function: AggregateFunction::Sum,
                alias:    "nonexistent_sum".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about measure not found, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_invalid_temporal_column() {
        let metadata = create_test_metadata();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::TemporalBucket {
                column: "nonexistent".to_string(),
                bucket: TemporalBucket::Day,
                alias:  "day".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about column not found, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_from_graphql_json_object_format() {
        let json = serde_json::json!({ "name": "DESC", "created_at": "ASC" });
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
        assert!(clauses.iter().any(|c| c.field == "name" && c.direction == OrderDirection::Desc));
        assert!(
            clauses
                .iter()
                .any(|c| c.field == "created_at" && c.direction == OrderDirection::Asc)
        );
    }

    #[test]
    fn test_order_by_from_graphql_json_array_format() {
        let json = serde_json::json!([
            { "field": "name", "direction": "DESC" },
            { "field": "age" }
        ]);
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].field, "name");
        assert_eq!(clauses[0].direction, OrderDirection::Desc);
        assert_eq!(clauses[1].field, "age");
        assert_eq!(clauses[1].direction, OrderDirection::Asc); // default
    }

    #[test]
    fn test_order_by_from_graphql_json_invalid_direction() {
        let json = serde_json::json!({ "name": "INVALID" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for invalid direction, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_sql_injection_in_field() {
        let json = serde_json::json!({ "x' || pg_sleep(5) || '": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for SQL injection in field, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_field_with_dot() {
        let json = serde_json::json!({ "a.b": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for dot in field name, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_rejects_empty_field() {
        let json = serde_json::json!({ "": "ASC" });
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for empty field name, got: {result:?}"
        );
    }

    #[test]
    fn test_order_by_accepts_valid_identifiers() {
        let json = serde_json::json!({ "created_at": "DESC", "_score": "ASC" });
        let clauses = OrderByClause::from_graphql_json(&json).unwrap();
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn test_order_by_array_rejects_injection_field() {
        let json = serde_json::json!([{ "field": "x' OR '1'='1", "direction": "ASC" }]);
        let result = OrderByClause::from_graphql_json(&json);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for SQL injection in array field, got: {result:?}"
        );
    }

    /// Helper: metadata with declared dimension paths (for allowlist tests).
    fn create_metadata_with_paths() -> FactTableMetadata {
        use crate::compiler::fact_table::DimensionPath;
        let mut meta = create_test_metadata();
        meta.dimensions.paths = vec![DimensionPath {
            name:      "category".to_string(),
            json_path: "dimensions->>'category'".to_string(),
            data_type: "text".to_string(),
        }];
        meta
    }

    #[test]
    fn test_dimension_allowlist_accepts_declared_path() {
        let metadata = create_metadata_with_paths();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        AggregationPlanner::plan(request, metadata)
            .unwrap_or_else(|e| panic!("declared dimension path should be accepted: {e}"));
    }

    #[test]
    fn test_dimension_allowlist_rejects_unknown_path() {
        let metadata = create_metadata_with_paths();
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "undeclared_path".to_string(),
                alias: "x".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        let result = AggregationPlanner::plan(request, metadata);
        assert!(
            matches!(&result, Err(FraiseQLError::Validation { message, .. }) if message.contains("not found")),
            "expected Validation error about undeclared dimension path, got: {result:?}"
        );
    }

    #[test]
    fn test_dimension_allowlist_accepts_any_path_when_paths_empty() {
        // When metadata.dimensions.paths is empty, any path is allowed
        // (schema did not declare a dimension allowlist).
        let metadata = create_test_metadata(); // paths: vec![]
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![GroupBySelection::Dimension {
                path:  "any_undeclared_path".to_string(),
                alias: "x".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        AggregationPlanner::plan(request, metadata)
            .unwrap_or_else(|e| panic!("any path should be accepted when paths empty: {e}"));
    }
}
