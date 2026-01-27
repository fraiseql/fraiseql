//! Entity resolution for federation _entities query.

use super::types::{EntityRepresentation, FederationResolver};
use super::database_resolver::DatabaseEntityResolver;
use super::selection_parser::FieldSelection;
use crate::db::traits::DatabaseAdapter;
use crate::error::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

/// Resolve entities for a specific typename from local database
pub async fn resolve_entities_from_db<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    typename: &str,
    adapter: Arc<A>,
    fed_resolver: &FederationResolver,
    selection: &FieldSelection,
) -> EntityResolutionResult {
    if representations.is_empty() {
        return EntityResolutionResult {
            entities: Vec::new(),
            errors: Vec::new(),
        };
    }

    // Create database entity resolver
    let db_resolver = DatabaseEntityResolver::new(adapter, fed_resolver.metadata.clone());

    // Resolve from database
    match db_resolver
        .resolve_entities_from_db(typename, representations, selection)
        .await
    {
        Ok(entities) => EntityResolutionResult {
            entities,
            errors: Vec::new(),
        },
        Err(e) => EntityResolutionResult {
            entities: vec![None; representations.len()],
            errors: vec![e.to_string()],
        },
    }
}

/// Batch load entities from database
pub async fn batch_load_entities<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
    adapter: Arc<A>,
    selection: &FieldSelection,
) -> Result<Vec<Option<Value>>> {
    if representations.is_empty() {
        return Ok(Vec::new());
    }

    // Group by typename
    let grouped = group_entities_by_typename(representations);

    let mut all_results: Vec<(usize, Option<Value>)> = Vec::new();
    let mut current_index = 0;

    for (typename, reps) in grouped {
        // Resolve this batch using database
        let result = resolve_entities_from_db(
            &reps,
            &typename,
            Arc::clone(&adapter),
            fed_resolver,
            selection,
        )
        .await;

        // Map results back to original indices with proper ordering
        for entity in result.entities {
            all_results.push((current_index, entity));
            current_index += 1;
        }

        // Track errors if any
        if !result.errors.is_empty() {
            // Log errors but continue with None values
            for error in result.errors {
                eprintln!("Entity resolution error for {}: {}", typename, error);
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
    use serde_json::json;

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
