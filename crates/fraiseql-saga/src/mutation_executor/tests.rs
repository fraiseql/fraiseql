//! The saga's local arm dispatches through the engine, and faces its gates.
//!
//! These tests exist because the arm they cover used to face none of them: it
//! built `INSERT`/`UPDATE`/`DELETE` SQL from entity metadata and ran it with
//! `execute_raw_query`, taking no `SecurityContext` at all (#1354). The
//! assertions below are about that difference, so each one is written to fail if
//! the dispatch ever leaves the chokepoint again.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    runtime::Executor,
    schema::{CompiledSchema, RunAs},
};
use fraiseql_federation::types::FederationMetadata;
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestMutationBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use serde_json::json;

use super::FederationMutationExecutor;

/// A schema with one composite-returning mutation, optionally role-guarded.
fn schema(requires_role: Option<&str>) -> CompiledSchema {
    let mut create_order = TestMutationBuilder::new("createOrder", "Order")
        .with_sql_source("fn_create_order")
        .build();
    create_order.requires_role = requires_role.map(str::to_string);

    TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("Order", "v_order")
                .with_field(
                    TestFieldBuilder::new("id", fraiseql_core::schema::FieldType::String).build(),
                )
                .with_field(
                    TestFieldBuilder::new("amount", fraiseql_core::schema::FieldType::Int).build(),
                )
                .build(),
        )
        .with_mutation(create_order)
        .build()
}

/// The `mutation_response` envelope a successful mutation function returns.
fn success_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("ok"));
    row.insert("entity".to_string(), json!({"id": "order-1", "amount": 99}));
    row.insert("entity_type".to_string(), json!("Order"));
    row
}

fn saga_over(
    schema: CompiledSchema,
    roles: &[&str],
) -> (FederationMutationExecutor<FailingAdapter>, Arc<FailingAdapter>) {
    let adapter = Arc::new(
        FailingAdapter::new().with_function_response("fn_create_order", vec![success_row()]),
    );
    let engine = Arc::new(Executor::new(schema, Arc::clone(&adapter)));
    let run_as = RunAs {
        roles:  roles.iter().map(|r| (*r).to_string()).collect(),
        scopes: vec![],
        tenant: None,
    };
    (
        FederationMutationExecutor::new(engine, FederationMetadata::default(), "test-saga", run_as),
        adapter,
    )
}

/// The gate the old path could not run, because it never held a principal.
///
/// The refusal deliberately reads "not found in schema" rather than "forbidden":
/// the chokepoint answers `requires_role` that way so a caller cannot enumerate
/// mutations by probing them (`runners/mutation/mod.rs` step 1b). So this test
/// cannot assert on the message — what makes it a *role* refusal rather than a
/// genuine unknown name is its twin below: same schema, same mutation name, only
/// the authority differs, and that one succeeds.
#[tokio::test]
async fn a_role_guarded_mutation_is_refused_when_the_saga_authority_lacks_the_role() {
    let (saga, adapter) = saga_over(schema(Some("saga_writer")), &[]);

    let err = saga
        .execute_local_mutation("createOrder", &json!({"amount": 99}), "step-1")
        .await
        .expect_err("a saga with no roles must not pass a requires_role mutation");

    let msg = err.to_string();
    assert!(
        !msg.to_lowercase().contains("role") && !msg.to_lowercase().contains("forbidden"),
        "the refusal must not disclose that a role gate exists, got: {msg}"
    );
    assert!(
        adapter.recorded_queries().is_empty(),
        "a refused mutation must not reach the database at all, saw: {:?}",
        adapter.recorded_queries()
    );
}

/// The twin. A gate that refuses everything is not a gate — this is the case
/// that must stay green, so the test above is about the *role* and not about
/// the saga being unable to write at all.
#[tokio::test]
async fn the_same_mutation_succeeds_when_the_authority_names_the_role() {
    let (saga, _) = saga_over(schema(Some("saga_writer")), &["saga_writer"]);

    saga.execute_local_mutation("createOrder", &json!({"amount": 99}), "step-1")
        .await
        .expect("an authority holding the required role must be allowed through");
}

/// What the dispatch *is*. The old path issued `INSERT INTO "order" (…)` through
/// `execute_raw_query`; the engine calls the mutation's compiled `sql_source`.
/// Asserting on the adapter's own log discriminates between the two — a
/// regression to string SQL would fail here even if the write still succeeded.
#[tokio::test]
async fn the_write_is_the_compiled_function_call_not_a_built_statement() {
    let (saga, adapter) = saga_over(schema(None), &[]);

    saga.execute_local_mutation("createOrder", &json!({"amount": 99}), "step-1")
        .await
        .expect("an unguarded mutation should execute");

    let seen = adapter.recorded_queries();
    assert!(
        seen.iter().any(|q| q == "fn_create_order"),
        "the engine should have called the mutation's compiled sql_source, saw: {seen:?}"
    );
    assert!(
        !seen.iter().any(|q| {
            let q = q.to_uppercase();
            q.contains("INSERT INTO") || q.contains("UPDATE ") || q.contains("DELETE FROM")
        }),
        "no statement may be built and dispatched raw, saw: {seen:?}"
    );
}

/// An operation name the compiled schema does not know fails loud. The old path
/// resolved a name by its leading verb, so `shipOrder` — a real mutation with no
/// recognised prefix — was refused, while a *typo* beginning with `update`
/// silently issued an `UPDATE` against the entity table.
#[tokio::test]
async fn an_unknown_mutation_name_is_refused_rather_than_guessed() {
    let (saga, adapter) = saga_over(schema(None), &[]);

    saga.execute_local_mutation("updateOrdr", &json!({"amount": 99}), "step-1")
        .await
        .expect_err("a name the compiled schema does not define must not execute");

    assert!(
        adapter.recorded_queries().is_empty(),
        "nothing should reach the database, saw: {:?}",
        adapter.recorded_queries()
    );
}

/// The remote arm is unchanged and still refuses to fabricate success (#785).
#[tokio::test]
async fn an_extended_mutation_still_fails_loud() {
    let (saga, _) = saga_over(schema(None), &[]);

    let err = saga
        .execute_extended_mutation("Order", "createOrder", &json!({}))
        .await
        .expect_err("extended mutations are not implemented on this executor");
    assert!(
        err.to_string().contains("not \nimplemented")
            || err.to_string().contains("not implemented")
    );
}
