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
             DROP SEQUENCE IF EXISTS core.seq_entity_change_log;
             CREATE SEQUENCE core.seq_entity_change_log;
             CREATE TABLE core.tb_entity_change_log (\
               pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
               object_type TEXT NOT NULL, modification_type TEXT NOT NULL, \
               id UUID NOT NULL DEFAULT gen_random_uuid(), \
               created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
               object_id UUID, object_data JSONB, updated_fields TEXT[], cascade JSONB, \
               duration_ms INTEGER, started_at TIMESTAMPTZ, extra_metadata JSONB, \
               tenant_id UUID, commit_time TIMESTAMPTZ, \
               seq BIGINT DEFAULT nextval('core.seq_entity_change_log'), \
               actor_type TEXT, acting_for UUID, schema_version TEXT, \
               trace_id TEXT, trace_context JSONB);",
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

/// A successful, state-changing mutation function returning a fixed identity —
/// the workhorse used by the envelope-stamping tests below.
async fn create_ok_fn(client: &tokio_postgres::Client, fn_name: &str, etype: &str) {
    client
        .batch_execute(&format!(
            "CREATE OR REPLACE FUNCTION public.{fn_name}(p_id uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; BEGIN \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := '{etype}'; v.entity_id := p_id; \
             RETURN v; END; $$;"
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn tenant_id_stamped_explicitly_from_the_envelope() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxTenant";
    create_ok_fn(&client, "fn_outbox_tenant", obj_type).await;

    // The tenant partition id is carried on the ChangeLogWrite (sourced from the
    // SecurityContext in production) and written to the tenant_id UUID column —
    // NOT reconstructed from any RLS/session GUC.
    let tenant = uuid::Uuid::new_v4();
    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT").with_tenant_id(Some(tenant));
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_tenant",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let stamped: uuid::Uuid = client
        .query_one("SELECT tenant_id FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("tenant_id");
    assert_eq!(stamped, tenant, "tenant_id stamped verbatim from the envelope");
}

#[tokio::test]
async fn actor_type_and_acting_for_stamped_explicitly_from_the_envelope() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxActor";
    create_ok_fn(&client, "fn_outbox_actor", obj_type).await;

    // An agent acting for a human: the actor classification + the delegated human's
    // UUID are carried on the ChangeLogWrite (sourced from the SecurityContext in
    // production) and written to the actor_type/acting_for columns (#390).
    let human = uuid::Uuid::new_v4();
    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT")
        .with_actor_type(Some("ai_agent"))
        .with_acting_for(Some(human));
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_actor",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT actor_type, acting_for FROM core.tb_entity_change_log WHERE object_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    assert_eq!(
        row.get::<_, Option<String>>("actor_type").as_deref(),
        Some("ai_agent"),
        "actor_type stamped verbatim from the envelope"
    );
    assert_eq!(
        row.get::<_, Option<uuid::Uuid>>("acting_for"),
        Some(human),
        "acting_for stamped as the delegated human's UUID"
    );
}

#[tokio::test]
async fn trace_id_stamped_explicitly_from_the_envelope() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxTrace";
    create_ok_fn(&client, "fn_outbox_trace", obj_type).await;

    // The originating request's W3C trace id is carried on the ChangeLogWrite
    // (sourced from SecurityContext.trace_id in production) and written verbatim to
    // the text trace_id column (#375) — the #392 perf tooling's investigation handle.
    let trace = "4bf92f3577b34da6a3ce929d0e0e4736";
    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT").with_trace_id(Some(trace));
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_trace",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let stamped: String = client
        .query_one("SELECT trace_id FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("trace_id");
    assert_eq!(stamped, trace, "trace_id stamped verbatim from the envelope");
}

#[tokio::test]
async fn schema_version_stamped_explicitly_from_the_envelope() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxSchemaVersion";
    create_ok_fn(&client, "fn_outbox_schema_version", obj_type).await;

    // The per-deployment compiled-schema version is carried on the ChangeLogWrite
    // (sourced from CompiledSchema::content_hash() in production) and written verbatim
    // to the text schema_version column (#377) — the replay/zero-downtime correctness
    // handle for #378.
    let version = "9f8e7d6c5b4a39281706a5b4c3d2e1f0";
    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT").with_schema_version(Some(version));
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_schema_version",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let stamped: String = client
        .query_one(
            "SELECT schema_version FROM core.tb_entity_change_log WHERE object_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get("schema_version");
    assert_eq!(stamped, version, "schema_version stamped verbatim from the envelope");
}

#[tokio::test]
async fn trace_context_stamped_explicitly_from_the_envelope() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxTraceContext";
    create_ok_fn(&client, "fn_outbox_trace_context", obj_type).await;

    // The full W3C trace context is carried on the ChangeLogWrite as JSON text
    // (built from the request traceparent/tracestate in production) and written to
    // the JSONB trace_context column (#375) — the re-propagation / full-trace handle.
    let trace_context = json!({
        "version": "00",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "parent_id": "00f067aa0ba902b7",
        "trace_flags": "01",
        "tracestate": "congo=t61rcWkgMzE"
    });
    let trace_context_text = trace_context.to_string();
    let id = uuid::Uuid::new_v4();
    let changelog =
        ChangeLogWrite::new(obj_type, "INSERT").with_trace_context(Some(&trace_context_text));
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_trace_context",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let stamped: serde_json::Value = client
        .query_one(
            "SELECT trace_context FROM core.tb_entity_change_log WHERE object_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get("trace_context");
    assert_eq!(stamped, trace_context, "trace_context stamped verbatim from the envelope");
}

#[tokio::test]
async fn tenant_id_is_null_when_unset() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxNoTenant";
    create_ok_fn(&client, "fn_outbox_no_tenant", obj_type).await;

    // No tenant on the envelope (unauthenticated / no-tenant / non-UUID tenant) →
    // the column is NULL, never a lossy cast and never an aborted mutation.
    let id = uuid::Uuid::new_v4();
    let changelog = ChangeLogWrite::new(obj_type, "INSERT");
    assert_eq!(changelog.tenant_id, None);
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_no_tenant",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&changelog),
        )
        .await
        .unwrap();

    let stamped: Option<uuid::Uuid> = client
        .query_one("SELECT tenant_id FROM core.tb_entity_change_log WHERE object_id = $1", &[&id])
        .await
        .unwrap()
        .get("tenant_id");
    assert_eq!(stamped, None, "tenant_id is NULL when the envelope carries none");
}

#[tokio::test]
async fn seq_is_monotonic_and_distinct_from_the_sequence_default() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxSeq";
    create_ok_fn(&client, "fn_outbox_seq", obj_type).await;

    // The executor INSERT omits `seq`, so the column's SEQUENCE default fires on
    // every row — proving any INSERTer (incl. cooperative external producers)
    // gets a monotonic value without the executor managing a counter.
    for _ in 0..3 {
        adapter
            .execute_function_call_with_changelog(
                "public.fn_outbox_seq",
                &[json!(uuid::Uuid::new_v4().to_string())],
                STARTED_AT,
                Some(&ChangeLogWrite::new(obj_type, "INSERT")),
            )
            .await
            .unwrap();
    }

    let seqs: Vec<i64> = client
        .query(
            "SELECT seq FROM core.tb_entity_change_log WHERE object_type = $1 \
             ORDER BY pk_entity_change_log",
            &[&obj_type],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get::<_, i64>("seq"))
        .collect();

    assert_eq!(seqs.len(), 3, "one seq per row");
    assert!(
        seqs.windows(2).all(|w| w[1] > w[0]),
        "seq is strictly increasing (monotonic, gap-tolerant): {seqs:?}"
    );
    let mut distinct = seqs.clone();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        3,
        "seq values are distinct (dedup on (object_type, seq)): {seqs:?}"
    );
}

#[tokio::test]
async fn commit_time_stamped_and_deferred_envelope_columns_left_null() {
    let (client, adapter, _svc) = connect().await;
    provision(&client).await;
    let obj_type = "OutboxEnvelope";
    create_ok_fn(&client, "fn_outbox_envelope", obj_type).await;

    let id = uuid::Uuid::new_v4();
    adapter
        .execute_function_call_with_changelog(
            "public.fn_outbox_envelope",
            &[json!(id.to_string())],
            STARTED_AT,
            Some(&ChangeLogWrite::new(obj_type, "INSERT")),
        )
        .await
        .unwrap();

    let row = client
        .query_one(
            "SELECT commit_time IS NOT NULL AS has_commit_time, \
             actor_type, acting_for, schema_version, trace_id, trace_context \
             FROM core.tb_entity_change_log WHERE object_id = $1",
            &[&id],
        )
        .await
        .unwrap();

    // commit_time is the DB clock at INSERT — the durable-ordering basis.
    assert!(
        row.get::<_, bool>("has_commit_time"),
        "commit_time stamped with clock_timestamp()"
    );
    // Envelope columns this ChangeLogWrite does not stamp are left NULL — and the
    // INSERT never errors on a missing source. This bare descriptor stamps none of
    // `actor_type`/`acting_for` (#390), `trace_id`/`trace_context` (#375) or
    // `schema_version` (#377); their populated paths are covered by the dedicated
    // `*_stamped_explicitly_from_the_envelope` tests.
    assert!(
        row.get::<_, Option<String>>("actor_type").is_none(),
        "actor_type NULL when the envelope does not stamp it"
    );
    assert!(
        row.get::<_, Option<uuid::Uuid>>("acting_for").is_none(),
        "acting_for NULL when the envelope does not stamp it"
    );
    assert!(
        row.get::<_, Option<String>>("schema_version").is_none(),
        "schema_version NULL when the envelope does not stamp it"
    );
    assert!(
        row.get::<_, Option<String>>("trace_id").is_none(),
        "trace_id NULL when the envelope does not stamp it"
    );
    assert!(
        row.get::<_, Option<serde_json::Value>>("trace_context").is_none(),
        "trace_context NULL when the envelope does not stamp it"
    );
}
