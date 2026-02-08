//! Mutual exclusivity and conditional requirement validators.
//!
//! This module provides validators for complex field-relationship rules:
//! - OneOf: Exactly one field from a set must be provided
//! - AnyOf: At least one field from a set must be provided
//! - ConditionalRequired: If one field is present, others must be too
//! - RequiredIfAbsent: If one field is missing, others must be provided

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Validates that exactly one field from the specified set is provided.
///
/// # Example
/// ```ignore
/// // Either entityId OR entityPayload, but not both
/// validator.validate_one_of(input, &["entityId", "entityPayload"])
/// ```
pub struct OneOfValidator;

impl OneOfValidator {
    /// Validate that exactly one field from the set is present and non-null.
    pub fn validate(
        input: &Value,
        field_names: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        let present_count = field_names
            .iter()
            .filter(|name| {
                if let Value::Object(obj) = input {
                    obj.get(*name).map(|v| !matches!(v, Value::Null)).unwrap_or(false)
                } else {
                    false
                }
            })
            .count();

        if present_count != 1 {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Exactly one of [{}] must be provided, but {} {} provided",
                    field_names.join(", "),
                    present_count,
                    if present_count == 1 { "was" } else { "were" }
                ),
                path:    Some(field_path.to_string()),
            });
        }

        Ok(())
    }
}

/// Validates that at least one field from the specified set is provided.
///
/// # Example
/// ```ignore
/// // At least one of: email, phone, address must be present
/// validator.validate_any_of(input, &["email", "phone", "address"])
/// ```
pub struct AnyOfValidator;

impl AnyOfValidator {
    /// Validate that at least one field from the set is present and non-null.
    pub fn validate(
        input: &Value,
        field_names: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        let has_any = field_names.iter().any(|name| {
            if let Value::Object(obj) = input {
                obj.get(name).map(|v| !matches!(v, Value::Null)).unwrap_or(false)
            } else {
                false
            }
        });

        if !has_any {
            return Err(FraiseQLError::Validation {
                message: format!("At least one of [{}] must be provided", field_names.join(", ")),
                path:    Some(field_path.to_string()),
            });
        }

        Ok(())
    }
}

/// Validates conditional requirement: if one field is present, others must be too.
///
/// # Example
/// ```ignore
/// // If isPremium is true, then paymentMethod is required
/// validator.validate_conditional_required(
///     input,
///     "isPremium",
///     &["paymentMethod"]
/// )
/// ```
pub struct ConditionalRequiredValidator;

impl ConditionalRequiredValidator {
    /// Validate that if `if_field_present` is present, all `then_required` fields must be too.
    pub fn validate(
        input: &Value,
        if_field_present: &str,
        then_required: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        if let Value::Object(obj) = input {
            // Check if the condition field is present and non-null
            let condition_met =
                obj.get(if_field_present).map(|v| !matches!(v, Value::Null)).unwrap_or(false);

            if condition_met {
                // If condition is met, check that all required fields are present
                let missing_fields: Vec<&String> = then_required
                    .iter()
                    .filter(|name| obj.get(*name).map(|v| matches!(v, Value::Null)).unwrap_or(true))
                    .collect();

                if !missing_fields.is_empty() {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Since '{}' is provided, {} must also be provided",
                            if_field_present,
                            missing_fields
                                .iter()
                                .map(|s| format!("'{}'", s))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        path:    Some(field_path.to_string()),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Validates conditional requirement based on absence: if one field is missing, others must be
/// provided.
///
/// # Example
/// ```ignore
/// // If addressId is not provided, then street, city, zip must all be provided
/// validator.validate_required_if_absent(
///     input,
///     "addressId",
///     &["street", "city", "zip"]
/// )
/// ```
pub struct RequiredIfAbsentValidator;

impl RequiredIfAbsentValidator {
    /// Validate that if `absent_field` is absent/null, all `then_required` fields must be provided.
    pub fn validate(
        input: &Value,
        absent_field: &str,
        then_required: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        if let Value::Object(obj) = input {
            // Check if the condition field is absent or null
            let field_absent =
                obj.get(absent_field).map(|v| matches!(v, Value::Null)).unwrap_or(true);

            if field_absent {
                // If field is absent, check that all required fields are present
                let missing_fields: Vec<&String> = then_required
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
                        path:    Some(field_path.to_string()),
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_one_of_validator_exactly_one_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_one_of_validator_both_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_one_of_validator_neither_present() {
        let input = json!({
            "entityId": null,
            "entityPayload": null
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_one_of_validator_missing_field() {
        let input = json!({
            "entityId": "123"
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_any_of_validator_one_present() {
        let input = json!({
            "email": "user@example.com",
            "phone": null,
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_any_of_validator_multiple_present() {
        let input = json!({
            "email": "user@example.com",
            "phone": "+1234567890",
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_any_of_validator_none_present() {
        let input = json!({
            "email": null,
            "phone": null,
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_conditional_required_validator_condition_met_requirement_met() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_conditional_required_validator_condition_met_requirement_missing() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_conditional_required_validator_condition_not_met() {
        let input = json!({
            "isPremium": null,
            "paymentMethod": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_conditional_required_validator_multiple_requirements() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": "50.00"
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isInternational",
            &["customsCode".to_string(), "importDuties".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_conditional_required_validator_one_requirement_missing() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isInternational",
            &["customsCode".to_string(), "importDuties".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_required_if_absent_validator_field_absent_requirements_met() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_required_if_absent_validator_field_absent_requirements_missing() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": null,
            "zip": "12345"
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_required_if_absent_validator_field_present() {
        let input = json!({
            "addressId": "addr_123",
            "street": null,
            "city": null,
            "zip": null
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_required_if_absent_validator_all_missing_from_object() {
        let input = json!({});
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string()],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_error_messages_include_context() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            Some("createInput"),
        );
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { path, .. }) = result {
            assert_eq!(path, Some("createInput".to_string()));
        }
    }
}
