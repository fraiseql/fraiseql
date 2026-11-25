//! Tests for `directive_evaluator` module.

mod directive_evaluator_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use serde_json::Value as JsonValue;

    use super::super::*;
    use crate::graphql::types::{Directive, FieldSelection, GraphQLArgument};

    fn make_field(name: &str, directives: Vec<Directive>) -> FieldSelection {
        FieldSelection {
            name: name.to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives,
        }
    }

    fn make_directive(name: &str, if_value: &str) -> Directive {
        Directive {
            name:      name.to_string(),
            arguments: vec![GraphQLArgument {
                name:       "if".to_string(),
                value_type: "boolean".to_string(),
                value_json: if_value.to_string(),
            }],
        }
    }

    #[test]
    fn test_field_without_directives() {
        let field = make_field("email", vec![]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(result);
    }

    #[test]
    fn test_skip_with_true_literal() {
        let field = make_field("email", vec![make_directive("skip", "true")]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped
    }

    #[test]
    fn test_skip_with_false_literal() {
        let field = make_field("email", vec![make_directive("skip", "false")]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(result); // Should be included
    }

    #[test]
    fn test_include_with_true_literal() {
        let field = make_field("email", vec![make_directive("include", "true")]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(result); // Should be included
    }

    #[test]
    fn test_include_with_false_literal() {
        let field = make_field("email", vec![make_directive("include", "false")]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped
    }

    #[test]
    fn test_skip_with_variable() {
        let field = make_field("email", vec![make_directive("skip", r#"{"$var":"skipEmail"}"#)]);
        let mut variables = HashMap::new();
        variables.insert("skipEmail".to_string(), JsonValue::Bool(true));

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped when variable is true
    }

    #[test]
    fn test_include_with_variable() {
        let field =
            make_field("email", vec![make_directive("include", r#"{"$var":"includeEmail"}"#)]);
        let mut variables = HashMap::new();
        variables.insert("includeEmail".to_string(), JsonValue::Bool(false));

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped when variable is false
    }

    #[test]
    fn test_undefined_variable() {
        let field = make_field("email", vec![make_directive("skip", r#"{"$var":"undefined"}"#)]);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables);
        assert!(matches!(result, Err(DirectiveError::UndefinedVariable(_))));
    }

    #[test]
    fn test_multiple_directives() {
        // Both @skip and @include must pass
        let directives = vec![
            make_directive("skip", "false"),   // Don't skip
            make_directive("include", "true"), // Include
        ];
        let field = make_field("email", directives);
        let variables = HashMap::new();

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(result); // Should be included (both pass)
    }

    #[test]
    fn test_variable_type_mismatch() {
        let field = make_field("email", vec![make_directive("skip", r#"{"$var":"notABool"}"#)]);
        let mut variables = HashMap::new();
        variables.insert("notABool".to_string(), JsonValue::String("hello".to_string()));

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables);
        assert!(matches!(result, Err(DirectiveError::VariableTypeMismatch(_))));
    }

    #[test]
    fn test_filter_selections() {
        let selections = vec![
            make_field("id", vec![]),
            make_field("email", vec![make_directive("skip", "true")]),
            make_field("name", vec![make_directive("include", "true")]),
        ];

        let variables = HashMap::new();
        let filtered = DirectiveEvaluator::filter_selections(&selections, &variables).unwrap();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name, "id");
        assert_eq!(filtered[1].name, "name");
    }

    #[test]
    fn test_filter_nested_selections() {
        let selections = vec![FieldSelection {
            name:          "user".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![
                make_field("id", vec![]),
                make_field("secret", vec![make_directive("skip", "true")]),
            ],
            directives:    vec![],
        }];

        let variables = HashMap::new();
        let filtered = DirectiveEvaluator::filter_selections(&selections, &variables).unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].nested_fields.len(), 1);
        assert_eq!(filtered[0].nested_fields[0].name, "id");
    }

    #[test]
    fn test_evaluation_context_has_role() {
        let context = EvaluationContext::new(HashMap::new()).with_user_context(
            "roles",
            JsonValue::Array(vec![
                JsonValue::String("admin".to_string()),
                JsonValue::String("editor".to_string()),
            ]),
        );

        assert!(context.has_role("admin"));
        assert!(context.has_role("editor"));
        assert!(!context.has_role("viewer"));
    }

    #[test]
    fn test_evaluation_context_user_id() {
        let context = EvaluationContext::new(HashMap::new())
            .with_user_context("userId", JsonValue::String("user123".to_string()));

        assert_eq!(context.user_id(), Some("user123"));
    }

    #[test]
    fn test_evaluation_context_field_path() {
        let context = EvaluationContext::new(HashMap::new()).with_field_path("Query.users.email");

        assert_eq!(context.field_path.as_deref(), Some("Query.users.email"));
    }

    #[test]
    fn test_evaluation_context_operation_type() {
        let context =
            EvaluationContext::new(HashMap::new()).with_operation_type(OperationType::Mutation);

        assert_eq!(context.operation_type, Some(OperationType::Mutation));
    }

    #[test]
    fn test_directive_result_default() {
        assert_eq!(DirectiveResult::default(), DirectiveResult::Include);
    }

    #[test]
    fn test_parse_directive_args() {
        let directive = Directive {
            name:      "test".to_string(),
            arguments: vec![
                GraphQLArgument {
                    name:       "limit".to_string(),
                    value_type: "Int".to_string(),
                    value_json: "10".to_string(),
                },
                GraphQLArgument {
                    name:       "name".to_string(),
                    value_type: "String".to_string(),
                    value_json: "\"hello\"".to_string(),
                },
            ],
        };

        let variables = HashMap::new();
        let args = DirectiveEvaluator::parse_directive_args(&directive, &variables).unwrap();

        assert_eq!(args.get("limit"), Some(&JsonValue::Number(10.into())));
        assert_eq!(args.get("name"), Some(&JsonValue::String("hello".to_string())));
    }

    #[test]
    fn test_parse_directive_args_with_variable() {
        let directive = Directive {
            name:      "test".to_string(),
            arguments: vec![GraphQLArgument {
                name:       "limit".to_string(),
                value_type: "Int".to_string(),
                value_json: r#"{"$var":"myLimit"}"#.to_string(),
            }],
        };

        let mut variables = HashMap::new();
        variables.insert("myLimit".to_string(), JsonValue::Number(25.into()));

        let args = DirectiveEvaluator::parse_directive_args(&directive, &variables).unwrap();
        assert_eq!(args.get("limit"), Some(&JsonValue::Number(25.into())));
    }
}
