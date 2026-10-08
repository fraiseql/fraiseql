#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! #1425 — the opt-in check of a failed mutation's `error_detail.errors[]`.
//!
//! Clients translate a failure by `errors[].identifier`. With
//! `mutation_error_shape_check = warn`, a failure that carries no `errors` array, or an
//! entry whose identifier is not a translation key, is logged at `warn` and counted in
//! `mutation_error_shape_violations()`; the response a client gets is the same as with the
//! check off. Each function here returns its failure through the real
//! `app.mutation_response` composite on PostgreSQL.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** one test; creates and drops its own `issue_1425` schema.

mod common;

use std::sync::{Arc, Mutex};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, MutationErrorShapeCheck, RuntimeConfig, mutation_error_shape_violations},
    schema::CompiledSchema,
};
use serde_json::json;

const SCHEMA: &str = "issue_1425";

/// The `app.mutation_response` contract, and one function `fn_fail(p_mode)` whose failure
/// carries the `error_detail` that `p_mode` names.
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
    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_thing AS SELECT NULL::uuid AS id, '{{}}'::jsonb AS data \
             WHERE false"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE FUNCTION {SCHEMA}.fn_fail(p_mode text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v_response app.mutation_response; BEGIN \
             v_response.succeeded := false; \
             v_response.state_changed := false; \
             v_response.error_class := 'validation'; \
             v_response.message := 'The order cannot be placed'; \
             v_response.error_detail := CASE p_mode \
               WHEN 'no_errors' THEN NULL \
               WHEN 'bad_identifier' THEN '{{\"errors\": [{{\"code\": 422, \
                 \"identifier\": \"order line_not_found\", \"message\": \"No line\"}}]}}'::jsonb \
               ELSE '{{\"errors\": [{{\"code\": 422, \
                 \"identifier\": \"order_line_not_found\", \"message\": \"No line\"}}]}}'::jsonb \
             END; \
             RETURN v_response; END; $$"
        ))
        .await
        .unwrap();
}

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [{
            "name": "Thing",
            "sql_source": format!("{SCHEMA}.v_thing"),
            "fields": [{ "name": "id", "field_type": "ID" }]
        }],
        "queries": [],
        "mutations": [{
            "name": "placeOrder",
            "return_type": "Thing",
            "sql_source": format!("{SCHEMA}.fn_fail"),
            "operation": "Custom",
            "arguments": [{ "name": "mode", "arg_type": "String", "nullable": false }]
        }]
    }))
    .expect("schema")
}

/// A `tracing` writer that keeps what it is given.
#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CapturedLog {
    /// The `warn` lines the shape check logged since the last call. Other targets are left
    /// out: the PostgreSQL adapter logs its own warnings for the NULL columns of the row.
    fn take(&self) -> String {
        let text = String::from_utf8(std::mem::take(&mut *self.0.lock().unwrap())).unwrap();
        let mut lines = String::new();
        for line in text.lines().filter(|l| l.contains("fraiseql_core::runtime::mutation_result:"))
        {
            lines.push_str(line);
            lines.push('\n');
        }
        lines
    }
}

/// Run `placeOrder(mode)` once: the response, the `warn` lines it logged, and how much the
/// violation counter moved.
async fn place(
    executor: &Executor,
    log: &CapturedLog,
    mode: &str,
) -> (serde_json::Value, String, u64) {
    log.take();
    let before = mutation_error_shape_violations();
    let response = executor
        .execute(&format!("mutation {{ placeOrder(mode: \"{mode}\") {{ __typename }} }}"), None)
        .await
        .unwrap();
    let counted = mutation_error_shape_violations() - before;
    (response, log.take(), counted)
}

#[tokio::test]
async fn a_malformed_failure_is_warned_and_counted_only_when_the_check_is_on() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;

    let log = CapturedLog::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let checked = Executor::with_config(
        schema(),
        Arc::clone(&adapter),
        RuntimeConfig {
            mutation_error_shape_check: MutationErrorShapeCheck::Warn,
            ..RuntimeConfig::default()
        },
    );
    let unchecked = Executor::with_config(schema(), Arc::clone(&adapter), RuntimeConfig::default());

    for (mode, problem) in [
        ("no_errors", "no `errors` array"),
        ("bad_identifier", "order line_not_found"),
    ] {
        let (warned, logged, counted) = place(&checked, &log, mode).await;
        assert_eq!(counted, 1, "{mode}: one malformed response, counted once");
        assert_eq!(logged.matches(" WARN ").count(), 1, "{mode}: one warning, got {logged:?}");
        assert!(
            logged.contains("placeOrder") && logged.contains(problem),
            "{mode}: the warning names the mutation and the problem: {logged:?}"
        );

        let (plain, logged, counted) = place(&unchecked, &log, mode).await;
        assert_eq!((counted, logged.as_str()), (0, ""), "{mode}: the check is off by default");
        assert_eq!(warned, plain, "{mode}: the check does not change the response");
    }

    let (_, logged, counted) = place(&checked, &log, "well_formed").await;
    assert_eq!((counted, logged.as_str()), (0, ""), "a well-formed failure is not flagged");

    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
}
