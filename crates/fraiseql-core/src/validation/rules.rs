//! Validation rule types and definitions.
//!
//! This module defines the validation rules that can be applied to input fields
//! in a GraphQL schema. Rules are serializable and can be embedded in the compiled schema.

use serde::{Deserialize, Serialize};

/// A validation rule that can be applied to a field.
///
/// Rules define constraints on field values and are evaluated during input validation.
/// Multiple rules can be combined on a single field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        field:    String,
        /// Comparison operator ("lt", "lte", "eq", "gte", "gt").
        operator: String,
    },

    /// Conditional validation - only validate if condition is met.
    #[serde(rename = "conditional")]
    Conditional {
        /// The condition expression.
        condition:  String,
        /// Rules to apply if condition is true.
        then_rules: Vec<Box<ValidationRule>>,
    },

    /// Composite rule - all rules must pass.
    #[serde(rename = "all")]
    All(Vec<ValidationRule>),

    /// Composite rule - at least one rule must pass.
    #[serde(rename = "any")]
    Any(Vec<ValidationRule>),

    /// Exactly one field from the set must be provided (mutually exclusive).
    ///
    /// Useful for "create or reference" patterns where you must provide EITHER
    /// an ID to reference an existing entity OR the fields to create a new one,
    /// but not both.
    ///
    /// # Example
    /// ```ignore
    /// // Either provide entityId OR (name + description), but not both
    /// OneOf { fields: vec!["name".to_string(), "description".to_string()] }
    /// ```
    #[serde(rename = "one_of")]
    OneOf {
        /// List of field names - exactly one must be provided
        fields: Vec<String>,
    },

    /// At least one field from the set must be provided.
    ///
    /// Useful for optional but not-all-empty patterns.
    ///
    /// # Example
    /// ```ignore
    /// // Provide at least one of: email, phone, address
    /// AnyOf { fields: vec!["email".to_string(), "phone".to_string(), "address".to_string()] }
    /// ```
    #[serde(rename = "any_of")]
    AnyOf {
        /// List of field names - at least one must be provided
        fields: Vec<String>,
    },

    /// If a field is present, then other fields are required.
    ///
    /// Used for conditional requirements based on presence of another field.
    ///
    /// # Example
    /// ```ignore
    /// // If entityId is provided, then createdAt is required
    /// ConditionalRequired {
    ///     if_field_present: "entityId".to_string(),
    ///     then_required: vec!["createdAt".to_string()]
    /// }
    /// ```
    #[serde(rename = "conditional_required")]
    ConditionalRequired {
        /// If this field is present (not null/missing)
        if_field_present: String,
        /// Then these fields are required
        then_required:    Vec<String>,
    },

    /// If a field is absent/null, then other fields are required.
    ///
    /// Used for "provide this OR that" patterns at the object level.
    ///
    /// # Example
    /// ```ignore
    /// // If addressId is missing, then street+city+zip are required
    /// RequiredIfAbsent {
    ///     absent_field: "addressId".to_string(),
    ///     then_required: vec!["street".to_string(), "city".to_string(), "zip".to_string()]
    /// }
    /// ```
    #[serde(rename = "required_if_absent")]
    RequiredIfAbsent {
        /// If this field is absent/null
        absent_field:  String,
        /// Then these fields are required
        then_required: Vec<String>,
    },
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
            },
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
            Self::OneOf { fields } => {
                format!("Exactly one of these must be provided: {}", fields.join(", "))
            },
            Self::AnyOf { fields } => {
                format!("At least one of these must be provided: {}", fields.join(", "))
            },
            Self::ConditionalRequired {
                if_field_present,
                then_required,
            } => {
                format!(
                    "If '{}' is provided, then {} must be provided",
                    if_field_present,
                    then_required.join(", ")
                )
            },
            Self::RequiredIfAbsent {
                absent_field,
                then_required,
            } => {
                format!(
                    "If '{}' is absent, then {} must be provided",
                    absent_field,
                    then_required.join(", ")
                )
            },
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
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
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

    #[test]
    fn test_one_of_rule() {
        let rule = ValidationRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        };
        assert!(!rule.is_required());
        let desc = rule.description();
        assert!(desc.contains("Exactly one"));
        assert!(desc.contains("entityId"));
        assert!(desc.contains("entityPayload"));
    }

    #[test]
    fn test_any_of_rule() {
        let rule = ValidationRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        };
        let desc = rule.description();
        assert!(desc.contains("At least one"));
        assert!(desc.contains("email"));
        assert!(desc.contains("phone"));
        assert!(desc.contains("address"));
    }

    #[test]
    fn test_conditional_required_rule() {
        let rule = ValidationRule::ConditionalRequired {
            if_field_present: "entityId".to_string(),
            then_required:    vec!["createdAt".to_string(), "updatedAt".to_string()],
        };
        let desc = rule.description();
        assert!(desc.contains("If"));
        assert!(desc.contains("entityId"));
        assert!(desc.contains("createdAt"));
        assert!(desc.contains("updatedAt"));
    }

    #[test]
    fn test_required_if_absent_rule() {
        let rule = ValidationRule::RequiredIfAbsent {
            absent_field:  "addressId".to_string(),
            then_required: vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        };
        let desc = rule.description();
        assert!(desc.contains("If"));
        assert!(desc.contains("addressId"));
        assert!(desc.contains("absent"));
        assert!(desc.contains("street"));
    }

    #[test]
    fn test_one_of_serialization() {
        let rule = ValidationRule::OneOf {
            fields: vec!["id".to_string(), "payload".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::OneOf { .. }));
    }

    #[test]
    fn test_conditional_required_serialization() {
        let rule = ValidationRule::ConditionalRequired {
            if_field_present: "isPremium".to_string(),
            then_required:    vec!["paymentMethod".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::ConditionalRequired { .. }));
    }

    #[test]
    fn test_required_if_absent_serialization() {
        let rule = ValidationRule::RequiredIfAbsent {
            absent_field:  "presetId".to_string(),
            then_required: vec!["settings".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::RequiredIfAbsent { .. }));
    }
}
