//! Input object-level validation.
//!
//! This module provides validation capabilities at the input object level,
//! applying cross-field rules and aggregating errors from multiple validators.
//!
//! # Examples
//!
//! ```
//! use fraiseql_core::validation::{InputObjectRule, validate_input_object};
//! use serde_json::json;
//!
//! // Validate entire input object
//! let input = json!({
//!     "name": "John",
//!     "email": "john@example.com",
//!     "phone": null
//! });
//!
//! let validators = vec![
//!     InputObjectRule::AnyOf { fields: vec!["email".to_string(), "phone".to_string()] },
//!     InputObjectRule::ConditionalRequired {
//!         if_field: "name".to_string(),
//!         then_fields: vec!["email".to_string()],
//!     },
//! ];
//!
//! validate_input_object(&input, &validators, None).unwrap();
//! ```

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Rules that apply at the input object level.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum InputObjectRule {
    /// At least one field from the set must be provided
    AnyOf {
        /// Field names of which at least one must be present.
        fields: Vec<String>,
    },
    /// Exactly one field from the set must be provided
    OneOf {
        /// Field names of which exactly one must be present.
        fields: Vec<String>,
    },
    /// If one field is present, others must be present
    ConditionalRequired {
        /// The trigger field whose presence activates the requirement.
        if_field:    String,
        /// Fields that must be present when `if_field` is provided.
        then_fields: Vec<String>,
    },
    /// If one field is absent, others must be present
    RequiredIfAbsent {
        /// The field whose absence activates the requirement.
        absent_field: String,
        /// Fields that must be present when `absent_field` is missing.
        then_fields:  Vec<String>,
    },
    /// Custom validator function name to invoke
    Custom {
        /// Name of the registered custom validator function.
        name: String,
    },
}

/// Result of validating an input object, aggregating multiple errors.
#[derive(Debug, Clone, Default)]
pub struct InputObjectValidationResult {
    /// All validation errors
    pub errors:      Vec<String>,
    /// Count of errors
    pub error_count: usize,
}

impl InputObjectValidationResult {
    /// Create a new empty result.
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            errors:      Vec::new(),
            error_count: 0,
        }
    }

    /// Add an error to the result.
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.error_count += 1;
    }

    /// Add multiple errors at once.
    pub fn add_errors(&mut self, errors: Vec<String>) {
        self.error_count += errors.len();
        self.errors.extend(errors);
    }

    /// Check if there are any errors.
    #[must_use] 
    pub const fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Convert to a Result, failing if there are errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if any validation errors have been
    /// accumulated via [`add_error`][Self::add_error].
    pub fn into_result(self) -> Result<()> {
        self.into_result_with_path("input")
    }

    /// Convert to a Result with a custom path, failing if there are errors.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] with the given `path` if any
    /// validation errors have been accumulated.
    pub fn into_result_with_path(self, path: &str) -> Result<()> {
        if self.has_errors() {
            Err(FraiseQLError::Validation {
                message: format!("Input object validation failed: {}", self.errors.join("; ")),
                path:    Some(path.to_string()),
            })
        } else {
            Ok(())
        }
    }
}

/// Validate an input object against a set of rules.
///
/// Applies all rules to the input object and aggregates errors.
///
/// # Arguments
/// * `input` - The input object to validate
/// * `rules` - Rules to apply at the object level
/// * `object_path` - Optional path to the object for error reporting
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if any input object rule fails.
pub fn validate_input_object(
    input: &Value,
    rules: &[InputObjectRule],
    object_path: Option<&str>,
) -> Result<()> {
    let mut result = InputObjectValidationResult::new();
    let path = object_path.unwrap_or("input");

    if !matches!(input, Value::Object(_)) {
        return Err(FraiseQLError::Validation {
            message: "Input must be an object".to_string(),
            path:    Some(path.to_string()),
        });
    }

    for rule in rules {
        if let Err(FraiseQLError::Validation { message, .. }) = validate_rule(input, rule, path) {
            result.add_error(message);
        }
    }

    result.into_result_with_path(path)
}

/// Validate a single input object rule.
fn validate_rule(input: &Value, rule: &InputObjectRule, path: &str) -> Result<()> {
    match rule {
        InputObjectRule::AnyOf { fields } => validate_any_of(input, fields, path),
        InputObjectRule::OneOf { fields } => validate_one_of(input, fields, path),
        InputObjectRule::ConditionalRequired {
            if_field,
            then_fields,
        } => validate_conditional_required(input, if_field, then_fields, path),
        InputObjectRule::RequiredIfAbsent {
            absent_field,
            then_fields,
        } => validate_required_if_absent(input, absent_field, then_fields, path),
        InputObjectRule::Custom { name } => Err(FraiseQLError::Validation {
            message: format!(
                "Custom validator '{name}' is not registered. \
                 Register validators via InputValidatorRegistry before executing queries."
            ),
            path:    Some(path.to_string()),
        }),
    }
}

/// Validate that at least one field from the set is present.
fn validate_any_of(input: &Value, fields: &[String], path: &str) -> Result<()> {
    if let Value::Object(obj) = input {
        let has_any = fields
            .iter()
            .any(|name| obj.get(name).is_some_and(|v| !matches!(v, Value::Null)));

        if !has_any {
            return Err(FraiseQLError::Validation {
                message: format!("At least one of [{}] must be provided", fields.join(", ")),
                path:    Some(path.to_string()),
            });
        }
    }

    Ok(())
}

/// Validate that exactly one field from the set is present.
fn validate_one_of(input: &Value, fields: &[String], path: &str) -> Result<()> {
    if let Value::Object(obj) = input {
        let present_count = fields
            .iter()
            .filter(|name| obj.get(*name).is_some_and(|v| !matches!(v, Value::Null)))
            .count();

        if present_count != 1 {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Exactly one of [{}] must be provided, but {} {} provided",
                    fields.join(", "),
                    present_count,
                    if present_count == 1 { "was" } else { "were" }
                ),
                path:    Some(path.to_string()),
            });
        }
    }

    Ok(())
}

/// Validate conditional requirement: if one field is present, others must be too.
fn validate_conditional_required(
    input: &Value,
    if_field: &str,
    then_fields: &[String],
    path: &str,
) -> Result<()> {
    if let Value::Object(obj) = input {
        let condition_met = obj.get(if_field).is_some_and(|v| !matches!(v, Value::Null));

        if condition_met {
            let missing_fields: Vec<&String> = then_fields
                .iter()
                .filter(|name| obj.get(*name).is_none_or(|v| matches!(v, Value::Null)))
                .collect();

            if !missing_fields.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Since '{}' is provided, {} must also be provided",
                        if_field,
                        missing_fields
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    path:    Some(path.to_string()),
                });
            }
        }
    }

    Ok(())
}

/// Validate requirement based on absence: if one field is missing, others must be provided.
fn validate_required_if_absent(
    input: &Value,
    absent_field: &str,
    then_fields: &[String],
    path: &str,
) -> Result<()> {
    if let Value::Object(obj) = input {
        let field_absent = obj.get(absent_field).is_none_or(|v| matches!(v, Value::Null));

        if field_absent {
            let missing_fields: Vec<&String> = then_fields
                .iter()
                .filter(|name| obj.get(*name).is_none_or(|v| matches!(v, Value::Null)))
                .collect();

            if !missing_fields.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Since '{}' is not provided, {} must be provided",
                        absent_field,
                        missing_fields
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    path:    Some(path.to_string()),
                });
            }
        }
    }

    Ok(())
}
