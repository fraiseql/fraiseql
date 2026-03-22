//! Shared types for directive evaluation: errors, results, context, and trait.

use std::collections::HashMap;

use serde_json::Value as JsonValue;
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

/// Result of custom directive evaluation.
///
/// Determines how a field should be handled after directive processing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    pub const fn with_operation_type(mut self, op_type: OperationType) -> Self {
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
    ///
    /// # Errors
    ///
    /// Returns `DirectiveError` if argument validation or evaluation fails.
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
    ///
    /// # Errors
    ///
    /// Returns `DirectiveError` if the arguments are invalid for this directive.
    fn validate_args(&self, _args: &HashMap<String, JsonValue>) -> Result<(), DirectiveError> {
        Ok(())
    }
}
