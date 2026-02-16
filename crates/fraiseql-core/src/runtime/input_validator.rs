//! Input validation for GraphQL mutations and queries.
//!
//! This module provides the validation pipeline that processes GraphQL input
//! variables and validates them against defined validation rules before
//! execution.

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result, ValidationFieldError},
    schema::CompiledSchema,
    validation::ValidationRule,
};

/// Validation error aggregator - collects multiple validation errors.
#[derive(Debug, Clone, Default)]
pub struct ValidationErrorCollection {
    /// All collected validation errors.
    pub errors: Vec<ValidationFieldError>,
}

impl ValidationErrorCollection {
    /// Create a new empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error to the collection.
    pub fn add_error(&mut self, error: ValidationFieldError) {
        self.errors.push(error);
    }

    /// Check if there are any errors.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get the number of errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Convert to a FraiseQL error.
    pub fn to_error(&self) -> FraiseQLError {
        if self.errors.is_empty() {
            FraiseQLError::validation("No validation errors")
        } else if self.errors.len() == 1 {
            let err = &self.errors[0];
            FraiseQLError::Validation {
                message: err.to_string(),
                path: Some(err.field.clone()),
            }
        } else {
            let messages: Vec<String> = self.errors.iter().map(|e| e.to_string()).collect();
            FraiseQLError::Validation {
                message: format!("Multiple validation errors: {}", messages.join("; ")),
                path: None,
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
/// * `scalar_type_name` - Name of the custom scalar type (e.g., "LibraryCode")
/// * `schema` - The compiled schema containing custom scalar definitions
///
/// # Errors
///
/// Returns a validation error if the value doesn't match the custom scalar definition.
pub fn validate_custom_scalar(
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
pub fn validate_input(value: &Value, field_path: &str, rules: &[ValidationRule]) -> Result<()> {
    let mut errors = ValidationErrorCollection::new();

    match value {
        Value::String(s) => {
            for rule in rules {
                if let Err(FraiseQLError::Validation { message, .. }) =
                    validate_string_field(s, field_path, rule)
                {
                    if let Some(field_err) = extract_field_error(&message) {
                        errors.add_error(field_err);
                    }
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

/// Validate a string field against a validation rule.
fn validate_string_field(value: &str, field_path: &str, rule: &ValidationRule) -> Result<()> {
    match rule {
        ValidationRule::Required => {
            if value.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Field validation failed: {}",
                        ValidationFieldError::new(field_path, "required", "Field is required")
                    ),
                    path: Some(field_path.to_string()),
                });
            }
            Ok(())
        },
        ValidationRule::Pattern { pattern, message } => {
            let regex = regex::Regex::new(pattern)
                .map_err(|e| FraiseQLError::validation(format!("Invalid regex pattern: {}", e)))?;
            if regex.is_match(value) {
                Ok(())
            } else {
                let msg = message.clone().unwrap_or_else(|| "Pattern mismatch".to_string());
                Err(FraiseQLError::Validation {
                    message: format!(
                        "Field validation failed: {}",
                        ValidationFieldError::new(field_path, "pattern", msg)
                    ),
                    path: Some(field_path.to_string()),
                })
            }
        },
        ValidationRule::Length { min, max } => {
            let len = value.len();
            let valid = if let Some(m) = min { len >= *m } else { true }
                && if let Some(m) = max { len <= *m } else { true };

            if valid {
                Ok(())
            } else {
                let msg = match (min, max) {
                    (Some(m), Some(x)) => format!("Length must be between {} and {}", m, x),
                    (Some(m), None) => format!("Length must be at least {}", m),
                    (None, Some(x)) => format!("Length must be at most {}", x),
                    (None, None) => "Length validation failed".to_string(),
                };
                Err(FraiseQLError::Validation {
                    message: format!(
                        "Field validation failed: {}",
                        ValidationFieldError::new(field_path, "length", msg)
                    ),
                    path: Some(field_path.to_string()),
                })
            }
        },
        ValidationRule::Enum { values } => {
            if values.contains(&value.to_string()) {
                Ok(())
            } else {
                Err(FraiseQLError::Validation {
                    message: format!(
                        "Field validation failed: {}",
                        ValidationFieldError::new(
                            field_path,
                            "enum",
                            format!("Must be one of: {}", values.join(", "))
                        )
                    ),
                    path: Some(field_path.to_string()),
                })
            }
        },
        _ => Ok(()), // Other rule types handled elsewhere
    }
}

/// Extract field error information from an error message.
fn extract_field_error(message: &str) -> Option<ValidationFieldError> {
    // Format: "Field validation failed: field (rule): message"
    if message.contains("Field validation failed:") {
        if let Some(field_start) = message.find("Field validation failed: ") {
            let rest = &message[field_start + "Field validation failed: ".len()..];
            if let Some(paren_start) = rest.find('(') {
                if let Some(paren_end) = rest.find(')') {
                    let field = rest[..paren_start].trim().to_string();
                    let rule_type = rest[paren_start + 1..paren_end].to_string();
                    let msg_start = rest.find(": ").unwrap_or(0) + 2;
                    let message_text = rest[msg_start..].to_string();
                    return Some(ValidationFieldError::new(field, rule_type, message_text));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_collection() {
        let mut errors = ValidationErrorCollection::new();
        assert!(errors.is_empty());

        errors.add_error(ValidationFieldError::new("email", "pattern", "Invalid email"));
        assert!(!errors.is_empty());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_validation_error_collection_to_error() {
        let mut errors = ValidationErrorCollection::new();
        errors.add_error(ValidationFieldError::new("email", "pattern", "Invalid email"));

        let err = errors.to_error();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn test_validate_required_field() {
        let rule = ValidationRule::Required;
        let result = validate_string_field("value", "field", &rule);
        assert!(result.is_ok());

        let result = validate_string_field("", "field", &rule);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_pattern() {
        let rule = ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        };

        let result = validate_string_field("hello", "field", &rule);
        assert!(result.is_ok());

        let result = validate_string_field("Hello", "field", &rule);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_length() {
        let rule = ValidationRule::Length {
            min: Some(3),
            max: Some(10),
        };

        let result = validate_string_field("hello", "field", &rule);
        assert!(result.is_ok());

        let result = validate_string_field("hi", "field", &rule);
        assert!(result.is_err());

        let result = validate_string_field("this is too long", "field", &rule);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_enum() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };

        let result = validate_string_field("active", "field", &rule);
        assert!(result.is_ok());

        let result = validate_string_field("unknown", "field", &rule);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_null_field() {
        let rule = ValidationRule::Required;
        let result = validate_input(&Value::Null, "field", &[rule]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_custom_scalar_library_code_valid() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(Default::default());

            let mut def = CustomTypeDef::new("LibraryCode".to_string());
            def.validation_rules = vec![ValidationRule::Pattern {
                pattern: r"^LIB-[0-9]{4}$".to_string(),
                message: Some("Library code must be LIB-#### format".to_string()),
            }];

            registry.register("LibraryCode".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        let value = serde_json::json!("LIB-1234");
        let result = validate_custom_scalar(&value, "LibraryCode", &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_scalar_library_code_invalid() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(Default::default());

            let mut def = CustomTypeDef::new("LibraryCode".to_string());
            def.validation_rules = vec![ValidationRule::Pattern {
                pattern: r"^LIB-[0-9]{4}$".to_string(),
                message: Some("Library code must be LIB-#### format".to_string()),
            }];

            registry.register("LibraryCode".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        let value = serde_json::json!("INVALID");
        let result = validate_custom_scalar(&value, "LibraryCode", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_custom_scalar_student_id_with_length() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(Default::default());

            let mut def = CustomTypeDef::new("StudentID".to_string());
            def.validation_rules = vec![
                ValidationRule::Pattern {
                    pattern: r"^STU-[0-9]{4}-[0-9]{3}$".to_string(),
                    message: None,
                },
                ValidationRule::Length {
                    min: Some(12),
                    max: Some(12),
                },
            ];

            registry.register("StudentID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: matches pattern and length
        let value = serde_json::json!("STU-2024-001");
        let result = validate_custom_scalar(&value, "StudentID", &schema);
        assert!(result.is_ok());

        // Invalid: wrong pattern
        let value = serde_json::json!("STUDENT-2024");
        let result = validate_custom_scalar(&value, "StudentID", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_unknown_scalar_type_passthrough() {
        use crate::schema::CompiledSchema;

        let schema = CompiledSchema::new();

        // Unknown scalar types should pass through (they're built-in types)
        let value = serde_json::json!("any value");
        let result = validate_custom_scalar(&value, "UnknownType", &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_scalar_patient_id_passthrough() {
        use crate::schema::CompiledSchema;

        // Schema without PatientID definition
        let schema = CompiledSchema::new();

        let value = serde_json::json!("PAT-123456");
        let result = validate_custom_scalar(&value, "PatientID", &schema);
        // Should pass through (not registered as custom scalar)
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_custom_scalar_with_elo_expression() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(Default::default());

            let mut def = CustomTypeDef::new("StudentID".to_string());
            def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

            registry.register("StudentID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: matches ELO expression
        let value = serde_json::json!("STU-2024-001");
        let result = validate_custom_scalar(&value, "StudentID", &schema);
        assert!(result.is_ok());

        // Invalid: doesn't match ELO expression
        let value = serde_json::json!("INVALID");
        let result = validate_custom_scalar(&value, "StudentID", &schema);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_custom_scalar_combined_rules_and_elo() {
        use crate::{
            schema::CompiledSchema,
            validation::{CustomTypeDef, CustomTypeRegistry},
        };

        let schema = {
            let mut s = CompiledSchema::new();
            let registry = CustomTypeRegistry::new(Default::default());

            let mut def = CustomTypeDef::new("PatientID".to_string());
            def.validation_rules = vec![ValidationRule::Length {
                min: Some(10),
                max: Some(10),
            }];
            def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

            registry.register("PatientID".to_string(), def).unwrap();

            s.custom_scalars = registry;
            s
        };

        // Valid: passes both length rule and ELO expression
        let value = serde_json::json!("PAT-123456");
        let result = validate_custom_scalar(&value, "PatientID", &schema);
        assert!(result.is_ok());

        // Invalid: passes length but fails ELO expression
        let value = serde_json::json!("NOTVALID!");
        let result = validate_custom_scalar(&value, "PatientID", &schema);
        assert!(result.is_err());

        // Invalid: fails length rule
        let value = serde_json::json!("PAT-12345");
        let result = validate_custom_scalar(&value, "PatientID", &schema);
        assert!(result.is_err());
    }
}
