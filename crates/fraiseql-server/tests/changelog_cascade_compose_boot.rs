#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! Boot proof for the change-log + cascade composition (#665): a schema that turns on
//! `[observers]`, `[changelog] expose = true`, AND a `cascade = true` mutation together
//! compiles, and a `Server` loads the compiled schema and boots on it.
//!
//! This is the DB-free "loads and boots" half of the Phase 03 e2e: `SchemaConverter::convert`
//! runs the full compile pipeline (the #665 unblock), `Server::new` then loads the combined
//! compiled schema and mounts every subsystem on it, and the always-on `/health` liveness
//! probe confirms the process is serving. It uses a `FailingAdapter` (no database) — the DB
//! round-trip halves (cascade delivery, `entityChangeLogs` pagination) run in the
//! Dagger integration leg against real Postgres via the existing changelog/cascade harnesses.

mod common;

use std::sync::Arc;

use fraiseql_cli::schema::{IntermediateSchema, converter::SchemaConverter};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::common::server_harness::TestServer;

/// SDK-shaped `schema.json` with all three features on at once. Observers must be enabled
/// or the converter rejects an exposed change-log.
const SDK_SCHEMA_CHANGELOG_PLUS_CASCADE: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Post",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "title", "type": "String", "nullable": false}
      ],
      "sql_source": "v_post",
      "is_input": false
    }
  ],
  "queries": [],
  "mutations": [
    {
      "name": "createPost",
      "return_type": "Post",
      "cascade": true,
      "sql_source": "fn_create_post",
      "operation": "CREATE"
    }
  ],
  "subscriptions": [],
  "observers_config": {"enabled": true},
  "changelog_config": {"expose": true}
}
"#;

#[tokio::test]
async fn changelog_cascade_schema_loads_and_boots() {
    // Compile the combined surface (the #665 unblock — this returned Err before the fix).
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_CHANGELOG_PLUS_CASCADE).expect("parse SDK schema.json");
    let schema = SchemaConverter::convert(intermediate)
        .expect("#665: changelog + observers + cascade must compile together");

    // `Server::new` (inside TestServer::start) must load the combined compiled schema and
    // mount every subsystem without panicking — the "loads and boots" proof. A `FailingAdapter`
    // keeps this DB-free; its default `health_check` succeeds, so `/health` returns 200.
    let server = TestServer::start(schema, Arc::new(FailingAdapter::new())).await;

    let resp = reqwest::Client::new()
        .get(format!("{}/health", server.url))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "server must boot on the combined changelog+cascade schema and serve /health"
    );
}
