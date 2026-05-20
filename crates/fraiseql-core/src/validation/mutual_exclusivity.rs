//! Mutual exclusivity and conditional requirement validators.
//!
//! This module provides validators for complex field-relationship rules:
//! - `OneOf`: Exactly one field from a set must be provided
//! - `AnyOf`: At least one field from a set must be provided
//! - `ConditionalRequired`: If one field is present, others must be too
//! - `RequiredIfAbsent`: If one field is missing, others must be provided

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Validates that exactly one field from the specified set is provided.
///
/// # Example
/// ```
/// use fraiseql_core::validation::mutual_exclusivity::OneOfValidator;
/// use serde_json::json;
/// // Either entityId OR entityPayload, but not both
/// let input = json!({ "entityId": "123", "entityPayload": null });
/// assert!(
///     OneOfValidator::validate(&input, &["entityId".to_string(), "entityPayload".to_string()], None).is_ok(),
///     "one-of constraint satisfied when only entityId is present"
/// );
/// ```
pub struct OneOfValidator;

impl OneOfValidator {
    /// Validate that exactly one field from the set is present and non-null.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if zero or more than one field is present.
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
                    obj.get(*name).is_some_and(|v| !matches!(v, Value::Null))
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
                path: Some(field_path.to_string()),
            });
        }

        Ok(())
    }
}

/// Validates that at least one field from the specified set is provided.
///
/// # Example
/// ```
/// use fraiseql_core::validation::mutual_exclusivity::AnyOfValidator;
/// use serde_json::json;
/// // At least one of: email, phone, address must be present
/// let input = json!({ "email": "user@example.com", "phone": null, "address": null });
/// assert!(
///     AnyOfValidator::validate(&input, &["email".to_string(), "phone".to_string(), "address".to_string()], None).is_ok(),
///     "any-of constraint satisfied when at least one field is present"
/// );
/// ```
pub struct AnyOfValidator;

impl AnyOfValidator {
    /// Validate that at least one field from the set is present and non-null.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if none of the specified fields are present.
    pub fn validate(
        input: &Value,
        field_names: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        let has_any = field_names.iter().any(|name| {
            if let Value::Object(obj) = input {
                obj.get(name).is_some_and(|v| !matches!(v, Value::Null))
            } else {
                false
            }
        });

        if !has_any {
            return Err(FraiseQLError::Validation {
                message: format!("At least one of [{}] must be provided", field_names.join(", ")),
                path: Some(field_path.to_string()),
            });
        }

        Ok(())
    }
}

/// Validates conditional requirement: if one field is present, others must be too.
///
/// # Example
/// ```
/// use fraiseql_core::validation::mutual_exclusivity::ConditionalRequiredValidator;
/// use serde_json::json;
/// // If isPremium is true, then paymentMethod is required
/// let input = json!({ "isPremium": true, "paymentMethod": "credit_card" });
/// assert!(
///     ConditionalRequiredValidator::validate(&input, "isPremium", &["paymentMethod".to_string()], None).is_ok(),
///     "conditional requirement satisfied when condition field is true and required field present"
/// );
/// ```
pub struct ConditionalRequiredValidator;

impl ConditionalRequiredValidator {
    /// Validate that if `if_field_present` is present, all `then_required` fields must be too.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the condition field is present but any
    /// required field is missing.
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
                obj.get(if_field_present).is_some_and(|v| !matches!(v, Value::Null));

            if condition_met {
                // If condition is met, check that all required fields are present
                let missing_fields: Vec<&String> = then_required
                    .iter()
                    .filter(|name| obj.get(*name).is_none_or(|v| matches!(v, Value::Null)))
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
                        path: Some(field_path.to_string()),
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
/// ```
/// use fraiseql_core::validation::mutual_exclusivity::RequiredIfAbsentValidator;
/// use serde_json::json;
/// // If addressId is not provided, then street, city, zip must all be provided
/// let input = json!({ "addressId": null, "street": "123 Main St", "city": "Springfield", "zip": "12345" });
/// assert!(
///     RequiredIfAbsentValidator::validate(&input, "addressId", &["street".to_string(), "city".to_string(), "zip".to_string()], None).is_ok(),
///     "required-if-absent constraint satisfied when absent field is null and all required fields present"
/// );
/// ```
pub struct RequiredIfAbsentValidator;

impl RequiredIfAbsentValidator {
    /// Validate that if `absent_field` is absent/null, all `then_required` fields must be provided.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the condition field is absent and any
    /// required field is also missing.
    pub fn validate(
        input: &Value,
        absent_field: &str,
        then_required: &[String],
        context_path: Option<&str>,
    ) -> Result<()> {
        let field_path = context_path.unwrap_or("input");

        if let Value::Object(obj) = input {
            // Check if the condition field is absent or null
            let field_absent = obj.get(absent_field).is_none_or(|v| matches!(v, Value::Null));

            if field_absent {
                // If field is absent, check that all required fields are present
                let missing_fields: Vec<&String> = then_required
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
                        path: Some(field_path.to_string()),
                    });
                }
            }
        }

        Ok(())
    }
}
