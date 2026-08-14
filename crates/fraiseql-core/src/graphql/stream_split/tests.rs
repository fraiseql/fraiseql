//! Unit tests for the nested `@stream` delivery split.
//!
//! Driven directly with hand-built selection trees, like the `@defer` tests: what
//! matters is structural — which items leave `data`, what path each chunk is
//! addressed by, and what happens to a streamed list nested inside a list. The
//! end-to-end wiring (a real document, a real database, the SSE envelope) is
//! `graphql_sse_e2e_pg`.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{contains_nested_stream, split};
use crate::graphql::types::{Directive, FieldSelection, GraphQLArgument};

fn field(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_owned(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

fn with_children(name: &str, children: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        nested_fields: children,
        ..field(name)
    }
}

/// `@stream` with optional arguments, encoded the way the parser does.
fn stream(args: Vec<(&str, &str, &str)>) -> Directive {
    Directive {
        name:      "stream".to_owned(),
        arguments: args
            .into_iter()
            .map(|(name, value_type, value_json)| GraphQLArgument {
                name:       name.to_owned(),
                value_type: value_type.to_owned(),
                value_json: value_json.to_owned(),
            })
            .collect(),
    }
}

fn streamed(mut selection: FieldSelection, directive: Directive) -> FieldSelection {
    selection.directives.push(directive);
    selection
}

fn no_vars() -> HashMap<String, Value> {
    HashMap::new()
}

#[test]
fn a_document_without_stream_is_untouched() {
    let selections = vec![with_children("user", vec![field("id")])];
    let mut data = json!({"user": {"id": 1}});
    let before = data.clone();

    assert!(!contains_nested_stream(&selections, &no_vars()));
    let chunks = split(&selections, &mut data, &no_vars(), 10).unwrap();

    assert!(chunks.is_empty());
    assert_eq!(data, before);
}

/// A root-level `@stream` is *not* a nested one: it is real database paging, planned
/// elsewhere, and this splitter must leave it entirely alone. Claiming it here would
/// give one document two pagers.
#[test]
fn a_root_level_stream_is_not_a_nested_stream() {
    let selections = vec![streamed(
        with_children("users", vec![field("id")]),
        stream(vec![]),
    )];
    assert!(!contains_nested_stream(&selections, &no_vars()));
}

#[test]
fn the_initial_count_stays_and_the_tail_becomes_chunks() {
    let selections = vec![with_children(
        "user",
        vec![streamed(
            field("posts"),
            stream(vec![("initialCount", "Int", "1")]),
        )],
    )];
    let mut data = json!({"user": {"posts": ["a", "b", "c", "d", "e"]}});

    assert!(contains_nested_stream(&selections, &no_vars()));
    let chunks = split(&selections, &mut data, &no_vars(), 2).unwrap();

    assert_eq!(data, json!({"user": {"posts": ["a"]}}), "initialCount items stay in place");
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].items, vec![json!("b"), json!("c")]);
    assert_eq!(chunks[0].path, vec![json!("user"), json!("posts"), json!(1)]);
    assert_eq!(chunks[1].items, vec![json!("d"), json!("e")]);
    assert_eq!(
        chunks[1].path,
        vec![json!("user"), json!("posts"), json!(3)],
        "each chunk is addressed by the index of its first item"
    );
}

/// The default `initialCount` is 0: everything streams, nothing stays.
#[test]
fn without_an_initial_count_the_whole_list_streams() {
    let selections = vec![with_children(
        "user",
        vec![streamed(field("posts"), stream(vec![]))],
    )];
    let mut data = json!({"user": {"posts": [1, 2, 3]}});

    let chunks = split(&selections, &mut data, &no_vars(), 10).unwrap();

    assert_eq!(data, json!({"user": {"posts": []}}));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].path, vec![json!("user"), json!("posts"), json!(0)]);
}

/// A list shorter than its `initialCount` is delivered whole, with no continuation —
/// saying otherwise would leave a client waiting for a payload that never comes.
#[test]
fn a_list_within_the_initial_count_produces_no_chunks() {
    let selections = vec![with_children(
        "user",
        vec![streamed(
            field("posts"),
            stream(vec![("initialCount", "Int", "10")]),
        )],
    )];
    let mut data = json!({"user": {"posts": [1, 2]}});

    let chunks = split(&selections, &mut data, &no_vars(), 5).unwrap();

    assert!(chunks.is_empty());
    assert_eq!(data, json!({"user": {"posts": [1, 2]}}));
}

/// Each element of an enclosing list is its own path. Without that, every user's
/// posts would be addressed as the same list and a client would splice them all into
/// the first one.
#[test]
fn a_streamed_list_inside_a_list_is_addressed_per_element() {
    let selections = vec![with_children(
        "users",
        vec![field("id"), streamed(field("posts"), stream(vec![]))],
    )];
    let mut data = json!({"users": [
        {"id": 1, "posts": ["p1"]},
        {"id": 2, "posts": ["p2", "p3"]}
    ]});

    let chunks = split(&selections, &mut data, &no_vars(), 10).unwrap();

    assert_eq!(data, json!({"users": [{"id": 1, "posts": []}, {"id": 2, "posts": []}]}));
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].path, vec![json!("users"), json!(0), json!("posts"), json!(0)]);
    assert_eq!(chunks[1].path, vec![json!("users"), json!(1), json!("posts"), json!(0)]);
    assert_eq!(chunks[1].items, vec![json!("p2"), json!("p3")]);
}

/// `@stream(if: false)` is not a stream at all.
#[test]
fn a_disabled_stream_is_left_in_place() {
    let selections = vec![with_children(
        "user",
        vec![streamed(
            field("posts"),
            stream(vec![("if", "Boolean", "false")]),
        )],
    )];
    let mut data = json!({"user": {"posts": [1, 2, 3]}});

    assert!(!contains_nested_stream(&selections, &no_vars()));
    let chunks = split(&selections, &mut data, &no_vars(), 1).unwrap();

    assert!(chunks.is_empty());
    assert_eq!(data, json!({"user": {"posts": [1, 2, 3]}}));
}

/// A label routes a chunk without the client pattern-matching on the path.
#[test]
fn a_label_is_carried_on_every_chunk() {
    let selections = vec![with_children(
        "user",
        vec![streamed(
            field("posts"),
            stream(vec![("label", "String", "\"feed\"")]),
        )],
    )];
    let mut data = json!({"user": {"posts": [1, 2, 3]}});

    let chunks = split(&selections, &mut data, &no_vars(), 2).unwrap();

    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().all(|c| c.label.as_deref() == Some("feed")));
}

/// A `@stream` on a field that is not a list is refused, not ignored. On a transport
/// that negotiated incremental delivery, a directive that silently does nothing reads
/// to the client as "streaming worked".
#[test]
fn streaming_a_non_list_field_is_refused() {
    let selections = vec![with_children(
        "user",
        vec![streamed(field("email"), stream(vec![]))],
    )];
    let mut data = json!({"user": {"email": "a@b.c"}});

    let err = split(&selections, &mut data, &no_vars(), 10).unwrap_err();
    assert_eq!(err.field, "email");
}

/// A null field resolved to nothing; there is nothing to deliver later, and nothing
/// to refuse either.
#[test]
fn streaming_a_null_field_produces_no_chunks() {
    let selections = vec![with_children(
        "user",
        vec![streamed(field("posts"), stream(vec![]))],
    )];
    let mut data = json!({"user": {"posts": null}});

    let chunks = split(&selections, &mut data, &no_vars(), 10).unwrap();

    assert!(chunks.is_empty());
    assert_eq!(data, json!({"user": {"posts": null}}));
}

/// A batch size of zero must not mean "chunks of nothing, forever".
#[test]
fn a_zero_batch_size_is_treated_as_one() {
    let selections = vec![with_children(
        "user",
        vec![streamed(field("posts"), stream(vec![]))],
    )];
    let mut data = json!({"user": {"posts": [1, 2]}});

    let chunks = split(&selections, &mut data, &no_vars(), 0).unwrap();

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].items, vec![json!(1)]);
}
