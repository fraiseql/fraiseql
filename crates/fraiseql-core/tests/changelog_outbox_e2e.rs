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
        .execute_raw_query(
            "CREATE TABLE core.tb_entity_change_log (\
             pk_entity_change_log BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY, \
             object_type TEXT NOT NULL, modification_type TEXT NOT NULL, \
             id UUID NOT NULL DEFAULT gen_random_uuid(), \
             created_at TIMESTAMPTZ NOT NULL DEFAULT now(), \
             object_id UUID, object_data JSONB, updated_fields TEXT[], cascade JSONB, \
             duration_ms INTEGER, started_at TIMESTAMPTZ, extra_metadata JSONB)",
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
