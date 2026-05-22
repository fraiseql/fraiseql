//! SQL query construction for federation entity resolution.
//!
//! Builds WHERE IN clauses for batch entity queries, with SQL injection prevention
//! through proper escaping and parameterization.

use fraiseql_error::{FraiseQLError, Result};

use crate::{
    metadata_helpers::{find_federation_type, get_key_directive},
    sql_utils::{escape_sql_string, value_to_string},
    types::{EntityRepresentation, FederationMetadata},
};

/// Build a WHERE IN clause for batch entity resolution.
///
/// Example:
/// ```text
/// SELECT id, name, email FROM users WHERE id IN ('123', '456', '789')
/// ```
///
/// # Arguments
///
/// * `typename` - The entity type name (e.g., "User")
/// * `representations` - Entity representations with key field values
/// * `metadata` - Federation metadata for the schema
///
/// # Returns
///
/// WHERE clause string ready for SQL query
///
/// # Errors
///
/// Returns error if type not found in metadata or key fields missing
pub fn construct_where_in_clause(
    typename: &str,
    representations: &[EntityRepresentation],
    metadata: &FederationMetadata,
) -> Result<String> {
    // Find the entity type and its key directive
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;

    // For single-field keys, build simple WHERE IN
    if key_directive.fields.len() == 1 {
        let key_field = &key_directive.fields[0];
        let key_values = extract_key_values(representations, key_field)?;

        if key_values.is_empty() {
            return Ok("1 = 0".to_string()); // No entities to resolve
        }

        // Build: key_field IN ('val1', 'val2', ...)
        let values_str = key_values
            .iter()
            .map(|v| format!("'{}'", escape_sql_string(v)))
            .collect::<Vec<_>>()
            .join(", ");

        Ok(format!("{} IN ({})", key_field, values_str))
    } else {
        // For composite keys, build: (key1, key2) IN ((val1a, val1b), ...)
        construct_composite_where_in(&key_directive.fields, representations)
    }
}

/// Extract values of a specific key field from entity representations.
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

/// Build WHERE IN clause for composite keys.
fn construct_composite_where_in(
    key_fields: &[String],
    representations: &[EntityRepresentation],
) -> Result<String> {
    if representations.is_empty() {
        return Ok("1 = 0".to_string());
    }

    // Extract all key value combinations
    let mut value_tuples = Vec::new();

    for rep in representations {
        let mut tuple_values = Vec::new();
        for field in key_fields {
            let value = rep.key_fields.get(field).ok_or_else(|| FraiseQLError::Validation {
                message: format!("Key field '{}' missing in representation", field),
                path:    None,
            })?;
            tuple_values.push(format!("'{}'", escape_sql_string(&value_to_string(value)?)));
        }
        value_tuples.push(format!("({})", tuple_values.join(", ")));
    }

    let fields_list = key_fields.join(", ");
    let tuples_str = value_tuples.join(", ");

    Ok(format!("({}) IN ({})", fields_list, tuples_str))
}

#[cfg(test)]
mod tests;
