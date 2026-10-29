//! A `NUMERIC` column read by the row-shaped query the gRPC transport serves arrives as the
//! exact text PostgreSQL printed, on every path a read takes.
//!
//! The row read decoded each cell to JSON and re-parsed a `NUMERIC` as a JSON number, so
//! `12345678901234567.89` reached a gRPC client as the double `12345678901234568`, and
//! `1.50` as `1.5`. There are four reads, collecting and streaming, each with and without
//! session variables, and the shipped server reaches them through the result-cache wrapper,
//! so each is read here through that wrapper: a path left on the old decode fails its own
//! assertion.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** safe (a view of its own).

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: house SKIP pattern

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{
        DatabaseAdapter,
        dialect::RowViewColumnType,
        postgres::PostgresAdapter,
        types::{ColumnSpec, ColumnValue},
    },
};
use fraiseql_test_support::try_database_url;
use futures::StreamExt as _;

const VIEW: &str = "v_row_query_decimal";

/// The `amount` column of each row, in `id` order, as the read delivered it.
fn amounts(rows: &[Vec<ColumnValue>]) -> Vec<String> {
    rows.iter().map(|row| format!("{:?}", row[1])).collect()
}

#[tokio::test]
async fn every_row_read_keeps_a_numerics_digits() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP every_row_read_keeps_a_numerics_digits: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.expect("connect");
    for statement in [
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT * FROM (VALUES \
             (1, 12345678901234567.89::numeric), (2, 1.50::numeric), (3, NULL::numeric)) \
             t(id, amount)"
        ),
    ] {
        adapter.execute_raw_query(&statement).await.expect("fixture DDL");
    }
    let cached = CachedDatabaseAdapter::new(
        adapter,
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    let columns = [
        ColumnSpec {
            name:        "id".to_string(),
            column_type: RowViewColumnType::Int32,
        },
        ColumnSpec {
            name:        "amount".to_string(),
            column_type: RowViewColumnType::Decimal,
        },
    ];
    let session: &[(&str, &str)] = &[("fraiseql.row_query_decimal", "on")];
    let want = vec![
        r#"Decimal("12345678901234567.89")"#.to_string(),
        r#"Decimal("1.50")"#.to_string(),
        "Null".to_string(),
    ];

    let collected = cached
        .execute_row_query(VIEW, &columns, None, Some("id"), None, None)
        .await
        .expect("collecting read");
    assert_eq!(amounts(&collected), want, "collecting, no session variables");

    let collected = cached
        .execute_row_query_with_session(VIEW, &columns, None, Some("id"), None, None, session)
        .await
        .expect("collecting read with session variables");
    assert_eq!(amounts(&collected), want, "collecting, with session variables");

    let streamed: Vec<_> = cached
        .stream_row_query(VIEW, &columns, None, Some("id"), None, None)
        .await
        .expect("streaming read")
        .map(|row| row.expect("row"))
        .collect()
        .await;
    assert_eq!(amounts(&streamed), want, "streaming, no session variables");

    let streamed: Vec<_> = cached
        .stream_row_query_with_session(VIEW, &columns, None, Some("id"), None, None, session)
        .await
        .expect("streaming read with session variables")
        .map(|row| row.expect("row"))
        .collect()
        .await;
    assert_eq!(amounts(&streamed), want, "streaming, with session variables");
}
