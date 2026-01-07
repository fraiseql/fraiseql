//! Mutation response format detection and parsing
//!
//! This module provides automatic detection and parsing of `PostgreSQL` mutation response
//! formats. It supports two distinct response patterns, each suitable for different
//! mutation scenarios:
//!
//! # Format Auto-Detection
//!
//! The format is determined by the presence of a valid `status` field:
//!
//! | Condition | Format |
//! |-----------|--------|
//! | Has `status` field with valid value | Full |
//! | No `status` field OR invalid value | Simple |
//!
//! Valid status values:
//! - Success: `"success"`, `"created"`, `"updated"`, `"deleted"`, etc.
//! - Error: `"failed:reason"`, `"not_found:..."`, `"unauthorized:..."`, etc.
//! - No-op: `"noop:reason"`
//!
//! # Full Format
//!
//! Used when the response includes mutation metadata:
//!
//! ```json
//! {
//!     "status": "created",
//!     "message": "User created successfully",
//!     "entity_type": "User",
//!     "entity": { "id": "123", "name": "Alice" },
//!     "updated_fields": ["name", "email"],
//!     "cascade": { "updated": [...] },
//!     "metadata": { "timing_ms": 25 }
//! }
//! ```
//!
//! Required fields: `status`, `message`
//! Optional fields: `entity`, `entity_type`, `updated_fields`, `cascade`, `metadata`
//!
//! # Simple Format
//!
//! Used for direct entity returns without mutation metadata:
//!
//! ```json
//! {
//!     "id": "123",
//!     "name": "Alice",
//!     "email": "alice@example.com"
//! }
//! ```
//!
//! The entire JSON value becomes the entity data.
//!
//! # Backward Compatibility
//!
//! - Cascade data supports both `"cascade"` and `"_cascade"` field names
//! - Missing `message` field defaults to empty string in full format
//! - Invalid `status` values trigger fallback to simple format
//!
//! # Examples
//!
//! ## Detecting Format
//!
//! ```rust,ignore
//! // Full format (has valid status)
//! let json = r#"{"status": "success", "message": "OK"}"#;
//! let response = parse_mutation_response(json, None)?;
//! assert!(matches!(response, MutationResponse::Full(_)));
//!
//! // Simple format (no status or invalid status)
//! let json = r#"{"id": "123", "name": "Test"}"#;
//! let response = parse_mutation_response(json, None)?;
//! assert!(matches!(response, MutationResponse::Simple(_)));
//! ```
//!
//! # Use in Response Building
//!
//! The parser is used by the response builder to:
//! 1. Detect mutation response format
//! 2. Extract and validate required fields
//! 3. Prepare data for GraphQL schema mapping
//! 4. Handle entity and cascade field transformations

use crate::mutation::types::{FullResponse, MutationError, MutationResponse, SimpleResponse};
use serde_json::Value;

/// Parse JSONB string into `MutationResponse`
///
/// Automatically detects format:
/// - Full: Has valid status field
/// - Simple: No status field OR invalid status value
///
/// # Errors
///
/// Returns an error if:
/// - JSON string is invalid or malformed
/// - Full format parsing fails (missing required fields)
/// - Simple format parsing fails (invalid structure)
pub fn parse_mutation_response(
    json_str: &str,
    default_entity_type: Option<&str>,
) -> Result<MutationResponse, crate::mutation::types::MutationError> {
    // Parse JSON
    let value: Value =
        serde_json::from_str(json_str).map_err(|e| MutationError::InvalidJson(e.to_string()))?;

    // Detect format
    if is_full_format(&value) {
        parse_full(&value, default_entity_type).map(MutationResponse::Full)
    } else {
        Ok(MutationResponse::Simple(parse_simple(value)))
    }
}

/// Check if value is full format (has valid status field)
fn is_full_format(value: &Value) -> bool {
    value
        .get("status")
        .and_then(|s| s.as_str())
        .is_some_and(is_valid_mutation_status)
}

/// Check if status string is a valid mutation status
fn is_valid_mutation_status(status: &str) -> bool {
    const VALID_PREFIXES: &[&str] = &[
        "success",
        "created",
        "updated",
        "deleted",
        "completed",
        "ok",
        "new",
        "failed:",
        "unauthorized:",
        "forbidden:",
        "not_found:",
        "conflict:",
        "timeout:",
        "noop:",
    ];

    let status_lower = status.to_lowercase();
    VALID_PREFIXES
        .iter()
        .any(|prefix| status_lower == *prefix || status_lower.starts_with(prefix))
}

/// Parse simple format (entity only)
const fn parse_simple(value: Value) -> SimpleResponse {
    SimpleResponse { entity: value }
}

/// Parse full mutation response format
fn parse_full(
    value: &Value,
    default_entity_type: Option<&str>,
) -> Result<FullResponse, crate::mutation::types::MutationError> {
    // Required fields
    let status = value
        .get("status")
        .and_then(|s| s.as_str())
        .ok_or_else(|| MutationError::MissingField("status".to_string()))?
        .to_string();

    let message = value
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // Optional fields
    let entity_type = value
        .get("entity_type")
        .and_then(|t| t.as_str())
        .map(String::from)
        .or_else(|| default_entity_type.map(String::from));

    let entity = value.get("entity").filter(|e| !e.is_null()).cloned();

    let updated_fields = value
        .get("updated_fields")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    // CASCADE: support both "cascade" and "_cascade" (backward compat)
    let cascade = value
        .get("cascade")
        .or_else(|| value.get("_cascade"))
        .filter(|c| !c.is_null())
        .cloned();

    let metadata = value.get("metadata").filter(|m| !m.is_null()).cloned();

    Ok(FullResponse {
        status,
        message,
        entity_type,
        entity,
        updated_fields,
        cascade,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_simple_format() {
        let json = r#"{"id": "123", "name": "Test"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Simple(_)));
    }

    #[test]
    fn test_detect_full_format() {
        let json = r#"{"status": "success", "message": "OK"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Full(_)));
    }

    #[test]
    fn test_parse_simple() {
        let json = r#"{"id": "123", "name": "Test"}"#;
        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Simple(simple) => {
                assert_eq!(simple.entity.get("id").unwrap(), "123");
            }
            MutationResponse::Full(_) => panic!("Expected Simple format"),
        }
    }

    #[test]
    fn test_parse_full_with_cascade() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_type": "User",
            "entity": {"id": "123", "name": "John"},
            "cascade": {"updated": []}
        }"#;

        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Full(full) => {
                assert_eq!(full.status, "created");
                assert_eq!(full.entity_type, Some("User".to_string()));
                assert!(full.cascade.is_some());
            }
            MutationResponse::Simple(_) => panic!("Expected Full format"),
        }
    }

    #[test]
    fn test_cascade_underscore_backward_compat() {
        let json = r#"{
            "status": "success",
            "message": "OK",
            "_cascade": {"updated": []}
        }"#;

        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Full(full) => {
                assert!(full.cascade.is_some());
            }
            MutationResponse::Simple(_) => panic!("Expected Full format"),
        }
    }

    #[test]
    fn test_invalid_status_treated_as_simple() {
        // status field exists but value is not a valid mutation status
        let json = r#"{"status": "some_random_field", "data": "value"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Simple(_)));
    }
}
