//! Validation rule types and definitions.
//!
//! This module defines the validation rules that can be applied to input fields
//! in a GraphQL schema. Rules are serializable and can be embedded in the compiled schema.

use serde::{Deserialize, Serialize};

/// A validation rule that can be applied to a field.
///
/// Rules define constraints on field values and are evaluated during input validation.
/// Multiple rules can be combined on a single field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ValidationRule {
    /// Field is required (non-null) and must have a value.
    #[serde(rename = "required")]
    Required,

    /// Field value must match a regular expression pattern.
    #[serde(rename = "pattern")]
    Pattern {
        /// The regex pattern to match.
        pattern: String,
        /// Optional error message for when pattern doesn't match.
        message: Option<String>,
    },

    /// String field length constraints.
    #[serde(rename = "length")]
    Length {
        /// Minimum length (inclusive).
        min: Option<usize>,
        /// Maximum length (inclusive).
        max: Option<usize>,
    },

    /// Numeric field range constraints.
    #[serde(rename = "range")]
    Range {
        /// Minimum value (inclusive).
        min: Option<i64>,
        /// Maximum value (inclusive).
        max: Option<i64>,
    },

    /// Field value must be one of allowed enum values.
    #[serde(rename = "enum")]
    Enum {
        /// List of allowed values.
        values: Vec<String>,
    },

    /// Checksum validation for structured data.
    #[serde(rename = "checksum")]
    Checksum {
        /// Algorithm to use (e.g., "luhn", "mod97").
        algorithm: String,
    },

    /// Cross-field validation rule.
    #[serde(rename = "cross_field")]
    CrossField {
        /// Reference to another field to compare against.
        field: String,
        /// Comparison operator ("lt", "lte", "eq", "gte", "gt").
        operator: String,
    },

    /// Conditional validation - only validate if condition is met.
    #[serde(rename = "conditional")]
    Conditional {
        /// The condition expression.
        condition: String,
        /// Rules to apply if condition is true.
        then_rules: Vec<Box<ValidationRule>>,
    },

    /// Composite rule - all rules must pass.
    #[serde(rename = "all")]
    All(Vec<ValidationRule>),

    /// Composite rule - at least one rule must pass.
    #[serde(rename = "any")]
    Any(Vec<ValidationRule>),
}

impl ValidationRule {
    /// Check if this is a required field validation.
    pub const fn is_required(&self) -> bool {
        matches!(self, Self::Required)
    }

    /// Get a human-readable description of this rule.
    pub fn description(&self) -> String {
        match self {
            Self::Required => "Field is required".to_string(),
            Self::Pattern { message, .. } => {
                message.clone().unwrap_or_else(|| "Must match pattern".to_string())
            }
            Self::Length { min, max } => match (min, max) {
                (Some(m), Some(max_val)) => format!("Length between {} and {}", m, max_val),
                (Some(m), None) => format!("Length at least {}", m),
                (None, Some(max_val)) => format!("Length at most {}", max_val),
                (None, None) => "Length constraint".to_string(),
            },
            Self::Range { min, max } => match (min, max) {
                (Some(m), Some(max_val)) => format!("Value between {} and {}", m, max_val),
                (Some(m), None) => format!("Value at least {}", m),
                (None, Some(max_val)) => format!("Value at most {}", max_val),
                (None, None) => "Range constraint".to_string(),
            },
            Self::Enum { values } => format!("Must be one of: {}", values.join(", ")),
            Self::Checksum { algorithm } => format!("Invalid {}", algorithm),
            Self::CrossField { field, operator } => format!("Must be {} {}", operator, field),
            Self::Conditional { .. } => "Conditional validation".to_string(),
            Self::All(_) => "All rules must pass".to_string(),
            Self::Any(_) => "At least one rule must pass".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_rule() {
        let rule = ValidationRule::Required;
        assert!(rule.is_required());
    }

    #[test]
    fn test_pattern_rule() {
        let rule = ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: Some("Only lowercase letters allowed".to_string()),
        };
        assert!(!rule.is_required());
        let desc = rule.description();
        assert_eq!(desc, "Only lowercase letters allowed");
    }

    #[test]
    fn test_length_rule() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: Some(10),
        };
        let desc = rule.description();
        assert!(desc.contains("5"));
        assert!(desc.contains("10"));
    }

    #[test]
    fn test_rule_serialization() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule = serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::Enum { .. }));
    }

    #[test]
    fn test_composite_all_rule() {
        let rule = ValidationRule::All(vec![
            ValidationRule::Required,
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ]);
        let desc = rule.description();
        assert!(desc.contains("All rules"));
    }
}
