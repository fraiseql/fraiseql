//! Directive evaluation for GraphQL queries.
//!
//! Handles:
//! - `@skip` directive (conditionally skip a field)
//! - `@include` directive (conditionally include a field)
//! - Custom directive framework for extensibility

use crate::graphql::types::{Directive, FieldSelection};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during directive evaluation.
#[derive(Debug, Error)]
pub enum DirectiveError {
    /// Indicates that a required directive argument was missing.
    #[error("Missing directive argument: {0}")]
    MissingDirectiveArgument(String),

    /// Indicates that a referenced variable is undefined.
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    /// Indicates that a variable type does not match the directive argument type.
    #[error("Variable type mismatch: {0} should be Boolean")]
    VariableTypeMismatch(String),

    /// Indicates that a directive argument value is invalid.
    #[error("Invalid directive argument")]
    InvalidDirectiveArgument,

    /// Indicates a custom directive processing error.
    #[error("Custom directive error: {0}")]
    CustomDirectiveError(String),
}

/// Evaluates GraphQL directives in query field selections.
///
/// Handles `@skip` and `@include` directives, as well as custom directives.
/// Returns true if a field should be included, false if it should be skipped.
///
/// # Example
///
/// ```
/// use fraiseql_core::graphql::{DirectiveEvaluator, FieldSelection, Directive, GraphQLArgument};
/// use std::collections::HashMap;
/// use serde_json::json;
///
/// let field = FieldSelection {
///     name: "email".to_string(),
///     alias: None,
///     arguments: vec![],
///     nested_fields: vec![],
///     directives: vec![Directive {
///         name: "skip".to_string(),
///         arguments: vec![GraphQLArgument {
///             name: "if".to_string(),
///             value_type: "boolean".to_string(),
///             value_json: "true".to_string(),
///         }],
///     }],
/// };
///
/// let variables = HashMap::new();
/// let should_include = DirectiveEvaluator::evaluate_directives(&field, &variables).unwrap();
/// assert!(!should_include); // Field should be skipped
/// ```
pub struct DirectiveEvaluator;

impl DirectiveEvaluator {
    /// Evaluate all directives on a field.
    ///
    /// Returns true if the field should be INCLUDED in the response.
    /// Returns false if the field should be SKIPPED.
    ///
    /// # Errors
    /// Returns error if:
    /// - Required directive argument is missing
    /// - Variable is undefined
    /// - Variable has wrong type
    pub fn evaluate_directives(
        selection: &FieldSelection,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<bool, DirectiveError> {
        // If no directives, include the field by default
        if selection.directives.is_empty() {
            return Ok(true);
        }

        // Evaluate each directive in order
        for directive in &selection.directives {
            match directive.name.as_str() {
                "skip" => {
                    // @skip(if: condition) - skip if true
                    if Self::evaluate_skip(directive, variables)? {
                        return Ok(false); // Skip this field
                    }
                }
                "include" => {
                    // @include(if: condition) - include if true
                    if !Self::evaluate_include(directive, variables)? {
                        return Ok(false); // Skip if include is false
                    }
                }
                _ => {
                    // Unknown directive - for now, pass through with warning
                    // In the future, could support custom directives via hooks
                    tracing::warn!("Unknown directive @{}", directive.name);
                }
            }
        }

        // If all directives allow inclusion, include the field
        Ok(true)
    }

    /// Evaluate @skip(if: condition) directive.
    ///
    /// Returns true if the field should be SKIPPED (condition is true).
    fn evaluate_skip(
        directive: &Directive,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<bool, DirectiveError> {
        let if_arg = directive
            .arguments
            .iter()
            .find(|a| a.name == "if")
            .ok_or(DirectiveError::MissingDirectiveArgument("if".to_string()))?;

        Self::resolve_boolean_condition(&if_arg.value_json, variables)
    }

    /// Evaluate @include(if: condition) directive.
    ///
    /// Returns true if the field should be INCLUDED (condition is true).
    fn evaluate_include(
        directive: &Directive,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<bool, DirectiveError> {
        let if_arg = directive
            .arguments
            .iter()
            .find(|a| a.name == "if")
            .ok_or(DirectiveError::MissingDirectiveArgument("if".to_string()))?;

        Self::resolve_boolean_condition(&if_arg.value_json, variables)
    }

    /// Resolve a condition value to a boolean.
    ///
    /// Handles:
    /// - Literal boolean values: true/false
    /// - Variable references: $variableName
    fn resolve_boolean_condition(
        value_json: &str,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<bool, DirectiveError> {
        // Try to parse as JSON
        match serde_json::from_str::<JsonValue>(value_json) {
            Ok(JsonValue::Bool(b)) => Ok(b),
            Ok(JsonValue::String(s)) if s.starts_with('$') => {
                // Variable reference
                let var_name = &s[1..]; // Remove $ prefix
                let val = variables
                    .get(var_name)
                    .ok_or_else(|| DirectiveError::UndefinedVariable(var_name.to_string()))?;

                match val {
                    JsonValue::Bool(b) => Ok(*b),
                    _ => Err(DirectiveError::VariableTypeMismatch(var_name.to_string())),
                }
            }
            Ok(_) => Err(DirectiveError::InvalidDirectiveArgument),
            Err(_) => {
                // Try parsing as plain string for variable reference
                if let Some(var_name) = value_json.strip_prefix('$') {
                    let val = variables
                        .get(var_name)
                        .ok_or_else(|| DirectiveError::UndefinedVariable(var_name.to_string()))?;

                    match val {
                        JsonValue::Bool(b) => Ok(*b),
                        _ => Err(DirectiveError::VariableTypeMismatch(var_name.to_string())),
                    }
                } else {
                    Err(DirectiveError::InvalidDirectiveArgument)
                }
            }
        }
    }

    /// Filter selections based on directives.
    ///
    /// Recursively evaluates directives on all fields and returns only those
    /// that should be included.
    ///
    /// # Errors
    /// Returns error if directive evaluation fails for any field.
    pub fn filter_selections(
        selections: &[FieldSelection],
        variables: &HashMap<String, JsonValue>,
    ) -> Result<Vec<FieldSelection>, DirectiveError> {
        let mut result = Vec::new();

        for selection in selections {
            if Self::evaluate_directives(selection, variables)? {
                let mut field = selection.clone();

                // Recursively filter nested fields
                if !field.nested_fields.is_empty() {
                    field.nested_fields = Self::filter_selections(&field.nested_fields, variables)?;
                }

                result.push(field);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql::types::GraphQLArgument;

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
            name: name.to_string(),
            arguments: vec![GraphQLArgument {
                name: "if".to_string(),
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
        let field = make_field(
            "email",
            vec![make_directive("include", "\"$includeEmail\"")],
        );
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
        variables.insert(
            "notABool".to_string(),
            JsonValue::String("hello".to_string()),
        );

        let result = DirectiveEvaluator::evaluate_directives(&field, &variables);
        assert!(matches!(
            result,
            Err(DirectiveError::VariableTypeMismatch(_))
        ));
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
            name: "user".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![
                make_field("id", vec![]),
                make_field("secret", vec![make_directive("skip", "true")]),
            ],
            directives: vec![],
        }];

        let variables = HashMap::new();
        let filtered = DirectiveEvaluator::filter_selections(&selections, &variables).unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].nested_fields.len(), 1);
        assert_eq!(filtered[0].nested_fields[0].name, "id");
    }
}
