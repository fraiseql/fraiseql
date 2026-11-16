//! #1989: a `run_as` identity satisfies session variables mapped from enriched identity.
//!
//! A schema that maps session variables from the enriched identity
//! (`source = "enrichment"`) scopes its rows in the database. A background job (a function,
//! a scheduled source) writes and reads back through the `fraiseql_query` bridge under its
//! `run_as` identity, which no enrichment query resolves: it has no subject to resolve.
//! Every bridged query was refused because the enriched field was absent. `run_as` now
//! declares the fields it presents (`static_enriched`), as a service account does.
//!
//! The definitions are read from JSON, as the server's loader reads them, so a key the
//! definition does not carry is lost here exactly as it is in production.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1989` schema → run `--test-threads=1`.
#![cfg(feature = "functions-runtime")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, SourceDefinition},
    security::SecurityContext,
};
use fraiseql_functions::{FunctionDefinition, host::live::QueryExecutor as _};
use fraiseql_server::{
    query_bridge::{ExecutorSource, RunAsQueryExecutor},
    routes::graphql::AppState,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1989";
const WRITE: &str = r#"mutation { recordProbe(label: "BRIDGED") { id } }"#;
const READ: &str = "query { probes { label } }";

/// The enriched fields a `run_as` declares: one value is the empty string, which is a
/// value, not an absence.
fn static_enriched() -> Value {
    json!({ "agent_role": "manager", "agent_id": "" })
}

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
            "name": "recordProbe", "return_type": "Probe", "operation": "insert",
            "sql_source": format!("{SCHEMA}.fn_record_probe"),
            "arguments": [{ "name": "label", "type": "String", "nullable": false }]
        }],
        "session_variables": { "variables": [
            { "name": "app.agent_role", "source": "enrichment", "field": "agent_role" },
            { "name": "app.agent_id",   "source": "enrichment", "field": "agent_id" }
        ]}
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

/// A function as the server's loader reads it from the compiled artifact.
fn function(run_as: &Value) -> FunctionDefinition {
    serde_json::from_value(json!({
        "name": "expire_sessions",
        "trigger": "cron:*/5 * * * *",
        "runtime": "Deno",
        "run_as": run_as,
    }))
    .expect("the function definition loads")
}

/// A scheduled source as the compiled schema carries it.
fn source(run_as: &Value) -> SourceDefinition {
    serde_json::from_value(json!({
        "name": "poll_probes",
        "schedule": "*/5 * * * *",
        "function": "pollProbes",
        "run_as": run_as,
    }))
    .expect("the source definition loads")
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
             owner_role text NOT NULL, written_by text NOT NULL)"
        ),
        // Row scoping in the database: a session sees the rows of its own role only.
        format!(
            "CREATE VIEW {SCHEMA}.v_probe AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_probe WHERE owner_role = \
             current_setting('app.agent_role')"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_probe VALUES (gen_random_uuid(), 'MANAGERS', 'manager', \
             'seed'), (gen_random_uuid(), 'CLERKS', 'clerk', 'seed')"
        ),
        // The write records the session the database saw, as an audit column would.
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_record_probe(p_label text) RETURNS \
             TABLE({RESPONSE_ROW}) LANGUAGE plpgsql AS $$ DECLARE v uuid := \
             gen_random_uuid(); BEGIN INSERT INTO {SCHEMA}.tb_probe VALUES (v, p_label, \
             current_setting('app.agent_role'), current_setting('app.agent_id')); RETURN QUERY \
             SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v, 'label', p_label), \
             v, 'Probe'); END $$"
        ),
    ] {
        exec(&admin, &stmt).await;
    }
    let executor = Arc::new(Executor::new(schema(), Arc::new(admin.clone())));
    Some((admin, AppState::new(executor)))
}

/// `(owner_role, written_by)` of every bridged write.
async fn bridged_writes(admin: &PostgresAdapter) -> Vec<(String, String)> {
    let rows: Vec<HashMap<String, Value>> = admin
        .execute_raw_query(&format!(
            "SELECT owner_role, written_by FROM {SCHEMA}.tb_probe WHERE label = 'BRIDGED'"
        ))
        .await
        .unwrap();
    rows.iter()
        .map(|r| {
            (
                r["owner_role"].as_str().unwrap().to_string(),
                r["written_by"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Writes, then reads back, through `bridge`, and asserts both ran under the declared
/// fields.
async fn assert_runs_under_the_declared_fields(
    admin: &PostgresAdapter,
    bridge: &RunAsQueryExecutor,
) {
    bridge.execute_query(WRITE, None).await.expect("the bridged write runs");
    assert_eq!(
        bridged_writes(admin).await,
        [("manager".to_string(), String::new())],
        "the write saw the declared fields, the empty string included"
    );

    let read = bridge.execute_query(READ, None).await.expect("the bridged read runs");
    let mut labels: Vec<&str> = read["data"]["probes"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of probes: {read}"))
        .iter()
        .map(|p| p["label"].as_str().unwrap())
        .collect();
    labels.sort_unstable();
    assert_eq!(labels, ["BRIDGED", "MANAGERS"], "the view scoped the read to the declared role");
}

fn function_identity(run_as: &Value) -> SecurityContext {
    let definition = function(run_as);
    definition.run_as.unwrap_or_default().identity(&definition.name, "p1989_req")
}

#[tokio::test]
async fn a_function_run_as_presents_its_static_enriched_fields() {
    let Some((admin, state)) = setup().await else {
        eprintln!("skipping #1989: DATABASE_URL not set");
        return;
    };
    let identity = function_identity(
        &json!({ "roles": ["maintenance"], "static_enriched": static_enriched() }),
    );
    let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), identity);

    assert_runs_under_the_declared_fields(&admin, &bridge).await;
}

#[tokio::test]
async fn a_source_run_as_presents_its_static_enriched_fields() {
    let Some((admin, state)) = setup().await else {
        eprintln!("skipping #1989: DATABASE_URL not set");
        return;
    };
    let source =
        source(&json!({ "roles": ["ingest_writer"], "static_enriched": static_enriched() }));
    let bridge =
        RunAsQueryExecutor::for_source(ExecutorSource::of(&state), source.identity("p1989_req"));

    assert_runs_under_the_declared_fields(&admin, &bridge).await;
}

/// The fail-closed guarantee stays: a `run_as` that declares no enriched field is refused
/// before the database is reached, and writes nothing.
#[tokio::test]
async fn a_run_as_without_static_enriched_is_still_refused() {
    let Some((admin, state)) = setup().await else {
        eprintln!("skipping #1989: DATABASE_URL not set");
        return;
    };
    let identity = function_identity(&json!({ "roles": ["maintenance"] }));
    let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), identity);

    let error = bridge.execute_query(WRITE, None).await.expect_err("no enriched field declared");
    assert!(
        error.to_string().contains("maps to enriched field 'agent_role'"),
        "refused for the absent field: {error}"
    );
    assert!(bridged_writes(&admin).await.is_empty(), "nothing was written");
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    assert_eq!(schema.session_variables.variables.len(), 2, "both mappings survive compile");
    assert!(schema.find_mutation("recordProbe").is_some(), "the mutation is served");
}
