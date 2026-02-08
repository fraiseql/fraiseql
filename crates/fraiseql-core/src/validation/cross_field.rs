//! Cross-field comparison validators.
//!
//! This module provides validators for comparing values between two fields in an input object.
//! Supports operators: <, <=, >, >=, ==, !=
//!
//! # Examples
//!
//! ```ignore
//! // Date range validation: start_date < end_date
//! let rule = ValidationRule::CrossField {
//!     field: "end_date".to_string(),
//!     operator: "gt".to_string(),
//! };
//!
//! // Numeric range: min < max
//! let rule = ValidationRule::CrossField {
//!     field: "max_value".to_string(),
//!     operator: "lt".to_string(),
//! };
//! ```

use std::cmp::Ordering;

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Operators supported for cross-field comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    /// Less than (<)
    LessThan,
    /// Less than or equal (<=)
    LessEqual,
    /// Greater than (>)
    GreaterThan,
    /// Greater than or equal (>=)
    GreaterEqual,
    /// Equal (==)
    Equal,
    /// Not equal (!=)
    NotEqual,
}

impl ComparisonOperator {
    /// Parse operator from string representation.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "<" | "lt" => Some(Self::LessThan),
            "<=" | "lte" => Some(Self::LessEqual),
            ">" | "gt" => Some(Self::GreaterThan),
            ">=" | "gte" => Some(Self::GreaterEqual),
            "==" | "eq" => Some(Self::Equal),
            "!=" | "neq" => Some(Self::NotEqual),
            _ => None,
        }
    }

    /// Get the symbol for display.
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::LessThan => "<",
            Self::LessEqual => "<=",
            Self::GreaterThan => ">",
            Self::GreaterEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
        }
    }

    /// Get the long name for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::LessThan => "less than",
            Self::LessEqual => "less than or equal to",
            Self::GreaterThan => "greater than",
            Self::GreaterEqual => "greater than or equal to",
            Self::Equal => "equal to",
            Self::NotEqual => "not equal to",
        }
    }
}

/// Validates a cross-field comparison between two fields.
///
/// Compares `left_field` with `right_field` using the given operator.
///
/// # Arguments
///
/// * `input` - The input object containing both fields
/// * `left_field` - The name of the left field to compare
/// * `operator` - The comparison operator
/// * `right_field` - The name of the right field to compare against
/// * `context_path` - Optional field path for error reporting
///
/// # Errors
///
/// Returns an error if:
/// - Either field is missing from the input
/// - The fields have incompatible types
/// - The comparison fails
pub fn validate_cross_field_comparison(
    input: &Value,
    left_field: &str,
    operator: ComparisonOperator,
    right_field: &str,
    context_path: Option<&str>,
) -> Result<()> {
    let field_path = context_path.unwrap_or("input");

    if let Value::Object(obj) = input {
        let left_val = obj.get(left_field).ok_or_else(|| FraiseQLError::Validation {
            message: format!("Field '{}' not found in input", left_field),
            path:    Some(field_path.to_string()),
        })?;

        let right_val = obj.get(right_field).ok_or_else(|| FraiseQLError::Validation {
            message: format!("Field '{}' not found in input", right_field),
            path:    Some(field_path.to_string()),
        })?;

        // Skip validation if either field is null
        if matches!(left_val, Value::Null) || matches!(right_val, Value::Null) {
            return Ok(());
        }

        compare_values(left_val, right_val, left_field, operator, right_field, field_path)
    } else {
        Err(FraiseQLError::Validation {
            message: "Input is not an object".to_string(),
            path:    Some(field_path.to_string()),
        })
    }
}

/// Compare two JSON values and return result based on operator.
fn compare_values(
    left: &Value,
    right: &Value,
    left_field: &str,
    operator: ComparisonOperator,
    right_field: &str,
    context_path: &str,
) -> Result<()> {
    let ordering = match (left, right) {
        // Both are numbers
        (Value::Number(l), Value::Number(r)) => {
            let l_val = l.as_f64().unwrap_or(0.0);
            let r_val = r.as_f64().unwrap_or(0.0);
            if l_val < r_val {
                Ordering::Less
            } else if l_val > r_val {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        },
        // Both are strings (lexicographic comparison)
        (Value::String(l), Value::String(r)) => l.cmp(r),
        // Type mismatch
        _ => {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Cannot compare '{}' ({}) with '{}' ({})",
                    left_field,
                    value_type_name(left),
                    right_field,
                    value_type_name(right)
                ),
                path:    Some(context_path.to_string()),
            });
        },
    };

    let result = match operator {
        ComparisonOperator::LessThan => matches!(ordering, Ordering::Less),
        ComparisonOperator::LessEqual => !matches!(ordering, Ordering::Greater),
        ComparisonOperator::GreaterThan => matches!(ordering, Ordering::Greater),
        ComparisonOperator::GreaterEqual => !matches!(ordering, Ordering::Less),
        ComparisonOperator::Equal => matches!(ordering, Ordering::Equal),
        ComparisonOperator::NotEqual => !matches!(ordering, Ordering::Equal),
    };

    if !result {
        return Err(FraiseQLError::Validation {
            message: format!(
                "'{}' ({}) must be {} '{}' ({})",
                left_field,
                value_to_string(left),
                operator.name(),
                right_field,
                value_to_string(right)
            ),
            path:    Some(context_path.to_string()),
        });
    }

    Ok(())
}

/// Get the type name of a JSON value.
fn value_type_name(val: &Value) -> &'static str {
    match val {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Convert a JSON value to a string for display in error messages.
fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => format!("\"{}\"", s),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => val.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_operator_parsing() {
        assert_eq!(ComparisonOperator::from_str("<"), Some(ComparisonOperator::LessThan));
        assert_eq!(ComparisonOperator::from_str("lt"), Some(ComparisonOperator::LessThan));
        assert_eq!(ComparisonOperator::from_str("<="), Some(ComparisonOperator::LessEqual));
        assert_eq!(ComparisonOperator::from_str("lte"), Some(ComparisonOperator::LessEqual));
        assert_eq!(ComparisonOperator::from_str(">"), Some(ComparisonOperator::GreaterThan));
        assert_eq!(ComparisonOperator::from_str("gt"), Some(ComparisonOperator::GreaterThan));
        assert_eq!(ComparisonOperator::from_str(">="), Some(ComparisonOperator::GreaterEqual));
        assert_eq!(ComparisonOperator::from_str("gte"), Some(ComparisonOperator::GreaterEqual));
        assert_eq!(ComparisonOperator::from_str("=="), Some(ComparisonOperator::Equal));
        assert_eq!(ComparisonOperator::from_str("eq"), Some(ComparisonOperator::Equal));
        assert_eq!(ComparisonOperator::from_str("!="), Some(ComparisonOperator::NotEqual));
        assert_eq!(ComparisonOperator::from_str("neq"), Some(ComparisonOperator::NotEqual));
        assert_eq!(ComparisonOperator::from_str("invalid"), None);
    }

    #[test]
    fn test_numeric_less_than() {
        let input = json!({
            "start": 10,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_numeric_less_than_fails() {
        let input = json!({
            "start": 30,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_equal() {
        let input = json!({
            "a": 42,
            "b": 42
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::Equal, "b", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_numeric_not_equal() {
        let input = json!({
            "a": 10,
            "b": 20
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::NotEqual, "b", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_numeric_greater_than_or_equal() {
        let input = json!({
            "min": 10,
            "max": 10
        });
        let result = validate_cross_field_comparison(
            &input,
            "max",
            ComparisonOperator::GreaterEqual,
            "min",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_string_comparison() {
        let input = json!({
            "start_name": "alice",
            "end_name": "zoe"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_name",
            ComparisonOperator::LessThan,
            "end_name",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_string_comparison_fails() {
        let input = json!({
            "start_name": "zoe",
            "end_name": "alice"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_name",
            ComparisonOperator::LessThan,
            "end_name",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_date_string_comparison() {
        let input = json!({
            "start_date": "2024-01-01",
            "end_date": "2024-12-31"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_date",
            ComparisonOperator::LessThan,
            "end_date",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_float_comparison() {
        let input = json!({
            "price": 19.99,
            "budget": 25.50
        });
        let result = validate_cross_field_comparison(
            &input,
            "price",
            ComparisonOperator::LessThan,
            "budget",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_left_field() {
        let input = json!({
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_right_field() {
        let input = json!({
            "start": 10
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_null_fields_skipped() {
        let input = json!({
            "start": null,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_both_null_fields_skipped() {
        let input = json!({
            "start": null,
            "end": null
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_mismatch_error() {
        let input = json!({
            "start": 10,
            "end": "twenty"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("Cannot compare"));
        }
    }

    #[test]
    fn test_error_includes_context_path() {
        let input = json!({
            "start": 30,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            Some("dateRange"),
        );
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { path, .. }) = result {
            assert_eq!(path, Some("dateRange".to_string()));
        }
    }

    #[test]
    fn test_error_message_includes_values() {
        let input = json!({
            "price": 100,
            "max_price": 50
        });
        let result = validate_cross_field_comparison(
            &input,
            "price",
            ComparisonOperator::LessThan,
            "max_price",
            None,
        );
        assert!(result.is_err());
        if let Err(FraiseQLError::Validation { message, .. }) = result {
            assert!(message.contains("price"));
            assert!(message.contains("max_price"));
            assert!(message.contains("100"));
            assert!(message.contains("50"));
        }
    }

    #[test]
    fn test_all_operators() {
        let test_cases = vec![
            (10, 20, ComparisonOperator::LessThan, true),
            (10, 10, ComparisonOperator::LessEqual, true),
            (20, 10, ComparisonOperator::GreaterThan, true),
            (10, 10, ComparisonOperator::GreaterEqual, true),
            (42, 42, ComparisonOperator::Equal, true),
            (10, 20, ComparisonOperator::NotEqual, true),
            (20, 10, ComparisonOperator::LessThan, false),
            (10, 20, ComparisonOperator::GreaterThan, false),
        ];

        for (left, right, op, should_pass) in test_cases {
            let input = json!({ "a": left, "b": right });
            let result = validate_cross_field_comparison(&input, "a", op, "b", None);
            assert_eq!(
                result.is_ok(),
                should_pass,
                "Failed for {} {} {}",
                left,
                op.symbol(),
                right
            );
        }
    }

    #[test]
    fn test_non_object_input() {
        let input = json!([1, 2, 3]);
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_object() {
        let input = json!({});
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_comparison() {
        let input = json!({
            "a": 0,
            "b": 0
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::Equal, "b", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_negative_number_comparison() {
        let input = json!({
            "a": -10,
            "b": 5
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_string_comparison() {
        let input = json!({
            "a": "",
            "b": "text"
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        assert!(result.is_ok());
    }
}
