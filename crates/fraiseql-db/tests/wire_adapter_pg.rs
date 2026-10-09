//! `FraiseWireAdapter` driven against PostgreSQL (#1115).
//!
//! The wire backend had no test that executed it: these read a seeded view through the
//! adapter, check that it refuses the session variables it cannot apply rather than reading
//! unscoped, and that its streaming read is a stream.
//!
//! **Execution engine:** `PostgreSQL` through `fraiseql-wire` · **Infrastructure:**
//! `DATABASE_URL` · **Parallelism:** creates and drops its own `wire_rig` schema; run
//! `--test-threads=1`.
#![cfg(feature = "wire-backend")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_db::{
    DatabaseAdapter, FraiseWireAdapter, OrderByClause, OrderDirection, PostgresAdapter,
    ProjectionRequest, ScalarFieldType, WhereClause, WhereOperator,
    dialect::RowViewColumnType,
    types::{ColumnSpec, ReadRouting},
};
use futures::StreamExt as _;
use serde_json::{Value, json};

const VIEW: &str = "wire_rig.v_item";
const ROWS: i64 = 20_000;

/// The seeded view: `ROWS` items with an id, a group and a kilobyte of padding (so the
/// stream's rows outgrow the socket buffers and the server waits on the client).
async fn seed(url: &str) {
    let pg = PostgresAdapter::new(url).await.unwrap();
    for statement in [
        "DROP SCHEMA IF EXISTS wire_rig CASCADE".to_string(),
        "CREATE SCHEMA wire_rig".to_string(),
        format!(
            "CREATE TABLE wire_rig.tb_item AS SELECT i AS pk, jsonb_build_object('id', i, \
             'grp', i % 10, 'pad', repeat('x', 1000)) AS data FROM generate_series(1, {ROWS}) i"
        ),
        "CREATE VIEW wire_rig.v_item AS SELECT pk, data FROM wire_rig.tb_item".to_string(),
    ] {
        pg.execute_raw_query(&statement).await.unwrap();
    }
}

async fn adapter() -> Option<(String, FraiseWireAdapter)> {
    let url = fraiseql_test_support::try_database_url()?;
    seed(&url).await;
    Some((url.clone(), FraiseWireAdapter::new(&url).with_chunk_size(16)))
}

fn ids(rows: &[fraiseql_db::JsonbValue]) -> Vec<i64> {
    rows.iter().map(|r| r.as_value()["id"].as_i64().unwrap()).collect()
}

fn in_group(group: i64) -> WhereClause {
    WhereClause::Field {
        path:     vec!["grp".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(group),
    }
}

/// Cycle 1: a filtered, ordered, paged read returns exactly the expected rows, in order.
#[tokio::test]
async fn a_filtered_ordered_paged_read() {
    let Some((_, wire)) = adapter().await else {
        eprintln!("skipping wire adapter rig: DATABASE_URL not set");
        return;
    };
    // Typed as the runtime types a declared `Int` (an untyped key sorts as text).
    let mut by_id = OrderByClause::new("id".to_string(), OrderDirection::Desc);
    by_id.field_type = ScalarFieldType::Integer;
    let order = [by_id];
    let rows = wire
        .execute_where_query(VIEW, Some(&in_group(3)), Some(4), Some(2), Some(&order))
        .await
        .unwrap();
    // Group 3, newest first: 19993, 19983, 19973, … — skip two, take four.
    assert_eq!(ids(&rows), vec![19973, 19963, 19953, 19943]);
}

/// Cycle 2: every session-taking read refuses a non-empty set of session variables, naming
/// the backend, rather than reading without them (an RLS policy reading `current_setting`
/// would see none). An empty set still reads.
#[tokio::test]
async fn session_variables_are_refused_not_dropped() {
    let Some((_, wire)) = adapter().await else {
        return;
    };
    let vars: &[(&str, &str)] = &[("app.tenant_id", "t1")];
    let request = ProjectionRequest::new(VIEW);
    let columns = [ColumnSpec {
        name:        "pk".to_string(),
        column_type: RowViewColumnType::Int64,
    }];
    let refused = |what: &str, result: fraiseql_db::Result<()>| {
        let err = result.expect_err(what).to_string();
        assert!(err.contains("session variables") && err.contains("wire"), "{what}: {err}");
    };
    refused(
        "where",
        wire.execute_where_query_arc_with_session(
            VIEW,
            None,
            Some(1),
            None,
            None,
            vars,
            ReadRouting::Any,
        )
        .await
        .map(drop),
    );
    refused(
        "projection",
        wire.execute_with_projection_arc_with_session(&request, vars, ReadRouting::Any)
            .await
            .map(drop),
    );
    refused(
        "stream",
        wire.stream_with_projection(&request, vars, ReadRouting::Any).await.map(drop),
    );
    refused(
        "rows",
        wire.execute_row_query_with_session(VIEW, &columns, None, None, Some(1), None, vars)
            .await
            .map(drop),
    );
    refused(
        "row stream",
        wire.stream_row_query_with_session(VIEW, &columns, None, None, Some(1), None, vars)
            .await
            .map(drop),
    );
    refused(
        "aggregate",
        wire.execute_parameterized_aggregate_with_session("SELECT 1", &[], vars, ReadRouting::Any)
            .await
            .map(drop),
    );

    let rows = wire
        .execute_where_query_arc_with_session(
            VIEW,
            None,
            Some(1),
            None,
            None,
            &[],
            ReadRouting::Any,
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "an empty set still reads");
}

/// Cycle 3: the streaming read is a stream. Its first row arrives while the server-side
/// statement is still running (a buffered read would have finished it), every row arrives in
/// order, and `offset`/`limit` are honoured.
#[tokio::test]
async fn the_streaming_read_streams() {
    let Some((url, wire)) = adapter().await else {
        return;
    };
    let mut by_id = OrderByClause::new("id".to_string(), OrderDirection::Asc);
    by_id.field_type = ScalarFieldType::Integer;
    let order = [by_id];
    let mut request = ProjectionRequest::new(VIEW);
    request.order_by = Some(&order);
    let mut stream = wire.stream_with_projection(&request, &[], ReadRouting::Any).await.unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.as_value()["id"], json!(1));

    let pg = PostgresAdapter::new(&url).await.unwrap();
    let active = pg
        .execute_raw_query(
            "SELECT jsonb_build_object('n', count(*)) AS data FROM pg_stat_activity \
             WHERE state = 'active' AND query ILIKE '%wire_rig.v_item%' \
             AND pid <> pg_backend_pid()",
        )
        .await
        .unwrap();
    assert_eq!(active[0]["data"]["n"], json!(1), "the statement is still running");

    let mut count = 1;
    let mut previous = 1;
    while let Some(row) = stream.next().await {
        let id = row.unwrap().as_value()["id"].as_i64().unwrap();
        assert_eq!(id, previous + 1, "in order");
        previous = id;
        count += 1;
    }
    assert_eq!(count, ROWS, "every row");

    request.offset = Some(10);
    request.limit = Some(3);
    let window: Vec<Value> = wire
        .stream_with_projection(&request, &[], ReadRouting::Any)
        .await
        .unwrap()
        .map(|r| r.unwrap().as_value()["id"].clone())
        .collect()
        .await;
    assert_eq!(window, vec![json!(11), json!(12), json!(13)], "offset and limit");
}
