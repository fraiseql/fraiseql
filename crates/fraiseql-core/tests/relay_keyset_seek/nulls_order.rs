//! A `SortDirection` that names a placement (`ASC_NULLS_FIRST`, `DESC_NULLS_LAST`, …) puts a
//! key's NULLs first or last, in a list's `ORDER BY` and through every page of a relay
//! connection, both ways.
//!
//! Each walk is compared with the order PostgreSQL gives the same `ORDER BY … NULLS …`,
//! tie-broken by the cursor column, at page sizes that put a page boundary inside the NULLs.
//! A cursor records its ordering's placement: issued under `NULLS LAST` it is refused under
//! `NULLS FIRST`. A placement that is its direction's default is the ordering that omits
//! it, so cursors issued before a direction could name one still resume where they did.

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor};
use fraiseql_test_utils::schema_builder::TestQueryBuilder;
use serde_json::{Value, json};

use super::{TABLE, assert_walks, page, schema, setup};

fn item(direction: &str, nulls: &str) -> Value {
    json!({ "field": "note", "direction": format!("{direction}_NULLS_{nulls}") })
}

/// Every direction and placement of a key holding NULLs, as a native column and in the
/// document.
#[tokio::test]
async fn each_placement_walks_every_row_once_in_its_order() {
    for connection in ["itemsConnection", "documentItemsConnection"] {
        for (direction, nulls) in [
            ("ASC", "FIRST"),
            ("ASC", "LAST"),
            ("DESC", "FIRST"),
            ("DESC", "LAST"),
        ] {
            assert_walks(
                connection,
                json!([item(direction, nulls)]),
                &format!("note {direction} NULLS {nulls}"),
            )
            .await;
        }
    }
}

/// The NULLs tie on the first key, so a page boundary falls among rows the second key orders:
/// the predicate's `IS NULL` equality prefix, in both placements.
#[tokio::test]
async fn a_null_tie_is_broken_by_the_next_key() {
    assert_walks(
        "itemsConnection",
        json!([item("DESC", "LAST"), { "field": "word", "direction": "ASC" }]),
        "note DESC NULLS LAST, word ASC",
    )
    .await;
    assert_walks(
        "documentItemsConnection",
        json!([item("ASC", "FIRST"), { "field": "word", "direction": "DESC_NULLS_LAST" }]),
        "note ASC NULLS FIRST, word DESC NULLS LAST",
    )
    .await;
}

async fn refusal(executor: &Executor, order: &Value, after: &Value) -> String {
    executor
        .execute(
            "query($after: String, $order: JSON) { itemsConnection(first: 2, after: $after, \
             orderBy: $order) { edges { node { pk } } } }",
            Some(&json!({ "after": after, "order": order })),
        )
        .await
        .map_or_else(|e| e.to_string(), |response| response.to_string())
}

/// A cursor resumes only the placement it was issued under: one from `NULLS LAST` is refused
/// under `NULLS FIRST`, and resumes under the direction's default spelled either way.
#[tokio::test]
async fn a_cursor_is_refused_under_another_placement() {
    let Some((_adapter, executor)) = setup().await else {
        return;
    };
    let last = json!([item("ASC", "LAST")]);
    let (_, info) = page(&executor, "itemsConnection", &json!({ "first": 2, "order": last })).await;
    let cursor = info["endCursor"].clone();

    let refused = refusal(&executor, &json!([item("ASC", "FIRST")]), &cursor).await;
    assert!(refused.contains("invalid relay cursor for `after`"), "{refused}");

    let (resumed, _) = page(
        &executor,
        "itemsConnection",
        &json!({ "first": 2, "after": cursor, "order": last }),
    )
    .await;
    let (omitted, _) = page(
        &executor,
        "itemsConnection",
        &json!({ "first": 2, "after": cursor, "order": [{ "field": "note", "direction": "ASC" }] }),
    )
    .await;
    assert_eq!(resumed, omitted);
}

/// Cursors issued before a direction could name a placement (at 13e1a43e0, on this table, after the
/// first page of two, under the field-to-direction object `{note: ASC}` then accepted) resume on
/// the row they did, under the list item that names the same ordering and under its default
/// placement spelled out. The DESC cursor's row has a NULL key.
#[tokio::test]
async fn a_cursor_issued_before_nulls_existed_still_resumes_where_it_did() {
    let Some((_adapter, executor)) = setup().await else {
        return;
    };
    for (orders, cursor, next) in [
        (
            [
                json!([{ "field": "note", "direction": "ASC" }]),
                json!([item("ASC", "LAST")]),
            ],
            "eyJ2IjoyLCJvIjoiN2U3MjljYWYzNjQwNjhjZCIsImsiOlsidyJdLCJjIjo3fQ==",
            [1, 4],
        ),
        (
            [
                json!([{ "field": "note", "direction": "DESC" }]),
                json!([item("DESC", "FIRST")]),
            ],
            "eyJ2IjoyLCJvIjoiMWYxODUwYmU0Njc2NGU4ZSIsImsiOltudWxsXSwiYyI6Nn0=",
            [8, 5],
        ),
    ] {
        for order in orders {
            let (pks, _) = page(
                &executor,
                "itemsConnection",
                &json!({ "first": 2, "after": cursor, "order": order }),
            )
            .await;
            assert_eq!(pks, next, "{order}");
        }
    }
}

/// A list's `ORDER BY` (no cursor, no keyset) puts the NULLs where the direction asks: in a
/// list item, in one item written without the list, and as the enum literals a document writes,
/// which are held to the published `SortDirection`.
#[tokio::test]
async fn a_list_puts_its_nulls_where_the_item_asks() {
    let Some((adapter, _)) = setup().await else {
        return;
    };
    let url = fraiseql_test_support::try_database_url().unwrap();
    let mut schema = schema();
    let mut items = TestQueryBuilder::new("items", "Item")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    items.auto_params.has_order_by = true;
    schema.queries.push(items);
    schema.build_indexes();
    let executor =
        Executor::new_with_relay(schema, Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    let served = |response: Value| -> Vec<i64> {
        response["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{response}"))
            .iter()
            .map(|row| row["pk"].as_i64().unwrap())
            .collect()
    };
    for (direction, nulls) in [("ASC", "FIRST"), ("DESC", "LAST")] {
        let expected = super::oracle(&adapter, &format!("note {direction} NULLS {nulls}")).await;
        let named = format!("{direction}_NULLS_{nulls}");
        for order in [json!([item(direction, nulls)]), item(direction, nulls)] {
            let response = executor
                .execute(
                    "query($order: JSON) { items(orderBy: $order) { pk } }",
                    Some(&json!({ "order": order })),
                )
                .await
                .unwrap();
            assert_eq!(served(response), expected, "{order}");
        }
        let document =
            format!("{{ items(orderBy: [{{field: note, direction: {named}}}]) {{ pk }} }}");
        let response = executor.execute(&document, None).await.unwrap();
        assert_eq!(served(response), expected, "{document}");
    }
}
