//! UUID extraction from mutation responses for entity-level cache invalidation.
//!
//! This module extracts entity UUIDs from GraphQL mutation response objects,
//! enabling precise, entity-level cache invalidation instead of view-level invalidation.
//!
//! # Architecture
//!
//! ```text
//! Mutation Response
//! ┌──────────────────────────────────┐
//! │ {                                │
//! │   "id": "550e8400-e29b-...",     │
//! │   "name": "Alice",               │
//! │   "created_at": "2025-01-16"     │
//! │ }                                │
//! └──────────┬───────────────────────┘
//!            │
//!            ↓ extract_entity_uuid()
//! ┌──────────────────────────────────┐
//! │ "550e8400-e29b-41d4-..."         │
//! └──────────────────────────────────┘
//! ```
//!
//! # UUID Format Support
//!
//! - **UUID v4**: Standard format (RFC 4122)
//! - **UUID v1**: Timestamp-based
//! - **Custom UUIDs**: Any string matching UUID regex
//!
//! # Examples
//!
//! ```
//! use fraiseql_core::cache::UUIDExtractor;
//! use serde_json::json;
//!
//! let response = json!({
//!     "id": "550e8400-e29b-41d4-a716-446655440000",
//!     "name": "Alice"
//! });
//!
//! let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
//! assert_eq!(uuid, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
//! ```

use crate::error::Result;
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// UUID v4 format regex (RFC 4122).
/// Matches: 550e8400-e29b-41d4-a716-446655440000
fn uuid_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
        ).expect("UUID regex is valid")
    })
}

/// Extracts entity UUIDs from mutation response objects.
///
/// Handles various response formats:
/// - Simple: `{ "id": "uuid", "name": "..." }`
/// - Nested: `{ "user": { "id": "uuid", "name": "..." } }`
/// - Array: `[{ "id": "uuid1" }, { "id": "uuid2" }]`
#[derive(Debug, Clone)]
pub struct UUIDExtractor;

impl UUIDExtractor {
    /// Extract a single entity UUID from mutation response.
    ///
    /// # Arguments
    ///
    /// * `response` - JSON response from mutation
    /// * `entity_type` - The entity type (e.g., "User", "Post")
    ///
    /// # Returns
    ///
    /// - `Ok(Some(uuid))` - UUID found and valid
    /// - `Ok(None)` - No UUID found (e.g., null response)
    /// - `Err(_)` - Invalid UUID format
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::cache::UUIDExtractor;
    /// use serde_json::json;
    ///
    /// let response = json!({"id": "550e8400-e29b-41d4-a716-446655440000"});
    /// let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
    /// assert_eq!(uuid, Some("550e8400-e29b-41d4-a716-446655440000".to_string()));
    /// ```
    pub fn extract_entity_uuid(
        response: &Value,
        _entity_type: &str,
    ) -> Result<Option<String>> {
        match response {
            Value::Null => Ok(None),
            Value::Object(obj) => {
                // Try to find "id" field at top level
                if let Some(id_value) = obj.get("id") {
                    return extract_uuid_from_value(id_value);
                }

                // If not found at top level, try nested structure
                // (e.g., { user: { id: "uuid" } })
                for (_key, value) in obj.iter() {
                    if let Value::Object(nested) = value {
                        if let Some(id_value) = nested.get("id") {
                            return extract_uuid_from_value(id_value);
                        }
                    }
                }

                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Extract multiple entity UUIDs from mutation response (batch operations).
    ///
    /// # Arguments
    ///
    /// * `response` - JSON response (array or object)
    /// * `entity_type` - The entity type (e.g., "User", "Post")
    ///
    /// # Returns
    ///
    /// List of extracted UUIDs (empty if none found)
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::cache::UUIDExtractor;
    /// use serde_json::json;
    ///
    /// let response = json!([
    ///     {"id": "550e8400-e29b-41d4-a716-446655440001"},
    ///     {"id": "550e8400-e29b-41d4-a716-446655440002"},
    ///     {"id": "550e8400-e29b-41d4-a716-446655440003"}
    /// ]);
    /// let uuids = UUIDExtractor::extract_batch_uuids(&response, "User").unwrap();
    /// assert_eq!(uuids.len(), 3);
    /// ```
    pub fn extract_batch_uuids(
        response: &Value,
        _entity_type: &str,
    ) -> Result<Vec<String>> {
        match response {
            Value::Array(arr) => {
                let mut uuids = Vec::new();
                for item in arr {
                    if let Ok(Some(uuid)) = extract_uuid_from_value(item) {
                        uuids.push(uuid);
                    }
                }
                Ok(uuids)
            }
            Value::Object(_) => {
                // Single object - try to extract single UUID
                match Self::extract_entity_uuid(response, "")? {
                    Some(uuid) => Ok(vec![uuid]),
                    None => Ok(vec![]),
                }
            }
            Value::Null => Ok(vec![]),
            _ => Ok(vec![]),
        }
    }

    /// Validate if a string is a valid UUID format.
    ///
    /// # Arguments
    ///
    /// * `id` - String to validate
    ///
    /// # Returns
    ///
    /// `true` if valid UUID format, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_core::cache::UUIDExtractor;
    ///
    /// assert!(UUIDExtractor::is_valid_uuid("550e8400-e29b-41d4-a716-446655440000"));
    /// assert!(!UUIDExtractor::is_valid_uuid("not-a-uuid"));
    /// ```
    #[must_use]
    pub fn is_valid_uuid(id: &str) -> bool {
        uuid_regex().is_match(&id.to_lowercase())
    }
}

/// Helper function to extract UUID from a JSON value.
fn extract_uuid_from_value(value: &Value) -> Result<Option<String>> {
    match value {
        Value::String(s) => {
            if UUIDExtractor::is_valid_uuid(s) {
                Ok(Some(s.to_lowercase()))
            } else {
                // String value that's not a UUID - could be a valid use case
                // (e.g., response mutation returns string ID that isn't UUID format)
                Ok(None)
            }
        }
        Value::Object(obj) => {
            // Try to recursively find id in nested object
            if let Some(id_val) = obj.get("id") {
                return extract_uuid_from_value(id_val);
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_single_uuid_from_response() {
        let response = json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Alice"
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(
            uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn test_extract_uuid_from_nested_response() {
        let response = json!({
            "user": {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "name": "Alice"
            }
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(
            uuid,
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn test_extract_uuid_from_null_response() {
        let response = Value::Null;

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, None);
    }

    #[test]
    fn test_extract_batch_uuids_from_array() {
        let response = json!([
            {"id": "550e8400-e29b-41d4-a716-446655440000"},
            {"id": "550e8400-e29b-41d4-a716-446655440001"},
            {"id": "550e8400-e29b-41d4-a716-446655440002"}
        ]);

        let uuids = UUIDExtractor::extract_batch_uuids(&response, "User").unwrap();
        assert_eq!(uuids.len(), 3);
        assert!(uuids.contains(&"550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[test]
    fn test_is_valid_uuid() {
        assert!(UUIDExtractor::is_valid_uuid(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(UUIDExtractor::is_valid_uuid(
            "550E8400-E29B-41D4-A716-446655440000"
        ));
        assert!(!UUIDExtractor::is_valid_uuid("not-a-uuid"));
        assert!(!UUIDExtractor::is_valid_uuid("550e8400"));
    }

    #[test]
    fn test_skip_non_uuid_id_fields() {
        let response = json!({
            "id": "some-string-id",
            "name": "Alice"
        });

        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        // Non-UUID id field should not be extracted
        assert_eq!(uuid, None);
    }

    #[test]
    fn test_batch_mutations_multiple_entities() {
        let response = json!([
            {"id": "550e8400-e29b-41d4-a716-446655440000", "name": "Alice"},
            {"id": "550e8400-e29b-41d4-a716-446655440001", "name": "Bob"}
        ]);

        let uuids = UUIDExtractor::extract_batch_uuids(&response, "User").unwrap();
        assert_eq!(uuids.len(), 2);
    }

    #[test]
    fn test_error_cases_invalid_format() {
        let response = json!({"id": 12345});
        let uuid = UUIDExtractor::extract_entity_uuid(&response, "User").unwrap();
        assert_eq!(uuid, None);
    }
}
