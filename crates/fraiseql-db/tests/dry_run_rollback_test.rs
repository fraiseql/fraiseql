#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! Behavioural proof of `--dry-run` mutations (#501).
//!
//! The PostgreSQL adapter's `execute_function_call_dry_run` runs the mutation
//! function **for real** — so its `app.mutation_response` row is produced and any
//! constraint / trigger fires — but rolls the transaction back, so no writes
//! persist. The contrast test runs the same function via the committing
//! `execute_function_call` to prove the function genuinely inserts; the `0` rows
//! the dry-run leaves behind therefore come from the ROLLBACK, not a no-op.
//!
//! Runs against the harness-provided PostgreSQL (Dagger-bound in CI via the
//! `--test '*'` integration leg, or a local spawn with `local-testcontainers`).
//! Uniquely-named objects (`*_501`) keep it isolated from the shared database.

use fraiseql_db::{DatabaseAdapter, PostgresAdapter};
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

/// Provision the `app.mutation_response` contract, a probe table, and a function
/// that INSERTs into it and returns a success response.
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
             DROP TABLE IF EXISTS public.tb_dry_run_probe_501;
             CREATE TABLE public.tb_dry_run_probe_501 (id uuid PRIMARY KEY, name text);
             CREATE OR REPLACE FUNCTION public.fn_dry_run_insert_501(p_id uuid, p_name text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             INSERT INTO public.tb_dry_run_probe_501 (id, name) VALUES (p_id, p_name); \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'Thing'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id, 'name', p_name); \
             RETURN v; END; $$;",
        )
        .await
        .unwrap();
}

/// Count probe rows for a given id (the persistence check).
async fn probe_count(client: &tokio_postgres::Client, id: uuid::Uuid) -> i64 {
    client
        .query_one("SELECT COUNT(*) FROM public.tb_dry_run_probe_501 WHERE id = $1", &[&id])
        .await
        .unwrap()
        .get::<_, i64>(0)
}

#[tokio::test]
async fn dry_run_executes_function_but_rolls_back() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;

    let id = uuid::Uuid::new_v4();
    let rows = adapter
        .execute_function_call_dry_run(
            "public.fn_dry_run_insert_501",
            &[json!(id.to_string()), json!("DryRunOnly")],
            &[],
        )
        .await
        .expect("dry-run executes the function");

    // The function ran: its mutation_response row is returned to the caller.
    assert_eq!(rows.len(), 1, "the function's row is still returned");
    assert_eq!(rows[0].get("succeeded"), Some(&json!(true)), "function reports success");

    // …but nothing was committed: the INSERT was rolled back.
    assert_eq!(probe_count(&client, id).await, 0, "dry-run must not persist the INSERT");
}

#[tokio::test]
async fn committing_call_persists_for_contrast() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;

    // The same function, via the COMMITTING path, must persist its INSERT — proving
    // the `0` rows in the dry-run test come from the rollback, not a no-op function.
    let id = uuid::Uuid::new_v4();
    adapter
        .execute_function_call(
            "public.fn_dry_run_insert_501",
            &[json!(id.to_string()), json!("Committed")],
        )
        .await
        .expect("committing call executes the function");

    assert_eq!(probe_count(&client, id).await, 1, "committing call persists the INSERT");

    // Leave the shared table clean for any subsequent run.
    client
        .execute("DELETE FROM public.tb_dry_run_probe_501 WHERE id = $1", &[&id])
        .await
        .unwrap();
}
