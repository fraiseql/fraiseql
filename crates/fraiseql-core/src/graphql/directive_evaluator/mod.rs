//! Directive evaluation for GraphQL queries.
//!
//! Handles:
//! - `@skip` directive (conditionally skip a field)
//! - `@include` directive (conditionally include a field)
//! - Custom directive framework for extensibility

mod custom;
mod evaluator;
mod types;

pub use custom::CustomDirectiveEvaluator;
pub use evaluator::DirectiveEvaluator;
pub use types::{
    DirectiveError, DirectiveHandler, DirectiveResult, EvaluationContext, OperationType,
};

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::{collections::HashMap, sync::Arc};

    use serde_json::Value as JsonValue;

    use crate::graphql::types::{Directive, FieldSelection, GraphQLArgument};

    use super::*;

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
        let field = make_field("email", vec![make_directive("skip", "\"$skipEmail\"")]);
        let mut variables = HashMap::new();
        variables.insert("skipEmail".to_string(), JsonValue::Bool(true));

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped when variable is true
    }

    #[test]
    fn test_include_with_variable() {
        let field = make_field("email", vec![make_directive("include", "\"$includeEmail\"")]);
        let mut variables = HashMap::new();
        variables.insert("includeEmail".to_string(), JsonValue::Bool(false));

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
        assert!(!result); // Should be skipped when variable is false
    }

    #[test]
    fn test_undefined_variable() {
        let field = make_field("email", vec![make_directive("skip", "\"$undefined\"")]);
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
        let field = make_field("email", vec![make_directive("skip", "\"$notABool\"")]);
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

    // ==========================================================================
    // Custom Directive Framework Tests
    // ==========================================================================

    /// A test directive that checks for a specific role.
    struct AuthDirective {
        required_role: String,
    }

    #[allow(clippy::unnecessary_literal_bound)] // Reason: DirectiveHandler trait requires &str return, literal bound is clearest for test impls
    impl DirectiveHandler for AuthDirective {
        fn name(&self) -> &str {
            "auth"
        }

        fn evaluate(
            &self,
            args: &HashMap<String, JsonValue>,
            context: &EvaluationContext,
        ) -> Result<DirectiveResult, DirectiveError> {
            // Check if role is specified in directive args
            let required = args.get("role").and_then(|v| v.as_str()).unwrap_or(&self.required_role);

            if context.has_role(required) {
                Ok(DirectiveResult::Include)
            } else {
                Ok(DirectiveResult::Skip)
            }
        }
    }

    /// A test directive that always skips.
    struct AlwaysSkipDirective;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: DirectiveHandler trait requires &str return, literal bound is clearest for test impls
    impl DirectiveHandler for AlwaysSkipDirective {
        fn name(&self) -> &str {
            "alwaysSkip"
        }

        fn evaluate(
            &self,
            _args: &HashMap<String, JsonValue>,
            _context: &EvaluationContext,
        ) -> Result<DirectiveResult, DirectiveError> {
            Ok(DirectiveResult::Skip)
        }
    }

    /// A test directive that returns an error.
    struct ErrorDirective;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: DirectiveHandler trait requires &str return, literal bound is clearest for test impls
    impl DirectiveHandler for ErrorDirective {
        fn name(&self) -> &str {
            "error"
        }

        fn evaluate(
            &self,
            _args: &HashMap<String, JsonValue>,
            _context: &EvaluationContext,
        ) -> Result<DirectiveResult, DirectiveError> {
            Ok(DirectiveResult::Error("Test error".to_string()))
        }
    }

    #[test]
    fn test_custom_directive_evaluator_creation() {
        let evaluator = CustomDirectiveEvaluator::new();
        assert!(!evaluator.has_handler("auth"));
        assert!(evaluator.handler_names().is_empty());
    }

    #[test]
    fn test_custom_directive_handler_registration() {
        let auth = Arc::new(AuthDirective {
            required_role: "admin".to_string(),
        });
        let evaluator = CustomDirectiveEvaluator::new().with_handler(auth);

        assert!(evaluator.has_handler("auth"));
        assert!(!evaluator.has_handler("unknown"));
        assert_eq!(evaluator.handler_names(), vec!["auth"]);
    }

    #[test]
    fn test_custom_directive_with_context() {
        let auth = Arc::new(AuthDirective {
            required_role: "admin".to_string(),
        });
        let evaluator = CustomDirectiveEvaluator::new().with_handler(auth);

        // Create a context with admin role
        let context = EvaluationContext::new(HashMap::new()).with_user_context(
            "roles",
            JsonValue::Array(vec![JsonValue::String("admin".to_string())]),
        );

        // Create a field with @auth directive
        let field = FieldSelection {
            name:          "sensitiveData".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![Directive {
                name:      "auth".to_string(),
                arguments: vec![GraphQLArgument {
                    name:       "role".to_string(),
                    value_type: "String".to_string(),
                    value_json: "\"admin\"".to_string(),
                }],
            }],
        };

        let result = evaluator.evaluate_directives_with_context(&field, &context).unwrap();
        assert_eq!(result, DirectiveResult::Include);
    }

    #[test]
    fn test_custom_directive_denies_without_role() {
        let auth = Arc::new(AuthDirective {
            required_role: "admin".to_string(),
        });
        let evaluator = CustomDirectiveEvaluator::new().with_handler(auth);

        // Create a context without admin role
        let context = EvaluationContext::new(HashMap::new()).with_user_context(
            "roles",
            JsonValue::Array(vec![JsonValue::String("user".to_string())]),
        );

        // Create a field with @auth directive
        let field = FieldSelection {
            name:          "sensitiveData".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![Directive {
                name:      "auth".to_string(),
                arguments: vec![GraphQLArgument {
                    name:       "role".to_string(),
                    value_type: "String".to_string(),
                    value_json: "\"admin\"".to_string(),
                }],
            }],
        };

        let result = evaluator.evaluate_directives_with_context(&field, &context).unwrap();
        assert_eq!(result, DirectiveResult::Skip);
    }

    #[test]
    fn test_custom_directive_strict_mode_unknown() {
        let evaluator = CustomDirectiveEvaluator::new().strict();

        let context = EvaluationContext::new(HashMap::new());
        let field = make_field(
            "email",
            vec![Directive {
                name:      "unknown".to_string(),
                arguments: vec![],
            }],
        );

        let result = evaluator.evaluate_directives_with_context(&field, &context);
        assert!(matches!(result, Err(DirectiveError::UnknownDirective(_))));
    }

    #[test]
    fn test_custom_directive_lenient_mode_unknown() {
        let evaluator = CustomDirectiveEvaluator::new();

        let context = EvaluationContext::new(HashMap::new());
        let field = make_field(
            "email",
            vec![Directive {
                name:      "unknown".to_string(),
                arguments: vec![],
            }],
        );

        // In lenient mode, unknown directives pass through
        let result = evaluator.evaluate_directives_with_context(&field, &context).unwrap();
        assert_eq!(result, DirectiveResult::Include);
    }

    #[test]
    fn test_custom_directive_builtin_skip() {
        let evaluator = CustomDirectiveEvaluator::new();
        let context = EvaluationContext::new(HashMap::new());

        let field = make_field("email", vec![make_directive("skip", "true")]);
        let result = evaluator.evaluate_directives_with_context(&field, &context).unwrap();
        assert_eq!(result, DirectiveResult::Skip);
    }

    #[test]
    fn test_custom_directive_builtin_include() {
        let evaluator = CustomDirectiveEvaluator::new();
        let context = EvaluationContext::new(HashMap::new());

        let field = make_field("email", vec![make_directive("include", "false")]);
        let result = evaluator.evaluate_directives_with_context(&field, &context).unwrap();
        assert_eq!(result, DirectiveResult::Skip);
    }

    #[test]
    fn test_filter_selections_with_custom_directive() {
        let always_skip = Arc::new(AlwaysSkipDirective);
        let evaluator = CustomDirectiveEvaluator::new().with_handler(always_skip);

        let selections = vec![
            make_field("id", vec![]),
            make_field(
                "secret",
                vec![Directive {
                    name:      "alwaysSkip".to_string(),
                    arguments: vec![],
                }],
            ),
            make_field("name", vec![]),
        ];

        let context = EvaluationContext::new(HashMap::new());
        let filtered = evaluator.filter_selections_with_context(&selections, &context).unwrap();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name, "id");
        assert_eq!(filtered[1].name, "name");
    }

    #[test]
    fn test_filter_selections_with_error_directive() {
        let error = Arc::new(ErrorDirective);
        let evaluator = CustomDirectiveEvaluator::new().with_handler(error);

        let selections = vec![
            make_field("id", vec![]),
            make_field(
                "broken",
                vec![Directive {
                    name:      "error".to_string(),
                    arguments: vec![],
                }],
            ),
        ];

        let context = EvaluationContext::new(HashMap::new());
        let result = evaluator.filter_selections_with_context(&selections, &context);

        assert!(matches!(result, Err(DirectiveError::CustomDirectiveError(_))));
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
                value_json: "\"$myLimit\"".to_string(),
            }],
        };

        let mut variables = HashMap::new();
        variables.insert("myLimit".to_string(), JsonValue::Number(25.into()));

        let args = DirectiveEvaluator::parse_directive_args(&directive, &variables).unwrap();
        assert_eq!(args.get("limit"), Some(&JsonValue::Number(25.into())));
    }

    #[test]
    fn test_multiple_handlers() {
        let auth = Arc::new(AuthDirective {
            required_role: "admin".to_string(),
        });
        let skip = Arc::new(AlwaysSkipDirective);

        let evaluator = CustomDirectiveEvaluator::new().with_handlers(vec![auth, skip]);

        assert!(evaluator.has_handler("auth"));
        assert!(evaluator.has_handler("alwaysSkip"));
        assert_eq!(evaluator.handler_names().len(), 2);
    }
}
