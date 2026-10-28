//! `_entities` answers a `Decimal` with its stored digits, as a regular read does, against
//! **real PostgreSQL**.
//!
//! A `Decimal` travels as a JSON string carrying every stored digit: `1.50` keeps its scale
//! and a 23-digit value is not rounded through an `f64`. The entity resolver reads its rows
//! through its own SQL, in jsonb mode (`data->'key'`) and in flat mode (bare columns), so it
//! is pinned on its own, at a scalar, inside a nested object and inside a list.
//!
//! Skips cleanly when no Postgres is configured; runs on the Dagger
//! `integration --suite=postgres` leg.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip notes are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::{
        CompiledSchema, FederationConfig, FederationEntity, FieldDefinition, FieldType,
        QueryDefinition, TypeDefinition,
    },
};
use serde_json::{Value, json};

use super::common;

/// More significant digits than an `f64` holds.
const BIG: &str = "12345678901234567890.123";

/// `Product`, federated by `id`, read from `relation` through its `jsonb_column` (empty for
/// flat columns), with a `products` query over the same relation.
fn schema(relation: &str, jsonb_column: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name: "Product".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    });
    schema.types.push(
        TypeDefinition::new("Product", relation)
            .with_field(FieldDefinition::new("id", FieldType::String))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("price", FieldType::Decimal))
            .with_field(FieldDefinition::new("total", FieldType::Decimal))
            .with_field(FieldDefinition::new("big", FieldType::Decimal))
            .with_field(FieldDefinition::nullable("discount", FieldType::Decimal))
            .with_field(FieldDefinition::new("rates", FieldType::list(FieldType::Decimal)))
            .with_field(FieldDefinition::new("cost", FieldType::Object("Money".to_string())))
            .with_field(FieldDefinition::new(
                "lines",
                FieldType::list(FieldType::Object("Money".to_string())),
            )),
    );
    schema.types.push(
        TypeDefinition::new("Money", "")
            .with_field(FieldDefinition::new("amount", FieldType::Decimal)),
    );
    let mut products = QueryDefinition::new("products", "Product").with_sql_source(relation);
    products.returns_list = true;
    products.jsonb_column = jsonb_column.to_string();
    schema.queries.push(products);
    schema.build_indexes();
    schema
}

/// A `data`-jsonb view `relation`, one per test, holding each Decimal as the number literal it was
/// stored as.
async fn jsonb_fixture(relation: &str) -> Option<(fraiseql_test_support::Service, Executor)> {
    let (pg, adapter) = common::pg_adapter().await?;
    for sql in [
        format!(r#"DROP TABLE IF EXISTS "{relation}" CASCADE"#),
        format!(r#"CREATE TABLE "{relation}" (data jsonb)"#),
        format!(
            r#"INSERT INTO "{relation}" (data) VALUES ('{{"id": "p-1", "name": "Widget",
              "price": 1.50, "total": 100.00, "big": {BIG}, "cost": {{"amount": 0.10}},
              "lines": [{{"amount": 2.50}}, {{"amount": {BIG}}}], "rates": [0.10, {BIG}]}}'::jsonb)"#
        ),
    ] {
        adapter.execute_raw_query(&sql).await.expect("jsonb fixture");
    }
    Some((pg, Executor::new(schema(relation, "data"), Arc::clone(&adapter))))
}

/// A flat view `relation`, one per test: each Decimal a bare `numeric` column.
async fn flat_fixture(relation: &str) -> Option<(fraiseql_test_support::Service, Executor)> {
    let (pg, adapter) = common::pg_adapter().await?;
    for sql in [
        format!(r#"DROP TABLE IF EXISTS "{relation}" CASCADE"#),
        format!(
            r#"CREATE TABLE "{relation}" (id text, name text, price numeric,
              total numeric, big numeric, discount numeric, rates numeric[])"#
        ),
        format!(
            r#"INSERT INTO "{relation}" VALUES
              ('p-1', 'Widget', 1.50, 100.00, {BIG}, NULL, ARRAY[0.10, {BIG}]),
              ('p-nan', 'Unknown', 'NaN', 0, 0, NULL, NULL)"#
        ),
    ] {
        adapter.execute_raw_query(&sql).await.expect("flat fixture");
    }
    Some((pg, Executor::new(schema(relation, ""), Arc::clone(&adapter))))
}

async fn entity(executor: &Executor, id: &str, fields: &str) -> Value {
    let query = format!(
        r"query($representations: [_Any!]!) {{ _entities(representations: $representations) {{ ... on Product {{ {fields} }} }} }}"
    );
    let variables = json!({ "representations": [{ "__typename": "Product", "id": id }] });
    let response = executor
        .execute(&query, Some(&variables))
        .await
        .unwrap_or_else(|e| panic!("the _entities query must resolve: {e}"));
    let entity = response["data"]["_entities"][0].clone();
    assert!(entity.is_object(), "no entity resolved: {response}");
    entity
}

#[tokio::test]
async fn a_jsonb_entity_answers_each_decimal_with_its_stored_digits() {
    let Some((_pg, executor)) = jsonb_fixture("v_entity_decimal_jsonb").await else {
        eprintln!("SKIP a_jsonb_entity_answers_each_decimal_with_its_stored_digits: no postgres");
        return;
    };

    let product = entity(
        &executor,
        "p-1",
        "id name price total big discount rates cost { amount } lines { amount }",
    )
    .await;

    assert_eq!(product["price"], json!("1.50"), "{product}");
    assert_eq!(product["total"], json!("100.00"), "{product}");
    assert_eq!(product["big"], json!(BIG), "{product}");
    assert_eq!(product["cost"], json!({ "amount": "0.10" }), "{product}");
    assert_eq!(product["lines"], json!([{ "amount": "2.50" }, { "amount": BIG }]), "{product}");
    assert_eq!(product["rates"], json!(["0.10", BIG]), "{product}");
    assert_eq!(product["discount"], Value::Null, "a key the document lacks: {product}");
    assert_eq!(product["name"], json!("Widget"), "{product}");
}

#[tokio::test]
async fn a_jsonb_entity_equals_the_same_row_read_through_a_query() {
    let Some((_pg, executor)) = jsonb_fixture("v_entity_decimal_parity").await else {
        eprintln!("SKIP a_jsonb_entity_equals_the_same_row_read_through_a_query: no postgres");
        return;
    };
    let fields = "id name price total big discount rates cost { amount } lines { amount }";

    let mut product = entity(&executor, "p-1", fields).await;
    product.as_object_mut().unwrap().remove("__typename");
    let read = executor
        .execute(&format!("{{ products {{ {fields} }} }}"), None)
        .await
        .unwrap()["data"]["products"][0]
        .clone();

    assert_eq!(serde_json::to_string(&product).unwrap(), serde_json::to_string(&read).unwrap());
}

#[tokio::test]
async fn a_flat_entity_answers_each_decimal_with_its_stored_digits() {
    let Some((_pg, executor)) = flat_fixture("v_entity_decimal_flat").await else {
        eprintln!("SKIP a_flat_entity_answers_each_decimal_with_its_stored_digits: no postgres");
        return;
    };

    let product = entity(&executor, "p-1", "id name price total big discount rates").await;

    assert_eq!(product["price"], json!("1.50"), "{product}");
    assert_eq!(product["total"], json!("100.00"), "{product}");
    assert_eq!(product["big"], json!(BIG), "{product}");
    assert_eq!(product["discount"], Value::Null, "{product}");
    assert_eq!(product["rates"], json!(["0.10", BIG]), "a numeric[] column: {product}");
    assert_eq!(product["name"], json!("Widget"), "{product}");
}

#[tokio::test]
async fn a_flat_numeric_nan_travels_as_the_string_postgres_writes() {
    let Some((_pg, executor)) = flat_fixture("v_entity_decimal_nan").await else {
        eprintln!("SKIP a_flat_numeric_nan_travels_as_the_string_postgres_writes: no postgres");
        return;
    };

    let product = entity(&executor, "p-nan", "id price").await;

    assert_eq!(product["price"], json!("NaN"), "{product}");
}
