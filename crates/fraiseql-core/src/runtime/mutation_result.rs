//! Mutation response parser for `app.mutation_response` composite rows.
//!
//! This module implements the fix for issue #294: error types with scalar/primitive
//! fields (String, Int, DateTime, UUID, etc.) are now correctly populated from the
//! `metadata` JSONB column, in addition to the existing support for nested object fields.

use std::collections::HashMap;

use serde_json::{Map, Value as JsonValue};

use crate::{
    error::{FraiseQLError, Result},
    schema::FieldDefinition,
};

/// Scalar GraphQL type names that can be populated directly from JSONB values.
const SCALAR_TYPES: &[&str] = &[
    "String", "Int", "Float", "Boolean", "ID", "DateTime", "UUID", "Date", "Time",
];

/// Outcome of parsing a single `mutation_response` row.
#[derive(Debug, Clone)]
pub enum MutationOutcome {
    /// The mutation succeeded; the result entity is available.
    Success {
        /// The entity JSONB returned by the function.
        entity:      JsonValue,
        /// GraphQL type name for the entity (from the `entity_type` column).
        entity_type: Option<String>,
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
pub fn parse_mutation_row(
    row: &HashMap<String, JsonValue>,
) -> Result<MutationOutcome> {
    let status = row
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or_else(|| FraiseQLError::Validation {
            message: "mutation_response row is missing 'status' column".to_string(),
            path:    None,
        })?
        .to_string();

    let message = row
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if is_error_status(&status) {
        let metadata = row.get("metadata").cloned().unwrap_or(JsonValue::Null);
        Ok(MutationOutcome::Error { status, message, metadata })
    } else {
        let entity = row.get("entity").cloned().unwrap_or(JsonValue::Null);
        let entity_type = row.get("entity_type").and_then(|v| v.as_str()).map(str::to_string);
        let cascade = row.get("cascade").cloned().filter(|v| !v.is_null());
        Ok(MutationOutcome::Success { entity, entity_type, cascade })
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

/// Populate error-type fields from a `metadata` JSONB object.
///
/// This is the fix for issue #294: scalar fields (String, Int, Float, Boolean,
/// DateTime, UUID, …) are now populated directly from the JSON value, without
/// requiring the value to be a nested object.
///
/// Both camelCase and snake_case metadata keys are tried for each field.
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

    let obj = match metadata.as_object() {
        Some(o) => o,
        None => return output,
    };

    for field in fields {
        // Try camelCase first, then the raw field name (snake_case)
        let camel = to_camel_case(&field.name);
        let raw_val = obj.get(&camel).or_else(|| obj.get(&field.name));

        let raw_val = match raw_val {
            Some(v) => v,
            None => continue,
        };

        let base_type = strip_list_and_bang(&field.field_type.to_string());

        if SCALAR_TYPES.contains(&base_type.as_str()) {
            // #294 fix: copy scalar values directly (string, int, datetime, uuid, …)
            output.insert(field.name.clone(), raw_val.clone());
        } else if raw_val.is_object() {
            // Complex entity field: nested JSON object (existing behaviour)
            output.insert(field.name.clone(), raw_val.clone());
        }
        // else: non-scalar, non-object value in metadata — skip
    }

    output
}

/// Convert a snake_case field name to camelCase for metadata key lookup.
///
/// Examples: `"last_activity_date"` → `"lastActivityDate"`,
///            `"cascade_count"` → `"cascadeCount"`.
fn to_camel_case(snake: &str) -> String {
    let mut result = String::with_capacity(snake.len());
    let mut capitalise_next = false;

    for ch in snake.chars() {
        if ch == '_' {
            capitalise_next = true;
        } else if capitalise_next {
            result.push(ch.to_ascii_uppercase());
            capitalise_next = false;
        } else {
            result.push(ch);
        }
    }

    result
}

/// Strip list wrappers and non-null bangs from a field type string.
///
/// Examples:
/// - `"String!"` → `"String"`
/// - `"[String!]!"` → `"String"`
/// - `"DateTime"` → `"DateTime"`
fn strip_list_and_bang(field_type: &str) -> String {
    field_type
        .trim_matches(|c| c == '[' || c == ']' || c == '!')
        .to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::schema::FieldType;

    fn make_field(name: &str, type_str: &str) -> FieldDefinition {
        let known = std::collections::HashSet::new();
        FieldDefinition {
            name:           name.to_string(),
            field_type:     FieldType::parse(type_str, &known),
            nullable:       true,
            default_value:  None,
            description:    None,
            vector_config:  None,
            alias:          None,
            deprecation:    None,
            requires_scope: None,
            encryption:     None,
        }
    }

    #[test]
    fn test_parse_success_row() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("new"));
        row.insert("message".to_string(), json!("created"));
        row.insert("entity".to_string(), json!({"id": "abc", "name": "Foo"}));
        row.insert("entity_type".to_string(), json!("Machine"));

        let outcome = parse_mutation_row(&row).unwrap();
        assert!(matches!(outcome, MutationOutcome::Success { .. }));
        if let MutationOutcome::Success { entity, entity_type, .. } = outcome {
            assert_eq!(entity["id"], "abc");
            assert_eq!(entity_type.as_deref(), Some("Machine"));
        }
    }

    #[test]
    fn test_parse_error_row() {
        let mut row = HashMap::new();
        row.insert("status".to_string(), json!("failed:validation"));
        row.insert("message".to_string(), json!("invalid input"));
        row.insert("metadata".to_string(), json!({"last_activity_date": "2024-01-01"}));

        let outcome = parse_mutation_row(&row).unwrap();
        assert!(matches!(outcome, MutationOutcome::Error { .. }));
        if let MutationOutcome::Error { status, metadata, .. } = outcome {
            assert_eq!(status, "failed:validation");
            assert!(metadata.is_object());
        }
    }

    #[test]
    fn test_populate_scalar_string() {
        let fields = vec![make_field("reason", "String")];
        let metadata = json!({"reason": "some error"});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["reason"], "some error");
    }

    #[test]
    fn test_populate_scalar_int() {
        let fields = vec![make_field("cascade_count", "Int")];
        let metadata = json!({"cascade_count": 42});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["cascade_count"], 42);
    }

    #[test]
    fn test_populate_scalar_datetime() {
        let fields = vec![make_field("last_activity_date", "DateTime")];
        let metadata = json!({"last_activity_date": "2024-06-01T12:00:00Z"});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["last_activity_date"], "2024-06-01T12:00:00Z");
    }

    #[test]
    fn test_populate_scalar_uuid() {
        let fields = vec![make_field("entity_id", "UUID")];
        let metadata = json!({"entity_id": "550e8400-e29b-41d4-a716-446655440000"});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["entity_id"], "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_populate_complex_entity() {
        let fields = vec![make_field("related", "SomeType")];
        let metadata = json!({"related": {"id": "xyz", "name": "bar"}});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["related"]["id"], "xyz");
    }

    #[test]
    fn test_populate_missing_field_is_absent() {
        let fields = vec![make_field("reason", "String")];
        let metadata = json!({"other_key": "value"});

        let result = populate_error_fields(&fields, &metadata);
        assert!(!result.contains_key("reason"));
    }

    #[test]
    fn test_camel_case_key_lookup() {
        // Field name is snake_case; metadata key is camelCase
        let fields = vec![make_field("last_activity_date", "DateTime")];
        let metadata = json!({"lastActivityDate": "2024-01-01"});

        let result = populate_error_fields(&fields, &metadata);
        assert_eq!(result["last_activity_date"], "2024-01-01");
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
