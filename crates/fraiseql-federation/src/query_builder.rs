//! SQL query construction for federation entity resolution.
//!
//! Builds parameterized WHERE IN clauses for batch entity queries. Key-field
//! values are bound as parameters (never interpolated) and key-field identifiers
//! are validated, preventing SQL injection across every supported backend —
//! including MySQL's default backslash-escape mode, where the former
//! single-quote-only escaping was unsafe (H3).

use fraiseql_db::{DatabaseType, utils::to_snake_case};
use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{
    metadata_helpers::{find_federation_type, get_key_directive},
    sql_utils::{placeholder, validate_sql_identifier, value_to_string},
    types::{EntityRepresentation, FederationMetadata},
};

/// Render the left-hand side of a key `WHERE … IN` comparison, as text.
///
/// `@key` values arrive as JSON and are bound as `text` parameters, so the key
/// expression must produce text too:
///
/// - **jsonb mode** (`jsonb_column = Some(col)`): the key lives in the entity's jsonb column, so it
///   is read as `"col"->>'snake(field)'` (the `->>` operator yields text, with camelCase→snake
///   recasing) — already text, no cast needed.
/// - **flat mode** (`None`): the key is a real column. The column is frequently `uuid` (or
///   integer), and PostgreSQL is strictly typed, so `id IN ($1)` with a text-bound `$1` fails with
///   `operator does not exist: uuid = text` — silently turning every cross-subgraph join null
///   (#504). Casting the column to text (`id::text`) makes the comparison succeed uniformly for
///   `uuid` / integer / text keys. Other dialects coerce text operands implicitly, so their SQL is
///   left byte-for-byte unchanged.
fn key_match_expr(field: &str, db_type: DatabaseType, jsonb_column: Option<&str>) -> String {
    match jsonb_column {
        Some(col) => {
            let column = col.replace('"', "\"\"");
            let key = to_snake_case(field).replace('\'', "''");
            format!("\"{column}\"->>'{key}'")
        },
        None => match db_type {
            DatabaseType::PostgreSQL => format!("{field}::text"),
            _ => field.to_string(),
        },
    }
}

/// Build a parameterized WHERE IN clause for batch entity resolution.
///
/// Returns the SQL fragment (with dialect-native bind placeholders) and the
/// ordered parameter values to bind. Key-field values are bound, not
/// interpolated, so attacker-controlled `representations` cannot inject SQL.
///
/// Example (PostgreSQL): `id IN ($1, $2, $3)` with params `["a", "b", "c"]`.
///
/// # Arguments
///
/// * `typename` - The entity type name (e.g., "User")
/// * `representations` - Entity representations with key field values
/// * `metadata` - Federation metadata for the schema
/// * `db_type` - The target backend dialect (controls placeholder syntax)
///
/// # Errors
///
/// Returns error if the type is not found, a key field is missing, or a key
/// field name is not a safe SQL identifier.
pub fn construct_where_in_clause(
    typename: &str,
    representations: &[EntityRepresentation],
    metadata: &FederationMetadata,
    db_type: DatabaseType,
    jsonb_column: Option<&str>,
) -> Result<(String, Vec<Value>)> {
    // Find the entity type and its key directive
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;

    // For single-field keys, build a simple WHERE IN
    if key_directive.fields.len() == 1 {
        let key_field = &key_directive.fields[0];
        validate_sql_identifier(key_field)?;
        let key_values = extract_key_values(representations, key_field)?;

        if key_values.is_empty() {
            return Ok(("1 = 0".to_string(), Vec::new())); // No entities to resolve
        }

        // Build: key_field IN ($1, $2, ...) with values bound separately.
        let placeholders = (0..key_values.len())
            .map(|i| placeholder(db_type, i))
            .collect::<Vec<_>>()
            .join(", ");
        let params = key_values.into_iter().map(Value::String).collect();

        Ok((
            format!("{} IN ({placeholders})", key_match_expr(key_field, db_type, jsonb_column)),
            params,
        ))
    } else {
        // For composite keys, build: (key1, key2) IN (($1, $2), ...)
        construct_composite_where_in(&key_directive.fields, representations, db_type, jsonb_column)
    }
}

/// Extract values of a specific key field from entity representations.
///
/// Values are stringified (matching the legacy literal-comparison semantics) and
/// later bound as `Value::String` parameters by the caller.
fn extract_key_values(
    representations: &[EntityRepresentation],
    key_field: &str,
) -> Result<Vec<String>> {
    representations
        .iter()
        .map(|rep| {
            rep.key_fields
                .get(key_field)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!(
                        "Key field '{}' missing in entity representation for {}",
                        key_field, rep.typename
                    ),
                    path:    None,
                })
                .and_then(value_to_string)
        })
        .collect()
}

/// Build a parameterized WHERE IN clause for composite keys.
fn construct_composite_where_in(
    key_fields: &[String],
    representations: &[EntityRepresentation],
    db_type: DatabaseType,
    jsonb_column: Option<&str>,
) -> Result<(String, Vec<Value>)> {
    if representations.is_empty() {
        return Ok(("1 = 0".to_string(), Vec::new()));
    }

    // Validate each key-field identifier once (interpolated unquoted, like the
    // single-key path, to preserve PostgreSQL case-folding).
    for field in key_fields {
        validate_sql_identifier(field)?;
    }

    let mut params: Vec<Value> = Vec::new();
    let mut value_tuples = Vec::new();

    for rep in representations {
        let mut placeholders = Vec::new();
        for field in key_fields {
            let value = rep.key_fields.get(field).ok_or_else(|| FraiseQLError::Validation {
                message: format!("Key field '{}' missing in representation", field),
                path:    None,
            })?;
            // The placeholder index is the position this value occupies in the flat
            // `params` vector (0-based), so it must be read before the push.
            placeholders.push(placeholder(db_type, params.len()));
            params.push(Value::String(value_to_string(value)?));
        }
        value_tuples.push(format!("({})", placeholders.join(", ")));
    }

    let fields_list = key_fields
        .iter()
        .map(|f| key_match_expr(f, db_type, jsonb_column))
        .collect::<Vec<_>>()
        .join(", ");
    let tuples_str = value_tuples.join(", ");

    Ok((format!("({fields_list}) IN ({tuples_str})"), params))
}

#[cfg(test)]
mod tests;
