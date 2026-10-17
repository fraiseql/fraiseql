#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! The source reader of #1567 against the parser it mirrors.

use super::{DefinitionShapes, SelectionShapes, Shape, document_shapes};
use crate::graphql::parse_query;

fn object(keys: &[&str]) -> Shape {
    Shape::Object(keys.iter().map(|k| ((*k).to_string(), Shape::Leaf)).collect())
}

/// The shapes of the first field of the first definition.
fn first_field(source: &str) -> SelectionShapes {
    document_shapes(source).unwrap().remove(0).selection.remove(0)
}

#[test]
fn an_object_literal_keeps_the_order_written() {
    let field = first_field("{ docs(orderBy: {ratio: ASC, current: DESC}) { id } }");
    assert_eq!(field.arguments, [object(&["ratio", "current"])]);
}

#[test]
fn nested_objects_and_lists_keep_their_order() {
    let field = first_field("{ d(a: [{z: 1, y: 2}, {x: {w: 1, v: [1, 2]}}], b: 3) { id } }");
    assert_eq!(
        field.arguments,
        [
            Shape::List(vec![
                object(&["z", "y"]),
                Shape::Object(vec![(
                    "x".to_string(),
                    Shape::Object(vec![
                        ("w".to_string(), Shape::Leaf),
                        ("v".to_string(), Shape::List(vec![Shape::Leaf, Shape::Leaf])),
                    ]),
                )]),
            ]),
            Shape::Leaf,
        ]
    );
}

/// Everything the grammar lets a document hold besides values is followed, not read: an
/// alias, variables with list and non-null types, a default, directives on a field, a spread
/// and an inline fragment, strings and block strings holding braces, comments and commas.
#[test]
fn the_reader_follows_every_executable_construct() {
    let source = r#"
        # a comment { with braces
        query Q($o: [JSON!]! = [{b: 1, a: 2}], $s: String = "}{", $n: Int) @dir(x: {q: 1, p: 2}) {
          alias: docs(orderBy: {b: ASC, a: DESC}, s: """ block } { "quoted" \""" """) @include(if: true) {
            ...F @skip(if: false)
            ... on Doc @include(if: true) { id }
            ... { id }
            nested(where: {k2: {eq: "x"}, k1: {eq: -1.5e3}},) { id }
          }
        }
        fragment F on Doc @dir(y: {d: 1, c: 2}) { id(arg: {f: 1, e: 2}) }
        mutation { m(input: {two: 2, one: 1}) { id } }
        subscription S { s { id } }
        { anonymous }
    "#;

    let shapes = document_shapes(source).unwrap();

    assert_eq!(shapes.len(), 5);
    let q = &shapes[0];
    assert_eq!(
        q.variable_defaults,
        [
            Some(Shape::List(vec![object(&["b", "a"])])),
            Some(Shape::Leaf),
            None
        ]
    );
    let docs = &q.selection[0];
    assert_eq!(docs.arguments, [object(&["b", "a"]), Shape::Leaf]);
    assert_eq!(docs.directives, [vec![Shape::Leaf]]);
    assert_eq!(docs.selection.len(), 4);
    assert_eq!(docs.selection[0].directives, [vec![Shape::Leaf]]);
    assert!(docs.selection[0].selection.is_empty(), "a spread has no selection");
    assert_eq!(docs.selection[1].selection.len(), 1);
    assert_eq!(docs.selection[2].selection.len(), 1);
    assert_eq!(
        docs.selection[3].arguments,
        [Shape::Object(vec![
            ("k2".to_string(), object(&["eq"])),
            ("k1".to_string(), object(&["eq"])),
        ])]
    );
    assert_eq!(shapes[1].selection[0].arguments, [object(&["f", "e"])]);
    assert_eq!(shapes[2].selection[0].arguments, [object(&["two", "one"])]);
    assert_eq!(
        shapes[3],
        DefinitionShapes {
            variable_defaults: vec![],
            selection:         vec![SelectionShapes {
                arguments:  vec![],
                directives: vec![],
                selection:  vec![SelectionShapes::default()],
            }],
        }
    );

    // The parser accepts the same document, and parses every operation of it through the
    // reader without a mismatch.
    for name in ["Q", "S"] {
        crate::graphql::parse_query_with_operation_name(source, Some(name))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn a_parsed_argument_carries_the_written_order() {
    let parsed =
        parse_query("{ docs(orderBy: {ratio: ASC, current: DESC, a: ASC}) { id } }").unwrap();
    assert_eq!(
        parsed.selections[0].arguments[0].value_json,
        r#"{"ratio":"ASC","current":"DESC","a":"ASC"}"#
    );
}

#[test]
fn a_key_written_twice_keeps_its_first_place_and_its_last_value() {
    let parsed = parse_query("{ docs(orderBy: {b: ASC, a: ASC, b: DESC}) { id } }").unwrap();
    assert_eq!(parsed.selections[0].arguments[0].value_json, r#"{"b":"DESC","a":"ASC"}"#);
}

#[test]
fn a_document_the_reader_cannot_follow_is_refused() {
    assert!(document_shapes("{ docs(orderBy: {ratio ASC}) { id } }").is_err());
    assert!(document_shapes("{ docs(s: \"open").is_err());
}
