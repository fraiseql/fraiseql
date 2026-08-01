//! SQL utility functions for federation query building.
//!
//! Shared utilities for SQL generation across federation modules.

use fraiseql_db::DatabaseType;
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

/// Convert a JSON value to a SQL literal representation — **dialect-aware**
/// (#728).
///
/// Handles all JSON types and applies proper SQL escaping:
/// - Strings: wrapped in quotes with single quotes doubled
/// - Numbers: converted to string without quotes
/// - Booleans: converted to "true" or "false"
/// - Null: converted to "NULL"
/// - Arrays/Objects: returns error
///
/// Quote doubling is only sound on dialects that never treat backslash as a
/// string escape: PostgreSQL (`standard_conforming_strings=on`, the default
/// since 9.1), SQLite, and SQL Server. MySQL's **default** mode does interpret
/// backslash escapes, and whether it does is a per-connection server setting
/// this helper cannot observe — so any escaping it picked could be wrong
/// (injectably wrong) on the other mode. MySQL and unrecognised dialects are
/// therefore refused loud; the federation mutation builders should move to
/// bind parameters to support them (the `_entities` read path already did).
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `value` is a JSON array or object,
/// which cannot be represented as a scalar SQL literal, or if `db_type`'s
/// string-literal escaping rules are not soundly supported (MySQL).
///
/// # Examples
///
/// ```text
/// // Illustrative — see unit tests below for runnable examples.
/// value_to_sql_literal(DatabaseType::PostgreSQL, &json!("test")) // produces "'test'"
/// value_to_sql_literal(DatabaseType::PostgreSQL, &json!("O'Brien")) // produces "'O''Brien'"
/// ```
pub fn value_to_sql_literal(db_type: DatabaseType, value: &Value) -> Result<String> {
    // Allow-list, not deny-list: an unknown future dialect must fail loud
    // instead of inheriting escaping rules that were never checked against it
    // (#728 — MySQL's connection-dependent backslash escaping is why).
    if !matches!(db_type, DatabaseType::PostgreSQL) {
        return Err(FraiseQLError::Validation {
            message: format!(
                "SQL literal building is not supported for {db_type:?}; use bind parameters \
                 instead"
            ),
            path:    None,
        });
    }
    match value {
        Value::String(s) => {
            // Single-quote-doubling for the literal-building used by the federation
            // mutation builders (`mutation_query_builder`). The `_entities` read path
            // no longer builds literals — it binds parameters (see `query_builder`).
            let escaped = s.replace('\'', "''");
            Ok(format!("'{escaped}'"))
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

/// Bind-parameter placeholder for position `index` (0-based) in `db_type`'s
/// dialect.
///
/// Matches the convention `DatabaseAdapter::execute_parameterized_aggregate`
/// expects: 1-based `$N` for PostgreSQL, `@PN` for SQL Server, and `?` otherwise
/// (MySQL/SQLite). Federation entity resolution binds key-field values as
/// parameters rather than interpolating them, so the generated SQL must use the
/// dialect-native placeholder.
#[must_use]
pub fn placeholder(db_type: DatabaseType, index: usize) -> String {
    match db_type {
        DatabaseType::PostgreSQL => format!("${}", index + 1),
    }
}

/// Validate that `identifier` is a safe SQL identifier for unquoted
/// interpolation into a federation query (column / key-field names cannot be
/// bound as parameters).
///
/// The `_entities` read path interpolates identifiers **unquoted** to preserve
/// PostgreSQL case-folding (the entity views rely on it), so the charset guard
/// — restricting to `[A-Za-z0-9_]` via [`is_safe_sql_identifier`] — is what
/// keeps interpolation injection-safe.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `identifier` is not a safe SQL
/// identifier.
pub fn validate_sql_identifier(identifier: &str) -> Result<()> {
    if is_safe_sql_identifier(identifier) {
        Ok(())
    } else {
        Err(FraiseQLError::Validation {
            message: format!("Unsafe SQL identifier for federation query: '{identifier}'"),
            path:    None,
        })
    }
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
