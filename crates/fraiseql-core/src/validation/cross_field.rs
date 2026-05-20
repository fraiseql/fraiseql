//! Cross-field comparison validators.
//!
//! This module provides validators for comparing values between two fields in an input object.
//! Supports operators: <, <=, >, >=, ==, !=
//!
//! # Examples
//!
//! ```
//! use fraiseql_core::validation::ValidationRule;
//!
//! // Date range validation: start_date < end_date
//! let _rule = ValidationRule::CrossField {
//!     field: "end_date".to_string(),
//!     operator: "gt".to_string(),
//! };
//!
//! // Numeric range: min < max
//! let _rule = ValidationRule::CrossField {
//!     field: "max_value".to_string(),
//!     operator: "lt".to_string(),
//! };
//! ```

use std::cmp::Ordering;

use serde_json::Value;

use crate::error::{FraiseQLError, Result};

/// Operators supported for cross-field comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    #[allow(clippy::should_implement_trait)]
    // Reason: returns Option<Self> (unrecognized operators yield None), not a FromStr-compatible Result
    #[must_use]
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
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
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
    #[must_use]
    pub const fn name(&self) -> &'static str {
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
            path: Some(field_path.to_string()),
        })?;

        let right_val = obj.get(right_field).ok_or_else(|| FraiseQLError::Validation {
            message: format!("Field '{}' not found in input", right_field),
            path: Some(field_path.to_string()),
        })?;

        // Skip validation if either field is null
        if matches!(left_val, Value::Null) || matches!(right_val, Value::Null) {
            return Ok(());
        }

        compare_values(left_val, right_val, left_field, operator, right_field, field_path)
    } else {
        Err(FraiseQLError::Validation {
            message: "Input is not an object".to_string(),
            path: Some(field_path.to_string()),
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
                path: Some(context_path.to_string()),
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
            path: Some(context_path.to_string()),
        });
    }

    Ok(())
}

/// Get the type name of a JSON value.
const fn value_type_name(val: &Value) -> &'static str {
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
