#![allow(clippy::unwrap_used)] // Reason: test module
//! Unit tests for the shared seam-section merge.

use serde_json::json;

use super::*;

#[test]
fn array_sections_concatenate_across_files() {
    let mut acc = empty_accumulator();
    absorb_sections(&mut acc, &json!({"enums": [{"name": "A"}]}), "a.json").unwrap();
    absorb_sections(&mut acc, &json!({"enums": [{"name": "B"}]}), "b.json").unwrap();

    let names: Vec<&str> = acc["enums"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["A", "B"], "enums from two files must both survive");
}

#[test]
fn every_array_section_survives_a_merge() {
    let mut acc = empty_accumulator();
    for section in AUTHORABLE_ARRAY_SECTIONS {
        absorb_sections(&mut acc, &json!({*section: [{"name": "probe"}]}), "probe.json").unwrap();
    }
    for section in AUTHORABLE_ARRAY_SECTIONS {
        assert_eq!(
            acc[*section].as_array().map(Vec::len),
            Some(1),
            "section `{section}` was dropped by absorb_sections"
        );
    }
}

#[test]
fn an_unknown_section_is_carried_through_for_the_consumer_to_reject() {
    let mut acc = empty_accumulator();
    absorb_sections(&mut acc, &json!({"not_a_real_section": {"x": 1}}), "x.json").unwrap();
    assert!(
        acc.get("not_a_real_section").is_some(),
        "an unknown key must reach the consumer, which is the single place that decides \
         what is authorable — filtering it here would restore the silent drop"
    );
}

#[test]
fn a_conflicting_singleton_is_an_error_naming_the_section() {
    let mut acc = empty_accumulator();
    absorb_sections(&mut acc, &json!({"naming_convention": "camelCase"}), "a.json").unwrap();
    let err = absorb_sections(&mut acc, &json!({"naming_convention": "snake_case"}), "b.json")
        .expect_err("two files declaring different naming conventions must not silently race");

    let msg = err.to_string();
    assert!(msg.contains("naming_convention"), "error must name the section; got: {msg}");
    assert!(msg.contains("b.json"), "error must name the offending file; got: {msg}");
}

#[test]
fn an_identical_singleton_repeated_is_accepted() {
    let mut acc = empty_accumulator();
    absorb_sections(&mut acc, &json!({"naming_convention": "camelCase"}), "a.json").unwrap();
    absorb_sections(&mut acc, &json!({"naming_convention": "camelCase"}), "b.json")
        .expect("the same value declared twice is not a conflict");
}

#[test]
fn an_array_section_given_a_non_array_is_a_typed_error() {
    let mut acc = empty_accumulator();
    let err = absorb_sections(&mut acc, &json!({"enums": {"name": "A"}}), "a.json")
        .expect_err("an object where an array is expected must not be silently ignored");
    assert!(err.to_string().contains("an object"), "error must name the found type: {err}");
}

#[test]
fn the_two_section_lists_do_not_overlap() {
    for key in AUTHORABLE_ARRAY_SECTIONS {
        assert!(
            !AUTHORABLE_SINGLETON_SECTIONS.contains(key),
            "`{key}` is classified as both an array and a singleton section"
        );
    }
}
