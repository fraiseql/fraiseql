//! Mutation response parser for `app.mutation_response` composite rows.

use std::collections::HashMap;

use serde_json::{Map, Value as JsonValue};

use crate::{
    error::{FraiseQLError, Result},
    schema::FieldDefinition,
    utils::casing::to_camel_case,
};

/// Scalar GraphQL type names that can be populated directly from JSONB values.
const SCALAR_TYPES: &[&str] = &[
    "String", "Int", "Float", "Boolean", "ID", "DateTime", "UUID", "Date", "Time",
];

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

/// Parse a `mutation_response` row, dispatching on `schema_version`.
///
/// Version support matrix:
///
/// | `schema_version` column | Parser                                 |
/// |-------------------------|----------------------------------------|
/// | absent or `1`           | [`parse_mutation_row_v1`] (legacy path) |
/// | `2`                     | [`crate::runtime::mutation_result_v2::parse_mutation_row_v2`] |
/// | anything else           | [`FraiseQLError::Validation`]          |
///
/// The v1 path remains available during the v2 migration (Phases 01–03 of the
/// `app.mutation_response` v2 initiative). It will be removed in Phase 04 once
/// all emitters are v2. See `docs/architecture/mutation-response.md`.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if the row does not deserialize into
/// a supported version shape (see the per-version parsers for specifics).
pub fn parse_mutation_row<S: ::std::hash::BuildHasher>(
    row: &HashMap<String, JsonValue, S>,
) -> Result<MutationOutcome> {
    let version = row.get("schema_version").and_then(JsonValue::as_i64);
    match version {
        None | Some(1) => parse_mutation_row_v1(row),
        Some(2) => crate::runtime::mutation_result_v2::parse_mutation_row_v2(row),
        Some(other) => Err(FraiseQLError::Validation {
            message: format!("unsupported mutation_response schema_version: {other}"),
            path:    None,
        }),
    }
}

/// Parse a v1 `app.mutation_response` row (legacy, string-typed `status`).
///
/// Retained as part of the [`parse_mutation_row`] version dispatcher to keep
/// not-yet-migrated emitters working during the v2 migration. **Do not call
/// directly from new code** — call [`parse_mutation_row`] and let it dispatch.
/// This function will be gated behind a `legacy-mutation-v1` Cargo feature in
/// the first Phase 04 release and deleted in the following one.
///
/// Expects the row to contain the v1 columns: `status`, `message`, `entity`,
/// `entity_type`, `cascade`, `metadata`.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if the `status` column is missing.
pub fn parse_mutation_row_v1<S: ::std::hash::BuildHasher>(
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
/// - `"not_found:*"` — entity not found or access denied
/// - `"error"` — generic error status
pub fn is_error_status(status: &str) -> bool {
    status.starts_with("failed:")
        || status.starts_with("conflict:")
        || status.starts_with("not_found:")
        || status.starts_with("noop:")
        || status.starts_with("validation:")
        || status == "error"
}

/// Populate error-type fields from a `metadata` JSONB object.
///
/// This is the fix for issue #294: scalar fields (String, Int, Float, Boolean,
/// `DateTime`, UUID, …) are now populated directly from the JSON value, without
/// requiring the value to be a nested object.
///
/// Both camelCase and `snake_case` metadata keys are tried for each field.
///
/// # Arguments
///
/// * `fields` — field definitions from the error `TypeDefinition`
/// * `metadata` — the raw `metadata` JSON from the mutation response row
///
/// # Returns
///
/// A JSON object map containing the populated fields.
pub fn populate_error_fields(
    fields: &[FieldDefinition],
    metadata: &JsonValue,
) -> Map<String, JsonValue> {
    let mut output = Map::new();

    let Some(obj) = metadata.as_object() else {
        return output;
    };

    for field in fields {
        // Try camelCase first, then the raw field name (snake_case)
        let camel = to_camel_case(field.name.as_str());
        let raw_val = obj.get(&camel).or_else(|| obj.get(field.name.as_str()));

        let Some(raw_val) = raw_val else { continue };

        let base_type = strip_list_and_bang(&field.field_type.to_string());

        if SCALAR_TYPES.contains(&base_type.as_str()) {
            // #294 fix: copy scalar values directly (string, int, datetime, uuid, …)
            output.insert(field.name.to_string(), raw_val.clone());
        } else if raw_val.is_object() || raw_val.is_array() {
            // Complex entity field: nested JSON object or array relation
            output.insert(field.name.to_string(), raw_val.clone());
        }
        // else: non-scalar, non-object, non-array value in metadata — skip
    }

    output
}

/// Strip list wrappers and non-null bangs from a field type string.
///
/// Examples:
/// - `"String!"` → `"String"`
/// - `"[String!]!"` → `"String"`
/// - `"DateTime"` → `"DateTime"`
fn strip_list_and_bang(field_type: &str) -> String {
    field_type.trim_matches(|c| c == '[' || c == ']' || c == '!').to_string()
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
        assert!(is_error_status("not_found:dns_server"));
        assert!(is_error_status("not_found:user"));
        assert!(is_error_status("noop:no_changes"));
        assert!(is_error_status("validation:has_current_allocations"));
        assert!(is_error_status("error"));
        assert!(!is_error_status("new"));
        assert!(!is_error_status("updated"));
        assert!(!is_error_status("deleted"));
        assert!(!is_error_status(""));
    }

    // ---- Version dispatch (Cycle 4) -----------------------------------------

    #[test]
    fn dispatch_absent_schema_version_uses_v1() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("new"));
        row.insert("entity".to_string(), json!({"id": "abc"}));
        let outcome = parse_mutation_row(&row).expect("v1 row parses");
        assert!(matches!(outcome, MutationOutcome::Success { .. }));
    }

    #[test]
    fn dispatch_schema_version_1_uses_v1() {
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(1));
        row.insert("status".to_string(), json!("failed:validation"));
        row.insert("message".to_string(), json!("bad input"));
        let outcome = parse_mutation_row(&row).expect("v1 row parses");
        assert!(matches!(outcome, MutationOutcome::Error { .. }));
    }

    #[test]
    fn dispatch_schema_version_2_uses_v2() {
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
    fn dispatch_mixed_version_batch() {
        // v1 and v2 rows parse correctly through the same public API.
        let mut v1 = HashMap::new();
        v1.insert("status".to_string(), json!("new"));
        v1.insert("entity".to_string(), json!({"id": "v1"}));

        let mut v2 = HashMap::new();
        v2.insert("schema_version".to_string(), json!(2));
        v2.insert("succeeded".to_string(), json!(true));
        v2.insert("state_changed".to_string(), json!(true));
        v2.insert("entity".to_string(), json!({"id": "v2"}));

        let r1 = parse_mutation_row(&v1).expect("v1 parses");
        let r2 = parse_mutation_row(&v2).expect("v2 parses");
        assert!(matches!(r1, MutationOutcome::Success { .. }));
        assert!(matches!(r2, MutationOutcome::Success { .. }));
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
