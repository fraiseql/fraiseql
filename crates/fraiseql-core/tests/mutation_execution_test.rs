//! Integration-style tests for the mutation error projection pathway.
//!
//! These tests exercise `build_field_mappings_from_type` + `ProjectionMapper` with a
//! realistic schema and verify that scalar, complex, and array fields are correctly
//! projected from metadata JSONB, with camelCase key resolution and selection filtering.

use std::collections::HashSet;

use fraiseql_core::{
    runtime::{ProjectionMapper, build_field_mappings_from_type},
    schema::{CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType},
};
use serde_json::json;

/// Build a `FieldDefinition` with the given name and type string.
fn field(name: &str, type_str: &str) -> FieldDefinition {
    FieldDefinition {
        name:           name.into(),
        field_type:     FieldType::parse(type_str),
        nullable:       true,
        default_value:  None,
        description:    None,
        vector_config:  None,
        alias:          None,
        deprecation:    None,
        requires_scope: None,
        on_deny:        FieldDenyPolicy::default(),
        encryption:     None,
    }
}

/// Empty schema (no nested types to resolve).
fn empty_schema() -> CompiledSchema {
    serde_json::from_value(json!({})).expect("empty schema")
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

/// Helper: build mappings and project metadata through `ProjectionMapper`.
fn project_error_metadata(
    fields: &[FieldDefinition],
    schema: &CompiledSchema,
    metadata: &serde_json::Value,
    requested: Option<&[String]>,
) -> serde_json::Value {
    let mut visited = HashSet::new();
    let mappings = build_field_mappings_from_type(fields, schema, requested, &mut visited);
    let mapper = ProjectionMapper::with_mappings(mappings);
    let obj = metadata.as_object().cloned().unwrap_or_default();
    mapper.project_json_object(&obj).expect("projection should succeed")
}

// ---------------------------------------------------------------------------
// Scalar field population (the #294 fix, now via ProjectionMapper)
// ---------------------------------------------------------------------------

#[test]
fn test_scalar_datetime_populated_from_metadata() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({ "last_activity_date": "2024-06-01T12:00:00Z" });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(
        result["last_activity_date"], "2024-06-01T12:00:00Z",
        "DateTime scalar should be copied directly from metadata"
    );
}

#[test]
fn test_scalar_int_populated_from_metadata() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({ "cascade_count": 17 });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(
        result["cascade_count"], 17,
        "Int scalar should be copied directly from metadata"
    );
}

#[test]
fn test_scalar_uuid_populated_from_metadata() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({ "blocker_id": "550e8400-e29b-41d4-a716-446655440000" });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(
        result["blocker_id"], "550e8400-e29b-41d4-a716-446655440000",
        "UUID scalar should be copied directly from metadata"
    );
}

#[test]
fn test_scalar_string_populated_from_metadata() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({ "reason": "machine still has active sessions" });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(
        result["reason"], "machine still has active sessions",
        "String scalar should be copied directly from metadata"
    );
}

// ---------------------------------------------------------------------------
// camelCase key lookup (snake_case field -> camelCase metadata key)
// ---------------------------------------------------------------------------

#[test]
fn test_camel_case_key_lookup_for_datetime() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    // Python backend emits camelCase keys in metadata
    let metadata = json!({ "lastActivityDate": "2024-03-15T08:30:00Z" });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(
        result["last_activity_date"], "2024-03-15T08:30:00Z",
        "snake_case field should be found via camelCase metadata key"
    );
}

#[test]
fn test_camel_case_key_lookup_for_int() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({ "cascadeCount": 5 });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

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
    let schema = empty_schema();
    let metadata = json!({
        "last_activity_date": "2024-01-01T00:00:00Z",
        "cascade_count":      3,
        "blocker_id":         "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "reason":             "blocked by policy"
    });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

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
    let schema = empty_schema();
    let metadata = json!({ "reason": "only reason present" });

    let result = project_error_metadata(&fields, &schema, &metadata, None);
    let obj = result.as_object().expect("should be object");

    assert!(obj.contains_key("reason"));
    assert!(
        !obj.contains_key("cascade_count"),
        "absent metadata key should not appear in output"
    );
    assert!(
        !obj.contains_key("last_activity_date"),
        "absent metadata key should not appear in output"
    );
}

// ---------------------------------------------------------------------------
// Complex (object) fields still work
// ---------------------------------------------------------------------------

#[test]
fn test_complex_object_field_still_populated() {
    let fields = vec![field("machine", "Machine")];
    let schema = empty_schema();
    let metadata = json!({ "machine": { "id": "m-123", "name": "rack-42" } });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    assert_eq!(result["machine"]["id"], "m-123");
    assert_eq!(result["machine"]["name"], "rack-42");
}

// ---------------------------------------------------------------------------
// Array field population (#214)
// ---------------------------------------------------------------------------

#[test]
fn test_array_field_populated_from_metadata() {
    let fields = vec![field("affected_ids", "[UUID!]!")];
    let schema = empty_schema();
    let metadata = json!({ "affected_ids": ["id-1", "id-2", "id-3"] });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    let arr = result["affected_ids"]
        .as_array()
        .expect("array field should be populated");
    assert_eq!(arr.len(), 3, "all array elements should be preserved");
    assert_eq!(arr[0], "id-1");
}

#[test]
fn test_array_of_entity_objects_populated() {
    let fields = vec![field("blockers", "[Blocker!]!")];
    let schema = empty_schema();
    let metadata = json!({
        "blockers": [
            { "id": "b1", "name": "session-1" },
            { "id": "b2", "name": "session-2" }
        ]
    });

    let result = project_error_metadata(&fields, &schema, &metadata, None);

    let arr = result["blockers"]
        .as_array()
        .expect("array of entities should be populated");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["id"], "b1");
    assert_eq!(arr[1]["name"], "session-2");
}

// ---------------------------------------------------------------------------
// null / non-object metadata
// ---------------------------------------------------------------------------

#[test]
fn test_null_metadata_returns_empty_object() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let result = project_error_metadata(&fields, &schema, &serde_json::Value::Null, None);
    let obj = result.as_object().expect("should be object");
    assert!(obj.is_empty(), "null metadata should yield empty output object");
}

// ---------------------------------------------------------------------------
// Selection set filtering (#215 Gap 1)
// ---------------------------------------------------------------------------

#[test]
fn test_selection_filtering_only_returns_requested_fields() {
    let fields = decommission_error_fields();
    let schema = empty_schema();
    let metadata = json!({
        "last_activity_date": "2024-01-01T00:00:00Z",
        "cascade_count": 3,
        "blocker_id": "some-uuid",
        "reason": "blocked"
    });
    let requested = vec!["reason".to_string(), "cascade_count".to_string()];

    let result = project_error_metadata(&fields, &schema, &metadata, Some(&requested));
    let obj = result.as_object().expect("should be object");

    assert_eq!(obj.len(), 2, "only requested fields should be present");
    assert_eq!(result["reason"], "blocked");
    assert_eq!(result["cascade_count"], 3);
    assert!(
        !obj.contains_key("last_activity_date"),
        "unrequested field should be absent"
    );
}

// ---------------------------------------------------------------------------
// Nested object projection with __typename (#215 Gap 2 + Gap 3)
// ---------------------------------------------------------------------------

#[test]
fn test_nested_entity_gets_typename_and_field_projection() {
    let error_fields = vec![field("machine", "Machine"), field("reason", "String")];

    // Schema with the nested Machine type registered
    let schema: CompiledSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Machine",
            "sql_source": "v_machine",
            "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "name", "field_type": "String" },
                { "name": "location", "field_type": "String" }
            ]
        }]
    }))
    .expect("schema");

    let metadata = json!({
        "machine": { "id": "m-1", "name": "rack-42", "location": "dc-1" },
        "reason": "active sessions"
    });

    let result = project_error_metadata(&error_fields, &schema, &metadata, None);

    // Nested entity should have __typename injected and only schema fields projected
    assert_eq!(result["machine"]["__typename"], "Machine");
    assert_eq!(result["machine"]["id"], "m-1");
    assert_eq!(result["machine"]["name"], "rack-42");
    assert_eq!(result["machine"]["location"], "dc-1");
    assert_eq!(result["reason"], "active sessions");
}

#[test]
fn test_array_of_entities_gets_typename_injected() {
    let error_fields = vec![field("blockers", "[Blocker!]!")];

    let schema: CompiledSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Blocker",
            "sql_source": "v_blocker",
            "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "reason", "field_type": "String" }
            ]
        }]
    }))
    .expect("schema");

    let metadata = json!({
        "blockers": [
            { "id": "b1", "reason": "active" },
            { "id": "b2", "reason": "locked" }
        ]
    });

    let result = project_error_metadata(&error_fields, &schema, &metadata, None);

    let arr = result["blockers"].as_array().expect("should be array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["__typename"], "Blocker");
    assert_eq!(arr[0]["id"], "b1");
    assert_eq!(arr[1]["__typename"], "Blocker");
    assert_eq!(arr[1]["reason"], "locked");
}

// ---------------------------------------------------------------------------
// Cycle guard for self-referencing types
// ---------------------------------------------------------------------------

#[test]
fn test_self_referencing_type_does_not_infinite_loop() {
    let schema: CompiledSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Node",
            "sql_source": "v_node",
            "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "parent", "field_type": { "Object": "Node" } }
            ]
        }]
    }))
    .expect("schema");

    let fields = vec![field("node", "Node")];
    let metadata = json!({
        "node": { "id": "n-1", "parent": { "id": "n-0" } }
    });

    // Should not hang or stack overflow
    let result = project_error_metadata(&fields, &schema, &metadata, None);
    assert_eq!(result["node"]["__typename"], "Node");
    assert_eq!(result["node"]["id"], "n-1");
}
