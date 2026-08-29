//! `_entities` must honour the router's selection set inside a **nested** object
//! field, against **real PostgreSQL** (#1196).
//!
//! The same selection through an ordinary query projects exactly; `_entities`
//! returned every key of the nested JSONB sub-object. That is precisely the shape
//! an *extended* entity has — a subgraph contributes a field to an entity it does
//! not own, and that field is usually a list — so the over-return is not an edge
//! case of federation, it is federation's normal case.
//!
//! Two consequences, and the fixture is built to show both:
//!
//! * a router forwards the subgraph's response, so clients receive fields they did not ask for —
//!   and where the entity carries anything sensitive, the whole row crosses the subgraph boundary
//!   on a query for one field;
//! * the fields selected *inside* the nested object also landed on the **parent** as bare nulls —
//!   `orders { total }` put `"total": null` on `User`, which has no `total` field at all.
//!
//! #1196 left open whether those are one fault or two. They are one: the
//! `_entities` selection parser is a character scanner that flattens the whole
//! selection set into a single depth-less list, so `orders` is requested at the
//! top level (whole sub-blob) *and* `total` is requested on `User` (null).
//!
//! Every `orders` row here carries five keys, so a projection that degraded to
//! "return the sub-blob" is visible in the response rather than inferred.
//!
//! Skips cleanly when no Postgres is configured; runs on the Dagger
//! `integration --suite=postgres` leg.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip notes are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::Executor,
    schema::{
        CompiledSchema, FederationConfig, FederationEntity, FieldDefinition, FieldType,
        TypeDefinition,
    },
};
use serde_json::{Value, json};

use super::common;

const USER_RELATION: &str = "v_entity_nested_user";

/// `User`, federated by `id`, extended here with a list of `Order`s — the shape a
/// subgraph contributes to an entity it does not own.
fn user_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name: "User".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    });

    schema.types.push({
        let mut t = TypeDefinition::new("User", USER_RELATION);
        t.fields = vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new(
                "orders",
                FieldType::List(Box::new(FieldType::Object("Order".to_string()))),
            ),
        ];
        t
    });
    schema.types.push({
        let mut t = TypeDefinition::new("Order", "v_entity_nested_order");
        t.fields = vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new("user_id", FieldType::String),
            FieldDefinition::new("status", FieldType::String),
            FieldDefinition::new("total", FieldType::Float),
            FieldDefinition::new("created_at", FieldType::String),
        ];
        t
    });
    schema.build_indexes();
    schema
}

/// One user owning two orders, each carrying **five** keys.
async fn fixture() -> Option<(fraiseql_test_support::Service, Executor<PostgresAdapter>)> {
    let row: HashMap<String, Value> = std::iter::once((
        "data".to_string(),
        json!({
            "id": "u-1",
            "orders": [
                {"id": "o-1", "user_id": "u-1", "status": "completed",
                 "total": 99.99, "created_at": "2026-08-25T16:50:12Z"},
                {"id": "o-2", "user_id": "u-1", "status": "pending",
                 "total": 10.50, "created_at": "2026-08-26T09:01:00Z"}
            ]
        }),
    ))
    .collect();
    let (pg, adapter) = common::pg_entity_fixture(USER_RELATION, &["data jsonb"], &[row]).await?;
    Some((pg, Executor::new(user_schema(), Arc::clone(&adapter))))
}

fn entities_query(fields: &str) -> String {
    format!(
        r#"{{ _entities(representations: [{{ __typename: "User", id: "u-1" }}]) {{ ... on User {{ {fields} }} }} }}"#
    )
}

fn representations() -> Value {
    json!({ "representations": [{ "__typename": "User", "id": "u-1" }] })
}

async fn entity(executor: &Executor<PostgresAdapter>, fields: &str) -> Value {
    executor
        .execute(&entities_query(fields), Some(&representations()))
        .await
        .expect("the _entities query must resolve")["data"]["_entities"][0]
        .clone()
}

/// Keys of `orders[0]`, sorted so the assertion does not depend on JSONB key order.
fn order_keys(entity: &Value) -> Vec<String> {
    let mut keys: Vec<String> = entity["orders"][0]
        .as_object()
        .unwrap_or_else(|| panic!("expected an object at orders[0], got: {entity}"))
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// One field selected inside the nested list — one field returned.
#[tokio::test]
async fn a_nested_object_returns_only_the_selected_fields() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP a_nested_object_returns_only_the_selected_fields: no postgres");
        return;
    };

    let user = entity(&executor, "id orders { id }").await;
    assert_eq!(
        order_keys(&user),
        vec!["id".to_string()],
        "each order carries five stored keys; the router asked for one: {user}"
    );
}

/// Several fields selected — exactly those, and still not the other two.
#[tokio::test]
async fn a_nested_object_returns_exactly_the_selected_subset() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP a_nested_object_returns_exactly_the_selected_subset: no postgres");
        return;
    };

    let user = entity(&executor, "id orders { id status }").await;
    assert_eq!(
        order_keys(&user),
        vec!["id".to_string(), "status".to_string()],
        "response was: {user}"
    );
}

/// The parent half of the same fault: a field selected *inside* the nested object
/// must not appear on the entity that has no such field.
#[tokio::test]
async fn a_field_selected_inside_a_nested_object_does_not_land_on_the_parent() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!(
            "SKIP a_field_selected_inside_a_nested_object_does_not_land_on_the_parent: no postgres"
        );
        return;
    };

    let user = entity(&executor, "id orders { id status total }").await;
    let parent = user.as_object().expect("entity is an object");
    assert!(
        !parent.contains_key("total"),
        "`total` is a field of Order, not of User — the selection parser flattened it \
         onto the parent: {user}"
    );
    assert!(!parent.contains_key("status"), "`status` is likewise not a User field: {user}");
}

/// **Control.** Every element of the list is projected, not just the first, and
/// the list keeps its length — so a fix cannot pass by truncating.
#[tokio::test]
async fn every_element_of_the_nested_list_is_projected() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP every_element_of_the_nested_list_is_projected: no postgres");
        return;
    };

    let user = entity(&executor, "id orders { id }").await;
    let orders = user["orders"].as_array().unwrap_or_else(|| panic!("expected a list: {user}"));
    assert_eq!(orders.len(), 2, "both stored orders must survive projection: {user}");
    for (i, order) in orders.iter().enumerate() {
        let keys: Vec<&String> = order.as_object().expect("order is an object").keys().collect();
        assert_eq!(keys, vec!["id"], "orders[{i}] was: {order}");
    }
}

/// **Control.** The scalar path was never the defect and must keep working —
/// `__typename` included, which the resolver injects rather than reads.
#[tokio::test]
async fn scalar_fields_and_typename_are_unaffected() {
    let Some((_pg, executor)) = fixture().await else {
        eprintln!("SKIP scalar_fields_and_typename_are_unaffected: no postgres");
        return;
    };

    let user = entity(&executor, "__typename id").await;
    assert_eq!(user["__typename"], json!("User"), "response was: {user}");
    assert_eq!(user["id"], json!("u-1"), "response was: {user}");
}
