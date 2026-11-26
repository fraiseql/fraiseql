#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::{CompiledSchema, StoredKeys};
use crate::runtime::stored_key_candidates;

/// A project acronym no built-in default and no other test registers.
const PROJECT_ACRONYM: &str = "zork9";

fn order_schema(acronyms: &[&str]) -> serde_json::Value {
    json!({
        "types": [
            {"name": "Order", "sql_source": "v_order", "jsonb_column": "data", "fields": [
                {"name": "id", "field_type": "ID"},
                {"name": "fkUser", "field_type": "ID"},
                {"name": "createdAt", "field_type": "DateTime"},
                {"name": "line_total", "field_type": "Int"},
                {"name": "s3Bucket", "field_type": "String"},
                {"name": "zork9Count", "field_type": "Int"},
            ]},
            {"name": "User", "sql_source": "v_user", "jsonb_column": "data", "fields": [
                {"name": "id", "field_type": "ID"},
                {"name": "displayName", "field_type": "String"},
            ]},
        ],
        "queries": [],
        "naming_acronyms": acronyms,
    })
}

fn built(acronyms: &[&str]) -> CompiledSchema {
    CompiledSchema::from_json(&order_schema(acronyms).to_string(), false).unwrap()
}

fn keys(stored: &str, fallback: Option<&str>) -> (String, Option<String>) {
    (stored.to_string(), fallback.map(str::to_string))
}

fn read(schema: &CompiledSchema, type_name: &str, field: &str) -> (String, Option<String>) {
    let found: &StoredKeys = schema.stored_keys(type_name, field).unwrap();
    keys(found.stored(), found.fallback())
}

/// The #1271 shape: a camelCase field over a `snake_case` view reads `fk_user`, and falls
/// back to the response key `jsonb_build_object` emits.
#[test]
fn a_camel_case_field_reads_its_snake_case_key_then_its_response_key() {
    let schema = built(&[]);
    assert_eq!(read(&schema, "Order", "fkUser"), keys("fk_user", Some("fkUser")));
    assert_eq!(read(&schema, "Order", "createdAt"), keys("created_at", Some("createdAt")));
}

#[test]
fn a_name_with_one_spelling_has_no_fallback() {
    let schema = built(&[]);
    assert_eq!(read(&schema, "Order", "id"), keys("id", None));
}

/// A field declared `snake_case` is stored as written; its fallback is the camelCase
/// response name, as `stored_key_candidates` gives.
#[test]
fn a_snake_case_field_falls_back_to_its_camel_case_spelling() {
    let schema = built(&[]);
    assert_eq!(read(&schema, "Order", "line_total"), keys("line_total", Some("lineTotal")));
}

/// The table is resolved with the schema's own acronyms. Against the built-in defaults,
/// which is what the process-wide set is when the server builds the schema, the project
/// acronym would split.
#[test]
fn a_project_acronym_is_kept_whole_from_the_schema_s_own_set() {
    let schema = built(&[PROJECT_ACRONYM]);
    assert_eq!(read(&schema, "Order", "zork9Count"), keys("zork9_count", Some("zork9Count")));
    assert_eq!(read(&schema, "Order", "s3Bucket"), keys("s3_bucket", Some("s3Bucket")));

    let without = built(&[]);
    assert_eq!(read(&without, "Order", "zork9Count"), keys("zork_9_count", Some("zork9Count")));
}

/// Once the server installs the schema's acronyms, the runtime rule answers what the
/// table holds, for every declared field.
#[test]
fn the_table_agrees_with_the_runtime_rule_once_the_acronyms_are_installed() {
    let schema = built(&[PROJECT_ACRONYM]);
    crate::utils::casing::set_runtime_acronyms(&schema.naming_acronyms);
    for type_def in &schema.types {
        for field in &type_def.fields {
            let name = field.name.as_str();
            assert_eq!(
                read(&schema, type_def.name.as_str(), name),
                stored_key_candidates(name),
                "{}.{name}",
                type_def.name
            );
        }
    }
}

#[test]
fn an_undeclared_field_or_type_has_no_entry() {
    let schema = built(&[]);
    assert!(schema.stored_keys("Order", "displayName").is_none());
    assert!(schema.stored_keys("Invoice", "id").is_none());
}

/// A schema deserialized without `build_indexes` has no table; a caller resolves the name
/// itself, as before the table existed.
#[test]
fn an_unbuilt_schema_has_no_entry() {
    let schema: CompiledSchema = serde_json::from_value(order_schema(&[])).unwrap();
    assert!(schema.stored_keys("Order", "fkUser").is_none());
}

/// A copy of a built schema with types and fields filtered out, as a role-gated or
/// introspection view makes, resolves each name it still holds to that field's own keys.
#[test]
fn a_filtered_copy_resolves_the_names_it_keeps_to_their_own_keys() {
    let schema = built(&[]);
    let mut view = schema.clone();
    view.types.retain(|t| t.name.as_str() == "User");
    view.types[0].fields.retain(|f| f.name.as_str() != "id");
    assert_eq!(read(&view, "User", "displayName"), keys("display_name", Some("displayName")));
    // The original keeps every type it declares.
    assert_eq!(read(&schema, "Order", "fkUser"), keys("fk_user", Some("fkUser")));
}

#[test]
fn building_again_yields_the_same_table() {
    let mut schema = built(&[PROJECT_ACRONYM]);
    let first: Vec<_> = ["fkUser", "zork9Count", "id"]
        .iter()
        .map(|f| read(&schema, "Order", f))
        .collect();
    schema.build_indexes();
    let second: Vec<_> = ["fkUser", "zork9Count", "id"]
        .iter()
        .map(|f| read(&schema, "Order", f))
        .collect();
    assert_eq!(first, second);
}

/// A clone shares the table instead of copying it: a view cloned per request pays a
/// reference count, not a copy of every field's keys.
#[test]
fn a_clone_shares_the_table() {
    let schema = built(&[]);
    let view = schema.clone();
    assert!(std::sync::Arc::ptr_eq(
        &schema.derived.stored_keys.0,
        &view.derived.stored_keys.0
    ));
}
