//! SQL utility functions for federation query building.
//!
//! Shared utilities for SQL generation across federation modules.

use crate::error::{FraiseQLError, Result};
use serde_json::Value;

/// Convert a JSON value to a SQL literal representation.
///
/// Handles all JSON types and applies proper SQL escaping:
/// - Strings: wrapped in quotes with single quotes doubled (PostgreSQL style)
/// - Numbers: converted to string without quotes
/// - Booleans: converted to "true" or "false"
/// - Null: converted to "NULL"
/// - Arrays/Objects: returns error
///
/// # Examples
///
/// ```ignore
/// value_to_sql_literal(&json!("test")) → "'test'"
/// value_to_sql_literal(&json!("O'Brien")) → "'O''Brien'"
/// value_to_sql_literal(&json!(123)) → "123"
/// value_to_sql_literal(&json!(null)) → "NULL"
/// ```
pub fn value_to_sql_literal(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => {
            let escaped = escape_sql_string(s);
            Ok(format!("'{}'", escaped))
        }
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
        Value::Null => Ok("NULL".to_string()),
        _ => Err(FraiseQLError::Validation {
            message: format!("Cannot convert {} to SQL literal", value.type_str()),
            path: None,
        }),
    }
}

/// Convert a JSON value to its string representation for use in SQL.
///
/// This is used for extracting key values before they are escaped and quoted.
///
/// # Examples
///
/// ```ignore
/// value_to_string(&json!("test")) → "test"
/// value_to_string(&json!(123)) → "123"
/// ```
pub fn value_to_string(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok("null".to_string()),
        _ => Err(FraiseQLError::Validation {
            message: format!("Cannot convert {} to string for WHERE clause", value.type_str()),
            path: None,
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
/// # use fraiseql_core::federation::sql_utils::escape_sql_string;
/// assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
/// assert_eq!(escape_sql_string("test"), "test");
/// ```
pub fn escape_sql_string(value: &str) -> String {
    value.replace("'", "''")
}

/// Helper trait to get string representation of JSON value type for error messages.
pub trait JsonTypeStr {
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_value_to_sql_literal_string() {
        let result = value_to_sql_literal(&Value::String("John".to_string())).unwrap();
        assert_eq!(result, "'John'");
    }

    #[test]
    fn test_value_to_sql_literal_string_with_quotes() {
        let result = value_to_sql_literal(&Value::String("O'Brien".to_string())).unwrap();
        assert_eq!(result, "'O''Brien'");
    }

    #[test]
    fn test_value_to_sql_literal_number() {
        let result = value_to_sql_literal(&json!(123)).unwrap();
        assert_eq!(result, "123");

        let result = value_to_sql_literal(&json!(99.99)).unwrap();
        assert_eq!(result, "99.99");
    }

    #[test]
    fn test_value_to_sql_literal_bool() {
        let result = value_to_sql_literal(&Value::Bool(true)).unwrap();
        assert_eq!(result, "true");

        let result = value_to_sql_literal(&Value::Bool(false)).unwrap();
        assert_eq!(result, "false");
    }

    #[test]
    fn test_value_to_sql_literal_null() {
        let result = value_to_sql_literal(&Value::Null).unwrap();
        assert_eq!(result, "NULL");
    }

    #[test]
    fn test_value_to_sql_literal_array_error() {
        let result = value_to_sql_literal(&Value::Array(vec![]));
        assert!(result.is_err());
    }

    #[test]
    fn test_value_to_string() {
        assert_eq!(value_to_string(&Value::String("test".to_string())).unwrap(), "test");
        assert_eq!(value_to_string(&Value::Number(789.into())).unwrap(), "789");
        assert_eq!(value_to_string(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(value_to_string(&Value::Null).unwrap(), "null");
    }

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
        assert_eq!(escape_sql_string("test"), "test");
        assert_eq!(escape_sql_string("test''; DROP--"), "test''''; DROP--");
    }
}
