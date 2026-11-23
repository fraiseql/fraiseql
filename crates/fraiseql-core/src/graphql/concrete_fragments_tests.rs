#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::splice_query_roots;
use crate::{
    graphql::{FieldSelection, parse_query},
    schema::{CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition},
};

/// `User: Node { id name partner: Node friends: [User] }`, and the queries `users` (a list
/// of them), `usersConnection` (a Relay connection) and `userCount` (a count).
fn schema() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", "v_user");
    user.implements = vec!["Node".into()];
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("name", FieldType::String),
        FieldDefinition::nullable("partner", FieldType::Interface("Node".into())),
        FieldDefinition::nullable("friends", FieldType::list(FieldType::Object("User".into()))),
    ];
    s.types.push(user);
    s.queries.push(QueryDefinition::new("users", "User").returning_list());
    let mut connection = QueryDefinition::new("usersConnection", "User").returning_list();
    connection.relay = true;
    s.queries.push(connection);
    let mut count = QueryDefinition::new("userCount", "User");
    count.returns_count = true;
    s.queries.push(count);
    s.build_indexes();
    s
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

/// The first root's selection set in `document`, spliced, as `name[directives]`.
fn spliced(document: &str) -> Vec<String> {
    let roots = splice_query_roots(parse_query(document).unwrap().selections, &schema());
    describe(&roots[0].nested_fields)
}

#[test]
fn a_fragment_on_the_type_or_an_interface_it_implements_is_its_fields() {
    assert_eq!(spliced("{ users { id ... on User { name } } }"), ["id[]", "name[]"]);
    assert_eq!(spliced("{ users { ... on Node { id } n: name } }"), ["id[]", "n[]"]);
    assert_eq!(
        spliced("{ users { ... on Node { ... on User { name } id } } }"),
        ["name[]", "id[]"]
    );
}

#[test]
fn a_fragments_directives_travel_onto_its_fields() {
    assert_eq!(
        spliced("{ users { ... on User @skip(if: true) { id name @include(if: true) } } }"),
        ["id[\"skip\"]", "name[\"skip\", \"include\"]"]
    );
}

#[test]
fn a_fragment_below_an_object_field_is_its_fields() {
    let roots = splice_query_roots(
        parse_query("{ users { friends { ... on User { name } } } }")
            .unwrap()
            .selections,
        &schema(),
    );
    assert_eq!(describe(&roots[0].nested_fields[0].nested_fields), ["name[]"]);
}

#[test]
fn a_fragment_at_an_abstract_position_or_that_cannot_apply_is_left_as_written() {
    let roots = splice_query_roots(
        parse_query("{ users { partner { ... on User { name } } } }")
            .unwrap()
            .selections,
        &schema(),
    );
    assert_eq!(describe(&roots[0].nested_fields[0].nested_fields), ["...on User[]"]);
    assert_eq!(spliced("{ users { ... on Post { id } } }"), ["...on Post[]"]);
}

#[test]
fn a_root_that_reads_no_object_set_is_left_as_written() {
    assert_eq!(
        spliced("{ usersConnection { edges { node { ... on User { id } } } } }"),
        ["edges[]"]
    );
    // `... on Node` would apply to a `User`: the connection's set is not a `User`'s.
    assert_eq!(spliced("{ usersConnection { ... on Node { id } } }"), ["...on Node[]"]);
    assert_eq!(spliced("{ userCount { ... on User { id } } }"), ["...on User[]"]);
    assert_eq!(spliced("{ nothing { ... on User { id } } }"), ["...on User[]"]);
}
