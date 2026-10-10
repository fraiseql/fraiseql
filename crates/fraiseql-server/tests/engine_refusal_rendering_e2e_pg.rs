//! #1543: a ceiling the engine enforces is reported as that ceiling on every transport that
//! renders the engine's errors, not as a server fault.
//!
//! `[validation] max_response_bytes` and `[security.cost_budget] per_request_max` are
//! enforced inside the executor (#379, #1351), so they bind on every transport. The shared
//! conversion `GraphQLError::from_fraiseql_error` (used by `/graphql`, SSE, MCP, async
//! operations and the multi-root mutation renderer) had no arm for either refusal, and
//! rendered both as `INTERNAL_SERVER_ERROR` / `500` — which the sanitizer then blanked to
//! "An internal error occurred". A client refused for asking too much was told the server
//! broke, and every retry policy read that as retryable.
//!
//! `/graphql` scores cost in its own stage before the executor runs (`tenant_dispatch_error`
//! already renders that refusal correctly), so the engine's cost refusal is driven over MCP,
//! which does not. gRPC has its own mapping; its case lives in `locale_grpc_e2e_pg.rs`, the
//! gRPC suite.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1543_refusal` schema → run
//! `--test-threads=1`.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_server::{
    Server,
    config::error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer},
    mcp::{McpConfig, handler::FraiseQLMcpService},
    routes::graphql::AppState,
    server_config::ServerConfig,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use tempfile::TempDir;

const SCHEMA: &str = "p1543_refusal";

/// One of the two ceilings, small enough that the fixture's read crosses it.
#[derive(Clone, Copy)]
enum Ceiling {
    /// Two rows of a 2 000-character payload weigh far more than this.
    Bytes,
    /// Any read of `items` scores above one.
    Cost,
}

fn fraiseql_toml(ceiling: Ceiling) -> String {
    let bound = match ceiling {
        Ceiling::Bytes => "[validation]\nmax_response_bytes = 1000\n",
        Ceiling::Cost => "[security.cost_budget]\nper_request_max = 1\n",
    };
    format!(
        r#"
[schema]
name = "refusal-1543"
version = "1.0.0"
database_target = "postgresql"

{bound}
[types.Item]
sql_source = "{SCHEMA}.v_item"
fields.id = {{ type = "Int" }}
fields.payload = {{ type = "String" }}

[queries.items]
return_type = "Item"
return_array = true
sql_source = "{SCHEMA}.v_item"
"#
    )
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> =
        adapter.execute_raw_query(sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn seed(adapter: &PostgresAdapter) {
    exec(adapter, &format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE")).await;
    exec(adapter, &format!("CREATE SCHEMA {SCHEMA}")).await;
    exec(
        adapter,
        &format!("CREATE TABLE {SCHEMA}.tb_item (id bigint PRIMARY KEY, payload text NOT NULL)"),
    )
    .await;
    exec(
        adapter,
        &format!(
            "INSERT INTO {SCHEMA}.tb_item VALUES (1, repeat('x', 2000)), (2, repeat('y', 2000))"
        ),
    )
    .await;
    exec(
        adapter,
        &format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'payload', \
             payload) AS data FROM {SCHEMA}.tb_item"
        ),
    )
    .await;
}

async fn compile(ceiling: Ceiling) -> CompiledSchema {
    let dir = TempDir::new().unwrap();
    let toml_path = dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, fraiseql_toml(ceiling)).unwrap();
    let (compiled, _) = compile_to_schema(CompileOptions {
        skip_hash: true,
        ..CompileOptions::new(toml_path.to_str().unwrap())
    })
    .await
    .expect("the document must compile");
    let mut schema = CompiledSchema::from_json(&compiled.schema.to_json().unwrap(), false).unwrap();
    schema.build_indexes();
    schema
}

/// `/graphql` on the real server: the response's status and its first error.
async fn graphql(ceiling: Ceiling) -> Option<(u16, Value)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let config = ServerConfig {
        cors_enabled: false,
        database_url: url,
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(config, compile(ceiling).await, adapter, None))
        .await
        .expect("Server::new");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .json(&json!({ "query": "query { items { id payload } }" }))
        .send()
        .await
        .expect("graphql request");
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("JSON body");
    let _ = stop.send(());
    Some((status, body))
}

/// The `items` MCP tool, rendered under an **enabled** sanitizer — the shape a production
/// server runs, and the one under which a mis-coded refusal loses its message.
async fn mcp(ceiling: Ceiling) -> Option<(bool, String)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let schema = compile(ceiling).await;
    let runtime = fraiseql_core::runtime::RuntimeConfig::from_compiled_schema(&schema).unwrap();
    let state = AppState::new(Arc::new(Executor::with_config(schema, adapter, runtime)))
        .with_error_sanitizer(Arc::new(ErrorSanitizer::new(ErrorSanitizationConfig {
            enabled: true,
            ..ErrorSanitizationConfig::default()
        })));
    let service = FraiseQLMcpService::new(
        state,
        McpConfig {
            enabled: true,
            require_auth: false,
            ..McpConfig::default()
        },
    );
    let result = service
        .call_tool_authenticated(
            "items",
            None,
            None,
            "mcp-1543".to_string(),
            &http::HeaderMap::new(),
        )
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    Some((result.is_error == Some(true), text))
}

#[tokio::test]
async fn graphql_reports_an_oversized_response_as_payload_too_large() {
    let Some((status, body)) = graphql(Ceiling::Bytes).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert!(body.pointer("/data/items/0").is_none(), "the read must be refused: {body}");
    assert_eq!(status, 413, "a permanent, caller-caused refusal, not a server fault: {body}");
    assert_eq!(body.pointer("/errors/0/code"), Some(&json!("PAYLOAD_TOO_LARGE")), "{body}");
    let message = body.pointer("/errors/0/message").and_then(Value::as_str).unwrap_or_default();
    assert!(message.contains("max_response_bytes"), "the message names the ceiling: {body}");
}

#[tokio::test]
async fn mcp_reports_the_engines_cost_refusal_as_the_cost_ceiling() {
    let Some((is_error, text)) = mcp(Ceiling::Cost).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert!(is_error, "the tool call must be refused: {text}");
    assert!(
        text.contains("Operation cost exceeded"),
        "the engine's cost refusal must survive the sanitizer, not read as an internal error: \
         {text}"
    );
}

#[tokio::test]
async fn mcp_reports_an_oversized_response_as_the_response_ceiling() {
    let Some((is_error, text)) = mcp(Ceiling::Bytes).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert!(is_error, "the tool call must be refused: {text}");
    assert!(
        text.contains("max_response_bytes"),
        "the size refusal must survive the sanitizer, not read as an internal error: {text}"
    );
}

/// Both documents are what the rigs serve, so each must declare its ceiling where there is no
/// database.
#[tokio::test]
async fn the_documents_load_without_a_database() {
    let bytes = compile(Ceiling::Bytes).await;
    assert_eq!(bytes.validation_config.as_ref().and_then(|v| v.max_response_bytes), Some(1000));
    let cost = compile(Ceiling::Cost).await;
    assert_eq!(
        cost.security
            .as_ref()
            .and_then(|s| s.cost_budget.as_ref())
            .and_then(|c| c.per_request_max),
        Some(1)
    );
}
