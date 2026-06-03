#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Tests for the canonical entity projector [`project_entity`], used by **both**
//! the mutation success arm (projecting the returned entity) and the error arm
//! (projecting error metadata).
//!
//! It is behaviourally equivalent to the SQL query projection:
//! - output keys are the selection's response key (camelCase GraphQL surface),
//! - source keys are `to_snake_case(field.name)` (the stored JSONB key), with a camelCase fallback
//!   for legacy metadata,
//! - single object fields with a sub-selection are recursed,
//! - list / scalar / sub-selection-less / over-depth fields pass through verbatim,
//! - `__typename` is emitted only where the client selected it.

use fraiseql_core::{graphql::FieldSelection, runtime::project_entity, schema::CompiledSchema};
use serde_json::json;

/// Leaf selection (no sub-fields).
fn sel(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

/// Object selection with a sub-selection.
fn sel_nested(name: &str, children: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: children,
        directives:    vec![],
    }
}

/// Schema modelling the modern contract: `camelCase` GraphQL surface field names,
/// `snake_case` stored JSONB keys.
///
/// ```graphql
/// type DecommissionError {
///   lastActivityDate: DateTime
///   cascadeCount: Int
///   reason: String
///   machine: Machine
///   blockers: [Blocker!]!
/// }
/// type Machine { id: ID, name: String, assetTag: String }
/// type Blocker { id: ID, reason: String }
/// ```
fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "types": [
            { "name": "DecommissionError", "sql_source": "", "is_error": true, "fields": [
                { "name": "lastActivityDate", "field_type": "DateTime" },
                { "name": "cascadeCount", "field_type": "Int" },
                { "name": "reason", "field_type": "String" },
                { "name": "machine", "field_type": { "Object": "Machine" } },
                { "name": "blockers", "field_type": { "List": { "Object": "Blocker" } } }
            ]},
            { "name": "Machine", "sql_source": "v_machine", "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "name", "field_type": "String" },
                { "name": "assetTag", "field_type": "String" }
            ]},
            { "name": "Blocker", "sql_source": "v_blocker", "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "reason", "field_type": "String" }
            ]}
        ]
    }))
    .expect("schema")
}

// ── snake_case source → camelCase surface output ───────────────────────────

#[test]
fn scalar_read_from_snake_source_output_as_camel_surface() {
    let s = schema();
    let md = json!({ "last_activity_date": "2024-06-01T12:00:00Z" });
    let out = project_entity(&md, "DecommissionError", &[sel("lastActivityDate")], &s);
    assert_eq!(
        out["lastActivityDate"], "2024-06-01T12:00:00Z",
        "snake source 'last_activity_date' surfaces under the camelCase key"
    );
}

#[test]
fn scalar_camel_metadata_fallback() {
    // Legacy metadata that used the camelCase surface casing is still found.
    let s = schema();
    let md = json!({ "lastActivityDate": "2024-03-15T08:30:00Z" });
    let out = project_entity(&md, "DecommissionError", &[sel("lastActivityDate")], &s);
    assert_eq!(out["lastActivityDate"], "2024-03-15T08:30:00Z");
}

#[test]
fn multiple_scalars_projected() {
    let s = schema();
    let md = json!({ "last_activity_date": "2024-01-01T00:00:00Z", "cascade_count": 3, "reason": "blocked" });
    let out = project_entity(
        &md,
        "DecommissionError",
        &[sel("lastActivityDate"), sel("cascadeCount"), sel("reason")],
        &s,
    );
    assert_eq!(out["lastActivityDate"], "2024-01-01T00:00:00Z");
    assert_eq!(out["cascadeCount"], 3);
    assert_eq!(out["reason"], "blocked");
}

// ── selection filtering ────────────────────────────────────────────────────

#[test]
fn only_selected_fields_returned() {
    let s = schema();
    let md = json!({ "last_activity_date": "x", "cascade_count": 3, "reason": "blocked" });
    let out = project_entity(&md, "DecommissionError", &[sel("reason"), sel("cascadeCount")], &s);
    let obj = out.as_object().unwrap();
    assert_eq!(obj.len(), 2, "only the two requested fields are present");
    assert_eq!(out["reason"], "blocked");
    assert_eq!(out["cascadeCount"], 3);
    assert!(!obj.contains_key("lastActivityDate"));
}

#[test]
fn missing_source_key_is_omitted() {
    let s = schema();
    let md = json!({ "reason": "only reason present" });
    let out = project_entity(&md, "DecommissionError", &[sel("reason"), sel("cascadeCount")], &s);
    let obj = out.as_object().unwrap();
    assert!(obj.contains_key("reason"));
    assert!(!obj.contains_key("cascadeCount"), "absent source key → absent output key");
}

// ── nested object recursion ────────────────────────────────────────────────

#[test]
fn nested_object_recursed_and_subset_by_selection() {
    let s = schema();
    let md = json!({ "machine": { "id": "m-1", "name": "rack-42", "asset_tag": "A-99" } });
    let out = project_entity(
        &md,
        "DecommissionError",
        &[sel_nested("machine", vec![sel("id"), sel("assetTag")])],
        &s,
    );
    assert_eq!(out["machine"]["id"], "m-1");
    // snake source 'asset_tag' surfaces under camelCase 'assetTag'.
    assert_eq!(out["machine"]["assetTag"], "A-99");
    let machine = out["machine"].as_object().unwrap();
    assert!(!machine.contains_key("name"), "unselected nested field omitted");
    assert!(!machine.contains_key("__typename"), "no __typename unless selected");
}

#[test]
fn nested_typename_emitted_only_when_selected() {
    let s = schema();
    let md = json!({ "machine": { "id": "m-1" } });
    let out = project_entity(
        &md,
        "DecommissionError",
        &[sel_nested("machine", vec![sel("id"), sel("__typename")])],
        &s,
    );
    assert_eq!(out["machine"]["__typename"], "Machine");
    assert_eq!(out["machine"]["id"], "m-1");
}

#[test]
fn top_level_typename_emitted_only_when_selected() {
    let s = schema();
    let md = json!({ "reason": "blocked" });

    let without = project_entity(&md, "DecommissionError", &[sel("reason")], &s);
    assert!(
        !without.as_object().unwrap().contains_key("__typename"),
        "no __typename when not selected (matches the query contract)"
    );

    let with = project_entity(&md, "DecommissionError", &[sel("reason"), sel("__typename")], &s);
    assert_eq!(with["__typename"], "DecommissionError");
}

// ── list fields pass through (matching the SQL full-sub-blob fallback) ──────

#[test]
fn list_field_passes_through_verbatim() {
    let s = schema();
    let md = json!({ "blockers": [
        { "id": "b1", "reason": "active" },
        { "id": "b2", "reason": "locked" }
    ]});
    let out =
        project_entity(&md, "DecommissionError", &[sel_nested("blockers", vec![sel("id")])], &s);
    // Lists are returned as their stored sub-blob — not subset — exactly as the
    // SQL query projection does, so query and mutation stay shape-identical.
    let arr = out["blockers"].as_array().expect("blockers array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["id"], "b1");
    assert_eq!(arr[0]["reason"], "active", "list elements pass through unsubsetted");
}

// ── edge cases ─────────────────────────────────────────────────────────────

#[test]
fn empty_selection_returns_entity_unchanged() {
    let s = schema();
    let md = json!({ "reason": "blocked", "cascade_count": 1 });
    let out = project_entity(&md, "DecommissionError", &[], &s);
    assert_eq!(out, md, "no selection → no filtering (stored entity returned as-is)");
}

#[test]
fn non_object_entity_returned_as_is() {
    let s = schema();
    let out = project_entity(&serde_json::Value::Null, "DecommissionError", &[sel("reason")], &s);
    assert!(out.is_null());
}
