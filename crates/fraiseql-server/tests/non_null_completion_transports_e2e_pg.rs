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
//! An `Int` (#1928): GraphQL publishes it as a signed 32-bit integer (§ 3.5.1), so a stored
//! value past that range is a field error there; REST publishes it as an OpenAPI `integer`
//! with no `int32` format, so the same value is served as read.
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

/// Two notes, the second with no `title`, the first with `views` past an `Int`'s range; a
/// function that returns a note with no `title`.
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
             ('n1', jsonb_build_object('id', 'n1', 'title', 'First', 'views', 3000000000, \
             'label', 1.50, 'meta', jsonb_build_object('a', 1))), \
             ('n2', jsonb_build_object('id', 'n2')), \
             ('7', jsonb_build_object('id', 7, 'title', 'Seventh'))) AS t(id, data)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_gauge AS SELECT * FROM (VALUES \
             ('g1', jsonb_build_object('id', 'g1', 'count', '7', 'on', 'true', \
             'price', 12.50)), \
             ('g2', jsonb_build_object('id', 'g2', 'count', \
             jsonb_build_object('s3cr3t', 'v4lue')))) AS t(id, data)"
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
                    {"name": "title", "field_type": "String", "nullable": false},
                    {"name": "views", "field_type": "Int", "nullable": true},
                    {"name": "label", "field_type": "String", "nullable": true},
                    {"name": "meta", "field_type": "String", "nullable": true}
                ]
            }, {
                "name": "Gauge",
                "sql_source": format!("{SCHEMA}.v_gauge"),
                "fields": [
                    {"name": "id", "field_type": "ID", "nullable": false},
                    {"name": "count", "field_type": "Int", "nullable": true},
                    {"name": "on", "field_type": "Boolean", "nullable": true},
                    {"name": "price", "field_type": "Decimal", "nullable": true}
                ]
            }],
            "queries": [
                {"name": "notes", "return_type": "Note", "returns_list": true, "nullable": false,
                 "sql_source": format!("{SCHEMA}.v_note"), "jsonb_column": "data", "arguments": []},
                {"name": "note", "return_type": "Note", "returns_list": false, "nullable": true,
                 "sql_source": format!("{SCHEMA}.v_note"), "jsonb_column": "data",
                 "arguments": [{"name": "id", "arg_type": "ID", "nullable": false}]},
                {"name": "gauges", "return_type": "Gauge", "returns_list": true, "nullable": false,
                 "sql_source": format!("{SCHEMA}.v_gauge"), "jsonb_column": "data", "arguments": []},
                {"name": "gauge", "return_type": "Gauge", "returns_list": false, "nullable": true,
                 "sql_source": format!("{SCHEMA}.v_gauge"), "jsonb_column": "data",
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

/// REST publishes an `Int` field as an `integer` of any size, so a stored `bigint` is
/// served as read; GraphQL publishes it as a 32-bit `Int`, so the same row's value is a
/// field error there.
#[tokio::test]
async fn a_wide_int_is_served_by_rest_and_refused_by_graphql() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/notes/n1", server.base))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let views = body.get("data").unwrap_or(&body)["views"].clone();
    assert_eq!(views, json!(3_000_000_000_i64), "REST serves the stored value: {text}");

    let body = graphql(&server, r#"{ note(id: "n1") { id views } }"#, &json!({})).await;
    assert_eq!(body["data"]["note"], json!({"id": "n1", "views": null}), "{body}");
    assert_eq!(paths(&body), vec![json!(["note", "views"])], "{body}");
}

/// A `String` field holding a number or an object: REST, which has no field error to give,
/// serves its JSON text, the number as the `f64` it was read as (`1.50` → `"1.5"`) and the
/// object without the database's spacing. GraphQL refuses both (§ 3.5): neither is served
/// without loss.
#[tokio::test]
async fn a_string_field_holding_another_kind_is_text_on_rest_and_refused_by_graphql() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/notes/n1", server.base))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let note = body.get("data").unwrap_or(&body);
    assert_eq!(note["label"], json!("1.5"), "the trailing zero is not kept: {text}");
    assert_eq!(note["meta"], json!(r#"{"a":1}"#), "nor the spacing: {text}");

    let body = graphql(&server, r#"{ note(id: "n1") { id label meta } }"#, &json!({})).await;
    assert_eq!(body["data"]["note"], json!({"id": "n1", "label": null, "meta": null}), "{body}");
    let mut paths = paths(&body);
    paths.sort_by_key(Value::to_string);
    assert_eq!(paths, vec![json!(["note", "label"]), json!(["note", "meta"])], "{body}");
}

/// A note whose stored `id` is the integer 7: every transport serves it as the string
/// `"7"` (§ 3.5.5). A read by id, the REST representation and a federation `_entities`
/// lookup each find it by `"7"`, and its `id` is the same text in each.
#[tokio::test]
async fn an_integer_id_is_served_as_its_decimal_text_on_every_transport() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(&server, r#"{ note(id: "7") { id title } }"#, &json!({})).await;
    assert_eq!(body["data"]["note"], json!({"id": "7", "title": "Seventh"}), "{body}");

    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/notes/7", server.base))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    let rest: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(rest.get("data").unwrap_or(&rest)["id"], json!("7"), "REST: {text}");

    let body = graphql(
        &server,
        "query($representations: [_Any!]!) { _entities(representations: $representations) {          ... on Note { id title } } }",
        &json!({"representations": [{"__typename": "Note", "id": "7"}]}),
    )
    .await;
    assert_eq!(body["data"]["_entities"][0]["id"], json!("7"), "{body}");
    assert_eq!(body["data"]["_entities"][0]["title"], json!("Seventh"), "{body}");
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

/// The suite's schema loads with no database.
#[test]
fn the_document_loads_without_a_database() {
    assert!(schema().federation.is_some(), "the transports suite's schema loads");
}

/// REST serves a stored value of another kind as the OpenAPI type it publishes the field
/// as: `"7"` under an `integer` is `7`, `"true"` under a `boolean` is `true`, and a
/// `Decimal` stored as the number `12.50` is its digits, the trailing zero kept.
#[tokio::test]
async fn rest_serves_a_stored_value_as_the_type_it_publishes() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let response = reqwest::Client::new()
        .get(format!("{}/rest/v1/gauges/g1", server.base))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let gauge = body.get("data").unwrap_or(&body);
    assert_eq!(gauge["count"], json!(7), "{text}");
    assert_eq!(gauge["on"], json!(true), "{text}");
    assert_eq!(gauge["price"], json!("12.50"), "{text}");
}

/// A stored value its published type cannot hold refuses the read (`500`): REST has no
/// partial representation. The body names neither the value nor any of its text.
#[tokio::test]
async fn rest_refuses_a_value_its_published_type_cannot_hold_without_quoting_it() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    for path in ["gauges/g2", "gauges"] {
        let response = reqwest::Client::new()
            .get(format!("{}/rest/v1/{path}", server.base))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        assert_eq!(status.as_u16(), 500, "{path}: {text}");
        for secret in ["s3cr3t", "v4lue"] {
            assert!(!text.contains(secret), "{path}: the stored value is not quoted: {text}");
        }
    }
}
