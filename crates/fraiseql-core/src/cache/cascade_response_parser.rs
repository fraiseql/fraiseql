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
//! ```ignore
//! use fraiseql_core::cache::CascadeResponseParser;
//! use serde_json::json;
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
    pub fn new(updated: Vec<EntityKey>, deleted: Vec<EntityKey>) -> Self {
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
    pub fn has_changes(&self) -> bool {
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
    pub fn new() -> Self {
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
    /// ```ignore
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
    /// ```
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
            for (_key, value) in data.as_object().unwrap_or(&Default::default()).iter() {
                if let Some(cascade) = value.get("cascade") {
                    return Ok(cascade.clone());
                }
            }
        }

        // Try top-level mutation response
        for (_key, value) in response.as_object().unwrap_or(&Default::default()).iter() {
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
                            _ => "unknown",
                        }
                    ),
                    path:    Some(format!("cascade.{}", field_name)),
                });
            },
        };

        let mut entities = Vec::new();

        for entity_obj in entities_array.iter() {
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
            path:    Some("cascade.updated[*]".to_string()),
        })?;

        // Extract __typename
        let type_name = obj.get("__typename").and_then(Value::as_str).ok_or_else(|| {
            FraiseQLError::Validation {
                message: "Cascade entity missing __typename field".to_string(),
                path:    Some("cascade.updated[*].__typename".to_string()),
            }
        })?;

        // Extract id
        let entity_id =
            obj.get("id").and_then(Value::as_str).ok_or_else(|| FraiseQLError::Validation {
                message: "Cascade entity missing id field".to_string(),
                path:    Some("cascade.updated[*].id".to_string()),
            })?;

        EntityKey::new(type_name, entity_id)
    }
}

impl Default for CascadeResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_simple_cascade_response() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "createPost": {
                "cascade": {
                    "updated": [
                        {
                            "__typename": "User",
                            "id": "550e8400-e29b-41d4-a716-446655440000",
                            "postCount": 5
                        }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
        assert_eq!(entities.updated[0].entity_type, "User");
        assert_eq!(entities.updated[0].entity_id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(entities.deleted.len(), 0);
    }

    #[test]
    fn test_parse_multiple_updated_entities() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "updateUser": {
                "cascade": {
                    "updated": [
                        { "__typename": "User", "id": "uuid-1" },
                        { "__typename": "Post", "id": "uuid-2" },
                        { "__typename": "Notification", "id": "uuid-3" }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 3);
        assert_eq!(entities.updated[0].entity_type, "User");
        assert_eq!(entities.updated[1].entity_type, "Post");
        assert_eq!(entities.updated[2].entity_type, "Notification");
    }

    #[test]
    fn test_parse_deleted_entities() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "deletePost": {
                "cascade": {
                    "deleted": [
                        { "__typename": "Post", "id": "post-uuid" },
                        { "__typename": "Comment", "id": "comment-uuid" }
                    ]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 0);
        assert_eq!(entities.deleted.len(), 2);
        assert_eq!(entities.deleted[0].entity_type, "Post");
        assert_eq!(entities.deleted[1].entity_type, "Comment");
    }

    #[test]
    fn test_parse_both_updated_and_deleted() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "__typename": "User", "id": "u-1" }],
                    "deleted": [{ "__typename": "Session", "id": "s-1" }]
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
        assert_eq!(entities.deleted.len(), 1);
        assert_eq!(entities.all_affected().len(), 2);
    }

    #[test]
    fn test_parse_empty_cascade() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [],
                    "deleted": []
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert!(!entities.has_changes());
        assert_eq!(entities.all_affected().len(), 0);
    }

    #[test]
    fn test_parse_no_cascade_field() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "createPost": {
                "post": { "id": "post-1", "title": "Hello" }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert!(!entities.has_changes());
    }

    #[test]
    fn test_parse_nested_in_data_field() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "data": {
                "createPost": {
                    "cascade": {
                        "updated": [{ "__typename": "User", "id": "uuid-1" }]
                    }
                }
            }
        });

        let entities = parser.parse_cascade_response(&response).unwrap();
        assert_eq!(entities.updated.len(), 1);
    }

    #[test]
    fn test_parse_missing_typename() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "id": "uuid-1" }]
                }
            }
        });

        let result = parser.parse_cascade_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_id() {
        let parser = CascadeResponseParser::new();
        let response = json!({
            "mutation": {
                "cascade": {
                    "updated": [{ "__typename": "User" }]
                }
            }
        });

        let result = parser.parse_cascade_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_cascade_entities_all_affected() {
        let updated = vec![
            EntityKey::new("User", "u-1").unwrap(),
            EntityKey::new("User", "u-2").unwrap(),
        ];
        let deleted = vec![EntityKey::new("Post", "p-1").unwrap()];

        let cascade = CascadeEntities::new(updated, deleted);
        let all = cascade.all_affected();
        assert_eq!(all.len(), 3);
    }
}
