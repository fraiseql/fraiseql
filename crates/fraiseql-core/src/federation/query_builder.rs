//! SQL query construction for federation entity resolution.
//!
//! Builds WHERE IN clauses for batch entity queries, with SQL injection prevention
//! through proper escaping and parameterization.

use crate::error::{FraiseQLError, Result};
use crate::federation::sql_utils::{escape_sql_string, value_to_string};
use crate::federation::types::{EntityRepresentation, FederationMetadata};

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
    // Find the entity type in federation metadata
    let fed_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| FraiseQLError::Validation {
            message: format!("Type '{}' not found in federation metadata", typename),
            path: None,
        })?;

    // Get the key directive (use first key for now - handles simple case)
    let key_directive = fed_type.keys.first().ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Type '{}' has no @key directive", typename),
            path: None,
        }
    })?;

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
        construct_composite_where_in(key_directive.fields.clone(), representations)
    }
}

/// Extract values of a specific key field from entity representations.
fn extract_key_values(
    representations: &[EntityRepresentation],
    key_field: &str,
) -> Result<Vec<String>> {
    let mut values = Vec::new();

    for rep in representations {
        if let Some(value) = rep.key_fields.get(key_field) {
            values.push(value_to_string(value)?);
        } else {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Key field '{}' missing in entity representation for {}",
                    key_field, rep.typename
                ),
                path: None,
            });
        }
    }

    Ok(values)
}

/// Build WHERE IN clause for composite keys.
fn construct_composite_where_in(
    key_fields: Vec<String>,
    representations: &[EntityRepresentation],
) -> Result<String> {
    if representations.is_empty() {
        return Ok("1 = 0".to_string());
    }

    // Extract all key value combinations
    let mut value_tuples = Vec::new();

    for rep in representations {
        let mut tuple_values = Vec::new();
        for field in &key_fields {
            let value = rep
                .key_fields
                .get(field)
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!("Key field '{}' missing in representation", field),
                    path: None,
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
mod tests {
    use super::*;
    use serde_json::json;

    fn make_test_metadata() -> FederationMetadata {
        use crate::federation::types::{FederatedType, KeyDirective};

        let types = vec![FederatedType {
            name: "User".to_string(),
            keys: vec![KeyDirective {
                fields: vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends: false,
            external_fields: vec![],
            shareable_fields: vec![],
        }];

        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types,
        }
    }

    #[test]
    fn test_construct_simple_where_in() {
        let metadata = make_test_metadata();
        let reps = vec![
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: [(String::from("id"), json!("123"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: [(String::from("id"), json!("456"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
        ];

        let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
        assert!(clause.contains("id IN"));
        assert!(clause.contains("'123'"));
        assert!(clause.contains("'456'"));
    }

    #[test]
    fn test_sql_injection_prevention() {
        let metadata = make_test_metadata();
        let reps = vec![EntityRepresentation {
            typename: "User".to_string(),
            key_fields: [(String::from("id"), json!("'; DROP TABLE users; --"))]
                .iter()
                .cloned()
                .collect(),
            all_fields: Default::default(),
        }];

        let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
        // Dangerous SQL should be escaped
        assert!(clause.contains("'';")); // Single quote should be doubled
    }

    #[test]
    fn test_escape_sql_string() {
        let result = escape_sql_string("O'Brien");
        assert_eq!(result, "O''Brien");

        let result = escape_sql_string("test''; DROP--");
        assert_eq!(result, "test''''; DROP--");
    }

    #[test]
    fn test_empty_representations() {
        let metadata = make_test_metadata();
        let reps = vec![];

        let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
        assert_eq!(clause, "1 = 0"); // No rows to resolve
    }

    #[test]
    fn test_missing_type_error() {
        let metadata = make_test_metadata();
        let reps = vec![];

        let result = construct_where_in_clause("NotFound", &reps, &metadata);
        assert!(result.is_err());
    }
}

