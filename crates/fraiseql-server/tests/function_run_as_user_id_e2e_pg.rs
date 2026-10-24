//! #1700: a function's `run_as` identity answers a UUID `jwt:sub` inject.
//!
//! A function writes back through the `fraiseql_query` bridge under its `run_as` identity.
//! A mutation that injects the caller's id (`p_user_id` from `jwt:sub`) into a `UUID`
//! parameter received `system_job:<function>`, which PostgreSQL refuses to cast, so no
//! function could call it. `run_as.user_id` is what the identity presents to `jwt:sub` and
//! `jwt:user_id`; its provenance stays `system_job:<function>`.
//!
//! The function definition is read from JSON, as the server's loader reads it, so a key the
//! definition does not carry is lost here exactly as it is in production.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1700` schema → run `--test-threads=1`.
#![cfg(feature = "functions-runtime")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_functions::{FunctionDefinition, host::live::QueryExecutor as _};
use fraiseql_server::{
    query_bridge::{ExecutorSource, RunAsQueryExecutor},
    routes::graphql::AppState,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1700";
const SERVICE_PRINCIPAL: &str = "6f1c2a9e-3b4d-4e5f-8a7b-9c0d1e2f3a4b";
const WRITE: &str = r#"mutation { approveProbe(label: "BRIDGED") { id } }"#;

fn schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Probe", "sql_source": format!("{SCHEMA}.v_probe"), "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }],
        "queries": [{
            "name": "probes", "return_type": "Probe", "returns_list": true, "nullable": false,
            "sql_source": format!("{SCHEMA}.v_probe"), "arguments": []
        }],
        "mutations": [{
            "name": "approveProbe", "return_type": "Probe", "operation": "insert",
            "sql_source": format!("{SCHEMA}.fn_approve_probe"),
            "arguments": [{ "name": "label", "type": "String", "nullable": false }],
            "inject_params": { "p_user_id": "jwt:sub" }
        }]
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

/// The function as the server's loader reads it from the compiled artifact.
fn function(run_as: &Value) -> FunctionDefinition {
    serde_json::from_value(json!({
        "name": "approve_small_probes",
        "trigger": "after:mutation:Probe:insert",
        "runtime": "Deno",
        "run_as": run_as,
    }))
    .expect("the function definition loads")
}

/// The 13 columns of `mutation_response`, the row every write function returns.
const RESPONSE_ROW: &str = "succeeded boolean, state_changed boolean, error_class text, \
     status_detail text, http_status smallint, message text, entity_id uuid, entity_type \
     text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata \
     jsonb";

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

async fn setup() -> Option<(PostgresAdapter, AppState)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    for stmt in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_probe (id uuid PRIMARY KEY, label text NOT NULL, \
             created_by uuid NOT NULL)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_probe AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_probe"
        ),
        // Every write function takes its actor as a UUID, as an application's audit
        // columns do.
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_approve_probe(p_label text, p_user_id uuid) RETURNS \
             TABLE({RESPONSE_ROW}) LANGUAGE plpgsql AS $$ DECLARE v uuid := \
             gen_random_uuid(); BEGIN INSERT INTO {SCHEMA}.tb_probe VALUES (v, p_label, \
             p_user_id); RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', \
             v, 'label', p_label), v, 'Probe'); END $$"
        ),
    ] {
        exec(&admin, &stmt).await;
    }
    let executor = Arc::new(Executor::new(schema(), Arc::new(admin.clone())));
    Some((admin, AppState::new(executor)))
}

async fn written_by(admin: &PostgresAdapter) -> Vec<String> {
    let rows: Vec<HashMap<String, Value>> = admin
        .execute_raw_query(&format!("SELECT created_by::text AS by FROM {SCHEMA}.tb_probe"))
        .await
        .unwrap();
    rows.iter().map(|r| r["by"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn a_run_as_user_id_reaches_a_uuid_sub_inject() {
    let Some((admin, state)) = setup().await else {
        eprintln!("skipping #1700: DATABASE_URL not set");
        return;
    };
    let definition = function(&json!({ "user_id": SERVICE_PRINCIPAL }));
    let identity = definition.run_as.unwrap_or_default().identity(&definition.name, "p1700_req");
    let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), identity);

    bridge.execute_query(WRITE, None).await.expect("the write runs");

    assert_eq!(written_by(&admin).await, [SERVICE_PRINCIPAL]);
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    let mutation = schema.find_mutation("approveProbe").expect("the mutation is served");
    assert!(mutation.inject_params.contains_key("p_user_id"), "the inject survives compile");
}
