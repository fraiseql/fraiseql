//! Which types a payload selection is classified against, at which position.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{PayloadPosition, payload_roots};
use crate::{
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition, UnionDefinition},
};

fn select(name: &str, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name: name.to_string(),
        alias: None,
        arguments: Vec::new(),
        nested_fields,
        directives: Vec::new(),
    }
}

/// `Order` and `User` implement `CascadeNode`, `Account` does not; `Locked` and `Missing`
/// are error types; `OrderResult = Order | Locked`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    for (name, node, error) in [
        ("Order", true, false),
        ("User", true, false),
        ("Account", false, false),
        ("Locked", false, true),
        ("Missing", false, true),
    ] {
        let mut t = TypeDefinition::new(name, "");
        t.fields = vec![FieldDefinition::new("id", FieldType::Int)];
        if node {
            t.implements = vec!["CascadeNode".to_string()];
        }
        t.is_error = error;
        schema.types.push(t);
    }
    schema.unions.push(
        UnionDefinition::new("OrderResult")
            .with_members(vec!["Order".to_string(), "Locked".to_string()]),
    );
    schema.build_indexes();
    schema
}

fn names<'r>(
    roots: &'r [(PayloadPosition, String, &[FieldSelection])],
) -> Vec<(PayloadPosition, &'r str)> {
    let mut out: Vec<_> = roots.iter().map(|(p, t, _)| (*p, t.as_str())).collect();
    out.sort_by_key(|(p, t)| (format!("{p:?}"), t.to_string()));
    out
}

#[test]
fn a_union_payload_is_classified_as_every_member_and_every_error_type_once() {
    let schema = schema();
    let selections = [select("id", vec![])];
    let roots = payload_roots(&schema, "OrderResult", false, &selections);
    assert_eq!(
        names(&roots),
        [
            (PayloadPosition::Root, "Locked"),
            (PayloadPosition::Root, "Missing"),
            (PayloadPosition::Root, "Order"),
        ]
    );
}

#[test]
fn a_plain_payload_is_classified_as_its_type_and_every_error_type() {
    let schema = schema();
    let selections = [select("id", vec![])];
    let roots = payload_roots(&schema, "Order", false, &selections);
    assert_eq!(
        names(&roots),
        [
            (PayloadPosition::Root, "Locked"),
            (PayloadPosition::Root, "Missing"),
            (PayloadPosition::Root, "Order"),
        ]
    );
}

#[test]
fn a_cascade_classifies_its_entity_and_each_updated_entity_as_every_cascade_node() {
    let schema = schema();
    let selections = [
        select("entity", vec![select("id", vec![])]),
        select(
            "cascade",
            vec![select(
                "updated",
                vec![select("entity", vec![select("id", vec![])])],
            )],
        ),
    ];
    let roots = payload_roots(&schema, "TouchPayload", true, &selections);
    assert_eq!(
        names(&roots),
        [
            (PayloadPosition::CascadeEntity, "Order"),
            (PayloadPosition::CascadeEntity, "User"),
            (PayloadPosition::Root, "Locked"),
            (PayloadPosition::Root, "Missing"),
            (PayloadPosition::UpdatedEntity, "Order"),
            (PayloadPosition::UpdatedEntity, "User"),
        ]
    );
    // Each position is classified under its own selection.
    let entity = roots.iter().find(|r| r.0 == PayloadPosition::CascadeEntity).unwrap();
    assert!(std::ptr::eq(entity.2, selections[0].nested_fields.as_slice()));
}

#[test]
fn a_cascade_entity_declared_on_its_payload_is_classified_as_that_type_alone() {
    let mut schema = schema();
    let mut payload = TypeDefinition::new("TouchPayload", "");
    payload.fields = vec![FieldDefinition::new(
        "entity",
        FieldType::Object("User".to_string()),
    )];
    schema.types.push(payload);
    schema.build_indexes();
    let selections = [select("entity", vec![select("id", vec![])])];
    let roots = payload_roots(&schema, "TouchPayload", true, &selections);
    let entities: Vec<_> = roots
        .iter()
        .filter(|r| r.0 == PayloadPosition::CascadeEntity)
        .map(|r| &r.1)
        .collect();
    assert_eq!(entities, ["User"]);
}
