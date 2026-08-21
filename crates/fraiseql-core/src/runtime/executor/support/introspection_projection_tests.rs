//! Tests for [`introspection_projection`](super).

use serde_json::json;

use super::*;

fn sel(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: nested,
        directives:    vec![],
    }
}

fn aliased(alias: &str, name: &str) -> FieldSelection {
    FieldSelection {
        alias: Some(alias.to_string()),
        ..sel(name, vec![])
    }
}

#[test]
fn an_object_keeps_only_the_selected_keys() {
    let value = json!({"queryType": {"name": "Query"}, "types": [], "description": null});
    let projected = project(&value, &[sel("queryType", vec![sel("name", vec![])])]);
    assert_eq!(projected, json!({"queryType": {"name": "Query"}}));
}

#[test]
fn an_unselected_sibling_is_absent_not_null() {
    let value = json!({"name": "Query", "kind": "OBJECT"});
    let projected = project(&value, &[sel("name", vec![])]);
    assert!(projected.get("kind").is_none(), "got: {projected}");
}

#[test]
fn a_list_is_projected_element_wise() {
    let value = json!([{"name": "A", "kind": "OBJECT"}, {"name": "B", "kind": "SCALAR"}]);
    let projected = project(&value, &[sel("name", vec![])]);
    assert_eq!(projected, json!([{"name": "A"}, {"name": "B"}]));
}

#[test]
fn an_alias_names_the_response_key() {
    let value = json!({"name": "Query"});
    let projected = project(&value, &[aliased("typeName", "name")]);
    assert_eq!(projected, json!({"typeName": "Query"}));
}

/// The introspection value is the authority on what exists; a selection
/// naming something absent is omitted rather than fabricated as `null`.
#[test]
fn a_selection_the_value_does_not_carry_is_omitted() {
    let value = json!({"name": "Query"});
    let projected = project(&value, &[sel("name", vec![]), sel("noSuchThing", vec![])]);
    assert_eq!(projected, json!({"name": "Query"}));
}

#[test]
fn an_empty_selection_set_returns_the_value_whole() {
    let value = json!({"name": "Query", "kind": "OBJECT"});
    assert_eq!(project(&value, &[]), value);
}

#[test]
fn the_envelope_is_preserved() {
    let response = json!({"data": {"__schema": {"queryType": {"name": "Q"}, "types": []}}});
    let projected =
        project_response(&response, "__schema", &[sel("queryType", vec![sel("name", vec![])])]);
    assert_eq!(projected, json!({"data": {"__schema": {"queryType": {"name": "Q"}}}}));
}

#[test]
fn different_shapes_hash_differently_and_the_same_shape_is_stable() {
    let a = [sel("queryType", vec![sel("name", vec![])])];
    let b = [sel("types", vec![sel("name", vec![])])];
    assert_eq!(
        selection_shape_hash("__schema", &a),
        selection_shape_hash("__schema", &a),
        "the same shape must memoise to the same slot"
    );
    assert_ne!(selection_shape_hash("__schema", &a), selection_shape_hash("__schema", &b));
}

/// The root field is part of the key: `__schema` and `__type` project
/// different values and must not share a cache slot.
#[test]
fn the_root_field_participates_in_the_hash() {
    let s = [sel("name", vec![])];
    assert_ne!(selection_shape_hash("__schema", &s), selection_shape_hash("__type", &s));
}

#[test]
fn an_alias_changes_the_shape() {
    let plain = [sel("name", vec![])];
    let alias = [aliased("n", "name")];
    assert_ne!(
        selection_shape_hash("__schema", &plain),
        selection_shape_hash("__schema", &alias),
        "an alias changes the emitted key, so it is a different projection"
    );
}
