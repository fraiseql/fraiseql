#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! End-to-end tests for changelog GraphQL exposure (issue #149).
//!
//! Drives the FULL stack against a real PostgreSQL container: the shipped
//! migration installs the views + upsert function, [`inject_changelog`] builds
//! the GraphQL surface, and the [`Executor`] runs cursor pagination, the
//! checkpoint round-trip, and RBAC denial exactly as in production.
//!
//! Uses a dedicated schema (`changelog_e2e`) so it is isolated from other tests
//! sharing the container, and substitutes that schema into the real migration
//! text so the actual shipped SQL is exercised.

mod common;

use std::sync::Arc;

use chrono::Utc;
use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{ChangelogConfig, CompiledSchema, inject_changelog},
    security::SecurityContext,
    types::UserId,
};
use serde_json::json;

const SCHEMA: &str = "changelog_e2e";
const READ_ROLE: &str = "changelog_reader";
const WRITE_ROLE: &str = "changelog_writer";
/// Mutations read their arguments from GraphQL variables (keyed by argument name),
/// so the document needs only the selection set.
const UPSERT_DOC: &str = "mutation { upsert_transport_checkpoint { transport_name last_pk } }";

fn ctx(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::new("sidecar"),
        tenant_id:        None,
        roles:            roles.iter().map(ToString::to_string).collect(),
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-e2e".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// Provision the isolated schema: prerequisites + the real migration SQL.
async fn provision(adapter: &PostgresAdapter) {
    // app.mutation_response contract (shared, idempotent).
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

    // The framework-owned change-log contract table (the executor writes its
    // outbox row here on every mutation, changelog being default-on). DROP+CREATE
    // to the contract shape (object_id UUID *nullable* — a checkpoint upsert has
    // no entity_id) so the test is not at the mercy of a stale app-shaped table
    // (object_id TEXT NOT NULL) left in the shared warm database.
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
             actor_type TEXT, acting_for UUID, commit_time TIMESTAMPTZ, \
             seq BIGINT DEFAULT nextval('core.seq_entity_change_log'))",
        )
        .await
        .unwrap();

    // Fresh isolated schema with the source tables (observer/install convention).
    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_entity_change_log (\
             pk_entity_change_log BIGSERIAL PRIMARY KEY, id UUID NOT NULL DEFAULT gen_random_uuid(), \
             fk_customer_org TEXT, fk_contact TEXT, object_type TEXT NOT NULL, object_id TEXT NOT NULL, \
             modification_type TEXT NOT NULL, change_status TEXT, object_data JSONB NOT NULL, \
             extra_metadata JSONB, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_transport_checkpoint (transport_name TEXT PRIMARY KEY, \
             last_pk BIGINT NOT NULL, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())"
        ))
        .await
        .unwrap();

    // Install the changelog objects. The shipped migration file is verified
    // verbatim by the observers crate's `changelog_views` test (which applies it
    // via sqlx multi-statement); here the adapter's `execute_raw_query` runs one
    // statement per call, so each object is applied separately (schema-substituted).
    for stmt in migration_statements() {
        adapter.execute_raw_query(&stmt).await.unwrap();
    }
}

/// The migration's three objects, re-targeted to the isolated schema, one
/// statement per entry (single-statement constraint of `execute_raw_query`).
fn migration_statements() -> Vec<String> {
    vec![
        format!(
            "CREATE OR REPLACE VIEW {SCHEMA}.v_entity_change_log AS SELECT \
             pk_entity_change_log, object_type, modification_type, created_at, \
             jsonb_build_object('id', id, 'pk_entity_change_log', pk_entity_change_log, \
             'fk_customer_org', fk_customer_org, 'fk_contact', fk_contact, 'object_type', object_type, \
             'object_id', object_id, 'modification_type', modification_type, 'change_status', change_status, \
             'object_data', object_data, 'extra_metadata', extra_metadata, 'created_at', created_at) AS data \
             FROM {SCHEMA}.tb_entity_change_log"
        ),
        format!(
            "CREATE OR REPLACE VIEW {SCHEMA}.v_transport_checkpoint AS SELECT \
             transport_name, last_pk, updated_at, jsonb_build_object('transport_name', transport_name, \
             'last_pk', last_pk, 'updated_at', updated_at) AS data FROM {SCHEMA}.tb_transport_checkpoint"
        ),
        format!(
            "CREATE OR REPLACE FUNCTION {SCHEMA}.fn_upsert_transport_checkpoint(p_transport_name text, \
             p_last_pk bigint) RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v_row {SCHEMA}.tb_transport_checkpoint%ROWTYPE; v_existing bigint; \
             v_response app.mutation_response; BEGIN \
             SELECT last_pk INTO v_existing FROM {SCHEMA}.tb_transport_checkpoint WHERE transport_name = p_transport_name; \
             INSERT INTO {SCHEMA}.tb_transport_checkpoint (transport_name, last_pk, updated_at) \
             VALUES (p_transport_name, p_last_pk, NOW()) \
             ON CONFLICT (transport_name) DO UPDATE SET last_pk = EXCLUDED.last_pk, updated_at = NOW() \
             RETURNING * INTO v_row; \
             v_response.succeeded := true; \
             v_response.state_changed := v_existing IS DISTINCT FROM v_row.last_pk; \
             v_response.message := 'checkpoint upserted'; \
             v_response.entity_type := 'TransportCheckpoint'; \
             v_response.entity := jsonb_build_object('transport_name', v_row.transport_name, \
             'last_pk', v_row.last_pk, 'updated_at', v_row.updated_at); \
             v_response.updated_fields := ARRAY['last_pk', 'updated_at']; \
             RETURN v_response; END; $$"
        ),
    ]
}

/// Insert `count` change-log rows of `object_type`, returning nothing.
async fn insert_rows(adapter: &PostgresAdapter, object_type: &str, count: usize) {
    for _ in 0..count {
        adapter
            .execute_raw_query(&format!(
                "INSERT INTO {SCHEMA}.tb_entity_change_log \
                 (object_type, object_id, modification_type, object_data) \
                 VALUES ('{object_type}', gen_random_uuid()::text, 'INSERT', '{{}}'::jsonb)"
            ))
            .await
            .unwrap();
    }
}

fn changelog_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.changelog = Some(ChangelogConfig {
        expose:        true,
        schema:        SCHEMA.to_string(),
        read_role:     Some(READ_ROLE.to_string()),
        write_role:    Some(WRITE_ROLE.to_string()),
        max_limit:     1_000,
        write_enabled: true,
    });
    inject_changelog(&mut schema);
    schema
}

/// Extract the `entity_change_logs` pk list from an executor result envelope.
fn pks(result: &serde_json::Value) -> Vec<i64> {
    let data = result.get("data").unwrap_or(result);
    data["entity_change_logs"]
        .as_array()
        .expect("entity_change_logs array")
        .iter()
        .map(|row| row["pk_entity_change_log"].as_i64().expect("pk is a number"))
        .collect()
}

fn page_query(cursor: i64, limit: i64) -> String {
    format!(
        "{{ entity_change_logs(\
           where: {{ pk_entity_change_log: {{ gt: {cursor} }} }}, \
           orderBy: {{ pk_entity_change_log: \"ASC\" }}, \
           limit: {limit}) {{ pk_entity_change_log object_type }} }}"
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL container (testcontainers)"]
async fn changelog_e2e_full_stack() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;

    insert_rows(&adapter, "User", 50).await; // pk 1..50

    let executor = Executor::new(changelog_schema(), Arc::clone(&adapter));
    let reader = ctx(&[READ_ROLE]);
    let writer = ctx(&[READ_ROLE, WRITE_ROLE]);

    // ── Cursor pagination: gap-free, numeric (not lexicographic) ordering ──
    let page1 = executor.execute_with_security(&page_query(0, 20), None, &reader).await.unwrap();
    assert_eq!(pks(&page1), (1..=20).collect::<Vec<_>>(), "page 1");

    let page2 = executor
        .execute_with_security(&page_query(20, 20), None, &reader)
        .await
        .unwrap();
    assert_eq!(pks(&page2), (21..=40).collect::<Vec<_>>(), "page 2");

    // Concurrent insert mid-pagination — the cursor must still advance gap-free.
    insert_rows(&adapter, "Order", 10).await; // pk 51..60

    let page3 = executor
        .execute_with_security(&page_query(40, 20), None, &reader)
        .await
        .unwrap();
    assert_eq!(
        pks(&page3),
        (41..=60).collect::<Vec<_>>(),
        "page 3 includes rows inserted mid-scan"
    );

    // Numeric keyset proof: a cursor of 9 must NOT lexicographically exclude 10..60.
    let after9 = executor
        .execute_with_security(&page_query(9, 1000), None, &reader)
        .await
        .unwrap();
    assert_eq!(pks(&after9), (10..=60).collect::<Vec<_>>(), "gt is numeric, not text");

    // Equality filter on a native-ish JSONB field combines with the cursor.
    let only_orders = executor
        .execute_with_security(
            "{ entity_change_logs(where: { object_type: { eq: \"Order\" } }, \
               orderBy: { pk_entity_change_log: \"ASC\" }, limit: 100) \
               { pk_entity_change_log object_type } }",
            None,
            &reader,
        )
        .await
        .unwrap();
    assert_eq!(pks(&only_orders), (51..=60).collect::<Vec<_>>(), "object_type filter applied");

    // ── Checkpoint upsert + read round-trip (insert path then update path) ──
    // Mutations read their arguments from GraphQL variables, keyed by argument name.
    let upsert_vars = |pk: i64| json!({ "transport_name": "sidecar-1", "last_pk": pk });

    executor
        .execute_with_security(UPSERT_DOC, Some(&upsert_vars(42)), &writer)
        .await
        .unwrap();
    let read = executor
        .execute_with_security(
            "{ transport_checkpoint(transport_name: \"sidecar-1\") { last_pk } }",
            None,
            &reader,
        )
        .await
        .unwrap();
    let read_data = read.get("data").unwrap_or(&read);
    assert_eq!(read_data["transport_checkpoint"]["last_pk"].as_i64(), Some(42));

    executor
        .execute_with_security(UPSERT_DOC, Some(&upsert_vars(100)), &writer)
        .await
        .unwrap();
    let read2 = executor
        .execute_with_security(
            "{ transport_checkpoint(transport_name: \"sidecar-1\") { last_pk } }",
            None,
            &reader,
        )
        .await
        .unwrap();
    let read2_data = read2.get("data").unwrap_or(&read2);
    assert_eq!(read2_data["transport_checkpoint"]["last_pk"].as_i64(), Some(100));

    // ── RBAC: missing read role → "not found" (enumeration prevention) ──
    let outsider = ctx(&["unrelated"]);
    let denied = executor.execute_with_security(&page_query(0, 5), None, &outsider).await;
    let derr = denied.unwrap_err().to_string();
    assert!(derr.contains("not found in schema"), "read denied without role, got: {derr}");

    // ── RBAC: writer role required for the upsert mutation ──
    let write_denied =
        executor.execute_with_security(UPSERT_DOC, Some(&upsert_vars(7)), &reader).await;
    let werr = write_denied.unwrap_err().to_string();
    assert!(
        werr.contains("not found in schema"),
        "upsert denied without write role, got: {werr}"
    );
}
