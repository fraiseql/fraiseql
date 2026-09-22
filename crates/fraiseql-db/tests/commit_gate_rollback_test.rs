#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! Behavioural proof of the mutation commit gate (#1353).
//!
//! `execute_function_call_gated` runs the mutation function for real, hands the
//! rows it returned to a caller-supplied gate, and commits only if the gate agrees.
//! This is what lets a decision that can only be made *from the written row* — the
//! field-level authorizer (#423), whose contract takes the resolved entity as
//! `parent` — refuse the write rather than only its result.
//!
//! Before this seam existed the authorizer ran after the transaction had committed,
//! so a refused caller lost the field and kept the side effect. The refusing test
//! below is the one that reddens without the gate: the function's INSERT is in the
//! database either way, and only the ROLLBACK takes it back out.
//!
//! Runs against the harness-provided PostgreSQL (Dagger-bound in CI via the
//! `--test '*'` integration leg, or a local spawn with `local-testcontainers`).
//! Uniquely-named objects (`*_1353`) keep it isolated from the shared database.

use fraiseql_db::{DatabaseAdapter, PostgresAdapter};
use fraiseql_error::FraiseQLError;
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
/// that INSERTs into it and returns a success response carrying the written row.
///
/// Each test provisions its **own** table and function, suffixed by `case`. One
/// shared pair raced: these tests run in the same process on the same database, and
/// a `DROP TABLE` from one lands between another's `CREATE TABLE` and its first
/// INSERT. Per-case objects remove the race rather than serialising around it.
async fn provision(client: &tokio_postgres::Client, case: &str) {
    client
        .batch_execute(
            "CREATE SCHEMA IF NOT EXISTS app;
             DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); \
             EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$;
             DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$;",
        )
        .await
        .unwrap();
    client
        .batch_execute(&format!(
            "CREATE TABLE IF NOT EXISTS public.tb_commit_gate_{case}_1353 \
             (id uuid PRIMARY KEY, owner text);
             CREATE OR REPLACE FUNCTION public.fn_commit_gate_{case}_1353(p_id uuid, p_owner text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             INSERT INTO public.tb_commit_gate_{case}_1353 (id, owner) VALUES (p_id, p_owner); \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'Thing'; v.entity_id := p_id; \
             v.entity := jsonb_build_object('id', p_id, 'owner', p_owner); \
             RETURN v; END; $$;"
        ))
        .await
        .unwrap();
}

/// Count probe rows for a given id (the persistence check).
async fn probe_count(client: &tokio_postgres::Client, case: &str, id: uuid::Uuid) -> i64 {
    client
        .query_one(
            &format!("SELECT COUNT(*) FROM public.tb_commit_gate_{case}_1353 WHERE id = $1"),
            &[&id],
        )
        .await
        .unwrap()
        .get::<_, i64>(0)
}

/// The refusal the field authorizer raises, in the shape the executor raises it.
fn refusal() -> FraiseQLError {
    FraiseQLError::Authorization {
        message:  "Access denied to field 'owner' on type 'Thing'".to_string(),
        action:   Some("read".to_string()),
        resource: Some("Thing".to_string()),
    }
}

#[tokio::test]
async fn refusing_gate_rolls_the_write_back() {
    let (client, adapter, _svc) = connect().await;
    provision(&client, "refuse").await;

    let id = uuid::Uuid::new_v4();
    let gate = |rows: &[std::collections::HashMap<String, serde_json::Value>]| {
        // The gate sees the row the function produced — the thing a per-row decision
        // is keyed on, and the reason this decision cannot be taken before the write.
        assert_eq!(rows.len(), 1, "the gate is handed the function's row");
        assert_eq!(rows[0].get("succeeded"), Some(&json!(true)));
        Err(refusal())
    };
    let err = adapter
        .execute_function_call_gated(
            "public.fn_commit_gate_refuse_1353",
            &[json!(id.to_string()), json!("someone-else")],
            &[],
            None,
            &gate,
        )
        .await
        .expect_err("a refusing gate must fail the call");

    assert!(
        matches!(err, FraiseQLError::Authorization { .. }),
        "the gate's own refusal must reach the caller, not a transaction error: {err:?}"
    );
    assert_eq!(
        probe_count(&client, "refuse", id).await,
        0,
        "a refused write must not persist — this is #1353"
    );
}

#[tokio::test]
async fn allowing_gate_commits_the_write() {
    let (client, adapter, _svc) = connect().await;
    provision(&client, "allow").await;

    // Contrast: the same function, the same path, a gate that agrees. Without this
    // the `0` above could come from the function never writing at all.
    let id = uuid::Uuid::new_v4();
    let gate = |_rows: &[std::collections::HashMap<String, serde_json::Value>]| Ok(());
    let rows = adapter
        .execute_function_call_gated(
            "public.fn_commit_gate_allow_1353",
            &[json!(id.to_string()), json!("owner-1")],
            &[],
            None,
            &gate,
        )
        .await
        .expect("an allowing gate commits");

    assert_eq!(rows.len(), 1, "the function's row is returned to the caller");
    assert_eq!(probe_count(&client, "allow", id).await, 1, "an allowed write persists");

    // Leave the shared table clean for any subsequent run.
    client
        .execute("DELETE FROM public.tb_commit_gate_allow_1353 WHERE id = $1", &[&id])
        .await
        .unwrap();
}

#[tokio::test]
async fn refusing_gate_rolls_back_the_outbox_row_too() {
    let (client, adapter, _svc) = connect().await;
    provision(&client, "outbox").await;

    // The change-log outbox row is written by the same CTE, in the same transaction,
    // so a refusal must take it back out as well — an outbox row for a write that
    // never happened would be replayed by every spine consumer downstream.
    let id = uuid::Uuid::new_v4();
    let changelog = fraiseql_db::ChangeLogWrite::new("Thing", "INSERT");
    let gate = |_rows: &[std::collections::HashMap<String, serde_json::Value>]| Err(refusal());
    let err = adapter
        .execute_function_call_gated(
            "public.fn_commit_gate_outbox_1353",
            &[json!(id.to_string()), json!("someone-else")],
            &[],
            Some(&changelog),
            &gate,
        )
        .await
        .expect_err("a refusing gate must fail the call");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err:?}");

    assert_eq!(
        probe_count(&client, "outbox", id).await,
        0,
        "the refused write must not persist"
    );
    // Queried strictly: a `map_or(0, …)` here would turn "the outbox table is not
    // installed" into a passing assertion, which is the one way this test could claim
    // the rollback works without ever observing it.
    let outbox: i64 = client
        .query_one("SELECT COUNT(*) FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .expect("the change-log outbox table must exist for this assertion to mean anything")
        .get::<_, i64>(0);
    assert_eq!(outbox, 0, "the refused write must not leave an outbox row behind");
}
