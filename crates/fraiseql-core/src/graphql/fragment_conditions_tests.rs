#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use super::check;
use crate::{
    error::FraiseQLError,
    graphql::parse_query,
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition, QueryDefinition,
        TypeDefinition, UnionDefinition,
    },
};

/// `User: Node { id partner: Node pet: Pet }`, `Ledger: Node { id }`, `Tag { label }`, the
/// union `Pet = Dog`, `Dog { name }`, and the queries `users`, `usersConnection` (Relay).
fn schema() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    let object = |name: &str, implements: &[&str], fields: Vec<FieldDefinition>| {
        let mut t = TypeDefinition::new(name, format!("v_{name}"));
        t.implements = implements.iter().map(|i| (*i).to_string()).collect();
        t.fields = fields;
        t
    };
    s.types.push(object(
        "User",
        &["Node"],
        vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::nullable("partner", FieldType::Interface("Node".into())),
            FieldDefinition::nullable("pet", FieldType::Union("Pet".into())),
        ],
    ));
    s.types
        .push(object("Ledger", &["Node"], vec![FieldDefinition::new("id", FieldType::Id)]));
    s.types
        .push(object("Tag", &[], vec![FieldDefinition::new("label", FieldType::String)]));
    s.types
        .push(object("Dog", &[], vec![FieldDefinition::new("name", FieldType::String)]));
    s.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    s.unions.push(UnionDefinition {
        name:         "Pet".into(),
        member_types: vec!["Dog".into()],
        description:  None,
    });
    s.queries.push(QueryDefinition::new("users", "User").returning_list());
    let mut connection = QueryDefinition::new("usersConnection", "User").returning_list();
    connection.relay = true;
    s.queries.push(connection);
    s.build_indexes();
    s
}

fn checked(document: &str) -> Result<(), String> {
    let parsed = parse_query(document).unwrap();
    let schema = schema();
    match check(&parsed.selections, &parsed.fragments, &parsed.operation_type, &schema, &schema) {
        Ok(()) => Ok(()),
        Err(FraiseQLError::Validation { message, path }) => {
            Err(format!("{} @ {}", message, path.unwrap_or_default()))
        },
        Err(other) => panic!("{document}: {other:?}"),
    }
}

#[test]
fn a_fragment_that_can_apply_where_it_stands_is_accepted() {
    for document in [
        "{ users { ... on User { id } ... on Node { id } } }",
        "{ users { partner { ... on Ledger { id } ... on User { id } ... on Node { id } } } }",
        "{ users { pet { ... on Dog { name } ... on Pet { __typename } } } }",
        "{ node(id: \"x\") { ... on Ledger { id } ...U } } fragment U on User { id }",
        "{ usersConnection { edges { node { ... on User { id } } } nodes { ... on Node { id } } } }",
        "{ users { ... { ... on User { id } } } }",
        "{ users { ...A } } fragment A on User { ...A }",
    ] {
        assert_eq!(checked(document), Ok(()), "{document}");
    }
}

#[test]
fn a_fragment_on_a_type_the_schema_does_not_have_is_refused() {
    for (document, path) in [
        ("{ users { ... on Nosuch { id } } }", "users"),
        ("{ node(id: \"x\") { ... on Nosuch { nope } } }", "node"),
        ("{ users { partner { ...F } } } fragment F on Nosuch { id }", "users.partner"),
        (
            "{ usersConnection { edges { node { ... on Nosuch { id } } } } }",
            "usersConnection.edges.node",
        ),
        ("{ users { pet { ... on Dog { ... on Nosuch { name } } } } }", "users.pet"),
        (
            "{ usersConnection { nodes { ... on Nosuch { id } } } }",
            "usersConnection.nodes",
        ),
    ] {
        let refusal = checked(document).unwrap_err();
        assert!(refusal.starts_with("Unknown type `Nosuch`"), "{document}: {refusal}");
        assert!(refusal.ends_with(&format!("@ {path}")), "{document}: {refusal}");
    }
}

#[test]
fn a_fragment_that_can_never_apply_where_it_stands_is_refused() {
    for (document, what) in [
        ("{ users { ... on Ledger { id } } }", "the inline fragment on `Ledger`"),
        ("{ users { ... on Tag { label } } }", "the inline fragment on `Tag`"),
        ("{ node(id: \"x\") { ... on Tag { label } } }", "the inline fragment on `Tag`"),
        ("{ users { partner { ... on Dog { name } } } }", "the inline fragment on `Dog`"),
        ("{ users { pet { ... on User { id } } } }", "the inline fragment on `User`"),
        ("{ users { ...T } } fragment T on Tag { label }", "fragment `T`, on `Tag`,"),
        (
            "{ users { pet { ...N } } } fragment N on Node { id }",
            "fragment `N`, on `Node`,",
        ),
    ] {
        let refusal = checked(document).unwrap_err();
        assert!(refusal.starts_with(what), "{document}: {refusal}");
        assert!(refusal.contains("can never apply here"), "{document}: {refusal}");
    }
}

#[test]
fn a_position_the_schema_does_not_know_is_not_judged() {
    assert_eq!(checked("{ nothing { ... on Nosuch { id } } }"), Ok(()));
    assert_eq!(checked("{ users { ghost { ... on Nosuch { id } } } }"), Ok(()));
}
