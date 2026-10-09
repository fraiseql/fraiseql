//! #1522 on the read paths beyond a plain query: a stored value missing from a field the
//! schema publishes non-null is a field error that nulls the nearest nullable position
//! (GraphQL § 6.4.4), and never a `null` under `200` with nothing said.
//!
//! - GraphQL `_entities` (`[_Entity]!`): the entity that cannot be completed is `null`, with an
//!   error at `["_entities", i, field]`.
//! - GraphQL mutation payload (`createNote: Note`): the payload is `null`, with an error.
//! - REST: a representation has no partial form, so the read is refused (`500`), naming the field
//!   and where it was.
//! - MCP: the tool's result is the GraphQL response, `errors` included, and is an error.
//!
//! gRPC is driven in `locale_grpc_e2e_pg`, whose rig builds the service from descriptors.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `nn_transport` schema → run `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp", feature = "federation"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, McpConfig},
};
use fraiseql_server::{
    Server, mcp::handler::FraiseQLMcpService, routes::graphql::AppState,
    server_config::ServerConfig,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "nn_transport";

/// Two notes, the second with no `title`; a function that returns a note with no `title`.
async fn provision(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE VIEW {SCHEMA}.v_note AS SELECT * FROM (VALUES \
             ('n1', jsonb_build_object('id', 'n1', 'title', 'First')), \
             ('n2', jsonb_build_object('id', 'n2'))) AS t(id, data)"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_note(p_title text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid := gen_random_uuid(); BEGIN \
             v.succeeded := true; v.state_changed := true; v.entity_type := 'Note'; \
             v.entity_id := n; v.entity := jsonb_build_object('id', n::text); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// `Note { id: ID!, title: String! }`, a list query, a mutation, REST, and `Note` a
/// federation entity.
fn schema() -> CompiledSchema {
    CompiledSchema::from_json(
        &json!({
            "fraiseql_version": env!("CARGO_PKG_VERSION"),
            "types": [{
                "name": "Note",
                "sql_source": format!("{SCHEMA}.v_note"),
                "fields": [
                    {"name": "id", "field_type": "ID", "nullable": false},
                    {"name": "title", "field_type": "String", "nullable": false}
                ]
            }],
            "queries": [
                {"name": "notes", "return_type": "Note", "returns_list": true, "nullable": false,
                 "sql_source": format!("{SCHEMA}.v_note"), "jsonb_column": "data", "arguments": []},
                {"name": "note", "return_type": "Note", "returns_list": false, "nullable": true,
                 "sql_source": format!("{SCHEMA}.v_note"), "jsonb_column": "data",
                 "arguments": [{"name": "id", "arg_type": "ID", "nullable": false}]}
            ],
            "mutations": [{
                "name": "createNote", "return_type": "Note",
                "sql_source": format!("{SCHEMA}.fn_create_note"),
                "operation": {"Insert": {"table": "tb_note"}},
                "arguments": [{"name": "title", "arg_type": "String", "nullable": true}]
            }],
            "subscriptions": [],
            "rest_config": {"enabled": true},
            "federation": {
                "enabled": true, "version": "v2", "service_name": "notes",
                "entities": [{"name": "Note", "key_fields": ["id"]}]
            }
        })
        .to_string(),
        false,
    )
    .unwrap()
}

/// A running server; dropping it shuts the server down.
struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve(url: &str) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    provision(&adapter).await;
    let config = ServerConfig {
        database_url: url.to_string(),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(), adapter, None)).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    }
}

async fn graphql(server: &Running, query: &str, variables: &Value) -> Value {
    let response = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}"))
}

/// The paths of `response`'s errors.
fn paths(response: &Value) -> Vec<Value> {
    response["errors"]
        .as_array()
        .unwrap_or_else(|| panic!("an errors array: {response}"))
        .iter()
        .map(|e| e["path"].clone())
        .collect()
}

#[tokio::test]
async fn an_entity_that_cannot_be_completed_is_null_with_an_error() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1522 transports: DATABASE_URL not set");
        return;
    };
    let server = serve(&url).await;
    let body = graphql(
        &server,
        "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on Note { id title } } }",
        &json!({ "representations": [{ "__typename": "Note", "id": "n1" }, { "__typename": "Note", "id": "n2" }] }),
    )
    .await;
    assert_eq!(body["data"]["_entities"][0]["title"], json!("First"), "{body}");
    assert_eq!(body["data"]["_entities"][1], Value::Null, "{body}");
    assert_eq!(paths(&body), vec![json!(["_entities", 1, "title"])], "{body}");
}

#[tokio::test]
async fn a_mutation_payload_that_cannot_be_completed_is_null_with_an_error() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let body =
        graphql(&server, r#"mutation { createNote(title: "x") { id title } }"#, &json!({})).await;
    assert_eq!(body["data"]["createNote"], Value::Null, "{body}");
    assert_eq!(paths(&body), vec![json!(["createNote", "title"])], "{body}");
}

/// A REST representation has no partial form: the read is refused, naming the field.
#[tokio::test]
async fn a_rest_read_of_an_incomplete_row_is_refused() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/notes", server.base))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status.as_u16(), 500, "{text}");
    assert!(!text.contains("\"First\""), "no partial representation is served: {text}");

    let complete = reqwest::Client::new()
        .get(format!("{}/rest/v1/notes/n1", server.base))
        .send()
        .await
        .unwrap();
    assert!(complete.status().is_success(), "a complete row reads: {}", complete.status());
}

/// An MCP tool call's result is the GraphQL response: the error is in it, and the result
/// is an error.
#[tokio::test]
async fn an_mcp_read_of_an_incomplete_row_reports_the_error() {
    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema(), adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            ..McpConfig::default()
        },
    );
    let result = service
        .call_tool_authenticated(
            "notes",
            None,
            None,
            "mcp-nn".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    assert!(text.contains("Cannot return null for non-nullable field Note.title"), "{text}");
    assert_eq!(result.is_error, Some(true), "{text}");
}
