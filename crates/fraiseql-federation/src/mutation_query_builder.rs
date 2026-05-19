//! SQL mutation query building for federation mutations.
//!
//! Constructs UPDATE, INSERT, and DELETE queries from GraphQL mutations
//! with parameter validation and SQL injection prevention.

use fraiseql_db::quote_postgres_identifier;
use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{
    metadata_helpers::{find_federation_type, get_key_directive},
    sql_utils::value_to_sql_literal,
    types::FederationMetadata,
};

/// Quote a GraphQL typename as a SQL table name (lowercase, double-quoted).
///
/// Prevents SQL keyword collisions: `Order` → `"order"`, `Group` → `"group"`.
fn quote_table_name(typename: &str) -> String {
    quote_postgres_identifier(&typename.to_lowercase())
}

/// Build an UPDATE query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123", "name": "John", "email": "john@example.com" }`
/// Returns: `UPDATE "users" SET "name" = 'John', "email" = 'john@example.com' WHERE "id" = '123'`
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the type is not found in metadata, variables are
/// not a JSON object, the key field is missing, or no non-key fields are provided.
pub fn build_update_query(
    typename: &str,
    variables: &Value,
    metadata: &FederationMetadata,
) -> Result<String> {
    // Find type and key directive
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;

    let key_field = &key_directive.fields[0];
    let table_name = quote_table_name(typename);

    let vars = variables.as_object().ok_or_else(|| FraiseQLError::Validation {
        message: "Variables must be an object".to_string(),
        path: None,
    })?;

    // Extract key value
    let key_value = vars.get(key_field).ok_or_else(|| FraiseQLError::Validation {
        message: format!("Key field '{}' missing in variables", key_field),
        path: None,
    })?;

    // Build SET clauses
    let mut set_clauses = Vec::new();
    for (field, value) in vars {
        if field != key_field {
            let value_str = value_to_sql_literal(value)?;
            set_clauses.push(format!("{} = {}", quote_postgres_identifier(field), value_str));
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
        quote_postgres_identifier(key_field),
        key_value_str
    ))
}

/// Build an INSERT query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123", "name": "John", "email": "john@example.com" }`
/// Returns: `INSERT INTO "users" ("id", "name", "email") VALUES ('123', 'John',
/// 'john@example.com')`
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if variables are not a JSON object or are empty.
pub fn build_insert_query(
    typename: &str,
    variables: &Value,
    _metadata: &FederationMetadata,
) -> Result<String> {
    let table_name = quote_table_name(typename);

    let vars = variables.as_object().ok_or_else(|| FraiseQLError::Validation {
        message: "Variables must be an object".to_string(),
        path: None,
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
        .map(|col| {
            vars.get(*col)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!("Field '{}' missing in variables", col),
                    path: None,
                })
                .and_then(value_to_sql_literal)
        })
        .collect();

    let values = values?;
    let columns_str = columns
        .iter()
        .map(|s| quote_postgres_identifier(s))
        .collect::<Vec<_>>()
        .join(", ");
    let values_str = values.join(", ");

    Ok(format!("INSERT INTO {} ({}) VALUES ({})", table_name, columns_str, values_str))
}

/// Build a DELETE query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123" }`
/// Returns: `DELETE FROM "users" WHERE "id" = '123'`
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the type is not found in metadata, variables are
/// not a JSON object, or the key field is missing.
pub fn build_delete_query(
    typename: &str,
    variables: &Value,
    metadata: &FederationMetadata,
) -> Result<String> {
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;

    let key_field = &key_directive.fields[0];
    let table_name = quote_table_name(typename);

    let vars = variables.as_object().ok_or_else(|| FraiseQLError::Validation {
        message: "Variables must be an object".to_string(),
        path: None,
    })?;

    let key_value = vars.get(key_field).ok_or_else(|| FraiseQLError::Validation {
        message: format!("Key field '{}' missing in variables", key_field),
        path: None,
    })?;

    let key_value_str = value_to_sql_literal(key_value)?;

    Ok(format!(
        "DELETE FROM {} WHERE {} = {}",
        table_name,
        quote_postgres_identifier(key_field),
        key_value_str
    ))
}

#[cfg(test)]
mod tests;
