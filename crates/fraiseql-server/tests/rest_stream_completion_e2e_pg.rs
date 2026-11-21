//! A streamed REST read serves each row as the same read buffered serves it.
//!
//! A buffered read completes every row against its type: the OpenAPI type each field is
//! published as, and the non-null rule. A value that cannot be completed refuses the read
//! with `500`, naming the field, never the value. A streamed read (NDJSON, CSV, XLSX)
//! served its rows as the database produced them: a missing non-null value, an enum value
//! outside its members, an object under an `integer`, a `DateTime` without an offset.
//!
//! Each row of a stream is now completed as that row of the buffered read is. A refused
//! row ends the stream: the rows before it are served, then the format's terminal line
//! (`{"error":…}` for NDJSON, `# error: …` for CSV) carrying the buffered body's message,
//! and nothing after it. An XLSX workbook is sent only once built, so it answers the
//! buffered read's `500` itself. Each case runs with error sanitization off and on: the
//! terminal text is the buffered body's, sanitized as that body is.
//!
//! Each route reads three rows in `id` order: a clean one, the defect, a clean one.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tb_stream_completion_*` fixtures → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldType, RestConfig, SecurityConfig,
    },
};
use fraiseql_server::{
    config::error_sanitization::ErrorSanitizationConfig, server_config::ServerConfig,
};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const NDJSON: &str = "application/x-ndjson";
const CSV: &str = "text/csv";
const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// A clean row's document.
fn clean(id: i64) -> Value {
    json!({"id": id, "n": id, "need": id, "kind": "ON", "at": "2026-07-01T12:00:00Z"})
}

/// Each route, and the document its second row holds.
fn cases() -> Vec<(&'static str, Value)> {
    let mut missing = clean(2);
    missing.as_object_mut().unwrap().remove("need");
    vec![
        ("clean", clean(2)),
        ("missing", missing),
        ("enum", json!({"id": 2, "n": 2, "need": 2, "kind": "PURPLE", "at": null})),
        ("object", json!({"id": 2, "n": {"x": 1}, "need": 2, "kind": "ON", "at": null})),
        (
            "offsetless",
            json!({"id": 2, "n": 2, "need": 2, "kind": "ON", "at": "2026-07-01T12:00:00"}),
        ),
        ("rewrite", json!({"id": 2, "n": "7", "need": 2, "kind": "ON", "at": null})),
    ]
}

/// The routes whose second row the read refuses, and the field it names.
const REFUSED: [(&str, &str); 4] = [
    ("missing", "need"),
    ("enum", "kind"),
    ("object", "n"),
    ("offsetless", "at"),
];

async fn seed(adapter: &PostgresAdapter) {
    for (case, second) in cases() {
        let table = format!("tb_stream_completion_{case}");
        let view = format!("v_stream_completion_{case}");
        let rows = [clean(1), second, clean(3)]
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                format!("({}, '{}'::jsonb)", i + 1, doc.to_string().replace('\'', "''"))
            })
            .collect::<Vec<_>>()
            .join(", ");
        for stmt in [
            format!("DROP VIEW IF EXISTS {view}"),
            format!("DROP TABLE IF EXISTS {table}"),
            format!("CREATE TABLE {table} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
            format!("INSERT INTO {table} (id, data) VALUES {rows}"),
            format!("CREATE VIEW {view} AS SELECT id, data FROM {table} ORDER BY id"),
        ] {
            let _: Vec<std::collections::HashMap<String, Value>> =
                adapter.execute_raw_query(&stmt).await.expect("fixture setup");
        }
    }
}

fn schema(sanitized: bool) -> CompiledSchema {
    let mut builder = TestSchemaBuilder::new().with_type(
        TestTypeBuilder::new("Note", "v_stream_completion_clean")
            .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
            .with_nullable_field("n", FieldType::Int)
            .with_field(TestFieldBuilder::new("need", FieldType::Int).build())
            .with_nullable_field("kind", FieldType::Enum("Switch".to_string()))
            .with_nullable_field("at", FieldType::DateTime)
            .build(),
    );
    for (case, _) in cases() {
        let mut query = TestQueryBuilder::new(&format!("{case}Notes"), "Note")
            .returns_list(true)
            .with_sql_source(&format!("v_stream_completion_{case}"))
            .rest_stream(true)
            .build();
        query.rest_path = Some(format!("/{case}"));
        builder = builder.with_query(query);
    }
    let mut schema = builder.build();
    schema.enums.push(
        EnumDefinition::new("Switch")
            .with_value(EnumValueDefinition::new("ON"))
            .with_value(EnumValueDefinition::new("OFF")),
    );
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.security = Some(SecurityConfig {
        error_sanitization: Some(ErrorSanitizationConfig {
            enabled: sanitized,
            ..ErrorSanitizationConfig::default()
        }),
        ..SecurityConfig::default()
    });
    schema.build_indexes();
    schema
}

async fn start(sanitized: bool) -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    Some(
        Box::pin(TestServer::start_with_config(config, schema(sanitized), Arc::new(adapter))).await,
    )
}

/// `GET /rest/v1/{case}?sort=id` with `accept`: the status and the body.
async fn get(server: &TestServer, case: &str, accept: &str) -> (u16, bytes::Bytes) {
    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/{case}?sort=id", server.url))
        .header("accept", accept)
        .send()
        .await
        .unwrap_or_else(|e| panic!("{case} {accept}: {e}"));
    let status = response.status().as_u16();
    (status, response.bytes().await.unwrap())
}

fn json_body(case: &str, body: &[u8]) -> Value {
    serde_json::from_slice(body)
        .unwrap_or_else(|e| panic!("{case}: not JSON ({e}): {}", String::from_utf8_lossy(body)))
}

/// The buffered read: its `500` body, and the message it carries.
async fn buffered_refusal(server: &TestServer, case: &str) -> (Value, String) {
    let (status, body) = get(server, case, "application/json").await;
    let body = json_body(case, &body);
    assert_eq!(status, 500, "{case}: the buffered read refuses the row: {body}");
    let message = body["error"]["message"].as_str().unwrap().to_string();
    (body, message)
}

async fn assert_streams_end_at_the_refused_row(sanitized: bool) {
    let Some(server) = start(sanitized).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for (case, field) in REFUSED {
        let (buffered, message) = buffered_refusal(&server, case).await;
        assert_eq!(message.contains(&format!("Note.{field}")), !sanitized, "{case}: {message}");
        assert!(!message.contains("PURPLE") && !message.contains("12:00:00\""), "{message}");

        let (status, body) = get(&server, case, NDJSON).await;
        assert_eq!(status, 200, "{case}: NDJSON has begun before the refused row");
        let lines: Vec<Value> = String::from_utf8(body.to_vec())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            lines,
            [clean(1), json!({"error": message})],
            "{case} NDJSON: row 1, then the buffered message, and no row 3"
        );

        let (status, body) = get(&server, case, CSV).await;
        assert_eq!(status, 200, "{case}: CSV has begun before the refused row");
        let text = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
        assert_eq!(lines.len(), 3, "{case} CSV: header, row 1, the error line: {text}");
        assert!(lines[1].starts_with('1'), "{case} CSV: row 1 first: {text}");
        assert_eq!(lines[2], format!("# error: {message}"), "{case} CSV: {text}");

        let (status, body) = get(&server, case, XLSX).await;
        assert_eq!(status, 500, "{case}: a workbook is the buffered read's refusal");
        assert_eq!(json_body(case, &body), buffered, "{case}: XLSX answers the buffered body");
    }
}

#[tokio::test]
async fn a_stream_ends_at_a_row_the_buffered_read_refuses() {
    assert_streams_end_at_the_refused_row(false).await;
}

#[tokio::test]
async fn a_streams_terminal_line_is_sanitized_as_the_buffered_body_is() {
    assert_streams_end_at_the_refused_row(true).await;
}

/// The control, and the rewrite: rows the buffered read serves are streamed exactly as
/// it serves them, `"7"` at an `integer` as `7`, every row, no terminal line.
#[tokio::test]
async fn a_stream_serves_the_rows_the_buffered_read_serves() {
    let Some(server) = start(false).await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    for case in ["clean", "rewrite"] {
        let (status, body) = get(&server, case, "application/json").await;
        assert_eq!(status, 200, "{case}: {}", String::from_utf8_lossy(&body));
        let buffered = json_body(case, &body)["data"].clone();
        let (status, body) = get(&server, case, NDJSON).await;
        assert_eq!(status, 200, "{case}");
        let streamed: Vec<Value> = String::from_utf8(body.to_vec())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(Value::Array(streamed), buffered, "{case}");
        let (status, body) = get(&server, case, CSV).await;
        assert_eq!(status, 200, "{case}");
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(text.lines().count(), 4, "{case} CSV: header and three rows: {text}");
        assert!(!text.contains("# error"), "{case}: {text}");
        let (status, _) = get(&server, case, XLSX).await;
        assert_eq!(status, 200, "{case}: XLSX");
    }
    let (_, body) = get(&server, "rewrite", NDJSON).await;
    let second: Value =
        serde_json::from_str(String::from_utf8(body.to_vec()).unwrap().lines().nth(1).unwrap())
            .unwrap();
    assert_eq!(second["n"], json!(7), "`\"7\"` at an `integer` is served as `7`");
}
