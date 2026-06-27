//! Live-PostgreSQL smoke tests for `fraiseql query` and `doctor --runtime` (#501).
//!
//! Self-skips when no `DATABASE_URL` is set (the `--all-features` fast leg compiles
//! these but skips at runtime; they run fully against the integration DB / locally).
//! Uniquely-named objects (`*_501`) keep the suite isolated from the shared database.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code — panics and skip diagnostics are acceptable

use std::io::Write;

use fraiseql_cli::commands::{
    doctor::{CheckStatus, runtime_probe_checks},
    query,
};
use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldType, MutationDefinition, QueryDefinition, TypeDefinition,
};
use tempfile::{Builder, NamedTempFile};

const VIEW: &str = "public.v_rt_smoke_thing_501";

/// A one-row view exposing the FraiseQL `data` JSONB column the executor projects.
async fn setup_view(url: &str) -> tokio_postgres::Client {
    let (client, connection) =
        tokio_postgres::connect(url, tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {e}");
        }
    });
    client
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS {VIEW}; \
             CREATE VIEW {VIEW} AS \
               SELECT jsonb_build_object('id', gen_random_uuid()::text, 'name', 'Acme') AS data;"
        ))
        .await
        .expect("create probe view");
    client
}

/// Build a compiled schema with one list query `things` backed by `sql_source`.
fn thing_schema(sql_source: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", sql_source);
    thing.jsonb_column = "data".to_string();
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    thing.fields.push(FieldDefinition::new("name", FieldType::String));
    schema.types.push(thing);
    schema.queries.push(
        QueryDefinition::new("things", "Thing")
            .returning_list()
            .with_sql_source(sql_source),
    );
    schema
}

/// Serialize a compiled schema to a temp `.json` file the CLI can load.
fn schema_file(schema: &CompiledSchema) -> NamedTempFile {
    let json = schema.to_json().expect("serialize schema");
    let mut f = Builder::new().suffix(".json").tempfile().unwrap();
    f.write_all(json.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[tokio::test]
async fn query_command_resolves_against_db() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #501 query smoke: no DATABASE_URL");
        return;
    };
    let client = setup_view(&url).await;
    let file = schema_file(&thing_schema("v_rt_smoke_thing_501"));

    let result = query::run("{ things { id name } }", file.path(), Some(url), None, false).await;

    client.batch_execute(&format!("DROP VIEW IF EXISTS {VIEW};")).await.ok();
    assert!(result.is_ok(), "query should resolve against the seeded view: {result:?}");
}

#[tokio::test]
async fn doctor_runtime_passes_on_resolvable_schema() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #501 doctor --runtime pass smoke: no DATABASE_URL");
        return;
    };
    let client = setup_view(&url).await;
    let file = schema_file(&thing_schema("v_rt_smoke_thing_501"));

    let checks = runtime_probe_checks(Some(&url), file.path()).await;

    client.batch_execute(&format!("DROP VIEW IF EXISTS {VIEW};")).await.ok();

    assert!(
        checks.iter().all(|c| c.status != CheckStatus::Fail),
        "no probe should fail on a resolvable schema: {checks:?}"
    );
    assert!(
        checks
            .iter()
            .any(|c| c.detail.contains("things") && c.detail.contains("resolves")),
        "the `things` query should report as resolving: {checks:?}"
    );
}

#[tokio::test]
async fn doctor_runtime_fails_on_missing_view() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #501 doctor --runtime fail smoke: no DATABASE_URL");
        return;
    };
    // Schema points the query at a view that does not exist → the probe must FAIL.
    let file = schema_file(&thing_schema("v_rt_smoke_absent_501"));

    let checks = runtime_probe_checks(Some(&url), file.path()).await;

    assert!(
        checks.iter().any(|c| c.status == CheckStatus::Fail),
        "a probe against a missing view must fail: {checks:?}"
    );
}

/// End-to-end proof that `--dry-run` wiring (CLI flag → `RuntimeConfig` →
/// mutation runner → adapter rollback) holds through the full GraphQL path: a
/// mutation executed via `query --dry-run` runs its function but commits nothing.
#[tokio::test]
async fn query_dry_run_mutation_rolls_back_end_to_end() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #501 query --dry-run e2e: no DATABASE_URL");
        return;
    };

    let (client, connection) =
        tokio_postgres::connect(&url, tokio_postgres::NoTls).await.expect("connect");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {e}");
        }
    });

    // Provision: mutation_response contract + a probe table + a no-arg function
    // that INSERTs and returns a success response carrying the new entity.
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
             DROP TABLE IF EXISTS public.tb_dry_run_e2e_501;
             CREATE TABLE public.tb_dry_run_e2e_501 (id uuid PRIMARY KEY, name text);
             CREATE OR REPLACE FUNCTION public.fn_create_thing_501() \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; r_id uuid := gen_random_uuid(); BEGIN \
             INSERT INTO public.tb_dry_run_e2e_501 (id, name) VALUES (r_id, 'e2e'); \
             v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'Thing'; v.entity_id := r_id; \
             v.entity := jsonb_build_object('id', r_id::text, 'name', 'e2e'); \
             RETURN v; END; $$;",
        )
        .await
        .expect("provision mutation fixture");

    // Schema: a Thing type + a createThing mutation backed by the function.
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", "v_unused_501");
    thing.jsonb_column = "data".to_string();
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(thing);
    let mut mutation = MutationDefinition::new("createThing", "Thing");
    mutation.sql_source = Some("public.fn_create_thing_501".to_string());
    schema.mutations.push(mutation);
    let file = schema_file(&schema);

    let result =
        query::run("mutation { createThing { id } }", file.path(), Some(url), None, true).await;

    let committed: i64 = client
        .query_one("SELECT COUNT(*) FROM public.tb_dry_run_e2e_501", &[])
        .await
        .unwrap()
        .get(0);
    client
        .batch_execute("DROP TABLE IF EXISTS public.tb_dry_run_e2e_501;")
        .await
        .ok();

    assert!(result.is_ok(), "dry-run mutation should execute: {result:?}");
    assert_eq!(committed, 0, "--dry-run must roll back the mutation's INSERT");
}
