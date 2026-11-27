//! A `Float` round-trips exactly through a request and a response.
//!
//! Drives the real mount over a real socket, against PostgreSQL. Three doubles sit one
//! step apart at the top of the range: the largest double, and its two neighbours below.
//! A JSON reader that is off by one step anywhere reads one of them as another, so:
//!
//! | Defect | Caught by |
//! |---|---|
//! | a request variable read one step off | `a_float_variable_selects_its_own_row` |
//! | a stored argument re-read one step off | `a_float_literal_selects_its_own_row` |
//! | a stored double unreadable or served one step off | `the_largest_double_is_served_exactly` |
//!
//! Self-skips when no `DATABASE_URL` is set. **Parallelism:** creates and drops its own
//! `p_float_exact` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition},
};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p_float_exact";

/// `(id, double)`: the largest double and the two below it.
const ROWS: [(i64, &str); 3] = [
    (1, "1.7976931348623153e308"),
    (2, "1.7976931348623155e308"),
    (3, "1.7976931348623157e308"),
];

async fn seed(adapter: &PostgresAdapter) {
    let values: Vec<String> = ROWS.iter().map(|(id, v)| format!("({id}, '{v}'::float8)")).collect();
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_reading (id bigint PRIMARY KEY, ratio float8 NOT NULL)"),
        format!("INSERT INTO {SCHEMA}.tb_reading (id, ratio) VALUES {}", values.join(", ")),
        format!(
            "CREATE VIEW {SCHEMA}.v_reading AS SELECT id, \
             jsonb_build_object('id', id, 'ratio', ratio) AS data FROM {SCHEMA}.tb_reading"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut reading = TypeDefinition::new("Reading", format!("{SCHEMA}.v_reading"));
    reading.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("ratio", FieldType::Float),
    ];
    schema.types.push(reading);
    let mut readings = QueryDefinition::new("readings", "Reading")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_reading"));
    readings.auto_params.has_where = true;
    schema.queries.push(readings);
    schema.build_indexes();
    schema
}

fn config() -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

async fn boot() -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    seed(&adapter).await;
    Some(Box::pin(TestServer::start_with_config(config(), schema(), adapter)).await)
}

/// POST `body` as it is, byte for byte: the operand's text must reach the server unchanged.
async fn post(server: &TestServer, body: String) -> (reqwest::StatusCode, String) {
    let resp = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .expect("request");
    let status = resp.status();
    (status, resp.text().await.expect("body"))
}

fn ids(status: reqwest::StatusCode, body: &str) -> Vec<i64> {
    let value: Value = serde_json::from_str(body)
        .unwrap_or_else(|e| panic!("expected a JSON body, got {status}: {body:?} ({e})"));
    value["data"]["readings"]
        .as_array()
        .unwrap_or_else(|| panic!("expected rows, got {status}: {body}"))
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn a_float_variable_selects_its_own_row() {
    let Some(server) = Box::pin(boot()).await else {
        eprintln!("SKIP a_float_variable_selects_its_own_row: DATABASE_URL not set");
        return;
    };
    let query = "query($r: Float) { readings(where: { ratio: { eq: $r } }) { id } }";
    let body = format!(r#"{{"query": {}, "variables": {{"r": {}}}}}"#, json!(query), ROWS[1].1);
    let (status, text) = post(&server, body).await;
    assert_eq!(ids(status, &text), vec![2], "{} names row 2's double: {text}", ROWS[1].1);
}

#[tokio::test]
async fn a_float_literal_selects_its_own_row() {
    let Some(server) = Box::pin(boot()).await else {
        eprintln!("SKIP a_float_literal_selects_its_own_row: DATABASE_URL not set");
        return;
    };
    let query = format!("{{ readings(where: {{ ratio: {{ eq: {} }} }}) {{ id }} }}", ROWS[1].1);
    let (status, text) = post(&server, json!({ "query": query }).to_string()).await;
    assert_eq!(ids(status, &text), vec![2], "{} names row 2's double: {text}", ROWS[1].1);
}

#[tokio::test]
async fn the_largest_double_is_served_exactly() {
    let Some(server) = Box::pin(boot()).await else {
        eprintln!("SKIP the_largest_double_is_served_exactly: DATABASE_URL not set");
        return;
    };
    let query = "{ readings(where: { id: { eq: 3 } }) { id ratio } }";
    let (status, text) = post(&server, json!({ "query": query }).to_string()).await;
    // Read the served number from the body's text, not through a JSON reader that could
    // itself be the one off by a step.
    let served = text
        .split(r#""ratio":"#)
        .nth(1)
        .and_then(|rest| rest.split(['}', ',']).next())
        .unwrap_or_else(|| panic!("expected a served ratio, got {status}: {text}"));
    let served: f64 = served.trim().parse().unwrap_or_else(|e| panic!("{served:?}: {e}"));
    assert_eq!(served.to_bits(), f64::MAX.to_bits(), "served {served:e}: {text}");
}
