#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! Behavioural proof of the Change Spine transactional outbox (phase-02):
//! `execute_function_call_with_changelog` writes exactly one
//! `core.tb_entity_change_log` row **in the same transaction** as the mutation
//! function, atomically, with a wall-clock `duration_ms`, and only for an
//! effective change.
//!
//! Runs against the harness-provided PostgreSQL (Dagger-bound in CI, or a local
//! spawn with the `local-testcontainers` feature). All tests share
//! `core.tb_entity_change_log`, so each isolates on a unique `object_type`; run
//! the file with `--test-threads=1` (see CLAUDE.md) when in doubt.

use fraiseql_db::{
    ChangeLogWrite, DatabaseAdapter, PostgresAdapter,
    changelog::{CLOCK_TIMESTAMP_DIRECTIVE, STARTED_AT_VAR},
};
use serde_json::json;

/// Connect a raw client (for assertions) and build an adapter (under test).
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

/// Provision the `app.mutation_response` contract + a fresh framework-owned
/// `core.tb_entity_change_log` carrying the columns phase-02 writes (the
/// migration-08 contract shape: `object_id UUID`, `object_data` nullable).
///
/// The contract table is DROP+CREATEd to the canonical shape so the test is not
/// at the mercy of an older/app-shaped table left in the shared warm database
/// (e.g. `object_id TEXT` / `object_data NOT NULL`). Run the file with
/// `--test-threads=1` (it owns this shared table for the duration of the run).
async fn provision(client: &tokio_postgres::Client) {
    client
        .batch_execute(
            "CREATE SCHEMA IF NOT EXISTS app;
             DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;
             CREATE SCHEMA IF NOT EXISTS core;
             DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE;
             CREATE TABLE core.tb_entity_change_log (\
               pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
               object_type TEXT NOT NULL, modification_type TEXT NOT NULL, \
               id UUID NOT NULL DEFAULT gen_random_uuid(), \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               object_id UUID, object_data JSONB, updated_fields TEXT[], cascade JSONB, \
               duration_ms INTEGER, started_at TIMESTAMPTZ, extra_metadata JSONB);",
        )
        .await
        .unwrap();
}

/// Count change-log rows for an isolating `object_type`.
async fn count_rows(client: &tokio_postgres::Client, object_type: &str) -> i64 {
    client
        .query_one(
            "SELECT COUNT(*) FROM core.tb_entity_change_log WHERE object_type = $1",
            &[&object_type],
        )
        .await
        .unwrap()
        .get::<_, i64>(0)
}

/// The directive that stamps `fraiseql.started_at` on the call's own txn — what
/// the mutation runner injects via session variables in production.
const STARTED_AT: &[(&str, &str)] = &[(STARTED_AT_VAR, CLOCK_TIMESTAMP_DIRECTIVE)];

#[tokio::test]
async fn executor_writes_changelog_in_txn() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxUser";

    // A successful, state-changing mutation function returning the full payload.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_create(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'OutboxUser'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id, 'name', 'Ada'); \
             v.updated_fields := ARRAY['name']; \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();

    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT");
    let rows = adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_create",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .expect("mutation + outbox write");

    // The function's row is still returned to the caller, unchanged.
    assert_eq!(rows.len(), 1, "function row returned to the caller");
    assert_eq!(rows[0].get("succeeded"), Some(&json!(true)));

    // Exactly one outbox row, with the mutation's identity + payload.
    let row = client
        .query_one(
            "SELECT object_type, modification_type, object_id, object_data, updated_fields, \
             duration_ms, extra_metadata FROM core.tb_entity_change_log WHERE object_id = $1",
            &[&id],
        )
        .await
        .expect("exactly one outbox row");
    assert_eq!(row.get::<_, String>("object_type"), obj_type);
    assert_eq!(row.get::<_, String>("modification_type"), "INSERT");
    assert_eq!(row.get::<_, uuid::Uuid>("object_id"), id);
    let data: serde_json::Value = row.get("object_data");
    assert_eq!(data["name"], json!("Ada"), "object_data is the entity payload");
    let updated: Vec<String> = row.get("updated_fields");
    assert_eq!(updated, vec!["name".to_string()]);
    assert!(row.get::<_, Option<i32>>("duration_ms").is_some(), "duration_ms populated");
    let meta: serde_json::Value = row.get("extra_metadata");
    assert_eq!(meta["duration_calc_version"], json!(2), "data-quality marker stamped");
}

#[tokio::test]
async fn changelog_row_atomic_with_mutation() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxAtomic";

    // The function raises after (notionally) doing work — the whole txn, INCLUDING
    // the outbox INSERT, must roll back: neither the mutation nor the log row survives.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_boom() \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ BEGIN \
             RAISE EXCEPTION 'boom'; END; $$;",
        )
        .await
        .unwrap();

    let changelog = ChangeLogWrite::new(obj_type, "INSERT");
    let result = adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_boom",
            &[],
            STARTED_AT,
            Some(&changelog),
        )
        .await;

    assert!(result.is_err(), "raising function surfaces an error");
    assert_eq!(count_rows(&client, obj_type).await, 0, "no outbox row after rollback");
}

#[tokio::test]
async fn started_at_visible_to_outbox_insert() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxTimed";

    // The function sleeps ~60 ms, so a duration computed from `started_at` (set
    // BEFORE the function in the SAME txn) to the insert (AFTER it) must reflect
    // the elapsed time — proving the txn-local GUC is visible at the outbox write.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_slow(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             PERFORM pg_sleep(0.06); \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'OutboxTimed'; v.entity_id := p_id; \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();

    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "UPDATE");
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_slow",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let duration: i32 = client
        .query_one("SELECT duration_ms FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("duration_ms");
    assert!(
        (40..=5_000).contains(&duration),
        "duration_ms ~60 ms (>= the pg_sleep), got {duration} — proves started_at was visible"
    );
}

#[tokio::test]
async fn started_at_guaranteed_without_injected_session_var() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxNoSession";

    // No session variables injected (e.g. an unauthenticated mutation). The
    // adapter must still stamp started_at itself so the duration computation
    // never hits an unset GUC and aborts the mutation.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_nosession(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'OutboxNoSession'; v.entity_id := p_id; \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();

    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT");
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_nosession",
            &[json!(id.to_string())],
            &[], // no session vars at all
            Some(&changelog),
        )
        .await
        .expect("outbox write succeeds without an injected started_at");

    let duration: Option<i32> = client
        .query_one("SELECT duration_ms FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("duration_ms");
    assert!(duration.is_some(), "duration_ms populated from the adapter-stamped started_at");
}

#[tokio::test]
async fn noop_and_failed_mutations_write_no_changelog_row() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;

    // succeeded=true but state_changed=false (a no-op) → no spine event.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_noop() \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := false; \
             v.entity_type := 'OutboxNoop'; RETURN v; END; $$;",
        )
        .await
        .unwrap();
    // succeeded=false (a business-logic failure that still commits) → no event.
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_fail() \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := false; v.state_changed := false; \
             v.error_class := 'conflict'; v.entity_type := 'OutboxFail'; RETURN v; END; $$;",
        )
        .await
        .unwrap();

    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_noop",
            &[],
            STARTED_AT,
            Some(&ChangeLogWrite::new("OutboxNoop", "UPDATE")),
        )
        .await
        .unwrap();
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_fail",
            &[],
            STARTED_AT,
            Some(&ChangeLogWrite::new("OutboxFail", "INSERT")),
        )
        .await
        .unwrap();

    assert_eq!(count_rows(&client, "OutboxNoop").await, 0, "no-op writes no spine event");
    assert_eq!(count_rows(&client, "OutboxFail").await, 0, "failure writes no spine event");
}

#[tokio::test]
async fn object_type_falls_back_to_return_type_when_entity_type_is_null() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxFallback";

    // A state-changing mutation that returns NO entity_type — the NOT-NULL
    // object_type must fall back to the threaded value (the GraphQL return type).
    client
        .batch_execute(
            "CREATE OR REPLACE FUNCTION public.fn_outbox_noetype(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := true; v.entity_id := p_id; \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();

    let id = uuid::Uuid::new_v4();
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_noetype",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&ChangeLogWrite::new(obj_type, "DELETE")),
        )
        .await
        .unwrap();

    let object_type: String = client
        .query_one("SELECT object_type FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("object_type");
    assert_eq!(object_type, obj_type, "object_type falls back to the return type");
}
