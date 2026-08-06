#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! #569 — the two prepare-time failures every freshly authored stack hits must
//! carry **actionable** diagnostics, not a bare `PostgreSQL` message:
//!
//! 1. `core.tb_entity_change_log` missing (nothing in the authoring path installed it) → the error
//!    must say to run `fraiseql setup`.
//! 2. The mutation function does not return the v2.2 `mutation_response` row (the classic case is
//!    `RETURNS SETOF v_*`) → the error must name the contract instead of the obscure `column
//!    r.entity_type does not exist`.
//!
//! Runs against the harness-provided `PostgreSQL`; run with `--test-threads=1`
//! (the change-log table is shared, and one test drops it temporarily).

use fraiseql_db::{
    ChangeLogWrite, DatabaseAdapter, PostgresAdapter,
    changelog::{CLOCK_TIMESTAMP_DIRECTIVE, STARTED_AT_VAR},
};
const STARTED_AT: &[(&str, &str)] = &[(STARTED_AT_VAR, CLOCK_TIMESTAMP_DIRECTIVE)];

async fn connect() -> (tokio_postgres::Client, PostgresAdapter, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let (client, connection) = tokio_postgres::connect(svc.url(), tokio_postgres::NoTls)
        .await
        .expect("failed to connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {e}");
        }
    });
    let adapter = PostgresAdapter::new(svc.url()).await.expect("build adapter");
    (client, adapter, svc)
}

/// Restore the contract table for later suites via the ONE shared provisioner
/// (#942/#982 — the migration-08 contract byte-for-byte, not a local copy).
fn contract_table_sql() -> String {
    fraiseql_test_support::changelog::entity_change_log_provision_sql()
}

/// Missing outbox table → the error must point at `fraiseql setup` (#569.1).
#[tokio::test]
async fn missing_changelog_table_error_points_at_setup() {
    let (client, adapter, _svc) = connect().await;

    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_diag569_ok() \
             RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, \
             status_detail text, http_status smallint, message text, entity_id uuid, \
             entity_type text, entity jsonb, updated_fields text[], cascade jsonb, \
             error_detail jsonb, metadata jsonb) LANGUAGE sql AS $$ \
             SELECT true, true, NULL::text, NULL::text, NULL::smallint, NULL::text, \
             gen_random_uuid(), 'Diag'::text, '{}'::jsonb, NULL::text[], NULL::jsonb, \
             NULL::jsonb, NULL::jsonb $$;",
        )
        .await
        .unwrap();
    client
        .batch_execute("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .await
        .unwrap();

    let changelog = ChangeLogWrite::new("Diag", "INSERT");
    let err = adapter
        .execute_function_call_with_changelog(
            "public.fn_diag569_ok",
            &[],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .expect_err("must fail without the change-log table");

    let msg = err.to_string();
    assert!(
        msg.contains("fraiseql setup"),
        "the missing-outbox-table error must tell the operator to run `fraiseql setup`: {msg}"
    );

    client.batch_execute(&contract_table_sql()).await.unwrap();
}

/// A `RETURNS SETOF v_*` mutation function → the error must explain the v2.2
/// `mutation_response` contract, not leak `column r.entity_type` (#569.2).
#[tokio::test]
async fn setof_view_function_error_explains_the_response_contract() {
    let (client, adapter, _svc) = connect().await;
    client.batch_execute(&contract_table_sql()).await.unwrap();

    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS public.tb_diag569 (id uuid PRIMARY KEY DEFAULT \
             gen_random_uuid(), name text);
             CREATE OR REPLACE VIEW public.v_diag569 AS SELECT id, \
             jsonb_build_object('id', id, 'name', name) AS data FROM public.tb_diag569;
             CREATE OR REPLACE FUNCTION public.fn_diag569_setof() \
             RETURNS SETOF public.v_diag569 LANGUAGE sql AS $$ \
             SELECT * FROM public.v_diag569 $$;",
        )
        .await
        .unwrap();

    let changelog = ChangeLogWrite::new("Diag", "INSERT");
    let err = adapter
        .execute_function_call_with_changelog(
            "public.fn_diag569_setof",
            &[],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .expect_err("a SETOF-view function cannot satisfy the outbox CTE");

    let msg = err.to_string();
    assert!(
        msg.contains("mutation_response"),
        "the wrong-shape error must name the v2.2 mutation_response contract: {msg}"
    );
    assert!(
        msg.contains("fraiseql.mutation_ok") || msg.contains("mutation-response"),
        "the error must point at the documented builders or doc page: {msg}"
    );

    client
        .batch_execute(
            "DROP FUNCTION IF EXISTS public.fn_diag569_setof(); \
             DROP VIEW IF EXISTS public.v_diag569; DROP TABLE IF EXISTS public.tb_diag569;",
        )
        .await
        .ok();
}
