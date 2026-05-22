//! SQL utility functions for federation query building.
//!
//! Shared utilities for SQL generation across federation modules.

use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

/// Validates that a string is a safe SQL identifier.
///
/// Accepts only ASCII alphanumerics and underscores. Used to guard against
/// SQL injection when schema-derived names (view names, entity type names)
/// are interpolated into raw SQL strings.
#[must_use]
pub fn is_safe_sql_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Convert a JSON value to a SQL literal representation.
///
/// Handles all JSON types and applies proper SQL escaping:
/// - Strings: wrapped in quotes with single quotes doubled (PostgreSQL style)
/// - Numbers: converted to string without quotes
/// - Booleans: converted to "true" or "false"
/// - Null: converted to "NULL"
/// - Arrays/Objects: returns error
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `value` is a JSON array or object,
/// which cannot be represented as a scalar SQL literal.
///
/// # Examples
///
/// ```text
/// // Illustrative — see unit tests below for runnable examples.
/// value_to_sql_literal(&json!("test")) // produces "'test'"
/// value_to_sql_literal(&json!("O'Brien")) // produces "'O''Brien'"
/// value_to_sql_literal(&json!(123)) // produces "123"
/// value_to_sql_literal(&json!(null)) // produces "NULL"
/// ```
pub fn value_to_sql_literal(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => {
            let escaped = escape_sql_string(s);
            Ok(format!("'{}'", escaped))
        },
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
        Value::Null => Ok("NULL".to_string()),
        _ => Err(FraiseQLError::Validation {
            message: format!("Cannot convert {} to SQL literal", value.type_str()),
            path:    None,
        }),
    }
}

/// Convert a JSON value to its string representation for use in SQL.
///
/// This is used for extracting key values before they are escaped and quoted.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `value` is a JSON array or object,
/// which cannot be represented as a plain string for WHERE clause use.
///
/// # Examples
///
/// ```text
/// // Illustrative — see unit tests below for runnable examples.
/// value_to_string(&json!("test")) // produces "test"
/// value_to_string(&json!(123)) // produces "123"
/// ```
pub fn value_to_string(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok("null".to_string()),
        _ => Err(FraiseQLError::Validation {
            message: format!("Cannot convert {} to string for WHERE clause", value.type_str()),
            path:    None,
        }),
    }
}

/// Escape single quotes in SQL string values to prevent SQL injection.
///
/// Uses PostgreSQL/SQL Server style escaping where single quotes are doubled.
///
/// # Examples
///
/// ```
/// # use fraiseql_federation::sql_utils::escape_sql_string;
/// assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
/// assert_eq!(escape_sql_string("test"), "test");
/// ```
#[must_use]
pub fn escape_sql_string(value: &str) -> String {
    value.replace('\'', "''")
}

/// Helper trait to get string representation of JSON value type for error messages.
pub trait JsonTypeStr {
    /// Return a lowercase label for the JSON value's type (e.g. `"string"`, `"number"`).
    fn type_str(&self) -> &'static str;
}

impl JsonTypeStr for Value {
    fn type_str(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
}

#[cfg(test)]
mod tests;
