#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1533: a relay connection that seeks its index with a row comparison still returns every
//! row once, in its order, and one that must not seek keeps the expanded predicate.
//!
//! The row comparison `(k, pk) > ($1, $2)` is taken only when every sort key is a native
//! column proven `NOT NULL`, all ascending. Each walk here is compared with the order
//! PostgreSQL itself gives the same `ORDER BY`, tie-broken by the cursor column, at page sizes
//! that put a boundary between every pair of tied rows:
//!
//! * a proven `NOT NULL` key with duplicates (seeks);
//! * a native key nothing proves, holding NULLs (a row comparison would drop the NULL rows);
//! * the proven key descending (the position sorts ascending, so one comparison cannot read both);
//! * a document key holding NULLs, with no native column at all;
//! * the same proven column behind a view, which PostgreSQL reports nullable.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_seek_item` table and `v_seek_item` view.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldDefinition, FieldType, NativeColumn},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tv_seek_item";
const VIEW: &str = "v_seek_item";

/// `(pk, word, note)`: words tied across page boundaries, notes holding NULLs.
const ROWS: [(i64, &str, Option<&str>); 9] = [
    (1, "b", Some("x")),
    (2, "a", None),
    (3, "b", Some("w")),
    (4, "c", Some("x")),
    (5, "a", Some("y")),
    (6, "b", None),
    (7, "a", Some("w")),
    (8, "c", None),
    (9, "b", Some("y")),
];

fn connection(
    name: &str,
    source: &str,
    native: &[(&str, NativeColumn)],
) -> fraiseql_core::schema::QueryDefinition {
    let mut query = TestQueryBuilder::new(name, "Item")
        .returns_list(true)
        .with_sql_source(source)
        .relay_cursor_column("pk")
        .build();
    query.auto_params.has_order_by = true;
    for (column, native) in native {
        query.native_columns.insert((*column).to_string(), native.clone());
    }
    query
}

/// The native columns as `compile --database` records them: proven on the table's `NOT NULL`
/// column, never on the view's.
fn schema() -> CompiledSchema {
    let item = TestTypeBuilder::new("Item", TABLE)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_simple_field("word", FieldType::String)
        .with_field(FieldDefinition::nullable("note", FieldType::String))
        .build();
    let proven = NativeColumn {
        pg_type:  "text".to_string(),
        not_null: true,
    };
    let mut schema = TestSchemaBuilder::new()
        .with_type(item)
        .with_query(connection(
            "itemsConnection",
            TABLE,
            &[("word", proven), ("note", NativeColumn::nullable("text"))],
        ))
        .with_query(connection("documentItemsConnection", TABLE, &[]))
        .with_query(connection(
            "viewItemsConnection",
            VIEW,
            &[("word", NativeColumn::nullable("text"))],
        ))
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.build_indexes();
    schema
}

async fn setup() -> Option<(PostgresAdapter, Executor)> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let values: Vec<String> = ROWS
        .iter()
        .map(|(pk, word, note)| {
            let note = note.map_or_else(|| "null".to_string(), |n| format!("'{n}'"));
            format!(
                "({pk}, '{word}', {note}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'word', \
                 '{word}', 'note', {note}::text))"
            )
        })
        .collect();
    for ddl in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (pk bigint PRIMARY KEY, word text NOT NULL, note text, data \
             jsonb NOT NULL)"
        ),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
        format!("CREATE VIEW {VIEW} AS SELECT pk, word, note, data FROM {TABLE}"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let executor =
        Executor::new_with_relay(schema(), Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

/// The pks in the order PostgreSQL gives `order_sql`.
async fn oracle(adapter: &PostgresAdapter, order_sql: &str) -> Vec<i64> {
    adapter
        .execute_raw_query(&format!("SELECT pk FROM {TABLE} ORDER BY {order_sql}, pk"))
        .await
        .unwrap()
        .iter()
        .map(|r| r["pk"].as_i64().unwrap())
        .collect()
}

async fn page(executor: &Executor, connection: &str, variables: &Value) -> (Vec<i64>, Value) {
    let response = executor
        .execute(
            &format!(
                "query($first: Int, $last: Int, $after: String, $before: String, $order: JSON) \
                 {{ {connection}(first: $first, last: $last, after: $after, before: $before, \
                 orderBy: $order) {{ edges {{ node {{ pk }} }} pageInfo {{ hasNextPage \
                 hasPreviousPage startCursor endCursor }} }} }}"
            ),
            Some(variables),
        )
        .await
        .unwrap_or_else(|e| panic!("{connection} {variables}: {e}"));
    let page = &response["data"][connection];
    let pks = page["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|e| e["node"]["pk"].as_i64().unwrap())
        .collect();
    (pks, page["pageInfo"].clone())
}

async fn walk(
    executor: &Executor,
    connection: &str,
    order: &Value,
    size: u32,
    forward: bool,
) -> Vec<i64> {
    let mut pages: Vec<Vec<i64>> = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..=ROWS.len() {
        let variables = if forward {
            json!({ "first": size, "after": cursor, "order": order })
        } else {
            json!({ "last": size, "before": cursor, "order": order })
        };
        let (pks, info) = page(executor, connection, &variables).await;
        pages.push(pks);
        let more = if forward {
            &info["hasNextPage"]
        } else {
            &info["hasPreviousPage"]
        };
        if *more != json!(true) {
            if !forward {
                pages.reverse();
            }
            return pages.into_iter().flatten().collect();
        }
        cursor = if forward {
            info["endCursor"].clone()
        } else {
            info["startCursor"].clone()
        };
    }
    panic!("the walk of {connection} did not end: {pages:?}");
}

/// Walk `connection` under `order` both ways, at every page size, against `order_sql`.
async fn assert_walks(connection: &str, order: Value, order_sql: &str) {
    let Some((adapter, executor)) = setup().await else {
        eprintln!("skipping #1533: DATABASE_URL not set");
        return;
    };
    let expected = oracle(&adapter, order_sql).await;
    for size in [1, 2, 3] {
        for forward in [true, false] {
            assert_eq!(
                walk(&executor, connection, &order, size, forward).await,
                expected,
                "{connection} {order} size {size} forward {forward}"
            );
        }
    }
}

#[tokio::test]
async fn a_proven_not_null_key_with_ties_walks_every_row_once() {
    assert_walks("itemsConnection", json!({ "word": "ASC" }), "word ASC").await;
}

#[tokio::test]
async fn an_unproven_native_key_with_nulls_walks_every_row_once() {
    assert_walks("itemsConnection", json!({ "note": "ASC" }), "note ASC").await;
}

#[tokio::test]
async fn a_descending_proven_key_walks_every_row_once() {
    assert_walks("itemsConnection", json!({ "word": "DESC" }), "word DESC").await;
}

#[tokio::test]
async fn a_document_key_with_nulls_walks_every_row_once() {
    assert_walks("documentItemsConnection", json!({ "note": "ASC" }), "note ASC").await;
}

#[tokio::test]
async fn a_view_backed_key_walks_every_row_once() {
    assert_walks("viewItemsConnection", json!({ "word": "ASC" }), "word ASC").await;
}
