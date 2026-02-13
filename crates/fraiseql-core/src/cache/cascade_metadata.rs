//! Cascade metadata for mapping mutations to entity types.
//!
//! This module builds a mapping from mutation names to the entity types they modify,
//! extracted from the compiled schema. This enables tracking which entities are affected
//! by each mutation, critical for entity-level cache invalidation.
//!
//! # Architecture
//!
//! ```text
//! Compiled Schema
//! ┌──────────────────────────────────┐
//! │ mutations:                       │
//! │  - createUser: { return: User }  │
//! │  - updatePost: { return: Post }  │
//! └──────────┬───────────────────────┘
//!            │
//!            ↓ build_from_schema()
//! ┌──────────────────────────────────┐
//! │ CascadeMetadata:                 │
//! │  "createUser" → "User"           │
//! │  "updatePost" → "Post"           │
//! └──────────────────────────────────┘
//! ```
//!
//! # Examples
//!
//! ```ignore
//! use fraiseql_core::cache::cascade_metadata::CascadeMetadata;
//! use fraiseql_core::schema::CompiledSchema;
//!
//! let schema = CompiledSchema::from_file("schema.json")?;
//! let metadata = CascadeMetadata::from_schema(&schema);
//!
//! assert_eq!(metadata.get_entity_type("createUser"), Some("User"));
//! assert_eq!(metadata.get_entity_type("updatePost"), Some("Post"));
//! ```

use std::collections::HashMap;

#[cfg(test)]
use crate::schema::CompiledSchema;

/// Maps mutation names to the entity types they modify.
///
/// Built from compiled schema, this metadata enables determining which entities
/// are affected by each mutation operation.
#[derive(Debug, Clone)]
pub struct CascadeMetadata {
    /// Mutation name → Entity type mapping
    ///
    /// Example: { "createUser": "User", "updatePost": "Post" }
    mutation_entity_map: HashMap<String, String>,

    /// Entity type → List of mutations affecting it
    /// Useful for reverse lookups (which mutations affect "User"?)
    entity_mutations_map: HashMap<String, Vec<String>>,
}

impl CascadeMetadata {
    /// Create empty cascade metadata.
    ///
    /// Useful when building metadata programmatically or in tests.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mutation_entity_map:  HashMap::new(),
            entity_mutations_map: HashMap::new(),
        }
    }

    /// Add a mutation-to-entity mapping.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - Name of the mutation (e.g., "createUser")
    /// * `entity_type` - Type of entity it modifies (e.g., "User")
    pub fn add_mutation(&mut self, mutation_name: &str, entity_type: &str) {
        let mutation_name = mutation_name.to_string();
        let entity_type = entity_type.to_string();

        self.mutation_entity_map.insert(mutation_name.clone(), entity_type.clone());

        self.entity_mutations_map
            .entry(entity_type)
            .or_insert_with(Vec::new)
            .push(mutation_name);
    }

    /// Get the entity type modified by a mutation.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - Name of the mutation
    ///
    /// # Returns
    ///
    /// - `Some(&str)` - Entity type if mutation is known
    /// - `None` - If mutation is not in schema
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let entity = metadata.get_entity_type("createUser");
    /// assert_eq!(entity, Some("User"));
    /// ```
    #[must_use]
    pub fn get_entity_type(&self, mutation_name: &str) -> Option<&str> {
        self.mutation_entity_map.get(mutation_name).map(|s| s.as_str())
    }

    /// Get all mutations affecting a specific entity type.
    ///
    /// Useful for finding all caches that might be affected by changes to an entity type.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - Type of entity to query
    ///
    /// # Returns
    ///
    /// List of mutation names affecting this entity, or empty list if none
    #[must_use]
    pub fn get_mutations_for_entity(&self, entity_type: &str) -> Vec<String> {
        self.entity_mutations_map.get(entity_type).cloned().unwrap_or_default()
    }

    /// Get total number of mutation-entity mappings.
    #[must_use]
    pub fn count(&self) -> usize {
        self.mutation_entity_map.len()
    }

    /// Check if metadata contains a mutation.
    #[must_use]
    pub fn contains_mutation(&self, mutation_name: &str) -> bool {
        self.mutation_entity_map.contains_key(mutation_name)
    }

    #[cfg(test)]
    /// Build metadata from a compiled schema (for testing).
    ///
    /// In production, this would be called during server initialization
    /// to extract all mutations and their return types from the compiled schema.
    pub fn from_schema(_schema: &CompiledSchema) -> Self {
        // In a real implementation, this would:
        // 1. Extract mutations from schema.mutations()
        // 2. For each mutation, extract its return_type
        // 3. Map return_type to entity name
        //
        // For now, return empty - tests will build metadata manually
        Self::new()
    }
}

impl Default for CascadeMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_from_mutations() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("updateUser", "User");
        metadata.add_mutation("deleteUser", "User");

        assert_eq!(metadata.count(), 3);
    }

    #[test]
    fn test_map_mutation_to_entity_type() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("createPost", "Post");

        assert_eq!(metadata.get_entity_type("createUser"), Some("User"));
        assert_eq!(metadata.get_entity_type("createPost"), Some("Post"));
    }

    #[test]
    fn test_handle_unknown_mutation() {
        let metadata = CascadeMetadata::new();
        assert_eq!(metadata.get_entity_type("unknownMutation"), None);
    }

    #[test]
    fn test_multiple_mutations_same_entity() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");
        metadata.add_mutation("updateUser", "User");
        metadata.add_mutation("deleteUser", "User");

        let mutations = metadata.get_mutations_for_entity("User");
        assert_eq!(mutations.len(), 3);
        assert!(mutations.contains(&"createUser".to_string()));
        assert!(mutations.contains(&"updateUser".to_string()));
        assert!(mutations.contains(&"deleteUser".to_string()));
    }

    #[test]
    fn test_contains_mutation() {
        let mut metadata = CascadeMetadata::new();
        metadata.add_mutation("createUser", "User");

        assert!(metadata.contains_mutation("createUser"));
        assert!(!metadata.contains_mutation("unknownMutation"));
    }
}
