//! Live-PostgreSQL integration tests for the `compile --database` /
//! `validate_schema_against_database` existence probes (#485).
//!
//! Proves the three correctness fixes against a real catalog:
//! 1. A mutation's `sql_source` is probed as a **function**, not a relation.
//! 2. A schema-qualified, off-`search_path` relation is resolved **verbatim** via `to_regclass`, so
//!    a mixed-case view the runtime serves is not false-flagged.
//! 3. The L3 JSON-key check uses the acronym/digit-aware caser.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free
//! test leg (even under `--all-features`).

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::schema::database_validator::{
    DatabaseWarning, create_introspector, validate_schema_against_database,
};
use fraiseql_core::schema::{
    CompiledSchema, FieldDefinition, FieldType, MutationDefinition, QueryDefinition, TypeDefinition,
};
use tokio_postgres::NoTls;

const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_485_test CASCADE;
CREATE SCHEMA fql_485_test;
-- A backed mutation function (the runtime calls SELECT * FROM fn(...)).
CREATE FUNCTION fql_485_test.fn_create_order(p_input jsonb)
  RETURNS jsonb LANGUAGE sql AS $$ SELECT p_input $$;
-- A schema-qualified view in an off-search_path schema.
CREATE VIEW fql_485_test.v_orders AS SELECT '{}'::jsonb AS data;
-- A MIXED-CASE qualified view, referenced verbatim (a bare to_regclass folds case).
CREATE VIEW \"fql_485_test\".\"V_Orders\" AS SELECT '{}'::jsonb AS data;
-- A view exposing acronym/digit JSON keys for the L3 caser check.
CREATE VIEW fql_485_test.v_logs AS
  SELECT '{\"http_response\": 200, \"dns_1_id\": \"x\"}'::jsonb AS data;
";

const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_485_test CASCADE;";

/// Connect, run DDL. Returns `None` to signal skip (no/unreachable DB).
async fn setup() -> Option<String> {
    let url = fraiseql_test_support::try_database_url()?;
    let (client, connection) = match tokio_postgres::connect(&url, NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #485 against-db test: cannot connect ({e})");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(SETUP).await.expect("setup DDL failed");
    Some(url)
}

async fn teardown(url: &str) {
    if let Ok((client, connection)) = tokio_postgres::connect(url, NoTls).await {
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let _ = client.batch_execute(TEARDOWN).await;
    }
}

fn query(name: &str, return_type: &str, sql_source: &str) -> QueryDefinition {
    QueryDefinition::new(name, return_type)
        .with_sql_source(sql_source)
        .returning_list()
}

fn mutation(name: &str, sql_source: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "T");
    m.sql_source = Some(sql_source.to_string());
    m
}

async fn warnings_for(url: &str, schema: CompiledSchema) -> Vec<DatabaseWarning> {
    let introspector = create_introspector(url).await.expect("introspector");
    validate_schema_against_database(&schema, &introspector).await.unwrap().warnings
}

#[tokio::test]
async fn mutation_backed_by_function_is_not_probed_as_relation() {
    let Some(url) = setup().await else { return };

    let schema = CompiledSchema {
        mutations: vec![mutation("createOrder", "fql_485_test.fn_create_order")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        !warnings.iter().any(|w| matches!(
            w,
            DatabaseWarning::MissingRelation { .. } | DatabaseWarning::MissingFunction { .. }
        )),
        "a backed mutation function must not warn, got {warnings:?}"
    );

    teardown(&url).await;
}

#[tokio::test]
async fn missing_mutation_function_reports_missing_function() {
    let Some(url) = setup().await else { return };

    let schema = CompiledSchema {
        mutations: vec![mutation("createOrder", "fql_485_test.fn_absent")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            DatabaseWarning::MissingFunction { sql_source, .. }
                if sql_source == "fql_485_test.fn_absent"
        )),
        "absent function must report MissingFunction, got {warnings:?}"
    );

    teardown(&url).await;
}

#[tokio::test]
async fn qualified_off_search_path_view_is_resolved_verbatim() {
    let Some(url) = setup().await else { return };

    // fql_485_test is not on the connection search_path, so the relation MAP
    // (current_schemas-scoped) misses it — the old probe false-failed here. The
    // to_regclass probe resolves the qualified name regardless of search_path.
    let schema = CompiledSchema {
        queries: vec![query("orders", "T", "fql_485_test.v_orders")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        !warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingRelation { .. })),
        "an off-search_path qualified view must resolve, got {warnings:?}"
    );

    teardown(&url).await;
}

#[tokio::test]
async fn truly_missing_qualified_view_reports_missing_relation() {
    let Some(url) = setup().await else { return };

    let schema = CompiledSchema {
        queries: vec![query("orders", "T", "fql_485_test.v_missing")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            DatabaseWarning::MissingRelation { sql_source, .. }
                if sql_source == "fql_485_test.v_missing"
        )),
        "a genuinely-absent qualified view must still warn, got {warnings:?}"
    );

    teardown(&url).await;
}

#[tokio::test]
async fn mixed_case_qualified_view_resolves_case_sensitively() {
    let Some(url) = setup().await else { return };

    // "V_Orders" exists verbatim; a bare to_regclass('fql_485_test.V_Orders') would
    // case-fold to v_orders. Quoting each component proves verbatim resolution.
    let schema = CompiledSchema {
        queries: vec![query("orders", "T", "fql_485_test.V_Orders")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        !warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingRelation { .. })),
        "a mixed-case qualified view must resolve verbatim, got {warnings:?}"
    );

    teardown(&url).await;
}

#[tokio::test]
async fn acronym_digit_fields_do_not_false_flag_missing_json_key() {
    let Some(url) = setup().await else { return };

    // Field `dns1Id` snakes to `dns_1_id` with the canonical caser; the old
    // uppercase-only caser produced `dns1_id` → spurious MissingJsonKey.
    let log_type = TypeDefinition::new("Log", "fql_485_test.v_logs")
        .with_field(FieldDefinition::new("httpResponse", FieldType::Int))
        .with_field(FieldDefinition::new("dns1Id", FieldType::String));
    let schema = CompiledSchema {
        types: vec![log_type],
        queries: vec![query("logs", "Log", "fql_485_test.v_logs")],
        ..Default::default()
    };
    let warnings = warnings_for(&url, schema).await;
    assert!(
        !warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingJsonKey { .. })),
        "acronym/digit fields must not false-flag MissingJsonKey, got {warnings:?}"
    );

    teardown(&url).await;
}
