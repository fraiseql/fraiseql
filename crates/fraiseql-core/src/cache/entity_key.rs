//! Type-safe entity keys for entity-level cache invalidation.
//!
//! An `EntityKey` represents a specific entity instance, combining:
//! - `entity_type`: The type of entity (e.g., "User", "Post", "Comment")
//! - `entity_id`: The unique identifier (UUID) of that instance
//!
//! Entity keys are used to track which queries depend on which specific entities,
//! enabling precise invalidation when those entities are modified.
//!
//! # Format
//!
//! Entity keys are serialized as: `"EntityType:uuid"`
//!
//! Example:
//! ```text
//! "User:550e8400-e29b-41d4-a716-446655440000"
//! "Post:e7d7a1a1-b2c3-4d5e-f6g7-h8i9j0k1l2m3"
//! ```
//!
//! # Examples
//!
//! ```ignore
//! use fraiseql_core::cache::entity_key::EntityKey;
//!
//! let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000")?;
//! assert_eq!(key.to_cache_key(), "User:550e8400-e29b-41d4-a716-446655440000");
//! ```

use crate::error::{FraiseQLError, Result};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Type-safe entity key for cache invalidation.
///
/// Combines entity type and ID into a single, hashable key for use in
/// dependency tracking and cache invalidation.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntityKey {
    /// Entity type (e.g., "User", "Post")
    pub entity_type: String,

    /// Entity ID (UUID)
    pub entity_id: String,
}

impl EntityKey {
    /// Create a new entity key with validation.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - The type of entity (must be non-empty)
    /// * `entity_id` - The entity's unique identifier (must be non-empty)
    ///
    /// # Returns
    ///
    /// - `Ok(EntityKey)` - If both arguments are valid
    /// - `Err(_)` - If either argument is empty
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000")?;
    /// ```
    pub fn new(entity_type: &str, entity_id: &str) -> Result<Self> {
        if entity_type.is_empty() {
            return Err(FraiseQLError::Validation {
                message: "entity_type cannot be empty".to_string(),
                path: None,
            });
        }

        if entity_id.is_empty() {
            return Err(FraiseQLError::Validation {
                message: "entity_id cannot be empty".to_string(),
                path: None,
            });
        }

        Ok(Self {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
        })
    }

    /// Convert entity key to cache key format: "EntityType:entity_id"
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000")?;
    /// assert_eq!(key.to_cache_key(), "User:550e8400-e29b-41d4-a716-446655440000");
    /// ```
    #[must_use]
    pub fn to_cache_key(&self) -> String {
        format!("{}:{}", self.entity_type, self.entity_id)
    }

    /// Parse entity key from cache key format: "EntityType:entity_id"
    ///
    /// # Arguments
    ///
    /// * `cache_key` - String in format "Type:id"
    ///
    /// # Returns
    ///
    /// - `Ok(EntityKey)` - If format is valid
    /// - `Err(_)` - If format is invalid
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let key = EntityKey::from_cache_key("User:550e8400-e29b-41d4-a716-446655440000")?;
    /// assert_eq!(key.entity_type, "User");
    /// ```
    pub fn from_cache_key(cache_key: &str) -> Result<Self> {
        let parts: Vec<&str> = cache_key.splitn(2, ':').collect();

        if parts.len() != 2 {
            return Err(FraiseQLError::Validation {
                message: format!("Invalid entity key format: {}. Expected 'Type:id'", cache_key),
                path: None,
            });
        }

        Self::new(parts[0], parts[1])
    }
}

impl fmt::Display for EntityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_cache_key())
    }
}

impl Hash for EntityKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.entity_type.hash(state);
        self.entity_id.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_valid_entity_key() {
        let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(key.entity_type, "User");
        assert_eq!(key.entity_id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_reject_empty_entity_type() {
        let result = EntityKey::new("", "550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_empty_entity_id() {
        let result = EntityKey::new("User", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_to_cache_key_format() {
        let key = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        let cache_key = key.to_cache_key();
        assert_eq!(cache_key, "User:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_deserialize_from_cache_key_format() {
        let cache_key = "User:550e8400-e29b-41d4-a716-446655440000";
        let key = EntityKey::from_cache_key(cache_key).unwrap();
        assert_eq!(key.entity_type, "User");
        assert_eq!(key.entity_id, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_hash_consistency_for_hashmap() {
        use std::collections::HashMap;

        let key1 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key2 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440000").unwrap();

        let mut map = HashMap::new();
        map.insert(key1.clone(), "value1");

        // Same key should retrieve same value
        assert_eq!(map.get(&key2), Some(&"value1"));

        // Different key should not match
        let key3 = EntityKey::new("User", "550e8400-e29b-41d4-a716-446655440001").unwrap();
        assert_eq!(map.get(&key3), None);
    }
}
