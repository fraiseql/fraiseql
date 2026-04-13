//! Mutation response parser for `app.mutation_response` composite rows.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::error::{FraiseQLError, Result};

/// Outcome of parsing a single `mutation_response` row.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MutationOutcome {
    /// The mutation succeeded; the result entity is available.
    Success {
        /// The entity JSONB returned by the function.
        entity:      JsonValue,
        /// GraphQL type name for the entity (from the `entity_type` column).
        entity_type: Option<String>,
        /// UUID string of the mutated entity (from the `entity_id` column).
        ///
        /// Present for UPDATE and DELETE mutations. Used for entity-aware cache
        /// invalidation: only cache entries containing this UUID are evicted,
        /// leaving unrelated entries warm.
        entity_id:   Option<String>,
        /// Cascade operations associated with this mutation.
        cascade:     Option<JsonValue>,
    },
    /// The mutation failed; error metadata is available.
    Error {
        /// Status code (e.g. `"failed:validation"`, `"conflict:duplicate"`).
        status:   String,
        /// Human-readable error message.
        message:  String,
        /// Structured metadata JSONB containing error-type field values.
        metadata: JsonValue,
    },
}

/// Parse a single row from `execute_function_call` into a `MutationOutcome`.
///
/// Expects the row to contain the standard `app.mutation_response` columns:
/// `status`, `message`, `entity`, `entity_type`, `cascade`, `metadata`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the `status` column is missing.
pub fn parse_mutation_row<S: ::std::hash::BuildHasher>(
    row: &HashMap<String, JsonValue, S>,
) -> Result<MutationOutcome> {
    let status = row
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FraiseQLError::Validation {
            message: "mutation_response row is missing 'status' column".to_string(),
            path:    None,
        })?
        .to_string();

    let message = row.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if is_error_status(&status) {
        let metadata = row.get("metadata").cloned().unwrap_or(JsonValue::Null);
        Ok(MutationOutcome::Error {
            status,
            message,
            metadata,
        })
    } else {
        let entity = row.get("entity").cloned().unwrap_or(JsonValue::Null);
        let entity_type = row.get("entity_type").and_then(|v| v.as_str()).map(str::to_string);
        let entity_id = row.get("entity_id").and_then(|v| v.as_str()).map(str::to_string);
        let cascade = row.get("cascade").cloned().filter(|v| !v.is_null());
        Ok(MutationOutcome::Success {
            entity,
            entity_type,
            entity_id,
            cascade,
        })
    }
}

/// Classify a mutation status string as an error.
///
/// The following patterns are treated as errors:
/// - `"failed:*"` — validation, business-rule, or processing failures
/// - `"conflict:*"` — uniqueness or concurrency conflicts
/// - `"error"` — generic error status
pub fn is_error_status(status: &str) -> bool {
    status.starts_with("failed:") || status.starts_with("conflict:") || status == "error"
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_success_row() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("new"));
        row.insert("message".to_string(), json!("created"));
        row.insert("entity".to_string(), json!({"id": "abc", "name": "Foo"}));
        row.insert("entity_type".to_string(), json!("Machine"));

        let outcome = parse_mutation_row(&row).expect("test fixture must parse successfully");
        assert!(matches!(outcome, MutationOutcome::Success { .. }));
        if let MutationOutcome::Success {
            entity,
            entity_type,
            entity_id,
            ..
        } = outcome
        {
            assert_eq!(entity["id"], "abc");
            assert_eq!(entity_type.as_deref(), Some("Machine"));
            assert!(entity_id.is_none());
        }
    }

    #[test]
    fn test_parse_mutation_row_includes_entity_id() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("updated"));
        row.insert("message".to_string(), json!("updated"));
        row.insert("entity".to_string(), json!({"id": "550e8400-e29b-41d4-a716-446655440000"}));
        row.insert("entity_type".to_string(), json!("User"));
        row.insert("entity_id".to_string(), json!("550e8400-e29b-41d4-a716-446655440000"));

        let outcome = parse_mutation_row(&row).expect("test fixture must parse successfully");
        if let MutationOutcome::Success {
            entity_id,
            entity_type,
            ..
        } = outcome
        {
            assert_eq!(entity_id.as_deref(), Some("550e8400-e29b-41d4-a716-446655440000"));
            assert_eq!(entity_type.as_deref(), Some("User"));
        } else {
            panic!("expected Success");
        }
    }

    #[test]
    fn test_parse_mutation_row_entity_id_absent_when_missing() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("new"));
        row.insert("entity".to_string(), json!({"id": "abc"}));
        // entity_id column not present (CREATE mutation)

        let outcome = parse_mutation_row(&row).expect("test fixture must parse successfully");
        if let MutationOutcome::Success { entity_id, .. } = outcome {
            assert!(entity_id.is_none());
        } else {
            panic!("expected Success");
        }
    }

    #[test]
    fn test_parse_error_row() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("failed:validation"));
        row.insert("message".to_string(), json!("invalid input"));
        row.insert("metadata".to_string(), json!({"last_activity_date": "2024-01-01"}));

        let outcome = parse_mutation_row(&row).expect("test fixture must parse successfully");
        assert!(matches!(outcome, MutationOutcome::Error { .. }));
        if let MutationOutcome::Error {
            status, metadata, ..
        } = outcome
        {
            assert_eq!(status, "failed:validation");
            assert!(metadata.is_object());
        }
    }

    #[test]
    fn test_is_error_status() {
        assert!(is_error_status("failed:validation"));
        assert!(is_error_status("failed:business_rule"));
        assert!(is_error_status("conflict:duplicate"));
        assert!(is_error_status("conflict:concurrent_update"));
        assert!(is_error_status("error"));
        assert!(!is_error_status("new"));
        assert!(!is_error_status("updated"));
        assert!(!is_error_status("deleted"));
        assert!(!is_error_status(""));
    }
}
