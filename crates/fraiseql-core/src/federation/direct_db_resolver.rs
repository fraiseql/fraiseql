//! Direct database entity resolution for federation.
//!
//! Resolves entities from remote FraiseQL database instances via direct database connections,
//! achieving <20ms latency by eliminating HTTP overhead.

use crate::error::Result;
use crate::federation::selection_parser::FieldSelection;
use crate::federation::types::{EntityRepresentation, FederationMetadata};
use crate::federation::connection_manager::{ConnectionManager, RemoteDatabaseConfig};
use serde_json::Value;

/// Resolves entities from remote databases via direct connections
pub struct DirectDatabaseResolver {
    /// Manages connections to remote databases
    connection_manager: ConnectionManager,
    /// Federation metadata for schema validation
    /// Note: metadata will be used in Phase 6D for entity resolution
    #[allow(dead_code)]
    metadata: FederationMetadata,
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
    /// # Arguments
    ///
    /// * `typename` - The entity type name
    /// * `representations` - Entity representations with key field values
    /// * `connection_string` - Remote database connection URL
    /// * `selection` - Field selection from GraphQL query
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities)
    ///
    /// # Errors
    ///
    /// Returns error if connection fails or query execution fails
    ///
    /// # Implementation Note
    ///
    /// In Phase 6C, this method acquires a connection from the connection manager.
    /// In Phase 6D, it will execute queries using the remote connection.
    /// The actual resolution logic is deferred to the connection implementation.
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
        let _adapter = self
            .connection_manager
            .get_or_create_connection(config)
            .await?;

        // In Phase 6C, we have the adapter but execution is deferred to Phase 6D
        // This validates the connection can be created and cached for later use
        // Full implementation with DatabaseEntityResolver will come in Phase 6D

        // For now, return placeholder responses indicating direct DB resolution
        let mut results = Vec::with_capacity(representations.len());
        for rep in representations {
            let mut entity = rep.all_fields.clone();
            entity.insert(
                "__typename".to_string(),
                serde_json::Value::String(typename.to_string()),
            );
            entity.insert(
                "_direct_db".to_string(),
                serde_json::Value::Bool(true),
            );
            results.push(Some(serde_json::Value::Object(
                entity.into_iter().collect::<serde_json::Map<_, _>>(),
            )));
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
            types: vec![FederatedType {
                name: "User".to_string(),
                keys: vec![KeyDirective {
                    fields: vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends: false,
                external_fields: vec![],
                shareable_fields: vec![],
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
        assert!(resolver
            .close_connection("postgresql://localhost/db")
            .is_ok());
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
}
