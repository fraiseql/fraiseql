//! Auto-generated `_entities` resolver for Apollo Federation
//!
//! The `_entities` query is how the Apollo Gateway resolves entity references
//! across subgraphs. This module generates SQL queries that efficiently
//! resolve entities from CQRS query-side tables (tv_*).
//!
//! Performance targets:
//! - Single entity: < 2ms
//! - Batch (100 entities): < 50ms
//! - Uses GIN-indexed JSONB from CQRS views

use std::collections::HashMap;

/// Metadata about an entity type
#[derive(Debug, Clone)]
pub struct EntityMetadata {
    /// GraphQL type name (e.g., "User")
    pub type_name: String,

    /// Database table name (e.g., `tv_user`)
    pub table_name: String,

    /// Key field name (e.g., "id")
    pub key_field: String,

    /// Whether this entity uses CQRS (query-side table with JSONB)
    pub is_cqrs: bool,

    /// JSONB column name (typically "data")
    pub jsonb_column: Option<String>,

    /// Other fields available for selection
    pub fields: Vec<String>,
}

impl EntityMetadata {
    /// Create new entity metadata
    #[must_use]
    pub fn new(type_name: &str, table_name: &str, key_field: &str, is_cqrs: bool) -> Self {
        Self {
            type_name: type_name.to_string(),
            table_name: table_name.to_string(),
            key_field: key_field.to_string(),
            is_cqrs,
            jsonb_column: if is_cqrs {
                Some("data".to_string())
            } else {
                None
            },
            fields: Vec::new(),
        }
    }

    /// Set JSONB column name (for CQRS)
    #[must_use]
    pub fn with_jsonb_column(mut self, column: &str) -> Self {
        if self.is_cqrs {
            self.jsonb_column = Some(column.to_string());
        }
        self
    }
}

/// Builder for entity resolution queries
#[derive(Debug)]
pub struct EntityResolver {
    entities: HashMap<String, EntityMetadata>,
}

impl EntityResolver {
    /// Create new entity resolver
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    /// Register an entity type
    pub fn register(&mut self, metadata: EntityMetadata) {
        self.entities.insert(metadata.type_name.clone(), metadata);
    }

    /// Get metadata for entity type
    #[must_use]
    pub fn get(&self, type_name: &str) -> Option<&EntityMetadata> {
        self.entities.get(type_name)
    }

    /// Build SQL query for resolving a single entity
    ///
    /// For CQRS entities, returns the entire JSONB data column.
    /// For non-CQRS entities, selects specific fields.
    ///
    /// # Arguments
    ///
    /// * `type_name` - GraphQL type name
    /// * `key_value` - The key value to search for
    ///
    /// # Returns
    ///
    /// Tuple of (`query_string`, [`query_params`])
    ///
    /// # Errors
    ///
    /// Returns `EntityResolverError` if type is unknown
    pub fn build_single_query(
        &self,
        type_name: &str,
        key_value: &str,
    ) -> Result<(String, Vec<String>), EntityResolverError> {
        let entity = self
            .get(type_name)
            .ok_or_else(|| EntityResolverError::UnknownType(type_name.to_string()))?;

        let query = if entity.is_cqrs {
            // CQRS: SELECT JSONB data column directly
            let jsonb_col = entity.jsonb_column.as_deref().unwrap_or("data");
            format!(
                "SELECT {} FROM {} WHERE {} = $1",
                jsonb_col, entity.table_name, entity.key_field
            )
        } else {
            // Non-CQRS: SELECT all fields (or specified fields)
            let fields = if entity.fields.is_empty() {
                "*".to_string()
            } else {
                entity.fields.join(", ")
            };
            format!(
                "SELECT {} FROM {} WHERE {} = $1",
                fields, entity.table_name, entity.key_field
            )
        };

        Ok((query, vec![key_value.to_string()]))
    }

    /// Build SQL query for resolving multiple entities (batch loading)
    ///
    /// Combines all entities into a single query using IN clause.
    /// For optimal performance, batches should group by type first.
    ///
    /// # Arguments
    ///
    /// * `type_name` - GraphQL type name
    /// * `key_values` - Multiple key values
    ///
    /// # Returns
    ///
    /// Tuple of (`query_string`, [`query_params`])
    ///
    /// # Errors
    ///
    /// Returns `EntityResolverError` if type is unknown or batch is empty
    pub fn build_batch_query(
        &self,
        type_name: &str,
        key_values: &[String],
    ) -> Result<(String, Vec<String>), EntityResolverError> {
        if key_values.is_empty() {
            return Err(EntityResolverError::EmptyBatch);
        }

        let entity = self
            .get(type_name)
            .ok_or_else(|| EntityResolverError::UnknownType(type_name.to_string()))?;

        // Build placeholders: $1, $2, $3, ...
        let placeholders = (1..=key_values.len())
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", ");

        let query = if entity.is_cqrs {
            // CQRS: SELECT JSONB with IN clause
            let jsonb_col = entity.jsonb_column.as_deref().unwrap_or("data");
            format!(
                "SELECT {} FROM {} WHERE {} IN ({})",
                jsonb_col, entity.table_name, entity.key_field, placeholders
            )
        } else {
            // Non-CQRS: SELECT fields with IN clause
            let fields = if entity.fields.is_empty() {
                "*".to_string()
            } else {
                entity.fields.join(", ")
            };
            format!(
                "SELECT {} FROM {} WHERE {} IN ({})",
                fields, entity.table_name, entity.key_field, placeholders
            )
        };

        Ok((query, key_values.to_vec()))
    }

    /// Build optimized query for multiple types and keys
    ///
    /// Groups representations by type to minimize queries.
    ///
    /// # Arguments
    ///
    /// * `representations` - List of {__typename, `key_field`: value}
    ///
    /// # Returns
    ///
    /// Vec of (query, params) tuples
    ///
    /// # Errors
    ///
    /// Returns `EntityResolverError` if any type is unknown or batch is empty
    pub fn build_batch_multi_type_queries(
        &self,
        representations: &[HashMap<String, String>],
    ) -> Result<Vec<(String, Vec<String>)>, EntityResolverError> {
        // Group by type
        let mut by_type: HashMap<String, Vec<String>> = HashMap::new();

        for rep in representations {
            let type_name = rep
                .get("__typename")
                .ok_or(EntityResolverError::MissingTypeName)?;

            let entity = self
                .get(type_name)
                .ok_or_else(|| EntityResolverError::UnknownType(type_name.clone()))?;

            let key_value =
                rep.get(&entity.key_field)
                    .ok_or_else(|| EntityResolverError::MissingKeyField {
                        type_name: type_name.clone(),
                        key_field: entity.key_field.clone(),
                    })?;

            by_type
                .entry(type_name.clone())
                .or_default()
                .push(key_value.clone());
        }

        // Build queries
        let mut queries = Vec::new();
        for (type_name, keys) in by_type {
            let (query, params) = self.build_batch_query(&type_name, &keys)?;
            queries.push((query, params));
        }

        Ok(queries)
    }

    /// Get the key field name for an entity type
    ///
    /// # Errors
    ///
    /// Returns `EntityResolverError` if type is unknown
    pub fn get_key_field(&self, type_name: &str) -> Result<String, EntityResolverError> {
        self.get(type_name)
            .map(|e| e.key_field.clone())
            .ok_or_else(|| EntityResolverError::UnknownType(type_name.to_string()))
    }
}

impl Default for EntityResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during entity resolution
#[derive(Debug, Clone)]
pub enum EntityResolverError {
    /// Unknown entity type
    UnknownType(String),

    /// Empty batch request
    EmptyBatch,

    /// Missing __typename in representation
    MissingTypeName,

    /// Missing key field in representation
    MissingKeyField {
        /// Type name
        type_name: String,
        /// Key field name
        key_field: String,
    },

    /// Database error
    DatabaseError(String),
}

impl std::fmt::Display for EntityResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownType(t) => write!(f, "Unknown entity type: {t}"),
            Self::EmptyBatch => write!(f, "Empty batch request"),
            Self::MissingTypeName => write!(f, "Missing __typename in representation"),
            Self::MissingKeyField {
                type_name,
                key_field,
            } => {
                write!(
                    f,
                    "Missing key field '{key_field}' in {type_name} representation"
                )
            }
            Self::DatabaseError(msg) => write!(f, "Database error: {msg}"),
        }
    }
}

impl std::error::Error for EntityResolverError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_resolver() -> EntityResolver {
        let mut resolver = EntityResolver::new();

        // Register User entity
        resolver
            .register(EntityMetadata::new("User", "tv_user", "id", true).with_jsonb_column("data"));

        // Register Post entity
        resolver
            .register(EntityMetadata::new("Post", "tv_post", "id", true).with_jsonb_column("data"));

        resolver
    }

    #[test]
    fn test_build_single_query_cqrs() {
        let resolver = create_resolver();
        let (query, params) = resolver
            .build_single_query("User", "550e8400-e29b-41d4-a716-446655440000")
            .unwrap();

        assert_eq!(query, "SELECT data FROM tv_user WHERE id = $1");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_build_batch_query_cqrs() {
        let resolver = create_resolver();
        let keys = vec![
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "660f8401-f30c-51e5-b817-557766551111".to_string(),
            "770g8502-g41d-62f6-c818-668877662222".to_string(),
        ];

        let (query, params) = resolver.build_batch_query("User", &keys).unwrap();

        assert_eq!(query, "SELECT data FROM tv_user WHERE id IN ($1, $2, $3)");
        assert_eq!(params.len(), 3);
        assert_eq!(params, keys);
    }

    #[test]
    fn test_build_batch_query_empty() {
        let resolver = create_resolver();
        let result = resolver.build_batch_query("User", &[]);

        assert!(result.is_err());
        match result {
            Err(EntityResolverError::EmptyBatch) => {}
            _ => panic!("Expected EmptyBatch error"),
        }
    }

    #[test]
    fn test_unknown_type_error() {
        let resolver = create_resolver();
        let result = resolver.build_single_query("UnknownType", "123");

        assert!(result.is_err());
        match result {
            Err(EntityResolverError::UnknownType(t)) => assert_eq!(t, "UnknownType"),
            _ => panic!("Expected UnknownType error"),
        }
    }

    #[test]
    fn test_multi_type_batch_query() {
        let resolver = create_resolver();

        let mut rep1 = HashMap::new();
        rep1.insert("__typename".to_string(), "User".to_string());
        rep1.insert("id".to_string(), "user-1".to_string());

        let mut rep2 = HashMap::new();
        rep2.insert("__typename".to_string(), "User".to_string());
        rep2.insert("id".to_string(), "user-2".to_string());

        let mut rep3 = HashMap::new();
        rep3.insert("__typename".to_string(), "Post".to_string());
        rep3.insert("id".to_string(), "post-1".to_string());

        let queries = resolver
            .build_batch_multi_type_queries(&[rep1, rep2, rep3])
            .unwrap();

        // Should have 2 queries: one for User (2 keys), one for Post (1 key)
        assert_eq!(queries.len(), 2);

        // Find queries
        let user_query = queries.iter().find(|(q, _)| q.contains("tv_user")).unwrap();
        let post_query = queries.iter().find(|(q, _)| q.contains("tv_post")).unwrap();

        // Check User query
        assert_eq!(
            user_query.0,
            "SELECT data FROM tv_user WHERE id IN ($1, $2)"
        );
        assert_eq!(user_query.1.len(), 2);

        // Check Post query
        assert_eq!(post_query.0, "SELECT data FROM tv_post WHERE id IN ($1)");
        assert_eq!(post_query.1.len(), 1);
    }

    #[test]
    fn test_get_key_field() {
        let resolver = create_resolver();
        let key = resolver.get_key_field("User").unwrap();
        assert_eq!(key, "id");
    }
}
