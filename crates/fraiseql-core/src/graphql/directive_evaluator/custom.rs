//! Custom directive evaluator with pluggable handler registry.

use std::{collections::HashMap, sync::Arc};

use super::{
    evaluator::DirectiveEvaluator,
    types::{DirectiveError, DirectiveHandler, DirectiveResult, EvaluationContext},
};
use crate::graphql::types::FieldSelection;

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
    pub const fn strict(mut self) -> Self {
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
                DirectiveResult::Include => {},
                DirectiveResult::Skip => return Ok(DirectiveResult::Skip),
                DirectiveResult::Transform(_) | DirectiveResult::Error(_) => return Ok(result),
            }
        }

        Ok(DirectiveResult::Include)
    }

    /// Evaluate a single directive.
    fn evaluate_single_directive(
        &self,
        directive: &crate::graphql::types::Directive,
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
            },
            "include" => {
                if DirectiveEvaluator::evaluate_include(directive, &context.variables)? {
                    Ok(DirectiveResult::Include)
                } else {
                    Ok(DirectiveResult::Skip)
                }
            },
            "deprecated" => {
                // @deprecated is a schema directive, not a query directive
                // If it appears in a query, we just pass through
                Ok(DirectiveResult::Include)
            },
            // Custom directives
            name => {
                if let Some(handler) = self.handlers.get(name) {
                    let args =
                        DirectiveEvaluator::parse_directive_args(directive, &context.variables)?;
                    handler.evaluate(&args, context)
                } else if self.strict_mode {
                    Err(DirectiveError::UnknownDirective(name.to_string()))
                } else {
                    tracing::warn!("Unknown directive @{}, passing through", name);
                    Ok(DirectiveResult::Include)
                }
            },
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
                        field.nested_fields =
                            self.filter_selections_with_context(&field.nested_fields, context)?;
                    }

                    result.push(field);
                },
                DirectiveResult::Skip => {
                    // Don't include this field
                },
                DirectiveResult::Error(msg) => {
                    return Err(DirectiveError::CustomDirectiveError(msg));
                },
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
