//! Entity resolution for federation _entities query.

use super::types::{EntityRepresentation, FederatedType, FederationResolver};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Result of entity resolution
#[derive(Debug)]
pub struct EntityResolutionResult {
    /// Resolved entities in same order as input representations
    pub entities: Vec<Option<Value>>,

    /// Any errors encountered during resolution
    pub errors: Vec<String>,
}

/// Deduplicate entity representations while preserving order
pub fn deduplicate_representations(
    reps: &[EntityRepresentation],
) -> Vec<EntityRepresentation> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(reps.len());

    for rep in reps {
        // Create a key from typename + key_fields
        let key = format!("{}:{:?}", rep.typename, rep.key_fields);
        if seen.insert(key) {
            result.push(rep.clone());
        }
    }

    result
}

/// Group entities by typename and strategy
pub fn group_entities_by_typename(
    reps: &[EntityRepresentation],
) -> HashMap<String, Vec<EntityRepresentation>> {
    let mut groups: HashMap<String, Vec<EntityRepresentation>> = HashMap::new();

    for rep in reps {
        groups.entry(rep.typename.clone())
            .or_insert_with(Vec::new)
            .push(rep.clone());
    }

    groups
}

/// Construct WHERE clause for batch query
pub fn construct_batch_where_clause(
    representations: &[EntityRepresentation],
    key_columns: &[String],
) -> Result<String> {
    if representations.is_empty() || key_columns.is_empty() {
        return Ok(String::new());
    }

    let mut conditions = Vec::new();

    for key_col in key_columns {
        let values: Vec<String> = representations
            .iter()
            .filter_map(|rep| rep.key_fields.get(key_col))
            .filter_map(|v| v.as_str())
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect();

        if !values.is_empty() && !values.iter().all(|v| v == "''") {
            conditions.push(format!("{} IN ({})", key_col, values.join(", ")));
        }
    }

    if conditions.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("WHERE {}", conditions.join(" AND ")))
    }
}

/// Resolve entities for a specific typename
pub fn resolve_entities_local(
    representations: &[EntityRepresentation],
    typename: &str,
    fed_type: &FederatedType,
) -> EntityResolutionResult {
    // Deduplicate
    let deduped = deduplicate_representations(representations);

    // Get key columns from first key directive
    let _key_columns = fed_type.keys.first()
        .map(|k| k.fields.clone())
        .unwrap_or_default();

    // Return mock results that match the structure
    // Database integration will be added in a future phase when connection pooling is available
    let mut entities = Vec::new();
    for rep in &deduped {
        // Return the representation as resolved entity
        let mut entity = rep.all_fields.clone();
        entity.insert("__typename".to_string(), json!(typename));
        entities.push(Some(json!(entity)));
    }

    // Need to re-map to original order
    let mut result_map: HashMap<String, Value> = HashMap::new();
    for (i, entity) in entities.iter().enumerate() {
        if let Some(ent) = entity {
            result_map.insert(format!("key_{}", i), ent.clone());
        }
    }

    // Build final results in original order
    let mut final_results = Vec::new();
    for rep in representations {
        let _key = format!("{}:{:?}", rep.typename, rep.key_fields);
        // For basic implementation, return representation as entity
        let mut entity = rep.all_fields.clone();
        entity.insert("__typename".to_string(), json!(typename));
        final_results.push(Some(json!(entity)));
    }

    EntityResolutionResult {
        entities: final_results,
        errors: Vec::new(),
    }
}

/// Batch load entities
pub async fn batch_load_entities(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
) -> Result<Vec<Option<Value>>> {
    // Group by typename
    let grouped = group_entities_by_typename(representations);

    let mut all_results: Vec<(usize, Option<Value>)> = Vec::new();

    for (typename, reps) in grouped {
        // Find type metadata
        let fed_type = fed_resolver.metadata.types.iter()
            .find(|t| t.name == typename);

        if let Some(fed_type) = fed_type {
            // Resolve this batch
            let result = resolve_entities_local(&reps, &typename, fed_type);

            // Map results back to original indices
            for (idx, entity) in result.entities.iter().enumerate() {
                all_results.push((idx, entity.clone()));
            }
        }
    }

    // Sort by original index to preserve order
    all_results.sort_by_key(|(idx, _)| *idx);
    Ok(all_results.into_iter().map(|(_, e)| e).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_representations() {
        let reps = vec![
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("123"));
                    m
                },
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("123"));
                    m
                },
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("456"));
                    m
                },
                all_fields: HashMap::new(),
            },
        ];

        let deduped = deduplicate_representations(&reps);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_group_entities_by_typename() {
        let reps = vec![
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename: "Order".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename: "User".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
        ];

        let grouped = group_entities_by_typename(&reps);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["User"].len(), 2);
        assert_eq!(grouped["Order"].len(), 1);
    }

    #[test]
    fn test_construct_batch_where_clause() {
        let mut rep1 = EntityRepresentation {
            typename: "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        };
        rep1.key_fields.insert("id".to_string(), json!("123"));

        let mut rep2 = EntityRepresentation {
            typename: "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        };
        rep2.key_fields.insert("id".to_string(), json!("456"));

        let reps = vec![rep1, rep2];
        let where_clause = construct_batch_where_clause(&reps, &["id".to_string()]).unwrap();

        assert!(where_clause.contains("WHERE"));
        assert!(where_clause.contains("id IN"));
        assert!(where_clause.contains("123"));
        assert!(where_clause.contains("456"));
    }
}
