//! Live-`PostgreSQL` integration tests for the `validate --against-db`
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
    mutation_contract::{
        CallShape, ContractViolation, ExpectedCall, Severity, check_mutation,
        validate_mutation_contract,
    },
    pg_catalog::PgCatalog,
};
use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InputFieldDefinition,
    InputObjectDefinition, InputStyle, MutationDefinition, MutationOperation,
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
-- Correct single-JSONB convention: an Insert that takes the whole input as one
-- jsonb arg (input_style = jsonb), backed by a single-`jsonb` function (#484).
CREATE FUNCTION fql_397_test.fn_create_order(p_input jsonb)
  RETURNS SETOF fql_397_test.mutation_response LANGUAGE sql AS
  $$ SELECT NULL::fql_397_test.mutation_response $$;
-- Broken: first param is text, not jsonb (update payload).
CREATE FUNCTION fql_397_test.fn_bad_payload(input text, tenant_id uuid)
  RETURNS SETOF fql_397_test.mutation_response LANGUAGE sql AS
  $$ SELECT NULL::fql_397_test.mutation_response $$;
-- Broken: response row has no `succeeded` / `state_changed`.
CREATE FUNCTION fql_397_test.fn_bad_response(input jsonb)
  RETURNS TABLE(status text, message text)
  LANGUAGE plpgsql AS $$ BEGIN RETURN; END $$;
-- Response shapes (#1397): the two required columns alone; the 13 columns; the 13 plus
-- `result jsonb` (what fraiseql.mutation_ok_result returns); `result` of the wrong type.
CREATE FUNCTION fql_397_test.fn_two(p_input jsonb)
  RETURNS TABLE(succeeded boolean, state_changed boolean)
  LANGUAGE plpgsql AS $$ BEGIN RETURN; END $$;
CREATE FUNCTION fql_397_test.fn_thirteen(p_input jsonb)
  RETURNS SETOF fql_397_test.mutation_response LANGUAGE sql AS
  $$ SELECT NULL::fql_397_test.mutation_response $$;
CREATE FUNCTION fql_397_test.fn_fourteen(p_input jsonb)
  RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text,
    status_detail text, http_status smallint, message text, entity_id uuid,
    entity_type text, entity jsonb, updated_fields text[], cascade jsonb,
    error_detail jsonb, metadata jsonb, result jsonb)
  LANGUAGE plpgsql AS $$ BEGIN RETURN; END $$;
CREATE FUNCTION fql_397_test.fn_text_result(p_input jsonb)
  RETURNS TABLE(succeeded boolean, state_changed boolean, result text)
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
        payload_keys:           vec![],
        stamps:                 None,
        requires_result:        false,
    }
}

fn flat(sql_source: &str, base_arity: usize) -> ExpectedCall {
    ExpectedCall {
        sql_source: sql_source.to_string(),
        shape: CallShape::FlatArgs,
        base_arity,
        inject_names: vec![],
        first_is_jsonb_payload: false,
        payload_keys: vec![],
        stamps: None,
        requires_result: false,
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
    Some(
        PgCatalog::connect(&url, &fraiseql_db::postgres::PostgresTlsConfig::default())
            .await
            .expect("PgCatalog connect"),
    )
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

/// Build a flat single-arg mutation (`FlatArgs` path) pointing at `sql_source`.
fn flat_mutation(name: &str, sql_source: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "User");
    m.sql_source = Some(sql_source.to_string());
    // A single non-Input `input` arg takes the FlatArgs path (base arity 1, no inject).
    m.arguments = vec![ArgumentDefinition::new("input", FieldType::String)];
    m
}

/// End-to-end coverage of `validate_mutation_contract` — the schema-walking entry
/// point `compile --database` calls (#384 item 3). Mixes a clean mutation with a
/// broken one and asserts the aggregate report.
#[tokio::test]
async fn validate_mutation_contract_walks_schema_and_surfaces_violations() {
    let Some(catalog) = setup().await else { return };

    let schema = CompiledSchema {
        mutations: vec![
            // fn_create_user(p_input jsonb) RETURNS TABLE(succeeded, state_changed, entity) —
            // clean.
            flat_mutation("createUser", &qualified("fn_create_user")),
            // fn_bad_response(input jsonb) RETURNS TABLE(status, message) — missing required cols.
            flat_mutation("brokenUser", &qualified("fn_bad_response")),
        ],
        ..Default::default()
    };

    let report = validate_mutation_contract(&schema, &catalog).await.unwrap();
    assert_eq!(report.checked, 2, "both mutations are DB-backed");
    assert_eq!(report.skipped, 0);

    // Only the broken mutation appears in the report; the clean one yields nothing.
    assert_eq!(report.mutations.len(), 1, "got {:?}", report.mutations);
    let broken = &report.mutations[0];
    assert_eq!(broken.mutation, "brokenUser");
    assert!(
        broken.violations.contains(&ContractViolation::MissingRequiredColumn {
            column: "succeeded",
        }) && broken.violations.contains(&ContractViolation::MissingRequiredColumn {
            column: "state_changed",
        }),
        "got {:?}",
        broken.violations
    );
    assert!(report.error_count() >= 2);

    teardown().await;
}

/// A single-JSONB Insert (`input_style = jsonb`) with a known multi-field input
/// type, backed by `fn_create_order(p_input jsonb)`. Before #484 this false-failed
/// with `ArityMismatch{expected: N}` because the gate flattened the input fields.
#[tokio::test]
async fn single_jsonb_insert_has_no_false_arity_mismatch() {
    let Some(catalog) = setup().await else { return };

    let mut create_order = MutationDefinition::new("createOrder", "CreateOrderResult");
    create_order.sql_source = Some(qualified("fn_create_order"));
    create_order.operation = MutationOperation::Insert {
        table: "tb_order".to_string(),
    };
    create_order.input_style = InputStyle::Jsonb;
    create_order.arguments = vec![ArgumentDefinition::new(
        "input",
        FieldType::Input("CreateOrderInput".to_string()),
    )];

    let input_fields =
        (0..6).map(|i| InputFieldDefinition::new(format!("f{i}"), "String")).collect();
    let schema = CompiledSchema {
        mutations: vec![create_order],
        input_types: vec![InputObjectDefinition::new("CreateOrderInput").with_fields(input_fields)],
        ..Default::default()
    };

    let report = validate_mutation_contract(&schema, &catalog).await.unwrap();
    assert_eq!(report.checked, 1, "the single-jsonb mutation is DB-backed");
    assert_eq!(
        report.mutations.len(),
        0,
        "single-jsonb insert must report no violations, got {:?}",
        report.mutations
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

/// #1397: `result` is required only by a mutation that declares success fields. Every row
/// shape the contract accepted before stays accepted for a mutation without them; a
/// mutation with them is refused (an error, failing `compile --database`) when its row has
/// no `result`, or one that is not `jsonb`. Driven through `validate_mutation_contract`, so
/// the requirement is derived from the declared fields as `compile --database` derives it.
#[tokio::test]
async fn result_is_required_only_by_a_mutation_with_success_fields() {
    let Some(catalog) = setup().await else { return };

    // (function, declares success fields, the error its row earns, if any)
    let cases: [(&str, bool, Option<ContractViolation>); 8] = [
        ("fn_two", false, None),
        ("fn_thirteen", false, None),
        ("fn_fourteen", false, None),
        ("fn_text_result", false, None),
        ("fn_two", true, Some(ContractViolation::MissingResultColumn)),
        ("fn_thirteen", true, Some(ContractViolation::MissingResultColumn)),
        ("fn_fourteen", true, None),
        (
            "fn_text_result",
            true,
            Some(ContractViolation::ResultColumnWrongType {
                actual: "text".to_string(),
            }),
        ),
    ];
    for (function, declares, earns) in cases {
        let mut mutation = flat_mutation("placeOrder", &qualified(function));
        if declares {
            mutation.success_fields = vec![FieldDefinition::new("recovered_items", FieldType::Int)];
        }
        let schema = CompiledSchema {
            mutations: vec![mutation],
            ..Default::default()
        };
        let report = validate_mutation_contract(&schema, &catalog).await.unwrap();
        let errors: Vec<ContractViolation> = report
            .mutations
            .iter()
            .flat_map(|m| m.violations.iter())
            .filter(|v| v.severity() == Severity::Error)
            .cloned()
            .collect();
        assert_eq!(
            errors,
            earns.into_iter().collect::<Vec<_>>(),
            "{function}, success fields declared: {declares}"
        );
    }

    teardown().await;
}

/// The refusal says what to do: declare `result jsonb` in the function's own row and build
/// it with the 14-column helpers, never `ALTER TYPE` a shared type.
#[test]
fn the_missing_result_message_names_the_fourteen_column_helpers() {
    let message = ContractViolation::MissingResultColumn.to_string();
    assert!(
        message.contains("result jsonb") && message.contains("fraiseql.mutation_ok_result"),
        "{message}"
    );
    assert!(!message.contains("ALTER TYPE"), "{message}");
}
