//! Input validation for GraphQL mutations and queries.
//!
//! This module provides the validation pipeline that processes GraphQL input
//! variables and validates them against defined validation rules before
//! execution.

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result, ValidationFieldError},
    schema::CompiledSchema,
    validation::{LengthCheck, ValidationRule, check_length},
};

/// Validation error aggregator - collects multiple validation errors.
#[derive(Debug, Clone, Default)]
pub struct ValidationErrorCollection {
    /// All collected validation errors.
    pub errors: Vec<ValidationFieldError>,
}

impl ValidationErrorCollection {
    /// Create a new empty error collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error to the collection.
    pub fn add_error(&mut self, error: ValidationFieldError) {
        self.errors.push(error);
    }

    /// Check if there are any errors.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get the number of errors.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.errors.len()
    }

    /// Convert to a FraiseQL error.
    #[must_use]
    pub fn to_error(&self) -> FraiseQLError {
        if self.errors.is_empty() {
            FraiseQLError::validation("No validation errors")
        } else if self.errors.len() == 1 {
            let err = &self.errors[0];
            FraiseQLError::Validation {
                message: err.to_string(),
                path:    Some(err.field.clone()),
            }
        } else {
            let messages: Vec<String> = self.errors.iter().map(|e| e.to_string()).collect();
            FraiseQLError::Validation {
                message: format!("Multiple validation errors: {}", messages.join("; ")),
                path:    None,
            }
        }
    }
}

/// Validate a scalar value against a custom scalar type definition.
///
/// This function validates a JSON value against a custom scalar type registered
/// in the schema, checking both validation rules and ELO expressions.
///
/// # Arguments
///
/// * `value` - The JSON value to validate
/// * `scalar_type_name` - Name of the custom scalar type (e.g., "`LibraryCode`")
/// * `schema` - The compiled schema containing custom scalar definitions
///
/// # Errors
///
/// Returns a validation error if the value doesn't match the custom scalar definition.
pub fn validate_custom_scalar_from_schema(
    value: &Value,
    scalar_type_name: &str,
    schema: &CompiledSchema,
) -> Result<()> {
    // Check if this is a custom scalar type
    if schema.custom_scalars.exists(scalar_type_name) {
        schema.custom_scalars.validate(scalar_type_name, value)
    } else {
        // Not a custom scalar, pass through (built-in type)
        Ok(())
    }
}

/// Validate JSON input against validation rules.
///
/// This function recursively validates a JSON value against a set of
/// validation rules, collecting all errors that occur.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if any rule is violated (e.g., string
/// too short, value out of range, or a required field is null).
pub fn validate_input(value: &Value, field_path: &str, rules: &[ValidationRule]) -> Result<()> {
    let mut errors = ValidationErrorCollection::new();

    match value {
        Value::String(s) => {
            for rule in rules {
                // The structured error is kept structured. Formatting it into a
                // message and re-parsing that message with `find('(')` meant a
                // field path containing a parenthesis — or any change to the
                // message format — silently discarded the violation and let
                // validation *pass* (#720).
                if let Err(field_err) = check_string_field(s, field_path, rule) {
                    errors.add_error(field_err);
                }
            }
        },
        Value::Null => {
            for rule in rules {
                if rule.is_required() {
                    errors.add_error(ValidationFieldError::new(
                        field_path,
                        "required",
                        "Field is required",
                    ));
                }
            }
        },
        _ => {
            // Other types (number, bool, array, object) have different validation logic
        },
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.to_error())
    }
}

/// Check a string field against one validation rule.
///
/// Returns the violation as a structured [`ValidationFieldError`]. Callers that
/// The structured form is what the aggregator consumes, so no error is
/// laundered through its own display string on the way (#720).
pub(crate) fn check_string_field(
    value: &str,
    field_path: &str,
    rule: &ValidationRule,
) -> std::result::Result<(), ValidationFieldError> {
    match rule {
        ValidationRule::Required if value.is_empty() => {
            Err(ValidationFieldError::new(field_path, "required", "Field is required"))
        },
        ValidationRule::Pattern { pattern, message } if !pattern.is_match(value) => {
            let msg = message.clone().unwrap_or_else(|| "Pattern mismatch".to_string());
            Err(ValidationFieldError::new(field_path, "pattern", msg))
        },
        ValidationRule::Length { min, max } => match check_length(value, *min, *max) {
            LengthCheck::Ok => Ok(()),
            LengthCheck::TooShort { min, actual } => Err(ValidationFieldError::new(
                field_path,
                "length",
                format!("Length must be at least {min} characters, got {actual}"),
            )),
            LengthCheck::TooLong { max, actual } => Err(ValidationFieldError::new(
                field_path,
                "length",
                format!("Length must be at most {max} characters, got {actual}"),
            )),
        },
        ValidationRule::Enum { values } if !values.iter().any(|v| v == value) => {
            Err(ValidationFieldError::new(
                field_path,
                "enum",
                format!("Must be one of: {}", values.join(", ")),
            ))
        },
        // Every other rule either passed above or is evaluated elsewhere.
        _ => Ok(()),
    }
}
