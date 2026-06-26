//! Live-PostgreSQL integration tests for the opt-in fail-fast `sql_source` boot
//! check (#487), and the cross-crate guarantee that the server boot check and the
//! CLI `validate --against-db` gate agree on "backed".
//!
//! Self-skips when no `DATABASE_URL` is set.

#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code

use std::collections::BTreeSet;

use fraiseql_cli::schema::database_validator::{
    create_introspector, find_unbacked_sources as cli_find_unbacked,
};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    schema::{CompiledSchema, MutationDefinition, QueryDefinition, SourceProbe},
};
use fraiseql_server::sql_source_check::find_unbacked_sources as server_find_unbacked;
use fraiseql_test_utils::try_database_url;
use tokio_postgres::NoTls;

const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_487_test CASCADE;
CREATE SCHEMA fql_487_test;
CREATE VIEW fql_487_test.v_orders AS SELECT '{}'::jsonb AS data;
CREATE FUNCTION fql_487_test.fn_create_order(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
";

const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_487_test CASCADE;";

async fn run_sql(url: &str, sql: &str) {
    let (client, connection) = tokio_postgres::connect(url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(sql).await.unwrap();
}

fn query(name: &str, sql_source: &str) -> QueryDefinition {
    QueryDefinition::new(name, "T").with_sql_source(sql_source).returning_list()
}

fn mutation(name: &str, sql_source: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "T");
    m.sql_source = Some(sql_source.to_string());
    m
}

/// Render the unbacked probes as a comparable set of `display_name` strings.
fn names(probes: &[SourceProbe]) -> BTreeSet<String> {
    probes.iter().map(SourceProbe::display_name).collect()
}

fn fully_backed_schema() -> CompiledSchema {
    CompiledSchema {
        queries: vec![query("orders", "fql_487_test.v_orders")],
        mutations: vec![mutation("createOrder", "fql_487_test.fn_create_order")],
        ..Default::default()
    }
}

fn schema_with_two_missing() -> CompiledSchema {
    CompiledSchema {
        queries: vec![
            query("orders", "fql_487_test.v_orders"),
            query("missing", "fql_487_test.v_missing"),
        ],
        mutations: vec![
            mutation("createOrder", "fql_487_test.fn_create_order"),
            mutation("absent", "fql_487_test.fn_absent"),
        ],
        ..Default::default()
    }
}

#[tokio::test]
async fn boot_check_passes_when_all_sources_backed() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 boot-check test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let unbacked = server_find_unbacked(&fully_backed_schema(), &adapter).await.unwrap();
    assert!(unbacked.is_empty(), "fully-backed schema must boot clean, got {unbacked:?}");

    run_sql(&url, TEARDOWN).await;
}

#[tokio::test]
async fn boot_check_lists_each_unbacked_source() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 boot-check test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let unbacked = server_find_unbacked(&schema_with_two_missing(), &adapter).await.unwrap();
    assert_eq!(
        names(&unbacked),
        BTreeSet::from([
            "fql_487_test.v_missing".to_string(),
            "fql_487_test.fn_absent".to_string(),
        ]),
        "exactly the missing view + function must be reported",
    );

    run_sql(&url, TEARDOWN).await;
}

/// The point of the shared `sql_source_probes` core: the server boot check (via the
/// adapter) and the CLI `validate --against-db` gate (via the introspector) must
/// report the **same** unbacked set on the same database.
#[tokio::test]
async fn server_and_cli_agree_on_unbacked_set() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #487 symmetry test: no DATABASE_URL");
        return;
    };
    run_sql(&url, SETUP).await;

    let schema = schema_with_two_missing();

    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let server_set = names(&server_find_unbacked(&schema, &adapter).await.unwrap());

    let introspector = create_introspector(&url).await.unwrap();
    let cli_set = names(&cli_find_unbacked(&schema, &introspector).await.unwrap());

    assert_eq!(server_set, cli_set, "server boot check and CLI gate must agree on 'backed'");

    run_sql(&url, TEARDOWN).await;
}
