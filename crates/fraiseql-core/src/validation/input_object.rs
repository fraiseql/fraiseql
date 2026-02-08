//! Input object-level validation.
//!
//! This module provides validation capabilities at the input object level,
//! applying cross-field rules and aggregating errors from multiple validators.
//!
//! # Examples
//!
//! ```ignore
//! // Validate entire input object
//! let input = json!({
//!     "name": "John",
//!     "email": "john@example.com",
//!     "phone": null
//! });
//!
//! let validators = vec![
//!     InputObjectRule::AnyOf { fields: vec!["email", "phone"] },
//!     InputObjectRule::ConditionalRequired {
//!         if_field: "name",
//!         then_fields: vec!["email"]
//!     }
//! ];
//!
//! validate_input_object(&input, &validators)?;
//! ```

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Rules that apply at the input object level.
#[derive(Debug, Clone)]
pub enum InputObjectRule {
    /// At least one field from the set must be provided
    AnyOf { fields: Vec<String> },
    /// Exactly one field from the set must be provided
    OneOf { fields: Vec<String> },
    /// If one field is present, others must be present
    ConditionalRequired {
        if_field:    String,
        then_fields: Vec<String>,
    },
    /// If one field is absent, others must be present
    RequiredIfAbsent {
        absent_field: String,
        then_fields:  Vec<String>,
    },
    /// Custom validator function name to invoke
    Custom { name: String },
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
    pub fn new() -> Self {
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
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Convert to a Result, failing if there are errors.
    pub fn into_result(self) -> Result<()> {
        self.into_result_with_path("input")
    }

    /// Convert to a Result with a custom path, failing if there are errors.
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
/// # Returns
/// - `Ok(())` if all rules pass
/// - `Err` containing all error messages if any rule fails
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
            message: format!("Custom validator '{}' not implemented", name),
            path:    Some(path.to_string()),
        }),
    }
}

/// Validate that at least one field from the set is present.
fn validate_any_of(input: &Value, fields: &[String], path: &str) -> Result<()> {
    if let Value::Object(obj) = input {
        let has_any = fields
            .iter()
            .any(|name| obj.get(name).map(|v| !matches!(v, Value::Null)).unwrap_or(false));

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
            .filter(|name| obj.get(*name).map(|v| !matches!(v, Value::Null)).unwrap_or(false))
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
        let condition_met = obj.get(if_field).map(|v| !matches!(v, Value::Null)).unwrap_or(false);

        if condition_met {
            let missing_fields: Vec<&String> = then_fields
                .iter()
                .filter(|name| obj.get(*name).map(|v| matches!(v, Value::Null)).unwrap_or(true))
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
        let field_absent = obj.get(absent_field).map(|v| matches!(v, Value::Null)).unwrap_or(true);

        if field_absent {
            let missing_fields: Vec<&String> = then_fields
                .iter()
                .filter(|name| obj.get(*name).map(|v| matches!(v, Value::Null)).unwrap_or(true))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_any_of_passes() {
        let input = json!({
            "email": "user@example.com",
            "phone": null,
            "address": null
        });
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_any_of_fails() {
        let input = json!({
            "email": null,
            "phone": null,
            "address": null
        });
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_one_of_passes() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_one_of_fails_both_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_one_of_fails_neither_present() {
        let input = json!({
            "entityId": null,
            "entityPayload": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_conditional_required_passes() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_conditional_required_fails() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_conditional_required_skips_when_condition_false() {
        let input = json!({
            "isPremium": null,
            "paymentMethod": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_required_if_absent_passes() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_required_if_absent_fails() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": null,
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_required_if_absent_skips_when_field_present() {
        let input = json!({
            "addressId": "addr_123",
            "street": null,
            "city": null,
            "zip": null
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_rules_all_pass() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null,
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_rules_one_fails() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null,
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_rules_both_fail() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" },
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            // Should have both error messages aggregated
            assert!(message.contains("Exactly one") || message.contains("must also be provided"));
        }
    }

    #[test]
    fn test_error_aggregation() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" },
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];

        let result = validate_input_object(&input, &rules, Some("createInput"));
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, path }) = result {
            assert_eq!(path, Some("createInput".to_string()));
            assert!(message.contains("failed"));
        }
    }

    #[test]
    fn test_conditional_required_multiple_fields() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": "50.00"
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isInternational".to_string(),
            then_fields: vec!["customsCode".to_string(), "importDuties".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_conditional_required_multiple_fields_one_missing() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isInternational".to_string(),
            then_fields: vec!["customsCode".to_string(), "importDuties".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_result_aggregation() {
        let mut result = InputObjectValidationResult::new();
        assert!(!result.has_errors());
        assert_eq!(result.error_count, 0);

        result.add_error("Error 1".to_string());
        assert!(result.has_errors());
        assert_eq!(result.error_count, 1);

        result.add_errors(vec!["Error 2".to_string(), "Error 3".to_string()]);
        assert_eq!(result.error_count, 3);
    }

    #[test]
    fn test_validation_result_into_result_success() {
        let result = InputObjectValidationResult::new();
        assert!(result.into_result().is_ok());
    }

    #[test]
    fn test_validation_result_into_result_failure() {
        let mut result = InputObjectValidationResult::new();
        result.add_error("Test error".to_string());
        assert!(result.into_result().is_err());
    }

    #[test]
    fn test_non_object_input() {
        let input = json!([1, 2, 3]);
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec!["field".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_rules() {
        let input = json!({"field": "value"});
        let rules: Vec<InputObjectRule> = vec![];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_validator_not_implemented() {
        let input = json!({"field": "value"});
        let rules = vec![InputObjectRule::Custom {
            name: "myValidator".to_string(),
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("not implemented"));
        }
    }

    #[test]
    fn test_complex_create_or_reference_pattern() {
        // Either provide entityId OR provide (name + description), but not both
        let input = json!({
            "entityId": "123",
            "name": null,
            "description": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "name".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_complex_address_pattern() {
        // Either provide addressId OR provide all of (street, city, zip)
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(result.is_ok());
    }
}
