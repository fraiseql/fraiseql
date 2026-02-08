//! Validation framework for extended operator parameters.
//!
//! This module provides reusable validators that can be configured via TOML
//! at compile time. Validators are applied before SQL generation to ensure
//! parameters are valid before executing queries.
//!
//! # Design
//!
//! Validators are expressed as rules in fraiseql.toml:
//!
//! ```toml
//! [fraiseql.validation]
//! email_domain_eq = { pattern = "^[a-z0-9]..." }
//! vin_wmi_eq = { length = 3, pattern = "^[A-Z0-9]{3}$" }
//! iban_country_eq = { checksum = "mod97" }
//! ```
//!
//! Rules are compiled into schema.compiled.json and applied at runtime.

use regex::Regex;
use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Validation rule for an operator parameter.
#[derive(Debug, Clone)]
pub enum ValidationRule {
    /// Pattern matching (regex)
    Pattern(String),
    /// Exact length
    Length(usize),
    /// Min and max length
    LengthRange { min: usize, max: usize },
    /// Checksum algorithm
    Checksum(ChecksumType),
    /// Range of numeric values
    NumericRange { min: f64, max: f64 },
    /// Value must be one of these options
    Enum(Vec<String>),
    /// Composite rule (all must pass)
    All(Vec<ValidationRule>),
}

/// Supported checksum algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    /// IBAN MOD-97 checksum
    Mod97,
    /// Luhn algorithm (credit cards, VINs)
    Luhn,
}

impl ValidationRule {
    /// Validate a string value against this rule.
    pub fn validate(&self, value: &str) -> Result<()> {
        match self {
            ValidationRule::Pattern(pattern) => {
                let re = Regex::new(pattern).map_err(|e| {
                    FraiseQLError::validation(format!("Invalid validation pattern: {}", e))
                })?;

                if !re.is_match(value) {
                    return Err(FraiseQLError::validation(format!(
                        "Value '{}' does not match pattern '{}'",
                        value, pattern
                    )));
                }
                Ok(())
            },

            ValidationRule::Length(expected) => {
                if value.len() != *expected {
                    return Err(FraiseQLError::validation(format!(
                        "Value '{}' has length {}, expected {}",
                        value,
                        value.len(),
                        expected
                    )));
                }
                Ok(())
            },

            ValidationRule::LengthRange { min, max } => {
                let len = value.len();
                if len < *min || len > *max {
                    return Err(FraiseQLError::validation(format!(
                        "Value '{}' has length {}, expected between {} and {}",
                        value, len, min, max
                    )));
                }
                Ok(())
            },

            ValidationRule::Checksum(checksum_type) => {
                match checksum_type {
                    ChecksumType::Mod97 => validate_mod97(value)?,
                    ChecksumType::Luhn => validate_luhn(value)?,
                }
                Ok(())
            },

            ValidationRule::NumericRange { min, max } => {
                let num: f64 = value.parse().map_err(|_| {
                    FraiseQLError::validation(format!("Value '{}' is not a valid number", value))
                })?;

                if num < *min || num > *max {
                    return Err(FraiseQLError::validation(format!(
                        "Value {} is outside range [{}, {}]",
                        num, min, max
                    )));
                }
                Ok(())
            },

            ValidationRule::Enum(options) => {
                if !options.contains(&value.to_string()) {
                    return Err(FraiseQLError::validation(format!(
                        "Value '{}' must be one of: {}",
                        value,
                        options.join(", ")
                    )));
                }
                Ok(())
            },

            ValidationRule::All(rules) => {
                for rule in rules {
                    rule.validate(value)?;
                }
                Ok(())
            },
        }
    }

    /// Parse validation rules from JSON (compiled from TOML).
    pub fn from_json(value: &Value) -> Result<Self> {
        match value {
            Value::String(s) => {
                // Simple case: just a pattern
                Ok(ValidationRule::Pattern(s.clone()))
            },

            Value::Object(map) => {
                let mut rules = Vec::new();

                // Pattern rule
                if let Some(Value::String(pattern)) = map.get("pattern") {
                    rules.push(ValidationRule::Pattern(pattern.clone()));
                }

                // Length rule
                if let Some(Value::Number(n)) = map.get("length") {
                    if let Some(length) = n.as_u64() {
                        rules.push(ValidationRule::Length(length as usize));
                    }
                }

                // Length range rule
                if let (Some(Value::Number(min)), Some(Value::Number(max))) =
                    (map.get("min_length"), map.get("max_length"))
                {
                    if let (Some(min_val), Some(max_val)) = (min.as_u64(), max.as_u64()) {
                        rules.push(ValidationRule::LengthRange {
                            min: min_val as usize,
                            max: max_val as usize,
                        });
                    }
                }

                // Checksum rule
                if let Some(Value::String(checksum)) = map.get("checksum") {
                    let checksum_type = match checksum.as_str() {
                        "mod97" => ChecksumType::Mod97,
                        "luhn" => ChecksumType::Luhn,
                        _ => {
                            return Err(FraiseQLError::validation(format!(
                                "Unknown checksum type: {}",
                                checksum
                            )));
                        },
                    };
                    rules.push(ValidationRule::Checksum(checksum_type));
                }

                // Enum rule
                if let Some(Value::Array(options)) = map.get("enum") {
                    let enum_values: Vec<String> =
                        options.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();

                    if !enum_values.is_empty() {
                        rules.push(ValidationRule::Enum(enum_values));
                    }
                }

                // Numeric range rule
                if let (Some(Value::Number(min)), Some(Value::Number(max))) =
                    (map.get("min"), map.get("max"))
                {
                    if let (Some(min_val), Some(max_val)) = (min.as_f64(), max.as_f64()) {
                        rules.push(ValidationRule::NumericRange {
                            min: min_val,
                            max: max_val,
                        });
                    }
                }

                if rules.is_empty() {
                    return Err(FraiseQLError::validation(
                        "No valid validation rules found".to_string(),
                    ));
                }

                if rules.len() == 1 {
                    Ok(rules.into_iter().next().unwrap())
                } else {
                    Ok(ValidationRule::All(rules))
                }
            },

            _ => Err(FraiseQLError::validation(
                "Validation rule must be string or object".to_string(),
            )),
        }
    }
}

/// MOD-97 checksum validation for IBAN and similar formats.
fn validate_mod97(value: &str) -> Result<()> {
    // Move country code (first 4 chars) to end
    if value.len() < 4 {
        return Err(FraiseQLError::validation("IBAN must be at least 4 characters".to_string()));
    }

    let rearranged = format!("{}{}", &value[4..], &value[..4]);

    // Convert letters to numbers (A=10, B=11, ..., Z=35)
    let numeric_string: String = rearranged
        .chars()
        .map(|c| {
            if c.is_ascii_digit() {
                c.to_string()
            } else {
                ((c.to_ascii_uppercase() as u32 - 'A' as u32) + 10).to_string()
            }
        })
        .collect();

    // Compute MOD 97
    let mut remainder: u64 = 0;
    for digit_char in numeric_string.chars() {
        if let Some(digit) = digit_char.to_digit(10) {
            remainder = (remainder * 10 + u64::from(digit)) % 97;
        }
    }

    if remainder == 1 {
        Ok(())
    } else {
        Err(FraiseQLError::validation("Invalid IBAN checksum".to_string()))
    }
}

/// Luhn algorithm checksum validation (used for VINs, credit cards, etc.).
fn validate_luhn(value: &str) -> Result<()> {
    let digits: Vec<u32> = value.chars().filter_map(|c| c.to_digit(10)).collect();

    if digits.is_empty() {
        return Err(FraiseQLError::validation("Value must contain at least one digit".to_string()));
    }

    let mut sum = 0u32;
    let mut is_even = false;

    for digit in digits.iter().rev() {
        let mut n = *digit;
        if is_even {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        is_even = !is_even;
    }

    if sum % 10 == 0 {
        Ok(())
    } else {
        Err(FraiseQLError::validation("Invalid Luhn checksum".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_validation() {
        let rule = ValidationRule::Pattern("^[a-z]+$".to_string());
        assert!(rule.validate("hello").is_ok());
        assert!(rule.validate("Hello").is_err());
    }

    #[test]
    fn test_length_validation() {
        let rule = ValidationRule::Length(3);
        assert!(rule.validate("abc").is_ok());
        assert!(rule.validate("ab").is_err());
        assert!(rule.validate("abcd").is_err());
    }

    #[test]
    fn test_mod97_valid() {
        // Valid IBAN: GB82 WEST 1234 5698 7654 32
        let result = validate_mod97("GB82WEST12345698765432");
        assert!(result.is_ok());
    }

    #[test]
    fn test_luhn_valid() {
        // Valid credit card number
        let result = validate_luhn("4532015112830366");
        assert!(result.is_ok());
    }

    #[test]
    fn test_enum_validation() {
        let rule = ValidationRule::Enum(vec!["US".to_string(), "CA".to_string()]);
        assert!(rule.validate("US").is_ok());
        assert!(rule.validate("UK").is_err());
    }

    #[test]
    fn test_numeric_range_validation() {
        let rule = ValidationRule::NumericRange {
            min: 0.0,
            max: 90.0,
        };
        assert!(rule.validate("45.5").is_ok());
        assert!(rule.validate("91").is_err());
    }
}
