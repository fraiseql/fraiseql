#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::check;
use crate::{
    error::FraiseQLError,
    graphql::parse_query,
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition, MutationDefinition,
        QueryDefinition, TypeDefinition, UnionDefinition,
    },
};

/// `User: Node { id name count tags posts friend pet }`, `Post: Node { id title }`, the
/// union `Pet = Dog | Cat` with `Dog { name nick tags lives friend }` and
/// `Cat { name nick tags lives friend }` (whose fields differ in type and wrapping), the
/// queries `users` and `usersConnection` (Relay), and the mutation `createUser`.
fn schema() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    let object = |name: &str, implements: &[&str], fields: Vec<FieldDefinition>| {
        let mut t = TypeDefinition::new(name, format!("v_{name}"));
        t.implements = implements.iter().map(|i| (*i).to_string()).collect();
        t.fields = fields;
        t
    };
    let user = FieldType::Object("User".into());
    s.types.push(object(
        "User",
        &["Node"],
        vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("name", FieldType::String),
            FieldDefinition::new("count", FieldType::Int),
            FieldDefinition::nullable("tags", FieldType::list(FieldType::String)),
            FieldDefinition::new("posts", FieldType::list(FieldType::Object("Post".into()))),
            FieldDefinition::nullable("friend", user.clone()),
            FieldDefinition::nullable("pet", FieldType::Union("Pet".into())),
        ],
    ));
    s.types.push(object(
        "Post",
        &["Node"],
        vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("title", FieldType::String),
        ],
    ));
    for pet in ["Dog", "Cat"] {
        s.types.push(object(
            pet,
            &[],
            vec![
                FieldDefinition::new("name", FieldType::String),
                FieldDefinition::nullable("nick", FieldType::String),
                FieldDefinition::new("alias", FieldType::String),
                FieldDefinition::nullable("tags", FieldType::list(FieldType::String)),
                FieldDefinition::nullable(
                    "looseTags",
                    FieldType::list_of_nullable(FieldType::String),
                ),
                FieldDefinition::new("lives", FieldType::Int),
                FieldDefinition::nullable("friend", user.clone()),
            ],
        ));
    }
    s.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    s.unions.push(UnionDefinition {
        name:         "Pet".into(),
        member_types: vec!["Dog".into(), "Cat".into()],
        description:  None,
    });
    s.queries.push(QueryDefinition::new("users", "User").returning_list());
    let mut connection = QueryDefinition::new("usersConnection", "User").returning_list();
    connection.relay = true;
    s.queries.push(connection);
    s.mutations.push(MutationDefinition::new("createUser", "User"));
    s.build_indexes();
    s
}

/// `Ok` when `document` merges, or the refusal as `message @ path`.
fn checked(document: &str) -> Result<(), String> {
    let parsed = parse_query(document).unwrap();
    match check(&parsed.selections, &parsed.fragments, &parsed.operation_type, &schema()) {
        Ok(()) => Ok(()),
        Err(FraiseQLError::Validation { message, path }) => {
            Err(format!("{message} @ {}", path.unwrap_or_default()))
        },
        Err(other) => panic!("{document}: {other:?}"),
    }
}

/// `document` is refused at `path`, with a § 5.3.2 message naming `key` and `why`.
fn refused(document: &str, key: &str, why: &str, path: &str) {
    let refusal = checked(document).expect_err(document);
    assert!(refusal.contains(&format!("Fields \"{key}\" conflict")), "{document}: {refusal}");
    assert!(refusal.contains(why), "{document}: {refusal}");
    assert!(refusal.contains("§ 5.3.2"), "{document}: {refusal}");
    assert!(refusal.ends_with(&format!(" @ {path}")), "{document}: {refusal}");
}

// ── one key, two fields or two argument sets ────────────────────────────────────────

#[test]
fn one_key_naming_two_fields_is_refused() {
    refused(
        "{ users { a: id a: name } }",
        "a",
        "\"id\" and \"name\" are different fields",
        "users.a",
    );
}

#[test]
fn one_field_with_two_argument_sets_is_refused() {
    refused(
        "{ users { posts(limit: 1) { id } posts(limit: 2) { title } } }",
        "posts",
        "different arguments",
        "users.posts",
    );
    refused(
        "{ users { posts(limit: 1) { id } posts { title } } }",
        "posts",
        "different arguments",
        "users.posts",
    );
}

#[test]
fn the_same_field_with_the_same_arguments_merges() {
    assert_eq!(
        checked("{ users { posts(limit: 1) { id } posts(limit: 1) { title } } }"),
        Ok(())
    );
    assert_eq!(
        checked("{ users { posts(a: 1, b: 2) { id } posts(b: 2, a: 1) { id } } }"),
        Ok(())
    );
    assert_eq!(checked("{ users { id id name: name } }"), Ok(()));
}

#[test]
fn the_rule_holds_at_the_roots_and_on_every_operation() {
    refused(
        "{ a: users { id } a: usersConnection { nodes { id } } }",
        "a",
        "different fields",
        "a",
    );
    refused(
        "mutation { createUser { a: id a: name } }",
        "a",
        "different fields",
        "createUser.a",
    );
}

// ── per parent type ────────────────────────────────────────────────────────────────────

#[test]
fn fragments_on_distinct_union_members_may_alias_different_fields_to_one_key() {
    assert_eq!(
        checked("{ users { pet { ... on Dog { x: name } ... on Cat { x: alias } } } }"),
        Ok(())
    );
    assert_eq!(
        checked(
            "{ users { pet { ...D ...C } } } \
             fragment D on Dog { x: name } fragment C on Cat { x: alias }"
        ),
        Ok(())
    );
}

#[test]
fn fragments_on_one_object_type_are_one_parent() {
    refused(
        "{ users { ... on User { x: id } ... on User { x: name } } }",
        "x",
        "\"id\" and \"name\" are different fields",
        "users.x",
    );
    refused(
        "{ users { pet { ...D } } } fragment D on Dog { x: name ... on Dog { x: alias } }",
        "x",
        "different fields",
        "users.pet.x",
    );
}

#[test]
fn a_fragment_on_an_interface_or_untyped_is_no_distinct_parent() {
    refused(
        "{ users { ... on Node { x: id } x: name } }",
        "x",
        "different fields",
        "users.x",
    );
    refused("{ users { ... { x: id } x: name } }", "x", "different fields", "users.x");
}

// ── response shape ─────────────────────────────────────────────────────────────────────

#[test]
fn distinct_union_members_may_not_give_one_key_two_leaf_types() {
    refused(
        "{ users { pet { ... on Dog { x: name } ... on Cat { x: lives } } } }",
        "x",
        "conflicting types \"String!\" and \"Int!\"",
        "users.pet.x",
    );
}

#[test]
fn distinct_union_members_may_not_differ_in_nullability_or_list_wrapping() {
    refused(
        "{ users { pet { ... on Dog { x: name } ... on Cat { x: nick } } } }",
        "x",
        "conflicting types \"String!\" and \"String\"",
        "users.pet.x",
    );
    refused(
        "{ users { pet { ... on Dog { x: nick } ... on Cat { x: tags } } } }",
        "x",
        "conflicting types \"String\" and \"[String!]\"",
        "users.pet.x",
    );
    refused(
        "{ users { pet { ... on Dog { x: tags } ... on Cat { x: looseTags } } } }",
        "x",
        "conflicting types \"[String!]\" and \"[String]\"",
        "users.pet.x",
    );
}

// ── below the first level ──────────────────────────────────────────────────────────────

#[test]
fn sub_selections_of_one_key_merge_by_the_same_rule() {
    refused(
        "{ users { posts { x: id } posts { x: title } } }",
        "x",
        "\"id\" and \"title\" are different fields",
        "users.posts.x",
    );
    refused(
        "{ users { friend { ...F } friend { x: name } } } fragment F on User { x: id }",
        "x",
        "different fields",
        "users.friend.x",
    );
}

#[test]
fn below_distinct_union_members_fields_may_differ_but_shapes_may_not() {
    assert_eq!(
        checked(
            "{ users { pet { ... on Dog { friend { x: id } } ... on Cat { friend { x: id } } } } }"
        ),
        Ok(())
    );
    refused(
        "{ users { pet { ... on Dog { friend { x: id } } ... on Cat { friend { x: name } } } } }",
        "x",
        "conflicting types \"ID!\" and \"String\"",
        "users.pet.friend.x",
    );
}

#[test]
fn a_relay_connections_nodes_are_held_to_the_rule() {
    refused(
        "{ usersConnection { edges { node { x: id x: name } } } }",
        "x",
        "different fields",
        "usersConnection.edges.node.x",
    );
    refused(
        "{ usersConnection { nodes { x: id x: name } } }",
        "x",
        "different fields",
        "usersConnection.nodes.x",
    );
}

// ── what the check cannot judge, and repeated fragments ────────────────────────────────

#[test]
fn a_field_or_type_the_schema_does_not_know_is_not_judged_on_shape() {
    assert_eq!(
        checked("{ users { pet { ... on Dog { x: unknown } ... on Cat { x: lives } } } }"),
        Ok(())
    );
    // Below a root or a field the schema does not have, no parent type is known.
    assert_eq!(checked("{ notAQuery { a: x a: y } }"), Ok(()));
    assert_eq!(checked("{ users { unknown { a: x a: y } } }"), Ok(()));
}

#[test]
fn a_fragment_spread_twice_is_checked_as_one_selection() {
    assert_eq!(
        checked("{ users { ...F ...F friend { ...F } } } fragment F on User { x: id }"),
        Ok(())
    );
}

#[test]
fn a_fragment_at_the_root_selects_root_fields() {
    refused(
        "{ a: users { id } ...Q } fragment Q on Query { a: usersConnection { nodes { id } } }",
        "a",
        "different fields",
        "a",
    );
    refused(
        "{ ...Q users { a: id } } fragment Q on Query { users { a: name } }",
        "a",
        "different fields",
        "users.a",
    );
}

#[test]
fn typename_answers_a_non_null_string() {
    assert_eq!(
        checked("{ users { pet { ... on Dog { x: __typename } ... on Cat { x: alias } } } }"),
        Ok(())
    );
    refused(
        "{ users { pet { ... on Dog { x: __typename } ... on Cat { x: nick } } } }",
        "x",
        "conflicting types \"String!\" and \"String\"",
        "users.pet.x",
    );
}
