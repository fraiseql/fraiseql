//! Composite validation rules combining multiple validators.
//!
//! This module provides combinators for composing validators:
//! - `All`: All validators must pass
//! - `Any`: At least one validator must pass
//! - `Not`: Validator must fail (negation)
//! - `Optional`: Validator only applies if field is present
//!
//! # Examples
//!
//! ```ignore
//! // All validators must pass: required AND pattern
//! ValidationRule::All(vec![
//!     ValidationRule::Required,
//!     ValidationRule::Pattern { pattern: "^[a-z]+$".to_string(), message: None }
//! ])
//!
//! // At least one must pass: strong password OR long password
//! ValidationRule::Any(vec![
//!     ValidationRule::Pattern { pattern: strong_password.to_string(), message: None },
//!     ValidationRule::Length { min: Some(20), max: None }
//! ])
//! ```

use std::fmt;

use crate::{
    error::{FraiseQLError, Result},
    validation::rules::ValidationRule,
};

/// Composite validation error that aggregates multiple validation errors.
#[derive(Debug, Clone)]
pub struct CompositeError {
    /// The operator being applied (all, any, not, optional)
    pub operator: CompositeOperator,
    /// Individual validation errors
    pub errors:   Vec<String>,
    /// The field being validated
    pub field:    String,
}

impl fmt::Display for CompositeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operator)?;
        if !self.errors.is_empty() {
            write!(f, ": {}", self.errors.join("; "))?;
        }
        Ok(())
    }
}

/// Composite validation operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeOperator {
    /// All validators must pass
    All,
    /// At least one validator must pass
    Any,
    /// Validator must fail (negation)
    Not,
    /// Validator only applies if field is present
    Optional,
}

impl fmt::Display for CompositeOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(f, "All validators must pass"),
            Self::Any => write!(f, "At least one validator must pass"),
            Self::Not => write!(f, "Validator must fail"),
            Self::Optional => write!(f, "Optional validation"),
        }
    }
}

/// Validates that all rules pass (logical AND).
///
/// All validators in the list must pass successfully. If any validator fails,
/// the entire validation fails with aggregated error messages.
///
/// # Arguments
/// * `rules` - List of validation rules to apply (all must pass)
/// * `field_value` - The value being validated
/// * `field_name` - Name of the field for error reporting
/// * `is_present` - Whether the field is present/non-null
///
/// # Returns
/// - `Ok(())` if all rules pass
/// - `Err` if any rule fails, containing all error messages
pub fn validate_all(
    rules: &[ValidationRule],
    field_value: &str,
    field_name: &str,
    is_present: bool,
) -> Result<()> {
    let mut errors = Vec::new();

    for rule in rules {
        if let Err(e) = validate_single_rule(rule, field_value, field_name, is_present) {
            errors.push(format!("{}", e));
        }
    }

    if !errors.is_empty() {
        return Err(FraiseQLError::Validation {
            message: format!(
                "All validators must pass for '{}': {}",
                field_name,
                errors.join("; ")
            ),
            path:    Some(field_name.to_string()),
        });
    }

    Ok(())
}

/// Validates that at least one rule passes (logical OR).
///
/// At least one validator in the list must pass. Only if all validators fail
/// is the entire validation considered failed.
///
/// # Arguments
/// * `rules` - List of validation rules (at least one must pass)
/// * `field_value` - The value being validated
/// * `field_name` - Name of the field for error reporting
/// * `is_present` - Whether the field is present/non-null
///
/// # Returns
/// - `Ok(())` if at least one rule passes
/// - `Err` if all rules fail, containing all error messages
pub fn validate_any(
    rules: &[ValidationRule],
    field_value: &str,
    field_name: &str,
    is_present: bool,
) -> Result<()> {
    let mut errors = Vec::new();
    let mut passed_count = 0;

    for rule in rules {
        match validate_single_rule(rule, field_value, field_name, is_present) {
            Ok(()) => {
                passed_count += 1;
            },
            Err(e) => {
                errors.push(format!("{}", e));
            },
        }
    }

    if passed_count == 0 {
        return Err(FraiseQLError::Validation {
            message: format!(
                "At least one validator must pass for '{}': {}",
                field_name,
                errors.join("; ")
            ),
            path:    Some(field_name.to_string()),
        });
    }

    Ok(())
}

/// Validates that a rule fails (logical NOT/negation).
///
/// The validator is inverted - it passes if the rule would normally fail,
/// and fails if the rule would normally pass.
///
/// # Arguments
/// * `rule` - The validation rule to negate
/// * `field_value` - The value being validated
/// * `field_name` - Name of the field for error reporting
/// * `is_present` - Whether the field is present/non-null
///
/// # Returns
/// - `Ok(())` if the rule fails (as expected)
/// - `Err` if the rule passes (when it should fail)
pub fn validate_not(
    rule: &ValidationRule,
    field_value: &str,
    field_name: &str,
    is_present: bool,
) -> Result<()> {
    match validate_single_rule(rule, field_value, field_name, is_present) {
        Ok(()) => Err(FraiseQLError::Validation {
            message: format!("Validator for '{}' must fail but passed", field_name),
            path:    Some(field_name.to_string()),
        }),
        Err(_) => Ok(()), // Validator failed as expected
    }
}

/// Validates a rule only if the field is present.
///
/// If the field is absent/null, validation is skipped (passes).
/// If the field is present, the rule is applied normally.
///
/// # Arguments
/// * `rule` - The validation rule to conditionally apply
/// * `field_value` - The value being validated
/// * `field_name` - Name of the field for error reporting
/// * `is_present` - Whether the field is present/non-null
///
/// # Returns
/// - `Ok(())` if field is absent or if rule passes
/// - `Err` if field is present and rule fails
pub fn validate_optional(
    rule: &ValidationRule,
    field_value: &str,
    field_name: &str,
    is_present: bool,
) -> Result<()> {
    if !is_present {
        return Ok(());
    }

    validate_single_rule(rule, field_value, field_name, is_present)
}

/// Validates a single rule against a field value.
///
/// This is a helper function that applies a basic validation rule.
/// For complex rules, this would dispatch to specialized validators.
fn validate_single_rule(
    rule: &ValidationRule,
    field_value: &str,
    field_name: &str,
    _is_present: bool,
) -> Result<()> {
    match rule {
        ValidationRule::Required => {
            if field_value.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!("Field '{}' is required", field_name),
                    path:    Some(field_name.to_string()),
                });
            }
            Ok(())
        },
        ValidationRule::Pattern { pattern, message } => {
            if let Ok(regex) = regex::Regex::new(pattern) {
                if !regex.is_match(field_value) {
                    return Err(FraiseQLError::Validation {
                        message: message.clone().unwrap_or_else(|| {
                            format!("'{}' must match pattern: {}", field_name, pattern)
                        }),
                        path:    Some(field_name.to_string()),
                    });
                }
            }
            Ok(())
        },
        ValidationRule::Length { min, max } => {
            let len = field_value.len();
            if let Some(min_len) = min {
                if len < *min_len {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "'{}' must be at least {} characters",
                            field_name, min_len
                        ),
                        path:    Some(field_name.to_string()),
                    });
                }
            }
            if let Some(max_len) = max {
                if len > *max_len {
                    return Err(FraiseQLError::Validation {
                        message: format!("'{}' must be at most {} characters", field_name, max_len),
                        path:    Some(field_name.to_string()),
                    });
                }
            }
            Ok(())
        },
        ValidationRule::Enum { values } => {
            if !values.contains(&field_value.to_string()) {
                return Err(FraiseQLError::Validation {
                    message: format!("'{}' must be one of: {}", field_name, values.join(", ")),
                    path:    Some(field_name.to_string()),
                });
            }
            Ok(())
        },
        // For other rule types, we skip validation in this basic implementation
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::rules::ValidationRule;

    #[test]
    fn test_validate_all_passes() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let result = validate_all(&rules, "hello123", "password", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_all_fails_first() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(10),
                max: None,
            },
        ];
        let result = validate_all(&rules, "short", "password", true);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("All validators must pass"));
        }
    }

    #[test]
    fn test_validate_all_fails_second() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_all(&rules, "Hello123", "username", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_all_multiple_failures() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(10),
                max: None,
            },
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_all(&rules, "Hi", "field", true);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("All validators must pass"));
        }
    }

    #[test]
    fn test_validate_any_passes_first() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(20),
                max: None,
            },
        ];
        let result = validate_any(&rules, "abc", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_any_passes_second() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(2),
                max: None,
            },
        ];
        let result = validate_any(&rules, "Hi", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_any_fails_all() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(20),
                max: None,
            },
        ];
        let result = validate_any(&rules, "Hi", "field", true);
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("At least one validator must pass"));
        }
    }

    #[test]
    fn test_validate_any_multiple_passes() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(2),
                max: None,
            },
            ValidationRule::Enum {
                values: vec!["hello".to_string(), "world".to_string()],
            },
        ];
        let result = validate_any(&rules, "hello", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_not_passes_when_rule_fails() {
        let rule = ValidationRule::Pattern {
            pattern: "^[0-9]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_not_fails_when_rule_passes() {
        let rule = ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc", "field", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_optional_skips_absent() {
        let rule = ValidationRule::Length {
            min: Some(100),
            max: None,
        };
        let result = validate_optional(&rule, "", "field", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_applies_present() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: None,
        };
        let result = validate_optional(&rule, "hello", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_fails_present() {
        let rule = ValidationRule::Length {
            min: Some(10),
            max: None,
        };
        let result = validate_optional(&rule, "hi", "field", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_composite_operator_display() {
        assert_eq!(CompositeOperator::All.to_string(), "All validators must pass");
        assert_eq!(CompositeOperator::Any.to_string(), "At least one validator must pass");
        assert_eq!(CompositeOperator::Not.to_string(), "Validator must fail");
        assert_eq!(CompositeOperator::Optional.to_string(), "Optional validation");
    }

    #[test]
    fn test_nested_all_and_pattern() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: Some(20),
            },
            ValidationRule::Pattern {
                pattern: "^[A-Za-z0-9]+$".to_string(),
                message: Some("Username must be alphanumeric".to_string()),
            },
        ];
        let result = validate_all(&rules, "User1234", "username", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_nested_all_fails_on_length() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: Some(20),
            },
            ValidationRule::Pattern {
                pattern: "^[A-Za-z0-9]+$".to_string(),
                message: Some("Username must be alphanumeric".to_string()),
            },
        ];
        let result = validate_all(&rules, "Hi", "username", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_strong_password_pattern_all() {
        // Strong password: at least 1 uppercase, 1 lowercase, 1 digit
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: None,
            },
            ValidationRule::Pattern {
                pattern: "^(?=.*[A-Z])".to_string(), // Lookahead for uppercase
                message: Some("Must contain at least one uppercase letter".to_string()),
            },
        ];
        let result = validate_all(&rules, "Password123", "password", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enum_or_pattern_any() {
        let rules = vec![
            ValidationRule::Enum {
                values: vec!["admin".to_string(), "user".to_string()],
            },
            ValidationRule::Pattern {
                pattern: "^guest_[0-9]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_any(&rules, "guest_123", "role", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_not_numeric_for_string_field() {
        let rule = ValidationRule::Pattern {
            pattern: "^[0-9]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc123", "code", true);
        // Should pass because the regex doesn't match the whole string
        assert!(result.is_ok());
    }

    #[test]
    fn test_composite_error_display() {
        let error = CompositeError {
            operator: CompositeOperator::All,
            errors:   vec!["error1".to_string(), "error2".to_string()],
            field:    "field".to_string(),
        };
        let display_str = error.to_string();
        assert!(display_str.contains("All validators must pass"));
        assert!(display_str.contains("error1"));
        assert!(display_str.contains("error2"));
    }

    #[test]
    fn test_multiple_validators_with_required() {
        let rules = vec![ValidationRule::Required];
        let result = validate_all(&rules, "test", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_rules_all() {
        let rules: Vec<ValidationRule> = vec![];
        let result = validate_all(&rules, "test", "field", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_rules_any() {
        let rules: Vec<ValidationRule> = vec![];
        let result = validate_any(&rules, "test", "field", true);
        // Any with no rules vacuously fails (nothing passed)
        assert!(result.is_err());
    }

    #[test]
    fn test_length_min_max() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: Some(10),
        };
        let result = validate_single_rule(&rule, "hello", "password", true);
        assert!(result.is_ok());

        let result = validate_single_rule(&rule, "hi", "password", true);
        assert!(result.is_err());

        let result = validate_single_rule(&rule, "this_is_too_long", "password", true);
        assert!(result.is_err());
    }
}
