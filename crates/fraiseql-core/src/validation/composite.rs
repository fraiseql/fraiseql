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
//! ```
//! use fraiseql_core::validation::ValidationRule;
//!
//! // All validators must pass: required AND pattern
//! let _rule = ValidationRule::All(vec![
//!     ValidationRule::Required,
//!     ValidationRule::Pattern { pattern: "^[a-z]+$".to_string(), message: None },
//! ]);
//!
//! // At least one must pass: complex password OR long password
//! let _rule = ValidationRule::Any(vec![
//!     ValidationRule::Pattern { pattern: r"^(?=.*[A-Z])(?=.*[0-9]).+$".to_string(), message: None },
//!     ValidationRule::Length { min: Some(20), max: None },
//! ]);
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
    pub errors: Vec<String>,
    /// The field being validated
    pub field: String,
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
#[non_exhaustive]
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
/// # Errors
///
/// Returns `FraiseQLError::Validation` if any rule fails, with all failure messages combined.
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
            path: Some(field_name.to_string()),
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
/// # Errors
///
/// Returns `FraiseQLError::Validation` if all rules fail, with all failure messages combined.
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
            path: Some(field_name.to_string()),
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
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the negated rule passes (when it should fail).
pub fn validate_not(
    rule: &ValidationRule,
    field_value: &str,
    field_name: &str,
    is_present: bool,
) -> Result<()> {
    match validate_single_rule(rule, field_value, field_name, is_present) {
        Ok(()) => Err(FraiseQLError::Validation {
            message: format!("Validator for '{}' must fail but passed", field_name),
            path: Some(field_name.to_string()),
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
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the field is present and the rule fails.
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
pub(crate) fn validate_single_rule(
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
                    path: Some(field_name.to_string()),
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
                        path: Some(field_name.to_string()),
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
                        path: Some(field_name.to_string()),
                    });
                }
            }
            if let Some(max_len) = max {
                if len > *max_len {
                    return Err(FraiseQLError::Validation {
                        message: format!("'{}' must be at most {} characters", field_name, max_len),
                        path: Some(field_name.to_string()),
                    });
                }
            }
            Ok(())
        },
        ValidationRule::Enum { values } => {
            if !values.contains(&field_value.to_string()) {
                return Err(FraiseQLError::Validation {
                    message: format!("'{}' must be one of: {}", field_name, values.join(", ")),
                    path: Some(field_name.to_string()),
                });
            }
            Ok(())
        },
        // For other rule types, we skip validation in this basic implementation
        _ => Ok(()),
    }
}
