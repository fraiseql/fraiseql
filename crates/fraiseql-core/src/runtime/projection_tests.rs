//! The owned projection (#1556) against the borrowed one it must equal, value and bytes.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value as JsonValue, json};

use super::*;

fn rows(values: &[JsonValue]) -> Vec<JsonbValue> {
    values.iter().cloned().map(JsonbValue::new).collect()
}

/// The owned projection of `values` equals the borrowed one, as a value and as the bytes a
/// response serializes to (key order is part of a response).
fn assert_owned_equals_borrowed(projector: &ResultProjector, values: &[JsonValue], is_list: bool) {
    let borrowed = projector.project_results(&rows(values), is_list).unwrap();
    let owned = projector.project_results_owned(Arc::new(rows(values)), is_list).unwrap();
    assert_eq!(owned, borrowed, "values: {values:?}");
    assert_eq!(
        serde_json::to_string(&owned).unwrap(),
        serde_json::to_string(&borrowed).unwrap(),
        "key order, values: {values:?}"
    );
}

fn simple(names: &[&str]) -> ResultProjector {
    ResultProjector::new(names.iter().map(ToString::to_string).collect())
}

#[test]
fn a_row_with_exactly_the_requested_keys() {
    assert_owned_equals_borrowed(
        &simple(&["id", "content"]),
        &[
            json!({"id": "1", "content": "a"}),
            json!({"id": "2", "content": "b"}),
        ],
        true,
    );
}

#[test]
fn unrequested_keys_are_dropped_and_keys_follow_the_selection_order() {
    assert_owned_equals_borrowed(
        &simple(&["content", "id"]),
        &[json!({"id": "1", "extra": [1, 2], "content": "a"})],
        true,
    );
}

#[test]
fn a_missing_field_stays_absent() {
    assert_owned_equals_borrowed(&simple(&["id", "missing"]), &[json!({"id": "1"})], true);
}

#[test]
fn an_alias_reads_its_source_key() {
    let projector = ResultProjector::with_mappings(vec![
        FieldMapping::aliased("fullName", "myName"),
        FieldMapping::simple("id"),
    ]);
    assert_owned_equals_borrowed(&projector, &[json!({"id": "1", "full_name": "Ada"})], true);
}

#[test]
fn two_mappings_reading_one_stored_key_both_get_it() {
    // `fooBar` and `foo_bar` both resolve to the stored `foo_bar`.
    let projector = simple(&["fooBar", "foo_bar", "id"]);
    assert_owned_equals_borrowed(&projector, &[json!({"id": "1", "foo_bar": {"x": 1}})], true);
}

#[test]
fn an_alias_and_its_field_read_one_stored_key() {
    let projector = ResultProjector::with_mappings(vec![
        FieldMapping::aliased("name", "first"),
        FieldMapping::simple("name"),
    ]);
    assert_owned_equals_borrowed(&projector, &[json!({"name": "Ada"})], true);
}

#[test]
fn a_camel_case_stored_key_is_read_through_the_fallback() {
    assert_owned_equals_borrowed(&simple(&["fooBar"]), &[json!({"fooBar": 1})], true);
}

#[test]
fn the_root_typename_is_stamped_first() {
    let projector = simple(&["id"]).with_typename("Comment");
    assert_owned_equals_borrowed(&projector, &[json!({"id": "1"})], true);
}

#[test]
fn nested_objects_with_and_without_a_typename() {
    let projector = ResultProjector::with_mappings(vec![
        FieldMapping::nested_object(
            "author",
            "User",
            // `uid` and `id` both read the stored `id`.
            vec![
                FieldMapping::simple("username"),
                FieldMapping::aliased("id", "uid"),
                FieldMapping::simple("id"),
            ],
        ),
        FieldMapping::simple("post"),
        FieldMapping::simple("tags").with_nested_typename("Tag"),
        FieldMapping::simple("raw").with_nested_typename("Raw"),
    ]);
    assert_owned_equals_borrowed(
        &projector,
        &[json!({
            "author": {"id": "u1", "username": "ada", "secret": "x"},
            "post": {"title": "t", "body": "b"},
            "tags": [{"name": "a"}, {"name": "b"}, 3],
            "raw": {"z": 1, "a": 2},
        })],
        true,
    );
}

#[test]
fn a_localized_field_reads_its_chain() {
    let mut label = FieldMapping::simple("label");
    label.localized = Some(vec!["fr".to_string(), "en".to_string()]);
    let projector = ResultProjector::with_mappings(vec![label, FieldMapping::simple("id")]);
    assert_owned_equals_borrowed(
        &projector,
        &[
            json!({"id": "1", "label": {"en": "Hello", "fr": "Bonjour"}}),
            json!({"id": "2", "label": "SQL-resolved"}),
        ],
        true,
    );
}

#[test]
fn json_text_is_recovered_unless_the_field_is_declared_scalar() {
    let mut declared = FieldMapping::simple("payload");
    declared.declared_scalar = true;
    let projector = ResultProjector::with_mappings(vec![
        declared,
        FieldMapping::simple("doc").with_nested_typename("Doc"),
        FieldMapping::simple("other"),
    ]);
    assert_owned_equals_borrowed(
        &projector,
        &[json!({
            "payload": "{\"a\": 1}",
            "doc": "{\"b\": 2}",
            "other": "[1, 2]",
        })],
        true,
    );
}

#[test]
fn a_single_item_and_an_empty_result() {
    let projector = simple(&["id"]);
    assert_owned_equals_borrowed(&projector, &[json!({"id": "1", "x": 2})], false);
    assert_owned_equals_borrowed(&projector, &[], false);
    assert_owned_equals_borrowed(&projector, &[], true);
}

#[test]
fn rows_that_are_not_objects() {
    assert_owned_equals_borrowed(
        &simple(&["id"]),
        &[json!([{"id": "1", "x": 1}, 7]), json!("text"), json!(null)],
        true,
    );
}

#[test]
fn rows_another_holder_shares_are_projected_without_being_taken() {
    let projector = simple(&["id"]);
    let shared = Arc::new(rows(&[json!({"id": "1", "x": 2})]));
    let held = Arc::clone(&shared);
    let owned = projector.project_results_owned(shared, true).unwrap();
    assert_eq!(owned, json!([{"id": "1"}]));
    assert_eq!(held[0].as_value(), &json!({"id": "1", "x": 2}), "the other holder's rows");
}

/// A string long enough to own a heap buffer, and that buffer's address.
fn owned_text(text: &str) -> (JsonValue, *const u8) {
    let text = text.to_string();
    let buffer = text.as_ptr();
    (JsonValue::String(text), buffer)
}

#[test]
fn a_row_no_one_else_holds_gives_up_its_values() {
    // Built by insertion: `json!` would serialize the strings into copies.
    let (content, content_buffer) = owned_text("a comment long enough to own a heap buffer");
    let (username, username_buffer) = owned_text("a username long enough to own a buffer");
    let mut author = serde_json::Map::new();
    author.insert("username".to_string(), username);
    let mut row = serde_json::Map::new();
    row.insert("id".to_string(), json!("1"));
    row.insert("content".to_string(), content);
    row.insert("author".to_string(), JsonValue::Object(author));
    let results = Arc::new(vec![JsonbValue::new(JsonValue::Object(row))]);
    let projector = ResultProjector::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::simple("content"),
        FieldMapping::nested_object("author", "User", vec![FieldMapping::simple("username")]),
    ]);

    let owned = projector.project_results_owned(results, true).unwrap();

    assert_eq!(
        owned[0]["content"].as_str().unwrap().as_ptr(),
        content_buffer,
        "moved, not cloned"
    );
    assert_eq!(
        owned[0]["author"]["username"].as_str().unwrap().as_ptr(),
        username_buffer,
        "a nested object's values are moved too"
    );
}
