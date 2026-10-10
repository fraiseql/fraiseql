//! #1533: an ordered relay page seeks its index when every sort key is a native column
//! PostgreSQL proves `NOT NULL`, all ascending; otherwise it keeps the expanded predicate.
//!
//! The expanded form `(k > $1 OR k IS NULL) OR (k = $1 AND pk > $2)` is what lets keys mix
//! directions and hold NULLs, and PostgreSQL cannot seek an index with it: it walks the index
//! from the start and filters. The row comparison `(k, pk) > ($1, $2)` seeks, and is correct
//! only when no key can be NULL (a NULL in a row comparison drops the row) and every key reads
//! in the position's direction (the position always sorts ascending).
//!
//! What is asserted is the **plan** PostgreSQL 18 chooses for a page the engine's own keyset
//! functions build (`keyset_keys`, `keyset_predicate`, `keyset_order`, as the relay runs them),
//! read from `EXPLAIN (FORMAT JSON)`: an `Index Cond` and no `Filter` on the seeking scan. Not
//! the SQL text, and not a timing.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1533_seek` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use fraiseql_db::{
    OrderByClause, OrderDirection, ScalarFieldType,
    keyset::{keyset_keys, keyset_order, keyset_predicate},
};
use serde_json::Value;
use tokio_postgres::NoTls;

const SCHEMA: &str = "p1533_seek";
const ROWS: i64 = 60_000;
/// Where the cursor sits: deep enough that a walk from the start is unmistakable in the plan,
/// and before the trailing NULLs of `note` (every seventh row, sorted last), so the cursor's
/// key is a value and the predicate keeps its value branch.
const CURSOR_AT: i64 = 40_000;

async fn client() -> tokio_postgres::Client {
    let url = fraiseql_test_support::database_url();
    let (client, conn) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {SCHEMA} CASCADE;
             CREATE SCHEMA {SCHEMA};
             CREATE TABLE {SCHEMA}.tv_word (
                 pk bigint PRIMARY KEY, word text NOT NULL, note text, data jsonb NOT NULL);
             INSERT INTO {SCHEMA}.tv_word
                 SELECT k, md5(k::text), CASE WHEN k % 7 = 0 THEN NULL ELSE md5(k::text) END,
                        jsonb_build_object('word', md5(k::text))
                 FROM generate_series(1, {ROWS}) k;
             CREATE INDEX ON {SCHEMA}.tv_word (word, pk);
             CREATE INDEX ON {SCHEMA}.tv_word (note, pk);
             ANALYZE {SCHEMA}.tv_word;"
        ))
        .await
        .unwrap();
    client
}

/// A clause ordering by native column `column`, proven `NOT NULL` when `not_null`.
fn native(column: &str, direction: OrderDirection, not_null: bool) -> OrderByClause {
    let mut clause = OrderByClause::new(column.to_string(), direction);
    clause.field_type = ScalarFieldType::Text;
    clause.native_column = Some(format!("\"{column}\""));
    clause.native_type = Some("text".to_string());
    clause.native_not_null = not_null;
    clause
}

/// The plan of the forward page after the row at `CURSOR_AT` in `order` (its values read from
/// the table), as the relay composes it: the keyset predicate, the keyset order, a limit.
async fn page_plan(client: &tokio_postgres::Client, order: &[OrderByClause]) -> Value {
    let keys = keyset_keys(Some(order)).unwrap();
    let order_sql = keyset_order(&keys, "\"pk\"", true);
    let cursor_row = client
        .query_one(
            &format!(
                "SELECT pk, {} FROM {SCHEMA}.tv_word ORDER BY {order_sql} OFFSET {} LIMIT 1",
                keys.iter()
                    .map(|k| format!("({})::text", k.expr))
                    .collect::<Vec<_>>()
                    .join(", "),
                CURSOR_AT
            ),
            &[],
        )
        .await
        .unwrap();
    let pk: i64 = cursor_row.get(0);
    let values: Vec<Option<String>> = (1..=keys.len()).map(|i| cursor_row.get(i)).collect();
    let (predicate, params, position) =
        keyset_predicate(&keys, &values, "\"pk\"", str::to_string, true, 1).unwrap();
    assert_eq!(position, params.len() + 1, "the position is the last parameter");
    let sql = format!(
        "EXPLAIN (FORMAT JSON) SELECT data FROM {SCHEMA}.tv_word WHERE {predicate} \
         ORDER BY {order_sql} LIMIT 10"
    );
    let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        params.iter().map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    refs.push(&pk);
    let row = client.query_one(&sql, &refs).await.unwrap();
    let plan: Value = row.get(0);
    plan[0]["Plan"].clone()
}

/// Every node of `plan`, depth first.
fn nodes(plan: &Value) -> Vec<&Value> {
    let mut out = vec![plan];
    for child in plan["Plans"].as_array().into_iter().flatten() {
        out.extend(nodes(child));
    }
    out
}

/// Whether the plan seeks: an index scan with an `Index Cond` and no `Filter` anywhere.
fn seeks(plan: &Value) -> bool {
    let all = nodes(plan);
    all.iter().any(|n| n.get("Index Cond").is_some())
        && all.iter().all(|n| n.get("Filter").is_none())
}

#[tokio::test]
async fn a_page_ordered_by_a_proven_not_null_column_seeks_its_index() {
    let client = client().await;
    let plan = page_plan(&client, &[native("word", OrderDirection::Asc, true)]).await;
    assert!(seeks(&plan), "expected an Index Cond and no Filter: {plan:#}");
}

/// The cases that must keep the expanded form, and so walk: a key nothing proves `NOT NULL`
/// (here a column that holds NULLs), and a descending key (the position sorts ascending).
#[tokio::test]
async fn a_nullable_or_descending_key_keeps_the_expanded_predicate() {
    let client = client().await;
    for (what, order) in [
        ("an unproven key", vec![native("note", OrderDirection::Asc, false)]),
        ("a descending key", vec![native("word", OrderDirection::Desc, true)]),
        (
            "a proven key beside an unproven one",
            vec![
                native("word", OrderDirection::Asc, true),
                native("note", OrderDirection::Asc, false),
            ],
        ),
    ] {
        let plan = page_plan(&client, &order).await;
        assert!(!seeks(&plan), "{what} must not be compared as a row: {plan:#}");
    }
}
