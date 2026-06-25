//! Live-PostgreSQL integration tests for the `validate --against-db` existence
//! gate (#487): an unbacked `sql_source` makes the command exit non-zero, a
//! fully-backed schema passes. Self-skips when no `DATABASE_URL` is set.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code

use std::io::Write;

use fraiseql_core::schema::{CompiledSchema, QueryDefinition};
use tempfile::NamedTempFile;
use tokio_postgres::NoTls;

const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_487_cli CASCADE;
CREATE SCHEMA fql_487_cli;
CREATE VIEW fql_487_cli.v_orders AS SELECT '{}'::jsonb AS data;
CREATE FUNCTION fql_487_cli.fn_create_order(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
";
const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_487_cli CASCADE;";

async fn run_sql(url: &str, sql: &str) {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(sql).await.unwrap();
}

/// Serialize a compiled schema to a temp `.json` file; keep the handle alive.
fn write_compiled(schema: &CompiledSchema) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(serde_json::to_string(schema).unwrap().as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

fn query(name: &str, sql_source: &str) -> QueryDefinition {
    QueryDefinition::new(name, "T").with_sql_source(sql_source).returning_list()
}

#[tokio::test]
async fn validate_against_db_exits_nonzero_on_unbacked_source() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #487 CLI-gate test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let schema = CompiledSchema {
        queries: vec![query("orders", "fql_487_cli.v_missing")],
        ..Default::default()
    };
    let file = write_compiled(&schema);
    let result = fraiseql_cli::commands::validate::run_against_db(
        file.path().to_str().unwrap(),
        &url,
        false,
    )
    .await;
    assert!(result.is_err(), "an unbacked sql_source must fail validate --against-db");

    run_sql(&url, TEARDOWN).await;
}

#[tokio::test]
async fn validate_against_db_succeeds_when_fully_backed() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #487 CLI-gate test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    // Query-only: isolates the #487 existence gate from the #484 mutation-contract
    // arity check (the backed mutation function is exercised by the server-side
    // boot-check integration test).
    let schema = CompiledSchema {
        queries: vec![query("orders", "fql_487_cli.v_orders")],
        ..Default::default()
    };
    let file = write_compiled(&schema);
    let result = fraiseql_cli::commands::validate::run_against_db(
        file.path().to_str().unwrap(),
        &url,
        false,
    )
    .await;
    assert!(result.is_ok(), "a fully-backed schema must pass: {result:?}");

    run_sql(&url, TEARDOWN).await;
}
