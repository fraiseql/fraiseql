//! Basic field validators for input validation.
//!
//! Provides simple validators for patterns, lengths, numeric ranges, and enums.
//! These validators are combined to create comprehensive input validation rules.

use regex::Regex;

use super::rules::ValidationRule;
use crate::error::{FraiseQLError, Result, ValidationFieldError};

/// Basic validator trait for field validation.
pub trait Validator {
    /// Validate a value and return an error if validation fails.
    fn validate(&self, value: &str, field: &str) -> Result<()>;
}

/// Pattern validator using regular expressions.
pub struct PatternValidator {
    regex:   Regex,
    message: String,
}

impl PatternValidator {
    /// Create a new pattern validator.
    ///
    /// # Errors
    /// Returns error if the regex pattern is invalid.
    pub fn new(pattern: impl Into<String>, message: impl Into<String>) -> Result<Self> {
        let pattern_str = pattern.into();
        let regex = Regex::new(&pattern_str)
            .map_err(|e| FraiseQLError::validation(format!("Invalid regex pattern: {}", e)))?;
        Ok(Self {
            regex,
            message: message.into(),
        })
    }

    /// Create a new pattern validator with default message.
    pub fn new_default_message(pattern: impl Into<String>) -> Result<Self> {
        let pattern_str = pattern.into();
        Self::new(pattern_str.clone(), format!("Value must match pattern: {}", pattern_str))
    }

    /// Validate that a value matches the pattern.
    pub fn validate_pattern(&self, value: &str) -> bool {
        self.regex.is_match(value)
    }
}

impl Validator for PatternValidator {
    fn validate(&self, value: &str, field: &str) -> Result<()> {
        if self.validate_pattern(value) {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Field validation failed: {}",
                    ValidationFieldError::new(field, "pattern", &self.message)
                ),
                path:    Some(field.to_string()),
            })
        }
    }
}

/// String length validator.
pub struct LengthValidator {
    min: Option<usize>,
    max: Option<usize>,
}

impl LengthValidator {
    /// Create a new length validator.
    pub fn new(min: Option<usize>, max: Option<usize>) -> Self {
        Self { min, max }
    }

    /// Validate that a string is within the specified length bounds.
    pub fn validate_length(&self, value: &str) -> bool {
        let len = value.len();
        if let Some(min) = self.min {
            if len < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if len > max {
                return false;
            }
        }
        true
    }

    /// Get a descriptive error message for length validation failure.
    pub fn error_message(&self) -> String {
        match (self.min, self.max) {
            (Some(m), Some(x)) => format!("Length must be between {} and {}", m, x),
            (Some(m), None) => format!("Length must be at least {}", m),
            (None, Some(x)) => format!("Length must be at most {}", x),
            (None, None) => "Length validation failed".to_string(),
        }
    }
}

impl Validator for LengthValidator {
    fn validate(&self, value: &str, field: &str) -> Result<()> {
        if self.validate_length(value) {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Field validation failed: {}",
                    ValidationFieldError::new(field, "length", self.error_message())
                ),
                path:    Some(field.to_string()),
            })
        }
    }
}

/// Numeric range validator.
pub struct RangeValidator {
    min: Option<i64>,
    max: Option<i64>,
}

impl RangeValidator {
    /// Create a new range validator.
    pub fn new(min: Option<i64>, max: Option<i64>) -> Self {
        Self { min, max }
    }

    /// Validate that a number is within the specified range.
    pub fn validate_range(&self, value: i64) -> bool {
        if let Some(min) = self.min {
            if value < min {
                return false;
            }
        }
        if let Some(max) = self.max {
            if value > max {
                return false;
            }
        }
        true
    }

    /// Get a descriptive error message for range validation failure.
    pub fn error_message(&self) -> String {
        match (self.min, self.max) {
            (Some(m), Some(x)) => format!("Value must be between {} and {}", m, x),
            (Some(m), None) => format!("Value must be at least {}", m),
            (None, Some(x)) => format!("Value must be at most {}", x),
            (None, None) => "Range validation failed".to_string(),
        }
    }
}

/// Enum validator - allows only specified values.
pub struct EnumValidator {
    allowed_values: std::collections::HashSet<String>,
}

impl EnumValidator {
    /// Create a new enum validator.
    pub fn new(values: Vec<String>) -> Self {
        Self {
            allowed_values: values.into_iter().collect(),
        }
    }

    /// Validate that a value is in the allowed set.
    pub fn validate_enum(&self, value: &str) -> bool {
        self.allowed_values.contains(value)
    }

    /// Get the list of allowed values.
    pub fn allowed_values(&self) -> Vec<&str> {
        self.allowed_values.iter().map(|s| s.as_str()).collect()
    }
}

impl Validator for EnumValidator {
    fn validate(&self, value: &str, field: &str) -> Result<()> {
        if self.validate_enum(value) {
            Ok(())
        } else {
            let mut allowed_vec: Vec<_> = self.allowed_values.iter().cloned().collect();
            allowed_vec.sort();
            let allowed = allowed_vec.join(", ");
            Err(FraiseQLError::Validation {
                message: format!(
                    "Field validation failed: {}",
                    ValidationFieldError::new(
                        field,
                        "enum",
                        format!("Must be one of: {}", allowed)
                    )
                ),
                path:    Some(field.to_string()),
            })
        }
    }
}

/// Required field validator.
pub struct RequiredValidator;

impl Validator for RequiredValidator {
    fn validate(&self, value: &str, field: &str) -> Result<()> {
        if value.is_empty() {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Field validation failed: {}",
                    ValidationFieldError::new(field, "required", "Field is required")
                ),
                path:    Some(field.to_string()),
            })
        } else {
            Ok(())
        }
    }
}

/// Create a validator from a ValidationRule.
pub fn create_validator_from_rule(rule: &ValidationRule) -> Option<Box<dyn Validator>> {
    match rule {
        ValidationRule::Pattern { pattern, message } => {
            let msg = message.clone().unwrap_or_else(|| "Pattern mismatch".to_string());
            PatternValidator::new(pattern.clone(), msg)
                .ok()
                .map(|v| Box::new(v) as Box<dyn Validator>)
        },
        ValidationRule::Length { min, max } => {
            Some(Box::new(LengthValidator::new(*min, *max)) as Box<dyn Validator>)
        },
        ValidationRule::Enum { values } => {
            Some(Box::new(EnumValidator::new(values.clone())) as Box<dyn Validator>)
        },
        ValidationRule::Required => Some(Box::new(RequiredValidator) as Box<dyn Validator>),
        _ => None, // Other validators handled separately
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_validator() {
        let validator = PatternValidator::new_default_message("^[a-z]+$").unwrap();
        assert!(validator.validate_pattern("hello"));
        assert!(!validator.validate_pattern("Hello"));
        assert!(!validator.validate_pattern("hello123"));
    }

    #[test]
    fn test_pattern_validator_validation() {
        let validator = PatternValidator::new_default_message("^[a-z]+$").unwrap();
        assert!(validator.validate("hello", "name").is_ok());
        assert!(validator.validate("Hello", "name").is_err());
    }

    #[test]
    fn test_length_validator() {
        let validator = LengthValidator::new(Some(3), Some(10));
        assert!(validator.validate_length("hello"));
        assert!(!validator.validate_length("ab"));
        assert!(!validator.validate_length("this is too long"));
    }

    #[test]
    fn test_length_validator_error_message() {
        let validator = LengthValidator::new(Some(5), Some(10));
        let msg = validator.error_message();
        assert!(msg.contains("5"));
        assert!(msg.contains("10"));
    }

    #[test]
    fn test_range_validator() {
        let validator = RangeValidator::new(Some(0), Some(100));
        assert!(validator.validate_range(50));
        assert!(!validator.validate_range(-1));
        assert!(!validator.validate_range(101));
    }

    #[test]
    fn test_enum_validator() {
        let validator = EnumValidator::new(vec![
            "active".to_string(),
            "inactive".to_string(),
            "pending".to_string(),
        ]);
        assert!(validator.validate_enum("active"));
        assert!(!validator.validate_enum("unknown"));
    }

    #[test]
    fn test_required_validator() {
        let validator = RequiredValidator;
        assert!(validator.validate("hello", "name").is_ok());
        assert!(validator.validate("", "name").is_err());
    }

    #[test]
    fn test_create_validator_from_rule() {
        let rule = ValidationRule::Pattern {
            pattern: "^test".to_string(),
            message: None,
        };
        let validator = create_validator_from_rule(&rule);
        assert!(validator.is_some());
    }
}
