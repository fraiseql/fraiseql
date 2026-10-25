//! Unit tests for the shared selection-resolution routine.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use serde_json::json;

use super::*;
use crate::{
    graphql::{parse_query, types::FieldSelection},
    schema::DEFAULT_MAX_QUERY_DEPTH,
};

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
    let out =
        resolve_and_filter(&parsed.selections, &parsed.fragments, &vars, DEFAULT_MAX_QUERY_DEPTH)
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
    let err = resolve_and_filter(
        &parsed.selections,
        &parsed.fragments,
        &variables_map(None),
        DEFAULT_MAX_QUERY_DEPTH,
    )
    .expect_err("an undefined condition variable must not silently include the fragment");
    assert!(matches!(err, SelectionError::Directive(_)), "got: {err:?}");
}

#[test]
fn an_undefined_fragment_is_an_error() {
    let parsed = parse_query("{ users { id ...Missing } }").expect("query must parse");
    let err = resolve(&parsed.selections, &parsed.fragments, DEFAULT_MAX_QUERY_DEPTH)
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
    let a = resolve(&parsed.selections, &parsed.fragments, DEFAULT_MAX_QUERY_DEPTH).unwrap();
    let b = resolve(&parsed.selections, &parsed.fragments, DEFAULT_MAX_QUERY_DEPTH).unwrap();
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

/// The resolver refuses at the depth it is handed, not at a fixed one: the executor
/// hands it its gate's `max_query_depth`, so the two are one bound.
#[test]
fn resolve_refuses_past_the_depth_it_is_handed() {
    let nested = |levels: usize| {
        format!("{{ users {}{{ id }}{} }}", "{ f ".repeat(levels), " }".repeat(levels))
    };
    let parsed = parse_query(&nested(30)).unwrap();
    assert!(resolve(&parsed.selections, &parsed.fragments, 64).is_ok());
    let err = resolve(&parsed.selections, &parsed.fragments, 20).expect_err("30 levels past 20");
    assert!(err.to_string().contains("20-level limit"), "{err}");
}

// ── § 6.3.2 field merging ────────────────────────────────────────────────────

/// The resolved root's children, each with its own children's response keys.
fn resolved_tree(query: &str, variables: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    let parsed = parse_query(query).expect("query must parse");
    let vars = variables_map(Some(variables));
    let out =
        resolve_and_filter(&parsed.selections, &parsed.fragments, &vars, DEFAULT_MAX_QUERY_DEPTH)
            .expect("selection set must resolve");
    out[0]
        .nested_fields
        .iter()
        .map(|f| {
            let children = f.nested_fields.iter().map(|c| c.response_key().to_string()).collect();
            (f.response_key().to_string(), children)
        })
        .collect()
}

fn tree(entries: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    entries
        .iter()
        .map(|(key, children)| {
            ((*key).to_string(), children.iter().map(|c| (*c).to_string()).collect())
        })
        .collect()
}

#[test]
fn a_field_written_twice_is_one_field_with_both_sub_selections() {
    assert_eq!(
        resolved_tree("{ users { posts { id } posts { title } } }", &json!({})),
        tree(&[("posts", &["id", "title"])])
    );
}

#[test]
fn a_field_selected_by_two_fragments_is_one_field_with_both_sub_selections() {
    assert_eq!(
        resolved_tree(
            "{ users { ...A ...B } } \
             fragment A on User { posts { id } } fragment B on User { posts { title } }",
            &json!({})
        ),
        tree(&[("posts", &["id", "title"])])
    );
}

#[test]
fn a_field_under_one_alias_twice_is_one_field_with_both_sub_selections() {
    assert_eq!(
        resolved_tree("{ users { p: posts { id } p: posts { title } } }", &json!({})),
        tree(&[("p", &["id", "title"])])
    );
}

#[test]
fn a_second_alias_of_the_same_field_stays_its_own_key() {
    assert_eq!(
        resolved_tree("{ users { posts { id } p: posts { title } } }", &json!({})),
        tree(&[("posts", &["id"]), ("p", &["title"])])
    );
}

#[test]
fn a_merged_field_keeps_the_position_of_its_first_occurrence() {
    assert_eq!(
        resolved_tree("{ users { posts { id } name posts { title } } }", &json!({})),
        tree(&[("posts", &["id", "title"]), ("name", &[])])
    );
}

#[test]
fn a_skipped_occurrence_contributes_nothing_to_the_merge() {
    // Directives are evaluated per occurrence before merging (CollectFields).
    assert_eq!(
        resolved_tree(
            "query($s: Boolean!) { users { posts @skip(if: $s) { id } posts { title } } }",
            &json!({"s": true})
        ),
        tree(&[("posts", &["title"])])
    );
}

#[test]
fn nested_occurrences_merge_at_every_level() {
    assert_eq!(
        resolved_tree(
            "{ users { posts { author { id } } posts { author { name } } } }",
            &json!({})
        ),
        tree(&[("posts", &["author"])])
    );
    let parsed =
        parse_query("{ users { posts { author { id } } posts { author { name } } } }").unwrap();
    let out = resolve_and_filter(
        &parsed.selections,
        &parsed.fragments,
        &variables_map(Some(&json!({}))),
        DEFAULT_MAX_QUERY_DEPTH,
    )
    .unwrap();
    let author: Vec<&str> = out[0].nested_fields[0].nested_fields[0]
        .nested_fields
        .iter()
        .map(FieldSelection::response_key)
        .collect();
    assert_eq!(author, vec!["id", "name"]);
}

#[test]
fn occurrences_with_different_arguments_are_not_merged() {
    // Merging them would answer one of the two argument sets with the other's rows.
    // § 5.3.2 refuses such a document; that check needs the type conditions named
    // spreads lose at expansion, so until it exists the occurrences stay apart.
    assert_eq!(
        resolved_tree("{ users { posts(limit: 1) { id } posts(limit: 2) { title } } }", &json!({})),
        tree(&[("posts", &["id"]), ("posts", &["title"])])
    );
}

#[test]
fn two_fields_under_one_alias_are_not_merged() {
    // `a` cannot be both `id` and `name` (§ 5.3.2); the occurrences stay apart.
    assert_eq!(
        resolved_tree("{ users { a: id a: name } }", &json!({})),
        tree(&[("a", &[]), ("a", &[])])
    );
}
