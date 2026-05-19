//! Parser for GraphQL Cascade responses to extract entity invalidation data.
//!
//! This module parses GraphQL mutation responses following the GraphQL Cascade specification,
//! extracting all affected entities (updated and deleted) to enable entity-level cache
//! invalidation.
//!
//! # Architecture
//!
//! ```text
//! GraphQL Mutation Response
//! ┌──────────────────────────────────┐
//! │ {                                │
//! │   "createPost": {                │
//! │     "post": { ... },             │
//! │     "cascade": {                 │
//! │       "updated": [               │
//! │         {                        │
//! │           "__typename": "User",  │
//! │           "id": "uuid-123",      │
//! │           ...                    │
//! │         }                        │
//! │       ],                         │
//! │       "deleted": [ ... ]         │
//! │     }                            │
//! │   }                              │
//! │ }                                │
//! └──────────────────────────────────┘
//!            │
//!            ↓ parse_cascade_response()
//! ┌──────────────────────────────────┐
//! │ CascadeEntities:                 │
//! │ updated: [                       │
//! │   EntityKey("User", "uuid-123")  │
//! │ ]                                │
//! │ deleted: []                      │
//! └──────────────────────────────────┘
//! ```
//!
//! # Examples
//!
//! ```rust
//! use fraiseql_core::cache::CascadeResponseParser;
//! use serde_json::json;
//! # use fraiseql_core::error::Result;
//! # fn example() -> Result<()> {
//!
//! let parser = CascadeResponseParser::new();
//!
//! let response = json!({
//!   "createPost": {
//!     "cascade": {
//!       "updated": [
//!         { "__typename": "User", "id": "550e8400-e29b-41d4-a716-446655440000" }
//!       ]
//!     }
//!   }
//! });
//!
//! let entities = parser.parse_cascade_response(&response)?;
//! assert_eq!(entities.updated.len(), 1);
//! assert_eq!(entities.updated[0].entity_type, "User");
//! # Ok(())
//! # }
//! ```

use serde_json::Value;

use super::entity_key::EntityKey;
use crate::error::{FraiseQLError, Result};

/// Cascade entities extracted from a GraphQL mutation response.
///
/// Represents all entities affected by a mutation (both updated and deleted),
/// used to determine which caches need invalidation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CascadeEntities {
    /// Updated entities - entries that were modified or created
    pub updated: Vec<EntityKey>,

    /// Deleted entities - entries that were removed
    pub deleted: Vec<EntityKey>,
}

impl CascadeEntities {
    /// Create new cascade entities with separate updated and deleted lists.
    #[must_use]
    pub const fn new(updated: Vec<EntityKey>, deleted: Vec<EntityKey>) -> Self {
        Self { updated, deleted }
    }

    /// Get all affected entities (both updated and deleted).
    #[must_use]
    pub fn all_affected(&self) -> Vec<EntityKey> {
        let mut all = self.updated.clone();
        all.extend(self.deleted.clone());
        all
    }

    /// Check if cascade has any affected entities.
    #[must_use]
    pub const fn has_changes(&self) -> bool {
        !self.updated.is_empty() || !self.deleted.is_empty()
    }
}

/// Parser for GraphQL Cascade responses following the Cascade specification v1.1.
///
/// Extracts all affected entities from mutation responses to enable
/// entity-level cache invalidation.
#[derive(Debug, Clone)]
pub struct CascadeResponseParser;

impl CascadeResponseParser {
    /// Create new cascade response parser.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Parse cascade data from a GraphQL mutation response.
    ///
    /// # Arguments
    ///
    /// * `response` - The full GraphQL response containing cascade field
    ///
    /// # Returns
    ///
    /// `CascadeEntities` with all updated and deleted entities
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_core::cache::CascadeResponseParser;
    /// use serde_json::json;
    /// # use fraiseql_core::error::Result;
    /// # fn example() -> Result<()> {
    /// let parser = CascadeResponseParser::new();
    /// let response = json!({
    ///   "createPost": {
    ///     "cascade": {
    ///       "updated": [
    ///         { "__typename": "User", "id": "uuid-123" }
    ///       ]
    ///     }
    ///   }
    /// });
    ///
    /// let entities = parser.parse_cascade_response(&response)?;
    /// assert_eq!(entities.updated.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the cascade field is present but
    /// malformed (e.g., `updated` or `deleted` is not an array, or an entity is
    /// missing `__typename` / `id`).
    pub fn parse_cascade_response(&self, response: &Value) -> Result<CascadeEntities> {
        // Find cascade field in response
        let cascade = self.find_cascade_field(response)?;

        if cascade.is_null() {
            // No cascade data - return empty
            return Ok(CascadeEntities {
                updated: Vec::new(),
                deleted: Vec::new(),
            });
        }

        // Extract updated entities
        let updated = self.extract_entities_list(&cascade, "updated")?;

        // Extract deleted entities
        let deleted = self.extract_entities_list(&cascade, "deleted")?;

        Ok(CascadeEntities { updated, deleted })
    }

    /// Find cascade field in nested response structure.
    ///
    /// Cascade field can be at various depths:
    /// - response.mutation { cascade { ... } }
    /// - response.data.mutation { cascade { ... } }
    /// - etc.
    fn find_cascade_field(&self, response: &Value) -> Result<Value> {
        // Try direct cascade field
        if let Some(cascade) = response.get("cascade") {
            return Ok(cascade.clone());
        }

        // Try nested in data
        if let Some(data) = response.get("data") {
            if let Some(cascade) = data.get("cascade") {
                return Ok(cascade.clone());
            }

            // Try deeper nesting (mutation result contains cascade)
            for (_key, value) in data.as_object().unwrap_or(&serde_json::Map::default()) {
                if let Some(cascade) = value.get("cascade") {
                    return Ok(cascade.clone());
                }
            }
        }

        // Try top-level mutation response
        for (_key, value) in response.as_object().unwrap_or(&serde_json::Map::default()) {
            if let Some(cascade) = value.get("cascade") {
                return Ok(cascade.clone());
            }
        }

        // No cascade field found - return null (valid case: no side effects)
        Ok(Value::Null)
    }

    /// Extract list of entities from cascade.updated or cascade.deleted.
    fn extract_entities_list(&self, cascade: &Value, field_name: &str) -> Result<Vec<EntityKey>> {
        let entities_array = match cascade.get(field_name) {
            Some(Value::Array(arr)) => arr,
            Some(Value::Null) | None => return Ok(Vec::new()),
            Some(val) => {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "cascade.{} should be array, got {}",
                        field_name,
                        match val {
                            Value::Object(_) => "object",
                            Value::String(_) => "string",
                            Value::Number(_) => "number",
                            Value::Bool(_) => "boolean",
                            Value::Null => "null",
                            Value::Array(_) => "unknown",
                        }
                    ),
                    path: Some(format!("cascade.{}", field_name)),
                });
            },
        };

        let mut entities = Vec::new();

        for entity_obj in entities_array {
            let entity = self.parse_cascade_entity(entity_obj)?;
            entities.push(entity);
        }

        Ok(entities)
    }

    /// Parse a single entity from cascade.updated or cascade.deleted.
    ///
    /// Expects object with `__typename` and `id` fields.
    fn parse_cascade_entity(&self, entity_obj: &Value) -> Result<EntityKey> {
        let obj = entity_obj.as_object().ok_or_else(|| FraiseQLError::Validation {
            message: "Cascade entity should be object".to_string(),
            path: Some("cascade.updated[*]".to_string()),
        })?;

        // Extract __typename
        let type_name = obj.get("__typename").and_then(Value::as_str).ok_or_else(|| {
            FraiseQLError::Validation {
                message: "Cascade entity missing __typename field".to_string(),
                path: Some("cascade.updated[*].__typename".to_string()),
            }
        })?;

        // Extract id
        let entity_id =
            obj.get("id").and_then(Value::as_str).ok_or_else(|| FraiseQLError::Validation {
                message: "Cascade entity missing id field".to_string(),
                path: Some("cascade.updated[*].id".to_string()),
            })?;

        EntityKey::new(type_name, entity_id)
    }
}

impl Default for CascadeResponseParser {
    fn default() -> Self {
        Self::new()
    }
}
