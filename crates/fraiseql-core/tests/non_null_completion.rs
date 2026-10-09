#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1522: a `null` in a non-null output position is a field error, and it nulls the nearest
//! nullable ancestor (GraphQL § 6.4.4).
//!
//! `Product.name`, `Maker.title` and `Tag.label` are `String!`; list items are non-null
//! (`products: [Product!]!`, `tags: [Tag!]`). Rows store `name` absent (row 3) or JSON `null`
//! (row 4), and row 2's maker has no `title` and its second tag no `label`. Each read is
//! answered with `data` and `errors` together: every error carries its response path (aliases
//! and list indices), and the value that could not be completed is `null` at the nearest
//! position that allows it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_nn_product` table.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tv_nn_product";

/// `(pk, data)`.
const ROWS: [(i64, &str); 4] = [
    (
        1,
        r#"{"id": "1", "pk": 1, "name": "Apple", "maker": {"id": "m1", "title": "Orchard"}}"#,
    ),
    (
        2,
        r#"{"id": "2", "pk": 2, "name": "Pear", "maker": {"id": "m2"},
            "tags": [{"id": "t1", "label": "ripe"}, {"id": "t2"}]}"#,
    ),
    (3, r#"{"id": "3", "pk": 3}"#),
    (4, r#"{"id": "4", "pk": 4, "name": null}"#),
];

fn schema() -> CompiledSchema {
    let product = TestTypeBuilder::new("Product", TABLE)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_field(FieldDefinition::new("name", FieldType::String))
        .with_field(FieldDefinition::nullable("maker", FieldType::Object("Maker".into())))
        .with_field(FieldDefinition::nullable(
            "tags",
            FieldType::List(Box::new(FieldType::Object("Tag".into()))),
        ))
        .build();
    let tag = TestTypeBuilder::new("Tag", "v_unused_tag")
        .with_simple_field("id", FieldType::Id)
        .with_field(FieldDefinition::new("label", FieldType::String))
        .build();
    let maker = TestTypeBuilder::new("Maker", "v_unused_maker")
        .with_simple_field("id", FieldType::Id)
        .with_field(FieldDefinition::new("title", FieldType::String))
        .build();
    let mut products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    products.auto_params.has_order_by = true;
    products.nullable = false;
    let mut by_id = TestQueryBuilder::new("product", "Product").with_sql_source(TABLE).build();
    by_id.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    by_id.nullable = true;
    let mut strict =
        TestQueryBuilder::new("productStrict", "Product").with_sql_source(TABLE).build();
    strict.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    strict.nullable = false;
    let mut connection = TestQueryBuilder::new("productsConnection", "Product")
        .returns_list(true)
        .with_sql_source(TABLE)
        .relay_cursor_column("pk")
        .build();
    connection.auto_params.has_order_by = true;
    let mut schema = TestSchemaBuilder::new()
        .with_type(product)
        .with_type(maker)
        .with_type(tag)
        .with_query(products)
        .with_query(by_id)
        .with_query(strict)
        .with_query(connection)
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    // The relay types `fraiseql compile` injects for a `relay = true` type
    // (`converter/relay.rs`): `node` is nullable, `edges` and `pageInfo` are not.
    schema.types.extend([
        TestTypeBuilder::new("ProductEdge", "")
            .with_field(FieldDefinition::new("cursor", FieldType::String))
            .with_field(FieldDefinition::nullable("node", FieldType::Object("Product".into())))
            .build(),
        TestTypeBuilder::new("ProductConnection", "")
            .with_field(FieldDefinition::new(
                "edges",
                FieldType::List(Box::new(FieldType::Object("ProductEdge".into()))),
            ))
            .with_field(FieldDefinition::new("pageInfo", FieldType::Object("PageInfo".into())))
            .build(),
        TestTypeBuilder::new("PageInfo", "")
            .with_field(FieldDefinition::new("hasNextPage", FieldType::Boolean))
            .with_field(FieldDefinition::new("hasPreviousPage", FieldType::Boolean))
            .with_field(FieldDefinition::nullable("startCursor", FieldType::String))
            .with_field(FieldDefinition::nullable("endCursor", FieldType::String))
            .build(),
    ]);
    schema.build_indexes();
    schema
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let values: Vec<String> =
        ROWS.iter().map(|(pk, data)| format!("({pk}, '{data}'::jsonb)")).collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (pk bigint, data jsonb)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new_with_relay(schema(), Arc::new(adapter)))
}

async fn run(executor: &Executor, query: &str) -> Value {
    executor.execute(query, None).await.unwrap_or_else(|e| panic!("{query}: {e}"))
}

/// The `(path, message)` of each error in `response`.
fn errors(response: &Value) -> Vec<(Value, String)> {
    response["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("an errors array: {response}"))
        .iter()
        .map(|e| (e["path"].clone(), e["message"].as_str().unwrap_or_default().to_string()))
        .collect()
}

/// `products: [Product!]!` under a non-null root: an item that cannot be completed nulls
/// the list, the list nulls `data`; each violation is reported at its own path.
#[tokio::test]
async fn a_null_in_a_non_null_field_of_a_non_null_list_nulls_data() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1522: DATABASE_URL not set");
        return;
    };
    let response = run(&executor, "{ products(orderBy: {pk: ASC}) { id name } }").await;
    assert_eq!(response["data"], Value::Null, "{response}");
    let errors = errors(&response);
    assert_eq!(
        errors.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
        vec![
            json!(["products", 2, "name"]),
            json!(["products", 3, "name"])
        ],
        "{response}"
    );
    assert!(errors[0].1.contains("Product.name"), "names the field: {errors:?}");
}

/// Aliases are response keys, and the path is written in them.
#[tokio::test]
async fn the_error_path_uses_aliases() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(&executor, "{ p: products(orderBy: {pk: ASC}) { n: name } }").await;
    assert_eq!(errors(&response)[0].0, json!(["p", 2, "n"]), "{response}");
}

/// A nested object: the nearest nullable ancestor of `maker.title` is `maker`.
#[tokio::test]
async fn a_null_inside_a_nullable_object_nulls_the_object() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(&executor, r#"{ product(id: "2") { id maker { id title } } }"#).await;
    assert_eq!(response["data"]["product"], json!({"id": "2", "maker": null}), "{response}");
    assert_eq!(errors(&response)[0].0, json!(["product", "maker", "title"]), "{response}");
}

/// `tags: [Tag!]` is nullable: an incomplete tag nulls the list, and the product stands.
#[tokio::test]
async fn a_null_inside_a_nullable_list_field_nulls_the_list() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(&executor, r#"{ product(id: "2") { id tags { id label } } }"#).await;
    assert_eq!(response["data"]["product"], json!({"id": "2", "tags": null}), "{response}");
    assert_eq!(errors(&response)[0].0, json!(["product", "tags", 1, "label"]), "{response}");
}

/// A non-null root field: the violation propagates to `data` itself.
#[tokio::test]
async fn a_null_under_a_non_null_root_field_nulls_data() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(&executor, r#"{ productStrict(id: "3") { id name } }"#).await;
    assert_eq!(response["data"], Value::Null, "{response}");
    assert_eq!(errors(&response)[0].0, json!(["productStrict", "name"]), "{response}");
}

/// A relay connection as compiled: `node: Product` is nullable, so a node that cannot be
/// completed is `null` in its edge, and the edge, `edges` and the connection stand.
#[tokio::test]
async fn a_null_inside_a_relay_node_nulls_the_node() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(
        &executor,
        "{ productsConnection(first: 10, orderBy: {pk: ASC}) { edges { node { id name } } } }",
    )
    .await;
    let nodes: Vec<Value> = response["data"]["productsConnection"]["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|edge| edge["node"].clone())
        .collect();
    assert_eq!(
        nodes,
        vec![
            json!({"id": "1", "name": "Apple"}),
            json!({"id": "2", "name": "Pear"}),
            Value::Null,
            Value::Null
        ],
        "{response}"
    );
    assert_eq!(
        errors(&response).iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
        vec![
            json!(["productsConnection", "edges", 2, "node", "name"]),
            json!(["productsConnection", "edges", 3, "node", "name"]),
        ],
        "{response}"
    );
}

/// A complete response is unchanged: no `errors` key at all.
#[tokio::test]
async fn a_complete_response_carries_no_errors() {
    let Some(executor) = executor().await else {
        return;
    };
    let response = run(&executor, r#"{ product(id: "1") { id name maker { title } } }"#).await;
    assert!(response.get("errors").is_none(), "{response}");
    assert_eq!(response["data"]["product"]["maker"]["title"], json!("Orchard"));
}
