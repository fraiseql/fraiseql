//! Live-PostgreSQL integration tests for the `validate --against-db`
//! mutation-contract check (#397).
//!
//! These exercise the real `pg_proc`/`pg_type` introspection in
//! [`fraiseql_cli::schema::pg_catalog`] against a database, creating a dedicated
//! `fql_397_test` schema with correct and broken mutation functions and
//! dropping it afterwards. They self-skip when no `DATABASE_URL` is set, so they
//! are inert in the database-free test leg (even under `--all-features`).

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::schema::{
    mutation_contract::{CallShape, ContractViolation, ExpectedCall, check_mutation},
    pg_catalog::PgCatalog,
};
use tokio_postgres::NoTls;

const SCHEMA: &str = "fql_397_test";

/// DDL: a correct mutation, plus several deliberately broken ones.
const SETUP: &str = "\
DROP SCHEMA IF EXISTS fql_397_test CASCADE;
CREATE SCHEMA fql_397_test;
CREATE TYPE fql_397_test.mutation_response AS (
  succeeded boolean, state_changed boolean, error_class text, status_detail text,
  http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb,
  updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb);
-- Correct: payload-first jsonb + trailing inject param, returns the composite.
CREATE FUNCTION fql_397_test.fn_update_user(input jsonb, tenant_id uuid)
  RETURNS SETOF fql_397_test.mutation_response LANGUAGE sql AS
  $$ SELECT NULL::fql_397_test.mutation_response $$;
-- Correct, RETURNS TABLE convention, flat single arg.
CREATE FUNCTION fql_397_test.fn_create_user(p_input jsonb)
  RETURNS TABLE(succeeded boolean, state_changed boolean, entity jsonb)
  LANGUAGE plpgsql AS $$ BEGIN RETURN; END $$;
-- Broken: first param is text, not jsonb (update payload).
CREATE FUNCTION fql_397_test.fn_bad_payload(input text, tenant_id uuid)
  RETURNS SETOF fql_397_test.mutation_response LANGUAGE sql AS
  $$ SELECT NULL::fql_397_test.mutation_response $$;
-- Broken: response row has no `succeeded` / `state_changed`.
CREATE FUNCTION fql_397_test.fn_bad_response(input jsonb)
  RETURNS TABLE(status text, message text)
  LANGUAGE plpgsql AS $$ BEGIN RETURN; END $$;
-- Ambiguous: two overloads at arity 1.
CREATE FUNCTION fql_397_test.fn_amb(a jsonb) RETURNS boolean LANGUAGE sql AS $$ SELECT true $$;
CREATE FUNCTION fql_397_test.fn_amb(a text) RETURNS boolean LANGUAGE sql AS $$ SELECT true $$;
";

const TEARDOWN: &str = "DROP SCHEMA IF EXISTS fql_397_test CASCADE;";

fn qualified(name: &str) -> String {
    format!("{SCHEMA}.{name}")
}

fn jsonb_update(sql_source: &str, inject: &[&str]) -> ExpectedCall {
    ExpectedCall {
        sql_source:             sql_source.to_string(),
        shape:                  CallShape::JsonbPayload,
        base_arity:             1,
        inject_names:           inject.iter().map(|s| (*s).to_string()).collect(),
        first_is_jsonb_payload: true,
    }
}

fn flat(sql_source: &str, base_arity: usize) -> ExpectedCall {
    ExpectedCall {
        sql_source: sql_source.to_string(),
        shape: CallShape::FlatArgs,
        base_arity,
        inject_names: vec![],
        first_is_jsonb_payload: false,
    }
}

/// Connect, run DDL, and return a [`PgCatalog`]. Returns `None` to signal skip.
async fn setup() -> Option<PgCatalog> {
    let url = fraiseql_test_support::try_database_url()?;
    let (client, connection) = match tokio_postgres::connect(&url, NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #397 against-db test: cannot connect ({e})");
            return None;
        },
    };
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(SETUP).await.expect("setup DDL failed");
    Some(PgCatalog::connect(&url).expect("PgCatalog connect"))
}

async fn teardown() {
    if let Some(url) = fraiseql_test_support::try_database_url() {
        if let Ok((client, connection)) = tokio_postgres::connect(&url, NoTls).await {
            tokio::spawn(async move {
                let _ = connection.await;
            });
            let _ = client.batch_execute(TEARDOWN).await;
        }
    }
}

#[tokio::test]
async fn correct_mutation_has_no_violations() {
    let Some(catalog) = setup().await else { return };

    let expected = jsonb_update(&qualified("fn_update_user"), &["tenant_id"]);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    assert_eq!(candidates.len(), 1, "one overload of fn_update_user");
    assert_eq!(candidates[0].in_types, vec!["jsonb", "uuid"]);
    // The composite return type expands to the 13-column response row.
    assert_eq!(candidates[0].out_columns.len(), 13);

    let violations = check_mutation(&expected, &candidates);
    assert!(violations.is_empty(), "expected clean, got {violations:?}");

    // RETURNS TABLE convention also resolves its output columns.
    let create = flat(&qualified("fn_create_user"), 1);
    let c2 = catalog.resolve_functions(&create.sql_source).await.unwrap();
    assert_eq!(c2[0].out_columns.len(), 3, "TABLE columns introspected");
    assert!(check_mutation(&create, &c2).is_empty());

    teardown().await;
}

#[tokio::test]
async fn missing_function_is_reported() {
    let Some(catalog) = setup().await else { return };

    let expected = jsonb_update(&qualified("fn_absent"), &[]);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    assert!(candidates.is_empty());
    assert_eq!(check_mutation(&expected, &candidates), vec![ContractViolation::MissingFunction]);

    teardown().await;
}

#[tokio::test]
async fn wrong_arity_is_reported() {
    let Some(catalog) = setup().await else { return };

    // fn_update_user takes 2 args; expect 1 (no inject) → mismatch.
    let expected = jsonb_update(&qualified("fn_update_user"), &[]);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    let violations = check_mutation(&expected, &candidates);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, ContractViolation::ArityMismatch { expected: 1, .. })),
        "got {violations:?}"
    );

    teardown().await;
}

#[tokio::test]
async fn non_jsonb_payload_is_reported() {
    let Some(catalog) = setup().await else { return };

    let expected = jsonb_update(&qualified("fn_bad_payload"), &["tenant_id"]);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    let violations = check_mutation(&expected, &candidates);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, ContractViolation::PayloadNotJsonb { .. })),
        "got {violations:?}"
    );

    teardown().await;
}

#[tokio::test]
async fn missing_response_columns_are_reported() {
    let Some(catalog) = setup().await else { return };

    let expected = flat(&qualified("fn_bad_response"), 1);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    let violations = check_mutation(&expected, &candidates);
    assert!(
        violations.contains(&ContractViolation::MissingRequiredColumn {
            column: "succeeded",
        }),
        "got {violations:?}"
    );
    assert!(
        violations.contains(&ContractViolation::MissingRequiredColumn {
            column: "state_changed",
        }),
        "got {violations:?}"
    );

    teardown().await;
}

#[tokio::test]
async fn ambiguous_overloads_are_reported() {
    let Some(catalog) = setup().await else { return };

    let expected = flat(&qualified("fn_amb"), 1);
    let candidates = catalog.resolve_functions(&expected.sql_source).await.unwrap();
    assert_eq!(candidates.len(), 2, "two overloads of fn_amb");
    let violations = check_mutation(&expected, &candidates);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, ContractViolation::AmbiguousFunction { .. })),
        "got {violations:?}"
    );

    teardown().await;
}
