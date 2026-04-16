//! Mutation response parser for `app.mutation_response` v2 composite rows.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::error::{FraiseQLError, Result};

use super::cascade::MutationErrorClass;

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
        /// Typed classification of the failure (mirrors `app.mutation_error_class`).
        error_class: MutationErrorClass,
        /// Human-readable error message.
        message:     String,
        /// Structured metadata JSONB containing error-type field values.
        metadata:    JsonValue,
    },
}

/// Parse a `mutation_response` row.
///
/// Requires `schema_version: 2`. Rows with a missing or `1` `schema_version`
/// are rejected — update the DB function to return a v2 composite.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if the row does not conform to the
/// v2 shape. See [`crate::runtime::mutation_result_v2::parse_mutation_row_v2`]
/// for the detailed v2 validation rules.
pub fn parse_mutation_row<S: ::std::hash::BuildHasher>(
    row: &HashMap<String, JsonValue, S>,
) -> Result<MutationOutcome> {
    let version = row.get("schema_version").and_then(JsonValue::as_i64);
    match version {
        Some(2) => crate::runtime::mutation_result_v2::parse_mutation_row_v2(row),
        None | Some(1) => Err(FraiseQLError::Validation {
            message: "mutation_response row has no schema_version or schema_version=1; \
                      update the DB function to return a v2 composite \
                      (schema_version=2)"
                .to_string(),
            path:    None,
        }),
        Some(other) => Err(FraiseQLError::Validation {
            message: format!("unsupported mutation_response schema_version: {other}"),
            path:    None,
        }),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    // ── v2 dispatch ─────────────────────────────────────────────────────────

    #[test]
    fn dispatch_schema_version_2_success() {
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(2));
        row.insert("succeeded".to_string(), json!(true));
        row.insert("state_changed".to_string(), json!(true));
        row.insert("entity".to_string(), json!({"id": "abc"}));
        row.insert("entity_type".to_string(), json!("User"));
        let outcome = parse_mutation_row(&row).expect("v2 row parses");
        match outcome {
            MutationOutcome::Success { entity_type, .. } => {
                assert_eq!(entity_type.as_deref(), Some("User"));
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    #[test]
    fn dispatch_schema_version_2_error() {
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(2));
        row.insert("succeeded".to_string(), json!(false));
        row.insert("state_changed".to_string(), json!(false));
        row.insert("error_class".to_string(), json!("validation"));
        row.insert("message".to_string(), json!("bad input"));
        let outcome = parse_mutation_row(&row).expect("v2 error row parses");
        match outcome {
            MutationOutcome::Error { error_class, message, .. } => {
                assert_eq!(error_class, MutationErrorClass::Validation);
                assert_eq!(message, "bad input");
            },
            MutationOutcome::Success { .. } => panic!("expected Error"),
        }
    }

    #[test]
    fn dispatch_absent_schema_version_rejected() {
        let mut row = HashMap::new();
        row.insert("entity".to_string(), json!({"id": "abc"}));
        let err = parse_mutation_row(&row).expect_err("v1-style row must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("schema_version"), "got: {message}");
            },
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_schema_version_1_rejected() {
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(1));
        row.insert("entity".to_string(), json!({"id": "abc"}));
        let err = parse_mutation_row(&row).expect_err("v1 schema_version must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("schema_version"), "got: {message}");
            },
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_unknown_schema_version_rejected() {
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(99));
        let err = parse_mutation_row(&row).expect_err("unknown version rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("schema_version"), "got: {message}");
            },
            other => panic!("expected Validation, got {other:?}"),
        }
    }
}
