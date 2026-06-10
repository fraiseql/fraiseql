#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! End-to-end proof that the mutation **executor** writes the Change Spine
//! outbox row (phase-02). Drives a real mutation through the full [`Executor`]
//! against PostgreSQL and asserts that exactly one `core.tb_entity_change_log`
//! row is written, in the mutation's transaction, with the runner-threaded
//! `object_type`/`modification_type` and a wall-clock `duration_ms`.
//!
//! The adapter-level mechanics (atomicity, no-op skipping, `started_at`
//! visibility) are covered by `fraiseql-db`'s `changelog_outbox_test`; this test
//! is specifically about the runner → adapter wiring.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
    security::SecurityContext,
};
use serde_json::json;

const SCHEMA: &str = "changelog_outbox_e2e";

/// Provision: the `app.mutation_response` contract, a fresh framework-owned
/// `core.tb_entity_change_log` (migration-08 contract shape), and an isolated
/// schema with a `tb_thing` table + view + a state-changing update function.
async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE SCHEMA IF NOT EXISTS app").await.unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();

    // Fresh contract table (owns the shape; safe to recreate — see the db-crate
    // outbox test note). One statement per execute_raw_query call.
    adapter.execute_raw_query("CREATE SCHEMA IF NOT EXISTS core").await.unwrap();
    adapter
        .execute_raw_query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .await
        .unwrap();
    adapter
        .execute_raw_query("DROP SEQUENCE IF EXISTS core.seq_entity_change_log")
        .await
        .unwrap();
    adapter
        .execute_raw_query("CREATE SEQUENCE core.seq_entity_change_log")
        .await
        .unwrap();
    adapter
        .execute_raw_query(
            "CREATE TABLE core.tb_entity_change_log (\
             pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
             object_type TEXT NOT NULL, modification_type TEXT NOT NULL, \
             id UUID NOT NULL DEFAULT gen_random_uuid(), \
             created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
             object_id UUID, object_data JSONB, updated_fields TEXT[], cascade JSONB, \
             duration_ms INTEGER, started_at TIMESTAMPTZ, extra_metadata JSONB, \
             tenant_id UUID, trace_id TEXT, schema_version TEXT, trace_context JSONB, \
             actor_type TEXT, acting_for UUID, \
             commit_time TIMESTAMPTZ, \
             seq BIGINT DEFAULT nextval('core.seq_entity_change_log'))",
        )
        .await
        .unwrap();

    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_thing (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), name TEXT)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_thing AS SELECT id, \
             jsonb_build_object('id', id, 'name', name) AS data FROM {SCHEMA}.tb_thing"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE FUNCTION {SCHEMA}.fn_update_thing(p_id uuid, p_name text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v_row {SCHEMA}.tb_thing%ROWTYPE; v_response app.mutation_response; BEGIN \
             UPDATE {SCHEMA}.tb_thing SET name = p_name WHERE id = p_id RETURNING * INTO v_row; \
             v_response.succeeded := true; v_response.state_changed := true; \
             v_response.entity_type := 'Thing'; v_response.entity_id := v_row.id; \
             v_response.entity := jsonb_build_object('id', v_row.id, 'name', v_row.name); \
             v_response.updated_fields := ARRAY['name']; \
             RETURN v_response; END; $$"
        ))
        .await
        .unwrap();
}

async fn seed_thing(adapter: &PostgresAdapter) -> String {
    let rows = adapter
        .execute_raw_query(&format!(
            "INSERT INTO {SCHEMA}.tb_thing (name) VALUES ('Acme') RETURNING id::text AS id"
        ))
        .await
        .unwrap();
    rows.into_iter()
        .next()
        .and_then(|r| r.get("id").and_then(|v| v.as_str().map(ToString::to_string)))
        .expect("seeded id")
}

/// `changelog`: `None` omits the per-mutation flag (serde default `true`);
/// `Some(false)` opts this mutation out of the change-log write.
fn schema_with(changelog: Option<bool>) -> CompiledSchema {
    let mut mutation = json!({
        "name": "updateThing",
        "return_type": "Thing",
        "sql_source": format!("{SCHEMA}.fn_update_thing"),
        "operation": { "Update": { "table": "tb_thing" } },
        "arguments": [
            { "name": "id", "arg_type": "ID", "nullable": false },
            { "name": "name", "arg_type": "String", "nullable": true }
        ]
    });
    if let Some(flag) = changelog {
        mutation["changelog"] = json!(flag);
    }
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [
            {
                "name": "Thing",
                "sql_source": format!("{SCHEMA}.v_thing"),
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "name", "field_type": "String" }
                ]
            }
        ],
        "mutations": [mutation]
    }))
    .expect("schema")
}

fn schema() -> CompiledSchema {
    schema_with(None)
}

async fn count_outbox_rows(adapter: &PostgresAdapter) -> usize {
    adapter
        .execute_raw_query("SELECT pk_entity_change_log FROM core.tb_entity_change_log")
        .await
        .unwrap()
        .len()
}

#[tokio::test]
async fn executor_writes_one_outbox_row_per_mutation() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    let executor = Executor::new(schema(), Arc::clone(&adapter));

    let vars = json!({ "id": id, "name": "Acme Updated" });
    let res = executor.execute("mutation { updateThing { id } }", Some(&vars)).await.unwrap();
    assert_eq!(
        res.get("data").and_then(|d| d.get("updateThing")).and_then(|t| t.get("id")),
        Some(&json!(id)),
        "mutation returns the updated entity"
    );

    // The executor wrote exactly one outbox row, stamped from the mutation
    // definition (object_type=return type, modification_type=UPDATE) with the
    // changed-entity identity + a wall-clock duration.
    let rows = adapter
        .execute_raw_query(
            "SELECT object_type, modification_type, object_id::text AS object_id, \
             duration_ms, (extra_metadata->>'duration_calc_version') AS calc_version \
             FROM core.tb_entity_change_log",
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row for one mutation");
    let row = &rows[0];
    assert_eq!(row.get("object_type"), Some(&json!("Thing")));
    assert_eq!(row.get("modification_type"), Some(&json!("UPDATE")));
    assert_eq!(row.get("object_id"), Some(&json!(id)));
    assert!(
        row.get("duration_ms").is_some_and(|v| !v.is_null()),
        "duration_ms populated, got {:?}",
        row.get("duration_ms")
    );
    assert_eq!(row.get("calc_version"), Some(&json!("2")), "data-quality marker stamped");
}

/// A `SecurityContext` carrying `tenant`, the way the authenticated mutation
/// entry point (`execute_with_security`) receives it from a validated JWT.
fn security_ctx_with_tenant(tenant: &str) -> SecurityContext {
    use fraiseql_core::types::{TenantId, UserId};
    SecurityContext {
        user_id:          UserId::new("user-e2e"),
        roles:            vec![],
        tenant_id:        Some(TenantId::new(tenant)),
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-e2e".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

#[tokio::test]
async fn executor_stamps_tenant_id_from_the_security_context() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // Drive the mutation through the authenticated entry point with a UUID tenant.
    // The runner stamps tenant_id EXPLICITLY from the SecurityContext (not from any
    // RLS/session GUC), written to the contract's tenant_id UUID column.
    let tenant = uuid::Uuid::new_v4();
    let ctx = security_ctx_with_tenant(&tenant.to_string());
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor
        .execute_with_security("mutation { updateThing { id } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let rows = adapter
        .execute_raw_query("SELECT tenant_id::text AS tenant_id FROM core.tb_entity_change_log")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row");
    assert_eq!(
        rows[0].get("tenant_id"),
        Some(&json!(tenant.to_string())),
        "tenant_id stamped from the SecurityContext"
    );
}

#[tokio::test]
async fn executor_stamps_actor_type_and_acting_for_from_the_security_context() {
    use fraiseql_core::security::ActorType;

    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // A SecurityContext classified as an agent acting for a human — the runner
    // stamps actor_type + acting_for from it onto the outbox row (#390).
    let human = uuid::Uuid::new_v4();
    let ctx = security_ctx_with_tenant(&uuid::Uuid::new_v4().to_string())
        .with_actor_type(ActorType::AiAgent)
        .with_acting_for(Some(human));
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor
        .execute_with_security("mutation { updateThing { id } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let rows = adapter
        .execute_raw_query(
            "SELECT actor_type, acting_for::text AS acting_for FROM core.tb_entity_change_log",
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row");
    assert_eq!(
        rows[0].get("actor_type"),
        Some(&json!("ai_agent")),
        "actor_type stamped from the SecurityContext"
    );
    assert_eq!(
        rows[0].get("acting_for"),
        Some(&json!(human.to_string())),
        "acting_for stamped as the delegated human's UUID"
    );
}

#[tokio::test]
async fn executor_stamps_trace_id_from_the_security_context() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // A SecurityContext carrying a W3C trace id (stamped by the server from the
    // inbound `traceparent`) → the runner writes it to the trace_id column (#375),
    // the #392 perf tooling's investigation handle.
    let trace = "4bf92f3577b34da6a3ce929d0e0e4736";
    let ctx = security_ctx_with_tenant(&uuid::Uuid::new_v4().to_string()).with_trace_id(trace);
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor
        .execute_with_security("mutation { updateThing { id } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let rows = adapter
        .execute_raw_query("SELECT trace_id FROM core.tb_entity_change_log")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row");
    assert_eq!(
        rows[0].get("trace_id"),
        Some(&json!(trace)),
        "trace_id stamped from the SecurityContext"
    );
}

#[tokio::test]
async fn executor_stamps_schema_version_from_the_compiled_schema() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // schema_version is a per-deployment constant derived from the COMPILED SCHEMA
    // (its content hash), NOT from the request — the executor stamps it on every
    // outbox row so #378 (zero-downtime / DLQ replay) can detect a row produced
    // under a different schema and reject loudly.
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let expected = executor.schema().content_hash();
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor.execute("mutation { updateThing { id } }", Some(&vars)).await.unwrap();

    let rows = adapter
        .execute_raw_query("SELECT schema_version FROM core.tb_entity_change_log")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row");
    assert_eq!(
        rows[0].get("schema_version"),
        Some(&json!(expected)),
        "schema_version stamped from the compiled schema's content hash"
    );
}

#[tokio::test]
async fn executor_stamps_trace_context_from_the_security_context() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // A SecurityContext carrying the full W3C trace context (the server builds it
    // from the inbound traceparent/tracestate) → the runner writes it verbatim to
    // the JSONB trace_context column (#375), completing the trace linkage beyond the
    // scalar trace_id.
    let trace_context = json!({
        "version": "00",
        "trace_id": "4bf92f3577b34da6a3ce929d0e0e4736",
        "parent_id": "00f067aa0ba902b7",
        "trace_flags": "01"
    });
    let ctx = security_ctx_with_tenant(&uuid::Uuid::new_v4().to_string())
        .with_trace_context(trace_context.clone());
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor
        .execute_with_security("mutation { updateThing { id } }", Some(&vars), &ctx)
        .await
        .unwrap();

    let rows = adapter
        .execute_raw_query("SELECT trace_context FROM core.tb_entity_change_log")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row");
    assert_eq!(
        rows[0].get("trace_context"),
        Some(&trace_context),
        "trace_context stamped from the SecurityContext"
    );
}

#[tokio::test]
async fn non_uuid_tenant_leaves_tenant_id_null_without_aborting() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // A non-UUID tenant identifier (the framework does not constrain TenantId to
    // UUID) must NOT abort the mutation — tenant_id is simply left NULL.
    let ctx = security_ctx_with_tenant("acme_corp_not_a_uuid");
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor
        .execute_with_security("mutation { updateThing { id } }", Some(&vars), &ctx)
        .await
        .expect("mutation succeeds despite a non-UUID tenant id");

    let rows = adapter
        .execute_raw_query("SELECT tenant_id FROM core.tb_entity_change_log")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "the mutation + outbox row still happen");
    assert!(
        rows[0].get("tenant_id").is_none_or(serde_json::Value::is_null),
        "tenant_id is NULL for a non-UUID tenant, got {:?}",
        rows[0].get("tenant_id")
    );
}

#[tokio::test]
async fn per_mutation_opt_out_writes_no_outbox_row() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // The mutation is individually opted out (`changelog: false`) — the runner
    // passes None, so the function still runs but no outbox row is written.
    let executor = Executor::new(schema_with(Some(false)), Arc::clone(&adapter));
    let vars = json!({ "id": id, "name": "Acme Updated" });
    let res = executor.execute("mutation { updateThing { id } }", Some(&vars)).await.unwrap();
    assert_eq!(
        res.get("data").and_then(|d| d.get("updateThing")).and_then(|t| t.get("id")),
        Some(&json!(id)),
        "the mutation itself still succeeds"
    );
    assert_eq!(count_outbox_rows(&adapter).await, 0, "opted-out mutation writes no outbox row");
}

#[tokio::test]
async fn global_opt_out_writes_no_outbox_row() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    // Global switch off (changelog_enabled=false) suppresses the write even though
    // the mutation itself is not individually opted out (default-on).
    let config = RuntimeConfig {
        changelog_enabled: false,
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema(), Arc::clone(&adapter), config);
    let vars = json!({ "id": id, "name": "Acme Updated" });
    executor.execute("mutation { updateThing { id } }", Some(&vars)).await.unwrap();
    assert_eq!(
        count_outbox_rows(&adapter).await,
        0,
        "globally-disabled change-log writes no outbox row"
    );
}
