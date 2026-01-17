//! Directive evaluation for GraphQL queries.
//!
//! Handles:
//! - `@skip` directive (conditionally skip a field)
//! - `@include` directive (conditionally include a field)
//! - Custom directive framework for extensibility
//!
//! # Custom Directive Framework
//!
//! The framework supports user-defined directives through the `DirectiveHandler` trait.
//! Custom handlers can:
//! - Include or skip fields conditionally
//! - Transform field values
//! - Enforce access control or validation
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::graphql::{
//!     DirectiveHandler, DirectiveResult, EvaluationContext,
//!     DirectiveEvaluatorBuilder,
//! };
//! use std::sync::Arc;
//!
//! struct AuthDirectiveHandler;
//!
//! impl DirectiveHandler for AuthDirectiveHandler {
//!     fn name(&self) -> &str {
//!         "auth"
//!     }
//!
//!     fn evaluate(
//!         &self,
//!         args: &HashMap<String, serde_json::Value>,
//!         context: &EvaluationContext,
//!     ) -> Result<DirectiveResult, DirectiveError> {
//!         let required_role = args.get("role")
//!             .and_then(|v| v.as_str())
//!             .unwrap_or("user");
//!
//!         if context.has_role(required_role) {
//!             Ok(DirectiveResult::Include)
//!         } else {
//!             Ok(DirectiveResult::Skip)
//!         }
//!     }
//! }
//!
//! let evaluator = DirectiveEvaluatorBuilder::new()
//!     .with_handler(Arc::new(AuthDirectiveHandler))
//!     .build();
//! ```

use crate::graphql::types::{Directive, FieldSelection};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
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

    /// Indicates that a directive is not registered.
    #[error("Unknown directive: @{0}")]
    UnknownDirective(String),

    /// Indicates that directive location is invalid.
    #[error("Directive @{0} cannot be used at {1}")]
    InvalidDirectiveLocation(String, String),
}

// =============================================================================
// Custom Directive Framework
// =============================================================================

/// Result of custom directive evaluation.
///
/// Determines how a field should be handled after directive processing.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum DirectiveResult {
    /// Include the field in the response (default behavior).
    #[default]
    Include,

    /// Skip the field entirely (like `@skip(if: true)`).
    Skip,

    /// Transform the field value before returning.
    /// The transformation is applied after field resolution.
    Transform(JsonValue),

    /// Directive encountered an error that should be reported.
    Error(String),
}

/// Context provided to directive handlers during evaluation.
///
/// Contains information about the current request, user context,
/// and variables that may be needed for directive evaluation.
#[derive(Debug, Clone, Default)]
pub struct EvaluationContext {
    /// GraphQL variables from the request.
    pub variables: HashMap<String, JsonValue>,

    /// User-defined context values (e.g., auth info, request metadata).
    /// Keys are context identifiers, values are arbitrary JSON.
    pub user_context: HashMap<String, JsonValue>,

    /// Field path being evaluated (e.g., "Query.users.email").
    pub field_path: Option<String>,

    /// Current operation type.
    pub operation_type: Option<OperationType>,
}

/// GraphQL operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Query operation.
    Query,
    /// Mutation operation.
    Mutation,
    /// Subscription operation.
    Subscription,
}

impl EvaluationContext {
    /// Create a new evaluation context with variables.
    #[must_use]
    pub fn new(variables: HashMap<String, JsonValue>) -> Self {
        Self {
            variables,
            ..Default::default()
        }
    }

    /// Add user context value.
    #[must_use]
    pub fn with_user_context(mut self, key: impl Into<String>, value: JsonValue) -> Self {
        self.user_context.insert(key.into(), value);
        self
    }

    /// Set field path.
    #[must_use]
    pub fn with_field_path(mut self, path: impl Into<String>) -> Self {
        self.field_path = Some(path.into());
        self
    }

    /// Set operation type.
    #[must_use]
    pub fn with_operation_type(mut self, op_type: OperationType) -> Self {
        self.operation_type = Some(op_type);
        self
    }

    /// Get a user context value by key.
    #[must_use]
    pub fn get_user_context(&self, key: &str) -> Option<&JsonValue> {
        self.user_context.get(key)
    }

    /// Check if user has a specific role (helper for auth directives).
    ///
    /// Expects user context to have a "roles" key with an array of role strings.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.user_context
            .get("roles")
            .and_then(|v| v.as_array())
            .is_some_and(|roles| roles.iter().any(|r| r.as_str() == Some(role)))
    }

    /// Get the current user ID if available.
    #[must_use]
    pub fn user_id(&self) -> Option<&str> {
        self.user_context.get("userId").and_then(|v| v.as_str())
    }
}

/// Trait for custom directive handlers.
///
/// Implement this trait to create custom directives that can be registered
/// with the `DirectiveEvaluatorBuilder`.
///
/// # Thread Safety
///
/// Handlers must be `Send + Sync` to be used across async contexts.
///
/// # Example
///
/// ```
/// use fraiseql_core::graphql::{DirectiveHandler, DirectiveResult, EvaluationContext, DirectiveError};
/// use std::collections::HashMap;
/// use serde_json::Value as JsonValue;
///
/// struct UppercaseDirective;
///
/// impl DirectiveHandler for UppercaseDirective {
///     fn name(&self) -> &str {
///         "uppercase"
///     }
///
///     fn evaluate(
///         &self,
///         _args: &HashMap<String, JsonValue>,
///         _context: &EvaluationContext,
///     ) -> Result<DirectiveResult, DirectiveError> {
///         // This directive would transform string values to uppercase
///         // (actual transformation happens during field resolution)
///         Ok(DirectiveResult::Include)
///     }
/// }
/// ```
pub trait DirectiveHandler: Send + Sync {
    /// Returns the directive name (without the @ prefix).
    fn name(&self) -> &str;

    /// Evaluate the directive with the given arguments and context.
    ///
    /// # Arguments
    ///
    /// * `args` - Parsed directive arguments as a map of name to value
    /// * `context` - Evaluation context with variables and user info
    ///
    /// # Returns
    ///
    /// A `DirectiveResult` indicating how to handle the field, or an error.
    fn evaluate(
        &self,
        args: &HashMap<String, JsonValue>,
        context: &EvaluationContext,
    ) -> Result<DirectiveResult, DirectiveError>;

    /// Optional: Validate directive arguments at schema load time.
    ///
    /// Called when a schema with this directive is loaded to ensure
    /// arguments are valid.
    ///
    /// Default implementation accepts all arguments.
    fn validate_args(&self, _args: &HashMap<String, JsonValue>) -> Result<(), DirectiveError> {
        Ok(())
    }
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
            }
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
            }
        }
    }
}

// =============================================================================
// Custom Directive Evaluator with Handler Registry
// =============================================================================

/// Evaluator with support for custom directive handlers.
///
/// Unlike `DirectiveEvaluator` which only handles built-in directives,
/// this evaluator can be configured with custom handlers for user-defined
/// directives.
///
/// # Example
///
/// ```
/// use fraiseql_core::graphql::{
///     CustomDirectiveEvaluator, DirectiveHandler, DirectiveResult,
///     EvaluationContext, DirectiveError,
/// };
/// use std::collections::HashMap;
/// use std::sync::Arc;
/// use serde_json::Value as JsonValue;
///
/// // Define a custom handler
/// struct LogDirective;
///
/// impl DirectiveHandler for LogDirective {
///     fn name(&self) -> &str { "log" }
///
///     fn evaluate(
///         &self,
///         _args: &HashMap<String, JsonValue>,
///         _context: &EvaluationContext,
///     ) -> Result<DirectiveResult, DirectiveError> {
///         // Log the field access (actual logging would go here)
///         Ok(DirectiveResult::Include)
///     }
/// }
///
/// // Create evaluator with custom handler
/// let evaluator = CustomDirectiveEvaluator::new()
///     .with_handler(Arc::new(LogDirective));
/// ```
#[derive(Clone)]
pub struct CustomDirectiveEvaluator {
    /// Registered custom directive handlers.
    handlers: HashMap<String, Arc<dyn DirectiveHandler>>,

    /// Whether to allow unknown directives (pass through with warning).
    /// If false, unknown directives will cause an error.
    strict_mode: bool,
}

impl Default for CustomDirectiveEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomDirectiveEvaluator {
    /// Create a new custom directive evaluator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            strict_mode: false,
        }
    }

    /// Enable strict mode where unknown directives cause errors.
    #[must_use]
    pub fn strict(mut self) -> Self {
        self.strict_mode = true;
        self
    }

    /// Register a custom directive handler.
    #[must_use]
    pub fn with_handler(mut self, handler: Arc<dyn DirectiveHandler>) -> Self {
        let name = handler.name().to_string();
        self.handlers.insert(name, handler);
        self
    }

    /// Register multiple custom directive handlers.
    #[must_use]
    pub fn with_handlers(mut self, handlers: Vec<Arc<dyn DirectiveHandler>>) -> Self {
        for handler in handlers {
            let name = handler.name().to_string();
            self.handlers.insert(name, handler);
        }
        self
    }

    /// Check if a custom handler is registered for a directive.
    #[must_use]
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get a registered handler by name.
    #[must_use]
    pub fn get_handler(&self, name: &str) -> Option<&Arc<dyn DirectiveHandler>> {
        self.handlers.get(name)
    }

    /// List all registered handler names.
    #[must_use]
    pub fn handler_names(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }

    /// Evaluate all directives on a field with custom handler support.
    ///
    /// Returns a `DirectiveResult` indicating how to handle the field.
    ///
    /// # Errors
    /// Returns error if directive evaluation fails.
    pub fn evaluate_directives_with_context(
        &self,
        selection: &FieldSelection,
        context: &EvaluationContext,
    ) -> Result<DirectiveResult, DirectiveError> {
        if selection.directives.is_empty() {
            return Ok(DirectiveResult::Include);
        }

        for directive in &selection.directives {
            let result = self.evaluate_single_directive(directive, context)?;

            match result {
                DirectiveResult::Include => {}
                DirectiveResult::Skip => return Ok(DirectiveResult::Skip),
                DirectiveResult::Transform(_) | DirectiveResult::Error(_) => return Ok(result),
            }
        }

        Ok(DirectiveResult::Include)
    }

    /// Evaluate a single directive.
    fn evaluate_single_directive(
        &self,
        directive: &Directive,
        context: &EvaluationContext,
    ) -> Result<DirectiveResult, DirectiveError> {
        match directive.name.as_str() {
            // Built-in directives
            "skip" => {
                if DirectiveEvaluator::evaluate_skip(directive, &context.variables)? {
                    Ok(DirectiveResult::Skip)
                } else {
                    Ok(DirectiveResult::Include)
                }
            }
            "include" => {
                if DirectiveEvaluator::evaluate_include(directive, &context.variables)? {
                    Ok(DirectiveResult::Include)
                } else {
                    Ok(DirectiveResult::Skip)
                }
            }
            "deprecated" => {
                // @deprecated is a schema directive, not a query directive
                // If it appears in a query, we just pass through
                Ok(DirectiveResult::Include)
            }
            // Custom directives
            name => {
                if let Some(handler) = self.handlers.get(name) {
                    let args = DirectiveEvaluator::parse_directive_args(directive, &context.variables)?;
                    handler.evaluate(&args, context)
                } else if self.strict_mode {
                    Err(DirectiveError::UnknownDirective(name.to_string()))
                } else {
                    tracing::warn!("Unknown directive @{}, passing through", name);
                    Ok(DirectiveResult::Include)
                }
            }
        }
    }

    /// Filter selections with custom directive support.
    ///
    /// # Errors
    /// Returns error if directive evaluation fails.
    pub fn filter_selections_with_context(
        &self,
        selections: &[FieldSelection],
        context: &EvaluationContext,
    ) -> Result<Vec<FieldSelection>, DirectiveError> {
        let mut result = Vec::new();

        for selection in selections {
            let directive_result = self.evaluate_directives_with_context(selection, context)?;

            match directive_result {
                DirectiveResult::Include | DirectiveResult::Transform(_) => {
                    let mut field = selection.clone();

                    // Recursively filter nested fields
                    if !field.nested_fields.is_empty() {
                        field.nested_fields = self.filter_selections_with_context(
                            &field.nested_fields,
                            context,
                        )?;
                    }

                    result.push(field);
                }
                DirectiveResult::Skip => {
                    // Don't include this field
                }
                DirectiveResult::Error(msg) => {
                    return Err(DirectiveError::CustomDirectiveError(msg));
                }
            }
        }

        Ok(result)
    }
}

impl std::fmt::Debug for CustomDirectiveEvaluator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomDirectiveEvaluator")
            .field("handlers", &self.handlers.keys().collect::<Vec<_>>())
            .field("strict_mode", &self.strict_mode)
            .finish()
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

    // ==========================================================================
    // Custom Directive Framework Tests
    // ==========================================================================

    /// A test directive that checks for a specific role.
    struct AuthDirective {
        required_role: String,
    }

    #[allow(clippy::unnecessary_literal_bound)]
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
            let required = args
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or(&self.required_role);

            if context.has_role(required) {
                Ok(DirectiveResult::Include)
            } else {
                Ok(DirectiveResult::Skip)
            }
        }
    }

    /// A test directive that always skips.
    struct AlwaysSkipDirective;

    #[allow(clippy::unnecessary_literal_bound)]
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

    #[allow(clippy::unnecessary_literal_bound)]
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
        let context = EvaluationContext::new(HashMap::new())
            .with_user_context("roles", JsonValue::Array(vec![JsonValue::String("admin".to_string())]));

        // Create a field with @auth directive
        let field = FieldSelection {
            name: "sensitiveData".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![Directive {
                name: "auth".to_string(),
                arguments: vec![GraphQLArgument {
                    name: "role".to_string(),
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
        let context = EvaluationContext::new(HashMap::new())
            .with_user_context("roles", JsonValue::Array(vec![JsonValue::String("user".to_string())]));

        // Create a field with @auth directive
        let field = FieldSelection {
            name: "sensitiveData".to_string(),
            alias: None,
            arguments: vec![],
            nested_fields: vec![],
            directives: vec![Directive {
                name: "auth".to_string(),
                arguments: vec![GraphQLArgument {
                    name: "role".to_string(),
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
        let field = make_field("email", vec![Directive {
            name: "unknown".to_string(),
            arguments: vec![],
        }]);

        let result = evaluator.evaluate_directives_with_context(&field, &context);
        assert!(matches!(result, Err(DirectiveError::UnknownDirective(_))));
    }

    #[test]
    fn test_custom_directive_lenient_mode_unknown() {
        let evaluator = CustomDirectiveEvaluator::new();

        let context = EvaluationContext::new(HashMap::new());
        let field = make_field("email", vec![Directive {
            name: "unknown".to_string(),
            arguments: vec![],
        }]);

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
            make_field("secret", vec![Directive {
                name: "alwaysSkip".to_string(),
                arguments: vec![],
            }]),
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
            make_field("broken", vec![Directive {
                name: "error".to_string(),
                arguments: vec![],
            }]),
        ];

        let context = EvaluationContext::new(HashMap::new());
        let result = evaluator.filter_selections_with_context(&selections, &context);

        assert!(matches!(result, Err(DirectiveError::CustomDirectiveError(_))));
    }

    #[test]
    fn test_evaluation_context_has_role() {
        let context = EvaluationContext::new(HashMap::new())
            .with_user_context("roles", JsonValue::Array(vec![
                JsonValue::String("admin".to_string()),
                JsonValue::String("editor".to_string()),
            ]));

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
        let context = EvaluationContext::new(HashMap::new())
            .with_field_path("Query.users.email");

        assert_eq!(context.field_path.as_deref(), Some("Query.users.email"));
    }

    #[test]
    fn test_evaluation_context_operation_type() {
        let context = EvaluationContext::new(HashMap::new())
            .with_operation_type(OperationType::Mutation);

        assert_eq!(context.operation_type, Some(OperationType::Mutation));
    }

    #[test]
    fn test_directive_result_default() {
        assert_eq!(DirectiveResult::default(), DirectiveResult::Include);
    }

    #[test]
    fn test_parse_directive_args() {
        let directive = Directive {
            name: "test".to_string(),
            arguments: vec![
                GraphQLArgument {
                    name: "limit".to_string(),
                    value_type: "Int".to_string(),
                    value_json: "10".to_string(),
                },
                GraphQLArgument {
                    name: "name".to_string(),
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
            name: "test".to_string(),
            arguments: vec![GraphQLArgument {
                name: "limit".to_string(),
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

        let evaluator = CustomDirectiveEvaluator::new()
            .with_handlers(vec![auth, skip]);

        assert!(evaluator.has_handler("auth"));
        assert!(evaluator.has_handler("alwaysSkip"));
        assert_eq!(evaluator.handler_names().len(), 2);
    }
}
