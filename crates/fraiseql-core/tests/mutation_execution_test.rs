//! Integration-style tests for the mutation execution pathway (issue #294).
//!
//! These tests exercise `populate_error_fields` with a realistic schema and
//! verify that scalar fields (DateTime, Int, UUID, String) are correctly
//! populated from `metadata` JSONB, without requiring a database.

use fraiseql_core::schema::FieldDenyPolicy;
use fraiseql_core::{
    runtime::mutation_result::populate_error_fields,
    schema::{FieldDefinition, FieldType},
};
use serde_json::json;

/// Build a `FieldDefinition` with the given name and type string.
fn field(name: &str, type_str: &str) -> FieldDefinition {
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
        on_deny: FieldDenyPolicy::default(),
        encryption:     None,
    }
}

/// The error type used across tests:
/// ```graphql
/// type DecommissionError {
///   last_activity_date: DateTime
///   cascade_count:      Int
///   blocker_id:         UUID
///   reason:             String
/// }
/// ```
fn decommission_error_fields() -> Vec<FieldDefinition> {
    vec![
        field("last_activity_date", "DateTime"),
        field("cascade_count", "Int"),
        field("blocker_id", "UUID"),
        field("reason", "String"),
    ]
}

// ---------------------------------------------------------------------------
// Scalar field population (the #294 fix)
// ---------------------------------------------------------------------------

#[test]
fn test_scalar_datetime_populated_from_metadata() {
    let fields = decommission_error_fields();
    let metadata = json!({ "last_activity_date": "2024-06-01T12:00:00Z" });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(
        result["last_activity_date"],
        "2024-06-01T12:00:00Z",
        "DateTime scalar should be copied directly from metadata"
    );
}

#[test]
fn test_scalar_int_populated_from_metadata() {
    let fields = decommission_error_fields();
    let metadata = json!({ "cascade_count": 17 });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(result["cascade_count"], 17, "Int scalar should be copied directly from metadata");
}

#[test]
fn test_scalar_uuid_populated_from_metadata() {
    let fields = decommission_error_fields();
    let metadata = json!({ "blocker_id": "550e8400-e29b-41d4-a716-446655440000" });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(
        result["blocker_id"],
        "550e8400-e29b-41d4-a716-446655440000",
        "UUID scalar should be copied directly from metadata"
    );
}

#[test]
fn test_scalar_string_populated_from_metadata() {
    let fields = decommission_error_fields();
    let metadata = json!({ "reason": "machine still has active sessions" });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(
        result["reason"], "machine still has active sessions",
        "String scalar should be copied directly from metadata"
    );
}

// ---------------------------------------------------------------------------
// camelCase key lookup (snake_case field ↔ camelCase metadata key)
// ---------------------------------------------------------------------------

#[test]
fn test_camel_case_key_lookup_for_datetime() {
    let fields = decommission_error_fields();
    // Python backend emits camelCase keys in metadata
    let metadata = json!({ "lastActivityDate": "2024-03-15T08:30:00Z" });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(
        result["last_activity_date"],
        "2024-03-15T08:30:00Z",
        "snake_case field should be found via camelCase metadata key"
    );
}

#[test]
fn test_camel_case_key_lookup_for_int() {
    let fields = decommission_error_fields();
    let metadata = json!({ "cascadeCount": 5 });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(
        result["cascade_count"], 5,
        "snake_case field should be found via camelCase metadata key"
    );
}

// ---------------------------------------------------------------------------
// Multiple fields in one metadata object
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_scalar_fields_populated() {
    let fields = decommission_error_fields();
    let metadata = json!({
        "last_activity_date": "2024-01-01T00:00:00Z",
        "cascade_count":      3,
        "blocker_id":         "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "reason":             "blocked by policy"
    });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(result["last_activity_date"], "2024-01-01T00:00:00Z");
    assert_eq!(result["cascade_count"], 3);
    assert_eq!(result["blocker_id"], "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
    assert_eq!(result["reason"], "blocked by policy");
}

// ---------------------------------------------------------------------------
// Missing keys produce absent (not null) fields
// ---------------------------------------------------------------------------

#[test]
fn test_missing_metadata_key_produces_no_field() {
    let fields = decommission_error_fields();
    let metadata = json!({ "reason": "only reason present" });

    let result = populate_error_fields(&fields, &metadata);

    assert!(result.contains_key("reason"));
    assert!(
        !result.contains_key("cascade_count"),
        "absent metadata key should not appear in output"
    );
    assert!(
        !result.contains_key("last_activity_date"),
        "absent metadata key should not appear in output"
    );
}

// ---------------------------------------------------------------------------
// Complex (object) fields still work
// ---------------------------------------------------------------------------

#[test]
fn test_complex_object_field_still_populated() {
    let fields = vec![field("machine", "Machine")];
    let metadata = json!({ "machine": { "id": "m-123", "name": "rack-42" } });

    let result = populate_error_fields(&fields, &metadata);

    assert_eq!(result["machine"]["id"], "m-123");
    assert_eq!(result["machine"]["name"], "rack-42");
}

// ---------------------------------------------------------------------------
// null / non-object metadata
// ---------------------------------------------------------------------------

#[test]
fn test_null_metadata_returns_empty_map() {
    let fields = decommission_error_fields();
    let result = populate_error_fields(&fields, &serde_json::Value::Null);
    assert!(result.is_empty(), "null metadata should yield empty output map");
}
