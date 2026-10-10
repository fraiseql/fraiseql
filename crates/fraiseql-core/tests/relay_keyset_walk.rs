#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1521: a relay connection walked page by page, with `after` (forward) or `before`
//! (backward), returns every row once, in the order its `orderBy` asks for.
//!
//! The keyset used to resume on the cursor column alone, so a page after a cursor under any
//! other ordering skipped and repeated rows. The rows here have duplicate sort values and
//! NULLs, and each walk is compared with the order PostgreSQL itself gives the same `ORDER BY`
//! (`ASC` places NULLs last, `DESC` first), tie-broken by the cursor column.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_kw_item` table.

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, CursorType, FieldDefinition, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tv_kw_item";

/// `(pk, word, rank)`: `cote` twice, two NULL ranks, ranks shared across words.
const ROWS: [(i64, &str, Option<i64>); 8] = [
    (1, "cote", Some(3)),
    (2, "côte", None),
    (3, "coté", Some(1)),
    (4, "côté", Some(3)),
    (5, "apfel", Some(2)),
    (6, "Äpfel", None),
    (7, "Zebra", Some(1)),
    (8, "cote", Some(2)),
];

fn schema() -> CompiledSchema {
    let item = TestTypeBuilder::new("Item", TABLE)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_simple_field("uid", FieldType::Uuid)
        .with_simple_field("word", FieldType::String)
        .with_field(FieldDefinition::nullable("rank", FieldType::Int))
        .build();
    let mut connection = TestQueryBuilder::new("itemsConnection", "Item")
        .returns_list(true)
        .with_sql_source(TABLE)
        .relay_cursor_column("pk")
        .build();
    connection.auto_params.has_order_by = true;
    // The same rows behind a UUID cursor (ordered against `pk`), with `rank` read from its
    // native `integer` column rather than the document.
    let mut by_uid = TestQueryBuilder::new("itemsByUid", "Item")
        .returns_list(true)
        .with_sql_source(TABLE)
        .relay_cursor_column("uid")
        .relay_cursor_type(CursorType::Uuid)
        .build();
    by_uid.auto_params.has_order_by = true;
    by_uid.native_columns.insert("rank".to_string(), "integer".to_string());
    by_uid.native_columns.insert("uid".to_string(), "uuid".to_string());
    let mut schema = TestSchemaBuilder::new()
        .with_type(item)
        .with_query(connection)
        .with_query(by_uid)
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
        .map(|(pk, word, rank)| {
            let rank = rank.map_or_else(|| "null".to_string(), |r| r.to_string());
            let uid = format!("00000000-0000-0000-0000-{:012}", 9 - pk);
            format!(
                "({pk}, '{uid}', {rank}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'uid', \
                 '{uid}', 'word', '{word}', 'rank', {rank}::int))"
            )
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (pk bigint, uid uuid, rank integer, data jsonb)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let executor =
        Executor::new_with_relay(schema(), Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

/// The pks in the order PostgreSQL gives `order_sql`, which ends with its tie-breaker.
async fn oracle(adapter: &PostgresAdapter, order_sql: &str) -> Vec<i64> {
    adapter
        .execute_raw_query(&format!("SELECT pk FROM {TABLE} ORDER BY {order_sql}"))
        .await
        .unwrap()
        .iter()
        .map(|r| r["pk"].as_i64().unwrap())
        .collect()
}

/// One page of `connection`: the node pks and the `pageInfo`.
async fn page_of(executor: &Executor, connection: &str, variables: &Value) -> (Vec<i64>, Value) {
    let response = try_page(executor, connection, variables)
        .await
        .unwrap_or_else(|e| panic!("{variables}: {e}"));
    let connection = &response["data"][connection];
    let pks = connection["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|e| e["node"]["pk"].as_i64().unwrap())
        .collect();
    (pks, connection["pageInfo"].clone())
}

async fn try_page(
    executor: &Executor,
    connection: &str,
    variables: &Value,
) -> fraiseql_core::error::Result<Value> {
    executor
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
}

/// One page of `itemsConnection`.
async fn page(executor: &Executor, variables: &Value) -> (Vec<i64>, Value) {
    page_of(executor, "itemsConnection", variables).await
}

/// The page sizes every walk is taken at: with 1, a page boundary falls after every row, ties
/// and NULLs included.
const SIZES: [u32; 3] = [1, 2, 3];

/// Every pk, walking forward `size` at a time from the first page.
async fn walk_forward(executor: &Executor, order: &Value, size: u32) -> Vec<i64> {
    walk_forward_of(executor, "itemsConnection", order, size).await
}

async fn walk_forward_of(
    executor: &Executor,
    connection: &str,
    order: &Value,
    size: u32,
) -> Vec<i64> {
    let mut seen = Vec::new();
    let mut after = Value::Null;
    for _ in 0..=ROWS.len() {
        let (pks, info) = page_of(
            executor,
            connection,
            &json!({ "first": size, "after": after, "order": order }),
        )
        .await;
        seen.extend(pks);
        if info["hasNextPage"] != json!(true) {
            return seen;
        }
        after = info["endCursor"].clone();
    }
    panic!("the forward walk did not end: {seen:?}");
}

/// Every pk, walking backward `size` at a time from the last page.
async fn walk_backward(executor: &Executor, order: &Value, size: u32) -> Vec<i64> {
    walk_backward_of(executor, "itemsConnection", order, size).await
}

async fn walk_backward_of(
    executor: &Executor,
    connection: &str,
    order: &Value,
    size: u32,
) -> Vec<i64> {
    let mut pages: Vec<Vec<i64>> = Vec::new();
    let mut before = Value::Null;
    for _ in 0..=ROWS.len() {
        let (pks, info) = page_of(
            executor,
            connection,
            &json!({ "last": size, "before": before, "order": order }),
        )
        .await;
        pages.push(pks);
        if info["hasPreviousPage"] != json!(true) {
            return pages.into_iter().rev().flatten().collect();
        }
        before = info["startCursor"].clone();
    }
    panic!("the backward walk did not end: {pages:?}");
}

/// Walk `order` both ways and compare each with `order_sql`'s order.
async fn assert_walks(order: Value, order_sql: &str) {
    let Some((adapter, executor)) = setup().await else {
        eprintln!("skipping #1521: DATABASE_URL not set");
        return;
    };
    let expected = oracle(&adapter, &format!("{order_sql}, pk")).await;
    for size in SIZES {
        assert_eq!(
            walk_forward(&executor, &order, size).await,
            expected,
            "forward {size}, {order}"
        );
        assert_eq!(
            walk_backward(&executor, &order, size).await,
            expected,
            "backward {size}, {order}"
        );
    }
}

#[tokio::test]
async fn a_walk_by_one_text_key_ascending_returns_every_row_once_in_order() {
    assert_walks(json!([{ "field": "word", "direction": "ASC" }]), "data->>'word' ASC").await;
}

#[tokio::test]
async fn a_walk_by_one_text_key_descending_returns_every_row_once_in_order() {
    assert_walks(json!([{ "field": "word", "direction": "DESC" }]), "data->>'word' DESC").await;
}

/// A nullable key: `ASC` places its NULLs last, `DESC` first, and the keyset crosses them.
#[tokio::test]
async fn a_walk_by_a_nullable_key_crosses_its_nulls_in_both_directions() {
    assert_walks(json!([{ "field": "rank", "direction": "ASC" }]), "(data->>'rank')::int ASC")
        .await;
    assert_walks(json!([{ "field": "rank", "direction": "DESC" }]), "(data->>'rank')::int DESC")
        .await;
}

/// Two keys in opposite directions, the first nullable.
#[tokio::test]
async fn a_walk_by_two_keys_in_mixed_directions_returns_every_row_once_in_order() {
    assert_walks(
        json!([
            { "field": "rank", "direction": "DESC" },
            { "field": "word", "direction": "ASC" }
        ]),
        "(data->>'rank')::int DESC, data->>'word' ASC",
    )
    .await;
}

/// Without `orderBy` the connection is ordered by its cursor column, and a cursor of the
/// pre-2.17 form (the cursor column alone) still resumes it.
#[tokio::test]
async fn an_unordered_walk_and_a_plain_cursor_still_page() {
    let Some((_, executor)) = setup().await else {
        return;
    };
    for size in SIZES {
        let all: Vec<i64> = (1..=8).collect();
        assert_eq!(walk_forward(&executor, &Value::Null, size).await, all, "forward {size}");
        // A backward page holds the rows nearest its cursor: its extra row, which only says
        // there is a previous page, is the farthest one and is dropped.
        assert_eq!(walk_backward(&executor, &Value::Null, size).await, all, "backward {size}");
    }
    let legacy = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode("3")
    };
    let (pks, _) = page(&executor, &json!({ "first": 2, "after": legacy })).await;
    assert_eq!(pks, vec![4, 5], "after pk 3");
}

/// A native key behind a UUID cursor: the cursor's `rank` is bound as the column's own
/// `integer` and its position as a `uuid`. The cursor column runs against `pk` (`uid` is
/// `…0008` for pk 1), so a keyset that fell back on it would page in the wrong order.
#[tokio::test]
async fn a_walk_by_a_native_key_behind_a_uuid_cursor_returns_every_row_once_in_order() {
    let Some((adapter, executor)) = setup().await else {
        return;
    };
    let order = json!([{ "field": "rank", "direction": "DESC" }]);
    let expected = oracle(&adapter, "rank DESC, uid").await;
    for size in SIZES {
        let forward = walk_forward_of(&executor, "itemsByUid", &order, size).await;
        assert_eq!(forward, expected, "forward {size}");
        let backward = walk_backward_of(&executor, "itemsByUid", &order, size).await;
        assert_eq!(backward, expected, "backward {size}");
    }

    // A key whose column type is not its field's: the cursor's `uid` is bound as the
    // column's `uuid`, which a `uuid` field's own (absent) cast would leave as `text`.
    let order = json!([{ "field": "uid", "direction": "DESC" }]);
    let expected = oracle(&adapter, "uid DESC").await;
    for size in SIZES {
        let forward = walk_forward_of(&executor, "itemsByUid", &order, size).await;
        assert_eq!(forward, expected, "forward {size}");
        let backward = walk_backward_of(&executor, "itemsByUid", &order, size).await;
        assert_eq!(backward, expected, "backward {size}");
    }
}

/// A cursor resumes only the ordering it was issued under; any other pairing is refused,
/// saying how to recover, rather than resumed out of place.
#[tokio::test]
async fn a_cursor_is_refused_under_an_ordering_it_was_not_issued_under() {
    let Some((_, executor)) = setup().await else {
        return;
    };
    let by_word = json!([{ "field": "word", "direction": "ASC" }]);
    let by_rank = json!([{ "field": "rank", "direction": "ASC" }]);
    let (_, ordered) = page(&executor, &json!({ "first": 1, "order": by_word })).await;
    let (_, plain) = page(&executor, &json!({ "first": 1 })).await;
    for (after, order, says) in [
        (&ordered["endCursor"], &by_rank, "another `orderBy` or locale"),
        (
            &ordered["endCursor"],
            &json!([{ "field": "word", "direction": "DESC" }]),
            "another",
        ),
        (&ordered["endCursor"], &Value::Null, "issued under an `orderBy`"),
        (&plain["endCursor"], &by_word, "issued without an `orderBy`"),
    ] {
        let err = try_page(
            &executor,
            "itemsConnection",
            &json!({ "first": 1, "after": after, "order": order }),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains(says), "{order}: {err}");
    }

    // Another format version, under the ordering it names, is refused, not read as this one.
    let mut cursor =
        fraiseql_core::runtime::relay::decode_keyset_cursor(ordered["endCursor"].as_str().unwrap())
            .unwrap();
    cursor.version += 1;
    let err = try_page(
        &executor,
        "itemsConnection",
        &json!({
            "first": 1,
            "after": fraiseql_core::runtime::relay::encode_keyset_cursor(&cursor),
            "order": by_word
        }),
    )
    .await
    .expect_err("refused");
    assert!(err.to_string().contains("not one this server reads"), "{err}");
}

/// A cursor is client data: one whose values are not of its keys' types, or whose position
/// is not a UUID on a UUID connection, is refused as a cursor, saying how to recover, rather
/// than answered with PostgreSQL's cast error.
#[tokio::test]
async fn a_forged_cursor_is_refused_as_a_cursor_not_a_database_error() {
    let Some((_, executor)) = setup().await else {
        return;
    };
    let by_rank = json!([{ "field": "rank", "direction": "ASC" }]);
    let forge = |cursor: &Value,
                 edit: &dyn Fn(&mut fraiseql_core::runtime::relay::KeysetCursor)| {
        let mut cursor =
            fraiseql_core::runtime::relay::decode_keyset_cursor(cursor.as_str().unwrap()).unwrap();
        edit(&mut cursor);
        fraiseql_core::runtime::relay::encode_keyset_cursor(&cursor)
    };

    let (_, ordered) = page(&executor, &json!({ "first": 1, "order": by_rank })).await;
    let (_, by_uid) =
        page_of(&executor, "itemsByUid", &json!({ "first": 1, "order": by_rank })).await;
    let forged = [
        (
            "itemsConnection",
            forge(&ordered["endCursor"], &|c| c.sort_keys = vec![Some("abc".to_string())]),
            by_rank.clone(),
        ),
        (
            "itemsByUid",
            forge(&by_uid["endCursor"], &|c| c.position = json!("not-a-uuid")),
            by_rank.clone(),
        ),
        (
            "itemsByUid",
            fraiseql_core::runtime::relay::encode_uuid_cursor("not-a-uuid"),
            Value::Null,
        ),
    ];
    // With a session variable the page runs in a transaction of its own, which the failed
    // statement aborts: the other path to the same refusal.
    let mut with_session = schema();
    with_session
        .session_variables
        .variables
        .push(fraiseql_core::schema::SessionVariableMapping {
            name:   "app.kw".to_string(),
            source: fraiseql_core::schema::SessionVariableSource::Literal {
                value: "1".to_string(),
            },
        });
    let url = fraiseql_test_support::try_database_url().unwrap();
    let in_session =
        Executor::new_with_relay(with_session, Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    for executor in [&executor, &in_session] {
        for (connection, cursor, order) in &forged {
            for (window, arg) in [("first", "after"), ("last", "before")] {
                let refusal = try_page(
                    executor,
                    connection,
                    &json!({ window: 1, arg: cursor, "order": order }),
                )
                .await
                .expect_err("a forged cursor is refused");
                assert!(
                    matches!(refusal, fraiseql_core::error::FraiseQLError::Validation { .. }),
                    "{connection} {arg}: a cursor refusal, not {refusal:?}"
                );
                assert!(
                    refusal.to_string().contains("request the first page again"),
                    "{connection} {arg}: says how to recover: {refusal}"
                );
            }
        }
    }
}

/// The refusal is the cursor's only: a data exception a valid cursor did not cause is the
/// error PostgreSQL raised, unchanged.
#[tokio::test]
async fn a_data_exception_beside_a_valid_cursor_is_not_blamed_on_it() {
    let Some((_, executor)) = setup().await else {
        return;
    };
    let by_rank = json!([{ "field": "rank", "direction": "ASC" }]);
    let (_, ordered) = page(&executor, &json!({ "first": 1, "order": by_rank })).await;
    let error = executor
        .execute(
            "query($after: String, $order: JSON) { itemsConnection(first: 1, after: $after, \
             orderBy: $order, where: { rank: { eq: \"abc\" } }) { edges { node { pk } } } }",
            Some(&json!({ "after": ordered["endCursor"], "order": by_rank })),
        )
        .await
        .expect_err("the filter's own value fails its cast");
    assert!(
        matches!(&error, fraiseql_core::error::FraiseQLError::Database { sql_state: Some(s), .. } if s.starts_with("22")),
        "PostgreSQL's data exception, not a cursor refusal: {error:?}"
    );
}

/// The suite's schema loads with no database.
#[test]
fn the_document_loads_without_a_database() {
    CompiledSchema::from_json(&serde_json::to_string(&schema()).unwrap(), false)
        .unwrap_or_else(|e| panic!("the walk suite's schema must load: {e}"));
}
