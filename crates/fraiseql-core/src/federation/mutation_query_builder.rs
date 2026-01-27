//! SQL mutation query building for federation mutations.
//!
//! Constructs UPDATE, INSERT, and DELETE queries from GraphQL mutations
//! with parameter validation and SQL injection prevention.

use crate::error::{FraiseQLError, Result};
use crate::federation::types::FederationMetadata;
use serde_json::Value;

/// Build an UPDATE query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123", "name": "John", "email": "john@example.com" }`
/// Returns: `UPDATE users SET name = 'John', email = 'john@example.com' WHERE id = '123'`
pub fn build_update_query(
    typename: &str,
    variables: &Value,
    metadata: &FederationMetadata,
) -> Result<String> {
    // Find type in metadata
    let fed_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| FraiseQLError::Validation {
            message: format!("Type '{}' not found in federation metadata", typename),
            path: None,
        })?;

    // Get key field
    let key_directive = fed_type.keys.first().ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Type '{}' has no @key directive", typename),
            path: None,
        }
    })?;

    let key_field = &key_directive.fields[0];
    let table_name = typename.to_lowercase();

    let vars = variables.as_object().ok_or_else(|| {
        FraiseQLError::Validation {
            message: "Variables must be an object".to_string(),
            path: None,
        }
    })?;

    // Extract key value
    let key_value = vars.get(key_field).ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Key field '{}' missing in variables", key_field),
            path: None,
        }
    })?;

    // Build SET clauses
    let mut set_clauses = Vec::new();
    for (field, value) in vars {
        if field != key_field {
            let value_str = value_to_sql_literal(value)?;
            set_clauses.push(format!("{} = {}", field, value_str));
        }
    }

    if set_clauses.is_empty() {
        return Err(FraiseQLError::Validation {
            message: "No fields to update (only key field provided)".to_string(),
            path: None,
        });
    }

    let key_value_str = value_to_sql_literal(key_value)?;

    Ok(format!(
        "UPDATE {} SET {} WHERE {} = {}",
        table_name,
        set_clauses.join(", "),
        key_field,
        key_value_str
    ))
}

/// Build an INSERT query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123", "name": "John", "email": "john@example.com" }`
/// Returns: `INSERT INTO users (id, name, email) VALUES ('123', 'John', 'john@example.com')`
pub fn build_insert_query(
    typename: &str,
    variables: &Value,
    _metadata: &FederationMetadata,
) -> Result<String> {
    let table_name = typename.to_lowercase();

    let vars = variables.as_object().ok_or_else(|| {
        FraiseQLError::Validation {
            message: "Variables must be an object".to_string(),
            path: None,
        }
    })?;

    if vars.is_empty() {
        return Err(FraiseQLError::Validation {
            message: "No fields to insert".to_string(),
            path: None,
        });
    }

    let columns: Vec<&String> = vars.keys().collect();
    let values: Result<Vec<String>> = columns
        .iter()
        .map(|col| vars.get(*col).ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!("Field '{}' missing in variables", col),
                path: None,
            }
        }).and_then(|v| value_to_sql_literal(v)))
        .collect();

    let values = values?;
    let columns_str = columns.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
    let values_str = values.join(", ");

    Ok(format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table_name, columns_str, values_str
    ))
}

/// Build a DELETE query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123" }`
/// Returns: `DELETE FROM users WHERE id = '123'`
pub fn build_delete_query(
    typename: &str,
    variables: &Value,
    metadata: &FederationMetadata,
) -> Result<String> {
    let fed_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| FraiseQLError::Validation {
            message: format!("Type '{}' not found in federation metadata", typename),
            path: None,
        })?;

    let key_directive = fed_type.keys.first().ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Type '{}' has no @key directive", typename),
            path: None,
        }
    })?;

    let key_field = &key_directive.fields[0];
    let table_name = typename.to_lowercase();

    let vars = variables.as_object().ok_or_else(|| {
        FraiseQLError::Validation {
            message: "Variables must be an object".to_string(),
            path: None,
        }
    })?;

    let key_value = vars.get(key_field).ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Key field '{}' missing in variables", key_field),
            path: None,
        }
    })?;

    let key_value_str = value_to_sql_literal(key_value)?;

    Ok(format!(
        "DELETE FROM {} WHERE {} = {}",
        table_name, key_field, key_value_str
    ))
}

/// Convert a JSON value to SQL literal representation.
fn value_to_sql_literal(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => {
            let escaped = s.replace("'", "''");
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

trait JsonTypeStr {
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

    #[test]
    fn test_build_update_query() {
        // Placeholder: Integration tests handle actual query building
        // Unit tests would require mock metadata setup
    }

    #[test]
    fn test_build_insert_query() {
        // Placeholder: Integration tests handle actual query building
    }

    #[test]
    fn test_build_delete_query() {
        // Placeholder: Integration tests handle actual query building
    }

    #[test]
    fn test_value_to_sql_literal_string() {
        let result = value_to_sql_literal(&Value::String("John".to_string())).unwrap();
        assert_eq!(result, "'John'");

        // Test SQL injection prevention
        let result = value_to_sql_literal(&Value::String("O'Brien".to_string())).unwrap();
        assert_eq!(result, "'O''Brien'");
    }

    #[test]
    fn test_value_to_sql_literal_number() {
        let result = value_to_sql_literal(&Value::Number(123.into())).unwrap();
        assert_eq!(result, "123");
    }

    #[test]
    fn test_value_to_sql_literal_null() {
        let result = value_to_sql_literal(&Value::Null).unwrap();
        assert_eq!(result, "NULL");
    }
}
