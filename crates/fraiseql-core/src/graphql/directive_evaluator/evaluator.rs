//! Built-in directive evaluator for `@skip` and `@include`.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use super::types::DirectiveError;
use crate::graphql::types::{Directive, FieldSelection};

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
                },
                "include" => {
                    // @include(if: condition) - include if true
                    if !Self::evaluate_include(directive, variables)? {
                        return Ok(false); // Skip if include is false
                    }
                },
                _ => {
                    // Unknown directive - for now, pass through with warning
                    // In the future, could support custom directives via hooks
                    tracing::warn!("Unknown directive @{}", directive.name);
                },
            }
        }

        // If all directives allow inclusion, include the field
        Ok(true)
    }

    /// Evaluate @skip(if: condition) directive.
    ///
    /// Returns true if the field should be SKIPPED (condition is true).
    pub fn evaluate_skip(
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
    pub fn evaluate_include(
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
            },
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
            },
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

    /// Parse directive arguments into a HashMap.
    ///
    /// Converts the directive argument list into a map with resolved values.
    pub fn parse_directive_args(
        directive: &Directive,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<HashMap<String, JsonValue>, DirectiveError> {
        let mut args = HashMap::new();

        for arg in &directive.arguments {
            let value = Self::resolve_argument_value(&arg.value_json, variables)?;
            args.insert(arg.name.clone(), value);
        }

        Ok(args)
    }

    /// Resolve an argument value, handling variable references.
    fn resolve_argument_value(
        value_json: &str,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<JsonValue, DirectiveError> {
        // Try to parse as JSON
        match serde_json::from_str::<JsonValue>(value_json) {
            Ok(JsonValue::String(s)) if s.starts_with('$') => {
                // Variable reference
                let var_name = &s[1..];
                variables
                    .get(var_name)
                    .cloned()
                    .ok_or_else(|| DirectiveError::UndefinedVariable(var_name.to_string()))
            },
            Ok(value) => Ok(value),
            Err(_) => {
                // Try parsing as plain string for variable reference
                if let Some(var_name) = value_json.strip_prefix('$') {
                    variables
                        .get(var_name)
                        .cloned()
                        .ok_or_else(|| DirectiveError::UndefinedVariable(var_name.to_string()))
                } else {
                    // Return as string if not JSON
                    Ok(JsonValue::String(value_json.to_string()))
                }
            },
        }
    }
}
