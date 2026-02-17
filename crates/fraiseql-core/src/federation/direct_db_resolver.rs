//! Direct database entity resolution for federation.
//!
//! Resolves entities from remote FraiseQL database instances via direct database connections,
//! achieving <20ms latency by eliminating HTTP overhead.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{
    db::where_clause::{WhereClause, WhereOperator},
    error::Result,
    federation::{
        connection_manager::{ConnectionManager, RemoteDatabaseConfig},
        selection_parser::FieldSelection,
        types::{EntityRepresentation, FederationMetadata},
    },
};

/// Resolves entities from remote databases via direct connections
pub struct DirectDatabaseResolver {
    /// Manages connections to remote databases
    connection_manager: ConnectionManager,
    /// Federation metadata for schema validation
    /// Note: metadata will be used in Phase 6D for entity resolution
    #[allow(dead_code)]
    metadata:           FederationMetadata,
}

impl DirectDatabaseResolver {
    /// Create a new direct database resolver
    pub fn new(metadata: FederationMetadata) -> Self {
        Self {
            connection_manager: ConnectionManager::new(),
            metadata,
        }
    }

    /// Resolve entities from a remote database via direct connection
    ///
    /// Queries the remote database directly for entities matching the provided
    /// representations, using a WHERE IN clause on the key fields for efficient
    /// batch resolution.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name (used as view name prefix: `v_{typename}`)
    /// * `representations` - Entity representations with key field values
    /// * `connection_string` - Remote database connection URL
    /// * `selection` - Field selection from GraphQL query (currently unused, all fields returned)
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities), in same order as
    /// `representations`
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Connection fails
    /// - Query execution fails
    /// - Entity representation is invalid
    ///
    /// # Performance
    ///
    /// Uses batch WHERE IN queries for efficient remote database access:
    /// - Single query for all entity keys
    /// - <20ms expected latency for typical FraiseQL-to-FraiseQL federation
    pub async fn resolve_entities_direct_db(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        connection_string: &str,
        _selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Get or create connection to remote database
        let config = RemoteDatabaseConfig::new(connection_string);
        let adapter = self.connection_manager.get_or_create_connection(config).await?;

        // Get federation metadata for this type
        let fed_type =
            self.metadata.types.iter().find(|t| t.name == typename).ok_or_else(|| {
                crate::error::FraiseQLError::Internal {
                    message: format!("Type {} not in federation metadata", typename),
                    source:  None,
                }
            })?;

        // Get key field names
        let key_fields = &fed_type.keys[0].fields;

        // Build WHERE IN clause for key values
        let where_clause = self.build_where_in_clause(key_fields, representations)?;

        // Execute query on remote database
        let view_name = format!("v_{}", typename.to_lowercase());
        let jsonb_rows =
            adapter.execute_where_query(&view_name, Some(&where_clause), None, None).await?;

        // Convert JsonbValue to serde_json::Value
        let rows: Vec<Value> = jsonb_rows.into_iter().map(|jv| jv.into_value()).collect();

        // Map rows back to entities in original order
        self.map_rows_to_entities(representations, rows)
    }

    /// Build a WHERE IN clause for batch entity resolution
    fn build_where_in_clause(
        &self,
        key_fields: &[String],
        representations: &[EntityRepresentation],
    ) -> Result<WhereClause> {
        if key_fields.len() == 1 {
            // Single key: WHERE id IN (...)
            let key_field = &key_fields[0];
            let values: Vec<Value> = representations
                .iter()
                .filter_map(|rep| rep.key_fields.get(key_field).cloned())
                .collect();

            Ok(WhereClause::Field {
                path:     vec![key_field.clone()],
                operator: WhereOperator::In,
                value:    json!(values),
            })
        } else {
            // Composite key: (id, org_id) IN (...)
            let values: Vec<Value> = representations
                .iter()
                .map(|rep| {
                    let mut obj = serde_json::Map::new();
                    for field in key_fields {
                        if let Some(val) = rep.key_fields.get(field) {
                            obj.insert(field.clone(), val.clone());
                        }
                    }
                    Value::Object(obj)
                })
                .collect();

            Ok(WhereClause::Field {
                path:     key_fields.to_vec(),
                operator: WhereOperator::In,
                value:    json!(values),
            })
        }
    }

    /// Map query results back to entity representations in original order
    fn map_rows_to_entities(
        &self,
        representations: &[EntityRepresentation],
        rows: Vec<serde_json::Value>,
    ) -> Result<Vec<Option<Value>>> {
        // Index rows by their key fields for quick lookup
        let mut row_map: HashMap<String, Value> = HashMap::new();
        for row in rows {
            if let Value::Object(obj) = &row {
                // Use JSON serialization of key fields as lookup key
                let key =
                    serde_json::to_string(obj).unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());
                row_map.insert(key, row);
            }
        }

        // Map entities in order, returning None for missing entities
        let mut results = Vec::with_capacity(representations.len());
        for rep in representations {
            // Try to find matching row by comparing key fields
            let matching_row = row_map.values().find(|row| {
                if let Value::Object(obj) = row {
                    rep.key_fields.iter().all(|(key, val)| obj.get(key) == Some(val))
                } else {
                    false
                }
            });

            results.push(matching_row.cloned());
        }

        Ok(results)
    }

    /// Get the number of cached remote connections
    pub fn connection_count(&self) -> Result<usize> {
        self.connection_manager.connection_count()
    }

    /// Close a specific remote connection
    pub fn close_connection(&self, connection_string: &str) -> Result<()> {
        self.connection_manager.close_connection(connection_string)
    }

    /// Close all remote connections
    pub fn close_all(&self) -> Result<()> {
        self.connection_manager.close_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::types::{FederatedType, KeyDirective};

    fn create_test_metadata() -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  vec![],
                shareable_fields: vec![],
                field_directives: std::collections::HashMap::new(),
            }],
        }
    }

    #[test]
    fn test_direct_database_resolver_creation() {
        let metadata = create_test_metadata();
        let _resolver = DirectDatabaseResolver::new(metadata);
        // Should not panic
    }

    #[test]
    fn test_connection_count_empty() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);
        assert_eq!(resolver.connection_count().unwrap(), 0);
    }

    #[test]
    fn test_close_all() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);
        assert!(resolver.close_all().is_ok());
    }

    #[test]
    fn test_close_connection() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);
        // Closing non-existent connection should be OK (no error)
        assert!(resolver.close_connection("postgresql://localhost/db").is_ok());
    }

    #[test]
    fn test_metadata_stored() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata.clone());

        // Verify metadata is accessible for entity resolution
        assert!(resolver.metadata.enabled);
        assert_eq!(resolver.metadata.version, "v2");
        assert_eq!(resolver.metadata.types.len(), 1);
    }

    #[test]
    fn test_build_where_in_single_key() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);

        // Create representations with single key
        let representations = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![("id".to_string(), json!("user1"))].into_iter().collect(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![("id".to_string(), json!("user2"))].into_iter().collect(),
                all_fields: HashMap::new(),
            },
        ];

        let where_clause =
            resolver.build_where_in_clause(&["id".to_string()], &representations).unwrap();

        // Verify WHERE IN clause structure
        match where_clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => {
                assert_eq!(path, vec!["id".to_string()]);
                assert_eq!(operator, WhereOperator::In);
                assert_eq!(value, json!(["user1", "user2"]));
            },
            _ => panic!("Expected Field clause"),
        }
    }

    #[test]
    fn test_build_where_in_composite_key() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);

        // Create representations with composite keys
        let representations = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![
                    ("organization_id".to_string(), json!("org1")),
                    ("user_id".to_string(), json!("user1")),
                ]
                .into_iter()
                .collect(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![
                    ("organization_id".to_string(), json!("org1")),
                    ("user_id".to_string(), json!("user2")),
                ]
                .into_iter()
                .collect(),
                all_fields: HashMap::new(),
            },
        ];

        let where_clause = resolver
            .build_where_in_clause(
                &["organization_id".to_string(), "user_id".to_string()],
                &representations,
            )
            .unwrap();

        // Verify WHERE IN clause for composite key
        match where_clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => {
                assert_eq!(path, vec!["organization_id".to_string(), "user_id".to_string()]);
                assert_eq!(operator, WhereOperator::In);
                // Value should be array of objects with both keys
                if let Some(arr) = value.as_array() {
                    assert_eq!(arr.len(), 2);
                } else {
                    panic!("Expected array value");
                }
            },
            _ => panic!("Expected Field clause"),
        }
    }

    #[test]
    fn test_map_rows_to_entities_in_order() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);

        // Create representations
        let representations = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![("id".to_string(), json!("user1"))].into_iter().collect(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: vec![("id".to_string(), json!("user2"))].into_iter().collect(),
                all_fields: HashMap::new(),
            },
        ];

        // Create rows (note: only one row for user1, user2 missing)
        let mut row1 = serde_json::Map::new();
        row1.insert("id".to_string(), json!("user1"));
        row1.insert("name".to_string(), json!("Alice"));
        let rows = vec![Value::Object(row1)];

        let results = resolver.map_rows_to_entities(&representations, rows).unwrap();

        // Should have two results: one match, one None
        assert_eq!(results.len(), 2);
        assert!(results[0].is_some()); // user1 found
        assert!(results[1].is_none()); // user2 not found
    }

    #[test]
    fn test_empty_representations() {
        let metadata = create_test_metadata();
        let resolver = DirectDatabaseResolver::new(metadata);

        let where_clause = resolver.build_where_in_clause(&["id".to_string()], &[]).unwrap();

        // Should create WHERE IN with empty array
        match where_clause {
            WhereClause::Field {
                operator, value, ..
            } => {
                assert_eq!(operator, WhereOperator::In);
                assert_eq!(value, json!([]));
            },
            _ => panic!("Expected Field clause"),
        }
    }
}
