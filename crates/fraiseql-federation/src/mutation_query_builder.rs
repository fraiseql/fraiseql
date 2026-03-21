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
        path:    None,
    })?;

    // Extract key value
    let key_value = vars.get(key_field).ok_or_else(|| FraiseQLError::Validation {
        message: format!("Key field '{}' missing in variables", key_field),
        path:    None,
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
            path:    None,
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
pub fn build_insert_query(
    typename: &str,
    variables: &Value,
    _metadata: &FederationMetadata,
) -> Result<String> {
    let table_name = quote_table_name(typename);

    let vars = variables.as_object().ok_or_else(|| FraiseQLError::Validation {
        message: "Variables must be an object".to_string(),
        path:    None,
    })?;

    if vars.is_empty() {
        return Err(FraiseQLError::Validation {
            message: "No fields to insert".to_string(),
            path:    None,
        });
    }

    let columns: Vec<&String> = vars.keys().collect();
    let values: Result<Vec<String>> = columns
        .iter()
        .map(|col| {
            vars.get(*col)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!("Field '{}' missing in variables", col),
                    path:    None,
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

    // SAFETY: table_name is schema-derived (typename from federation metadata, validated at
    // compile time) and passed through quote_table_name()/quote_postgres_identifier().
    // columns are also passed through quote_postgres_identifier().
    Ok(format!("INSERT INTO {} ({}) VALUES ({})", table_name, columns_str, values_str))
}

/// Build a DELETE query from mutation variables.
///
/// # Example
///
/// Variables: `{ "id": "123" }`
/// Returns: `DELETE FROM "users" WHERE "id" = '123'`
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
        path:    None,
    })?;

    let key_value = vars.get(key_field).ok_or_else(|| FraiseQLError::Validation {
        message: format!("Key field '{}' missing in variables", key_field),
        path:    None,
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
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;
    use crate::types::{FederatedType, FederationMetadata, KeyDirective};

    fn make_metadata(typename: &str, key_field: &str) -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             typename.to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec![key_field.to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  Vec::new(),
                shareable_fields: Vec::new(),
                field_directives: std::collections::HashMap::new(),
            }],
        }
    }

    #[test]
    fn test_build_update_query() {
        let meta = make_metadata("Order", "id");
        let vars = json!({ "id": "42", "status": "shipped" });
        let sql = build_update_query("Order", &vars, &meta).unwrap();
        assert!(sql.contains("UPDATE"), "missing UPDATE keyword: {sql}");
        assert!(sql.contains("\"order\""), "table name must be quoted: {sql}");
        assert!(sql.contains("SET"), "missing SET clause: {sql}");
        assert!(sql.contains("\"status\""), "column must be quoted: {sql}");
        assert!(sql.contains("WHERE"), "missing WHERE clause: {sql}");
        assert!(sql.contains("\"id\""), "key column must be quoted: {sql}");

        // Single-quote escaping in values
        let vars_with_apostrophe = json!({ "id": "1", "name": "O'Brien" });
        let sql2 = build_update_query("Order", &vars_with_apostrophe, &meta).unwrap();
        assert!(sql2.contains("O''Brien"), "apostrophe must be escaped: {sql2}");
    }

    #[test]
    fn test_build_insert_query() {
        let meta = make_metadata("Group", "id");
        let vars = json!({ "id": "7", "name": "Admins" });
        let sql = build_insert_query("Group", &vars, &meta).unwrap();
        assert!(sql.contains("INSERT INTO"), "missing INSERT INTO: {sql}");
        assert!(sql.contains("\"group\""), "table name must be quoted: {sql}");
        assert!(sql.contains("VALUES"), "missing VALUES clause: {sql}");
        assert!(sql.contains("\"id\""), "column must be quoted: {sql}");
        assert!(sql.contains("\"name\""), "column must be quoted: {sql}");

        // Single-quote escaping in values
        let vars_apostrophe = json!({ "id": "2", "label": "O'Hara's Team" });
        let sql2 = build_insert_query("Group", &vars_apostrophe, &meta).unwrap();
        assert!(sql2.contains("O''Hara''s Team"), "apostrophe must be escaped: {sql2}");
    }

    #[test]
    fn test_build_delete_query() {
        let meta = make_metadata("User", "id");
        let vars = json!({ "id": "99" });
        let sql = build_delete_query("User", &vars, &meta).unwrap();
        assert!(sql.contains("DELETE FROM"), "missing DELETE FROM: {sql}");
        assert!(sql.contains("\"user\""), "table name must be quoted: {sql}");
        assert!(sql.contains("WHERE"), "missing WHERE clause: {sql}");
        assert!(sql.contains("\"id\""), "key column must be quoted: {sql}");
        assert!(sql.contains("'99'"), "key value must appear in SQL: {sql}");
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
