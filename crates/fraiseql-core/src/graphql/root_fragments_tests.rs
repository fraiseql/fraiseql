#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use crate::graphql::{ParsedQuery, parse_query, parser::GraphQLParseError};

fn parsed(document: &str) -> ParsedQuery {
    parse_query(document).unwrap_or_else(|e| panic!("{document}: {e}"))
}

/// The root's fields as `name[directives]`, in order.
fn roots(document: &str) -> Vec<String> {
    parsed(document)
        .selections
        .iter()
        .map(|s| {
            let directives: Vec<&str> = s.directives.iter().map(|d| d.name.as_str()).collect();
            format!("{}{directives:?}", s.response_key())
        })
        .collect()
}

fn refusal(document: &str) -> String {
    match parse_query(document) {
        Err(e @ GraphQLParseError::RootFragment(_)) => e.to_string(),
        other => panic!("{document}: {other:?}"),
    }
}

#[test]
fn a_root_fragment_stands_for_its_fields() {
    for document in [
        "{ ... on Query { users { id } posts { id } } }",
        "{ ... { users { id } posts { id } } }",
        "{ ...Q } fragment Q on Query { users { id } posts { id } }",
        "{ ...Q } fragment Q on Query { ...R } fragment R on Query { users { id } posts { id } }",
        "{ users { id } ... on Query { ... { posts { id } } } }",
    ] {
        assert_eq!(roots(document), ["users[]", "posts[]"], "{document}");
        assert_eq!(parsed(document).root_field, "users", "{document}");
    }
    assert_eq!(roots("mutation { ... on Mutation { a: createUser { id } } }"), ["a[]"]);
    assert_eq!(
        roots("subscription { ...S } fragment S on Subscription { orderUpdated { id } }"),
        ["orderUpdated[]"]
    );
}

#[test]
fn a_root_fragments_directives_travel_onto_its_fields() {
    let document = "query($on: Boolean!) { a ...Q @include(if: $on) } fragment Q on Query { b c @skip(if: true) }";
    assert_eq!(roots(document), ["a[]", "b[\"include\"]", "c[\"include\", \"skip\"]"]);
    assert_eq!(
        roots("{ ... @skip(if: true) { ... on Query @include(if: true) { a } } }"),
        ["a[\"skip\", \"include\"]"]
    );
}

#[test]
fn a_document_with_no_root_fragment_is_read_as_written() {
    let document = "{ users { id ... on User { name } ...F } } fragment F on User { email }";
    let parsed = parsed(document);
    assert_eq!(parsed.selections[0].nested_fields.len(), 3);
    assert_eq!(parsed.selections[0].nested_fields[2].name, "...F");
}

#[test]
fn a_root_fragment_the_root_cannot_hold_is_refused_naming_it() {
    for (document, why) in [
        (
            "{ ... on User { id } }",
            "an inline fragment on `User`, whose fields are not `Query`'s",
        ),
        ("{ a ...U } fragment U on User { id }", "fragment `U` is on `User`"),
        ("{ ... on Mutation { a } }", "an inline fragment on `Mutation`"),
        (
            "mutation { ... on Query { a } }",
            "at the root of a mutation, a fragment selects fields of `Mutation`",
        ),
        ("{ ...Missing }", "fragment `Missing` is spread and not defined"),
        (
            "{ ...A } fragment A on Query { ...B } fragment B on Query { ...A }",
            "spreads itself",
        ),
    ] {
        let message = refusal(document);
        assert!(message.contains(why), "{document}: {message}");
        assert!(message.contains("at the root of a"), "{document}: {message}");
    }
}
