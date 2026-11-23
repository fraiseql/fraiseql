#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use crate::graphql::{FieldSelection, parse_query};

/// The selection set of `users` in `document`, as `name[directives]`.
fn users(document: &str) -> Vec<String> {
    let parsed = parse_query(document).unwrap();
    describe(&parsed.selections[0].nested_fields)
}

fn describe(fields: &[FieldSelection]) -> Vec<String> {
    fields
        .iter()
        .map(|f| {
            let directives: Vec<&str> = f.directives.iter().map(|d| d.name.as_str()).collect();
            format!("{}{directives:?}", f.response_key())
        })
        .collect()
}

#[test]
fn an_untyped_inline_fragments_fields_are_the_sets_own() {
    assert_eq!(users("{ users { id ... { name } } }"), ["id[]", "name[]"]);
    assert_eq!(users("{ users { ... { ... { name } } id } }"), ["name[]", "id[]"]);
    assert_eq!(users("{ users { ... { n: name email } } }"), ["n[]", "email[]"]);
}

#[test]
fn an_untyped_inline_fragments_directives_travel_onto_its_fields() {
    assert_eq!(
        users(
            "query($on: Boolean!) { users { id ... @include(if: $on) { name email @skip(if: true) } } }"
        ),
        ["id[]", "name[\"include\"]", "email[\"include\", \"skip\"]"]
    );
    assert_eq!(users("{ users { ... @defer(label: \"d\") { name } } }"), ["name[\"defer\"]"]);
}

#[test]
fn a_typed_inline_fragment_stays_a_fragment() {
    assert_eq!(users("{ users { id ... on User { name } } }"), ["id[]", "...on User[]"]);
    let parsed = parse_query("{ users { ... on User { ... { name } } } }").unwrap();
    assert_eq!(describe(&parsed.selections[0].nested_fields[0].nested_fields), ["name[]"]);
}
