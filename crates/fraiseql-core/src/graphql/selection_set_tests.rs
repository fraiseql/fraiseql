//! Unit tests for the shared selection-resolution routine.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use serde_json::json;

use super::*;
use crate::graphql::{parse_query, types::FieldSelection};

/// Response keys of the root field's children, in order.
fn child_names(selections: &[FieldSelection]) -> Vec<String> {
    selections
        .first()
        .map(|root| root.nested_fields.iter().map(|f| f.response_key().to_string()).collect())
        .unwrap_or_default()
}

fn resolved(query: &str, variables: &serde_json::Value) -> Vec<String> {
    let parsed = parse_query(query).expect("query must parse");
    let vars = variables_map(Some(variables));
    let out = resolve_and_filter(&parsed.selections, &parsed.fragments, &vars)
        .expect("selection set must resolve");
    child_names(&out)
}

#[test]
fn a_spread_contributes_its_fields_at_the_position_it_appears() {
    assert_eq!(
        resolved("fragment F on User { name } { users { email ...F id } }", &json!({})),
        vec!["email", "name", "id"]
    );
}

#[test]
fn a_skipped_spread_contributes_nothing() {
    assert_eq!(
        resolved(
            "fragment F on User { name email } { users { id ...F @skip(if: true) } }",
            &json!({})
        ),
        vec!["id"]
    );
}

#[test]
fn a_spread_directive_composes_with_a_field_directive() {
    // The spread permits; the field inside withholds. `@skip` wins.
    assert_eq!(
        resolved(
            "fragment F on User { name email @skip(if: true) } { users { id ...F @include(if: \
             true) } }",
            &json!({})
        ),
        vec!["id", "name"]
    );
}

#[test]
fn a_variable_condition_on_a_spread_is_evaluated() {
    let query = "fragment F on User { name } query($lite: Boolean!) { users { id ...F @skip(if: \
                 $lite) } }";
    assert_eq!(resolved(query, &json!({"lite": true})), vec!["id"]);
    assert_eq!(resolved(query, &json!({"lite": false})), vec!["id", "name"]);
}

#[test]
fn an_undefined_variable_in_a_condition_is_an_error() {
    let parsed = parse_query("fragment F on User { name } { users { id ...F @skip(if: $nope) } }")
        .expect("query must parse");
    let err = resolve_and_filter(&parsed.selections, &parsed.fragments, &variables_map(None))
        .expect_err("an undefined condition variable must not silently include the fragment");
    assert!(matches!(err, SelectionError::Directive(_)), "got: {err:?}");
}

#[test]
fn an_undefined_fragment_is_an_error() {
    let parsed = parse_query("{ users { id ...Missing } }").expect("query must parse");
    let err = resolve(&parsed.selections, &parsed.fragments)
        .expect_err("a spread naming no fragment must be refused");
    assert!(matches!(err, SelectionError::Fragment(_)), "got: {err:?}");
}

#[test]
fn resolve_is_independent_of_variables() {
    // The executor's parse cache is keyed by the query string alone, so the
    // expansion half must not depend on the request. If this ever stops holding,
    // cached mutation selections leak one request's variables into the next.
    let parsed =
        parse_query("fragment F on User { name } { users { id ...F @skip(if: $x) } }").unwrap();
    let a = resolve(&parsed.selections, &parsed.fragments).unwrap();
    let b = resolve(&parsed.selections, &parsed.fragments).unwrap();
    assert_eq!(child_names(&a), child_names(&b));
    assert_eq!(child_names(&a), vec!["id", "name"], "expansion must not evaluate conditions");
}

#[test]
fn variables_map_treats_a_non_object_payload_as_empty() {
    assert!(variables_map(None).is_empty());
    assert!(variables_map(Some(&json!(null))).is_empty());
    assert!(variables_map(Some(&json!([1, 2]))).is_empty());
    assert_eq!(variables_map(Some(&json!({"a": 1}))).len(), 1);
}
