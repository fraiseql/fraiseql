//! SQL query construction for federation entity resolution.
//!
//! Builds WHERE IN clauses for batch entity queries using parameterized queries
//! to eliminate SQL injection risk entirely.

use fraiseql_db::DatabaseType;
use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{
    metadata_helpers::{find_federation_type, get_key_directive},
    sql_utils::value_to_string,
    types::{EntityRepresentation, FederationMetadata},
};

/// Result of parameterized WHERE clause construction.
#[derive(Debug, Clone)]
pub struct ParameterizedWhereClause {
    /// SQL WHERE clause with bind-parameter placeholders (e.g. `id IN ($1, $2)`).
    pub sql:    String,
    /// Bind parameter values in placeholder order.
    pub params: Vec<Value>,
}

/// Build a parameterized WHERE IN clause for batch entity resolution.
///
/// Returns a `ParameterizedWhereClause` whose `sql` field contains placeholders
/// appropriate for the given `db_type`, and whose `params` field holds the
/// corresponding bind values.
///
/// # Arguments
///
/// * `typename` - The entity type name (e.g., "User")
/// * `representations` - Entity representations with key field values
/// * `metadata` - Federation metadata for the schema
/// * `db_type` - Database type, used to select placeholder syntax
///
/// # Errors
///
/// Returns error if type not found in metadata or key fields missing.
pub fn construct_where_in_clause(
    typename: &str,
    representations: &[EntityRepresentation],
    metadata: &FederationMetadata,
    db_type: DatabaseType,
) -> Result<ParameterizedWhereClause> {
    let fed_type = find_federation_type(typename, metadata)?;
    let key_directive = get_key_directive(fed_type)?;

    if key_directive.fields.len() == 1 {
        let key_field = &key_directive.fields[0];
        let key_values = extract_key_values(representations, key_field)?;

        if key_values.is_empty() {
            return Ok(ParameterizedWhereClause {
                sql:    "1 = 0".to_string(),
                params: vec![],
            });
        }

        let mut params = Vec::with_capacity(key_values.len());
        let mut placeholders = Vec::with_capacity(key_values.len());

        for (i, value) in key_values.into_iter().enumerate() {
            placeholders.push(db_type.placeholder(i + 1));
            params.push(Value::String(value));
        }

        Ok(ParameterizedWhereClause {
            sql:    format!("{} IN ({})", key_field, placeholders.join(", ")),
            params,
        })
    } else {
        construct_composite_where_in(key_directive.fields.clone(), representations, db_type)
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

/// Build parameterized WHERE IN clause for composite keys.
fn construct_composite_where_in(
    key_fields: Vec<String>,
    representations: &[EntityRepresentation],
    db_type: DatabaseType,
) -> Result<ParameterizedWhereClause> {
    if representations.is_empty() {
        return Ok(ParameterizedWhereClause {
            sql:    "1 = 0".to_string(),
            params: vec![],
        });
    }

    let mut params = Vec::new();
    let mut value_tuples = Vec::new();
    let mut param_idx = 1usize;

    for rep in representations {
        let mut tuple_placeholders = Vec::new();
        for field in &key_fields {
            let value = rep.key_fields.get(field).ok_or_else(|| FraiseQLError::Validation {
                message: format!("Key field '{}' missing in representation", field),
                path:    None,
            })?;
            tuple_placeholders.push(db_type.placeholder(param_idx));
            params.push(Value::String(value_to_string(value)?));
            param_idx += 1;
        }
        value_tuples.push(format!("({})", tuple_placeholders.join(", ")));
    }

    let fields_list = key_fields.join(", ");
    let tuples_str = value_tuples.join(", ");

    Ok(ParameterizedWhereClause {
        sql:    format!("({}) IN ({})", fields_list, tuples_str),
        params,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    #![allow(clippy::iter_on_single_items)] // Reason: test data uses single-element iter for structural clarity

    use serde_json::json;

    use super::*;

    fn make_test_metadata() -> FederationMetadata {
        use crate::types::{FederatedType, KeyDirective};

        let types = vec![FederatedType {
            name:             "User".to_string(),
            keys:             vec![KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
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
                typename:   "User".to_string(),
                key_fields: [(String::from("id"), json!("123"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: [(String::from("id"), json!("456"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
        ];

        let result =
            construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();
        assert_eq!(result.sql, "id IN ($1, $2)");
        assert_eq!(result.params, vec![json!("123"), json!("456")]);
    }

    #[test]
    fn test_construct_where_in_mysql_placeholders() {
        let metadata = make_test_metadata();
        let reps = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: [(String::from("id"), json!("1"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: [(String::from("id"), json!("2"))].iter().cloned().collect(),
                all_fields: Default::default(),
            },
        ];

        let result =
            construct_where_in_clause("User", &reps, &metadata, DatabaseType::MySQL).unwrap();
        assert_eq!(result.sql, "id IN (?, ?)");
        assert_eq!(result.params, vec![json!("1"), json!("2")]);
    }

    #[test]
    fn test_construct_where_in_sqlserver_placeholders() {
        let metadata = make_test_metadata();
        let reps = vec![EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: [(String::from("id"), json!("42"))].iter().cloned().collect(),
            all_fields: Default::default(),
        }];

        let result =
            construct_where_in_clause("User", &reps, &metadata, DatabaseType::SQLServer).unwrap();
        assert_eq!(result.sql, "id IN (@p1)");
        assert_eq!(result.params, vec![json!("42")]);
    }

    #[test]
    fn test_sql_injection_values_are_parameterized() {
        let metadata = make_test_metadata();
        let reps = vec![EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: [(String::from("id"), json!("'; DROP TABLE users; --"))]
                .iter()
                .cloned()
                .collect(),
            all_fields: Default::default(),
        }];

        let result =
            construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();
        // Dangerous value is in params, NOT in the SQL string
        assert_eq!(result.sql, "id IN ($1)");
        assert_eq!(result.params, vec![json!("'; DROP TABLE users; --")]);
        assert!(!result.sql.contains("DROP"));
    }

    #[test]
    fn test_empty_representations() {
        let metadata = make_test_metadata();
        let reps = vec![];

        let result =
            construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();
        assert_eq!(result.sql, "1 = 0");
        assert!(result.params.is_empty());
    }

    #[test]
    fn test_missing_type_error() {
        let metadata = make_test_metadata();
        let reps = vec![];

        let result =
            construct_where_in_clause("NotFound", &reps, &metadata, DatabaseType::PostgreSQL);
        assert!(result.is_err());
    }
}
