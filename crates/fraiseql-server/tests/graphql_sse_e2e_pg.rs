//! #387: GraphQL-over-SSE and root-field `@stream`, end to end against PostgreSQL.
//!
//! Drives the real `Server::serve_on_listener` mount over a real socket — the SSE
//! branch lives inside the authenticated `/graphql` route, and only this shape can
//! observe the mount-site properties this suite pins: auth enforcement before the
//! stream starts (#812's lesson), survival of the global request timeout, and the
//! compression predicate's `text/event-stream` exemption.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p27_sse` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldType, QueryDefinition, TypeDefinition},
};
use fraiseql_server::server_config::{Hs256Config, ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p27_sse";
const SECRET_ENV: &str = "FRAISEQL_TEST_P27_SSE_HS256_SECRET";
const SECRET: &str = "p27-sse-secret-0123456789-0123456789";
const ISSUER: &str = "https://sse.test.fraiseql";
const AUDIENCE: &str = "sse-test";

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

/// Five ordinary rows, plus a slow view where every row's projection sleeps
/// 400 ms — what makes timeout-survival and mid-stream expiry deterministic.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_item (id bigint PRIMARY KEY, label text NOT NULL)"),
        format!(
            "INSERT INTO {SCHEMA}.tb_item VALUES (1,'one'),(2,'two'),(3,'three'),(4,'four'),\
             (5,'five')"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_item ORDER BY id"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_slow_item AS SELECT id, jsonb_build_object('id', id, \
             'label', label, 'nap', (pg_sleep(0.4))::text) AS data FROM {SCHEMA}.tb_item"
        ),
        // A row carrying a nested list, for nested `@stream` (#958). The list lives
        // inside the row's own `data` document — which is the whole reason a nested
        // `@stream` cannot page the database the way a root one does.
        format!(
            "CREATE VIEW {SCHEMA}.v_tagged_item AS SELECT id, jsonb_build_object('id', id, \
             'label', label, 'tags', jsonb_build_array('t1', 't2', 't3', 't4')) AS data \
             FROM {SCHEMA}.tb_item ORDER BY id"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn list_query(name: &str, view: &str) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, "SseItem")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{view}"));
    q.auto_params.has_limit = true;
    q.auto_params.has_offset = true;
    q.auto_params.has_order_by = true;
    q
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut item = TypeDefinition::new("SseItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        fraiseql_core::schema::FieldDefinition::new("id", FieldType::Int),
        fraiseql_core::schema::FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);

    let mut tagged = TypeDefinition::new("SseTaggedItem", format!("{SCHEMA}.v_tagged_item"));
    tagged.fields = vec![
        fraiseql_core::schema::FieldDefinition::new("id", FieldType::Int),
        fraiseql_core::schema::FieldDefinition::new("label", FieldType::String),
        fraiseql_core::schema::FieldDefinition::new(
            "tags",
            FieldType::List(Box::new(FieldType::String)),
        ),
    ];
    schema.types.push(tagged);

    schema.queries.push(list_query("items", "v_item"));
    schema.queries.push(list_query("slowItems", "v_slow_item"));
    {
        let mut q = QueryDefinition::new("taggedItems", "SseTaggedItem")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_tagged_item"));
        q.auto_params.has_limit = true;
        q.auto_params.has_offset = true;
        schema.queries.push(q);
    }
    // Single-item query: the @stream-ineligible shape.
    schema
        .queries
        .push(QueryDefinition::new("item", "SseItem").with_sql_source(format!("{SCHEMA}.v_item")));

    schema.build_indexes();
    schema
}

fn sse_config(batch_size: u32) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        enable_graphql_incremental: true,
        graphql_incremental_batch_size: Some(batch_size),
        ..ServerConfig::default()
    }
}

/// One parsed SSE event: `event:` name, `id:` (the #958 resume point) and
/// concatenated `data:` payload.
#[derive(Debug)]
struct SseEvent {
    name: String,
    id:   Option<String>,
    data: String,
}

fn parse_sse(body: &str) -> Vec<SseEvent> {
    body.split("\n\n")
        .filter_map(|block| {
            let mut name = String::from("message");
            let mut id = None;
            let mut data = String::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    name = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("id:") {
                    id = Some(v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(v.trim_start());
                }
            }
            // Keep-alive comment blocks (":" lines only) parse to empty data +
            // default name; drop them.
            if data.is_empty() && name == "message" {
                None
            } else {
                Some(SseEvent { name, id, data })
            }
        })
        .collect()
}

/// `next` events parsed as JSON payloads.
fn next_payloads(events: &[SseEvent]) -> Vec<Value> {
    events
        .iter()
        .filter(|e| e.name == "next")
        .map(|e| serde_json::from_str(&e.data).expect("next event carries JSON"))
        .collect()
}

/// All streamed item ids, from the initial payload and every incremental batch.
fn streamed_ids(payloads: &[Value], key: &str) -> Vec<i64> {
    let mut ids = Vec::new();
    for p in payloads {
        if let Some(items) = p.get("data").and_then(|d| d.get(key)).and_then(Value::as_array) {
            ids.extend(items.iter().filter_map(|i| i.get("id").and_then(Value::as_i64)));
        }
        if let Some(incr) = p.get("incremental").and_then(Value::as_array) {
            for entry in incr {
                if let Some(items) = entry.get("items").and_then(Value::as_array) {
                    ids.extend(items.iter().filter_map(|i| i.get("id").and_then(Value::as_i64)));
                }
            }
        }
    }
    ids
}

async fn sse_post(server: &TestServer, body: &Value, bearer: Option<&str>) -> reqwest::Response {
    let mut req = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .json(body);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    req.send().await.expect("request")
}

async fn boot(config: ServerConfig) -> Option<TestServer> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    seed(&adapter).await;
    Some(Box::pin(TestServer::start_with_config(config, schema(), adapter)).await)
}

// ── Transport negotiation ────────────────────────────────────────────────────

#[tokio::test]
async fn sse_disabled_by_default_ignores_accept() {
    if database_url_or_skip("sse_disabled_by_default").is_none() {
        return;
    }
    let server = Box::pin(boot(ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    }))
    .await
    .unwrap();

    let resp = sse_post(&server, &json!({"query": "{ items { id label } }"}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "with enable_graphql_incremental=false the Accept header must be ignored; got {content_type}"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["items"].as_array().map(Vec::len), Some(5));
}

#[tokio::test]
async fn single_result_mode_delivers_next_then_complete() {
    if database_url_or_skip("single_result_mode").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(100))).await.unwrap();

    let resp = sse_post(&server, &json!({"query": "{ items { id label } }"}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("text/event-stream"),
        "negotiated SSE must answer text/event-stream; got {content_type}"
    );

    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);
    assert_eq!(payloads.len(), 1, "single-result mode is one next event: {events:?}");
    assert_eq!(payloads[0]["data"]["items"].as_array().map(Vec::len), Some(5));
    assert_eq!(
        events.last().map(|e| e.name.as_str()),
        Some("complete"),
        "the stream must terminate with a complete event: {events:?}"
    );
}

// ── @stream delivery ─────────────────────────────────────────────────────────

#[tokio::test]
async fn stream_delivers_initial_then_batches_in_order() {
    if database_url_or_skip("stream_batches").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    let query = r"{ items(orderBy: {id: ASC}) @stream(initialCount: 2) { id label } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    assert_eq!(resp.status(), 200);
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    // Initial payload: exactly initialCount rows and hasNext = true.
    assert_eq!(
        payloads[0]["data"]["items"].as_array().map(Vec::len),
        Some(2),
        "initial payload must carry initialCount items: {payloads:?}"
    );
    assert_eq!(payloads[0]["hasNext"], json!(true));

    // Every row arrives exactly once, in order.
    assert_eq!(streamed_ids(&payloads, "items"), vec![1, 2, 3, 4, 5]);

    // The final next payload closes the delivery; the stream ends with complete.
    assert_eq!(payloads.last().unwrap()["hasNext"], json!(false));
    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));

    // Incremental envelopes carry the response-key path.
    let first_incremental = payloads
        .iter()
        .find_map(|p| p.get("incremental").and_then(Value::as_array).and_then(|a| a.first()))
        .expect("at least one incremental payload");
    assert_eq!(first_incremental["path"][0], json!("items"));
}

#[tokio::test]
async fn stream_respects_the_client_row_limit() {
    if database_url_or_skip("stream_client_limit").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    let query = r"{ items(limit: 3, orderBy: {id: ASC}) @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    assert_eq!(resp.status(), 200);
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    assert_eq!(
        streamed_ids(&payloads, "items"),
        vec![1, 2, 3],
        "the client's limit is the total row budget; streaming must not exceed it"
    );
    assert_eq!(payloads.last().unwrap()["hasNext"], json!(false));
}

// ── multipart/mixed, the second framing (#958) ───────────────────────────────

/// Split a `multipart/mixed` body into its parts' JSON bodies.
///
/// Deliberately strict about the framing rather than "find the JSON": the delimiter
/// (`---`) and terminator (`-----`) differing is the whole contract, and a client that
/// cannot tell them apart hangs waiting for a body that never comes.
fn parse_multipart(body: &str) -> Vec<Value> {
    assert!(
        body.ends_with("-----\r\n"),
        "the body must end with the closing boundary: {body:?}"
    );
    body.trim_end_matches("-----\r\n")
        .split("\r\n---\r\n")
        .filter(|part| !part.trim().is_empty())
        .map(|part| {
            let (headers, json) = part
                .split_once("\r\n\r\n")
                .unwrap_or_else(|| panic!("each part has headers then a body: {part:?}"));
            assert!(
                headers.contains("content-type: application/json"),
                "each part declares its content type: {headers:?}"
            );
            serde_json::from_str(json.trim_end())
                .unwrap_or_else(|e| panic!("part body is JSON ({e}): {json:?}"))
        })
        .collect()
}

async fn multipart_post(server: &TestServer, body: &Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "multipart/mixed; deferSpec=20220824")
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .expect("request")
}

/// The same `@stream` delivery, framed as `multipart/mixed`: identical payload
/// sequence, different envelope. A client of either framing must see the same rows.
#[tokio::test]
async fn multipart_delivers_the_same_stream_payloads_as_sse() {
    if database_url_or_skip("multipart_stream").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();
    let query = r"{ items(orderBy: {id: ASC}) @stream(initialCount: 2) { id } }";

    let sse_ids = streamed_ids(
        &next_payloads(&parse_sse(
            &sse_post(&server, &json!({"query": query}), None).await.text().await.unwrap(),
        )),
        "items",
    );

    let resp = multipart_post(&server, &json!({"query": query})).await;
    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("multipart/mixed"),
        "a negotiated multipart request must answer multipart/mixed; got {content_type}"
    );
    assert!(
        content_type.contains("boundary=\"-\""),
        "the boundary must be declared for the client to split on: {content_type}"
    );

    let payloads = parse_multipart(&resp.text().await.unwrap());
    assert_eq!(
        streamed_ids(&payloads, "items"),
        sse_ids,
        "the two framings must deliver the same rows: {payloads:?}"
    );
    assert_eq!(
        payloads.last().unwrap()["hasNext"],
        json!(false),
        "multipart has no terminal event, so the last payload's hasNext:false is the \
         only end-of-delivery signal: {payloads:?}"
    );
}

/// `@defer` over multipart, the framing Apollo and Relay default to.
#[tokio::test]
async fn multipart_delivers_deferred_payloads() {
    if database_url_or_skip("multipart_defer").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let query = r#"
        { items(limit: 1, orderBy: {id: ASC}) { id ...Detail @defer(label: "detail") } }
        fragment Detail on SseItem { label }
    "#;
    let resp = multipart_post(&server, &json!({"query": query})).await;
    assert_eq!(resp.status(), 200);
    let payloads = parse_multipart(&resp.text().await.unwrap());

    assert_eq!(payloads.len(), 2, "an immediate part and a deferred part: {payloads:?}");
    assert_eq!(payloads[0]["data"]["items"], json!([{"id": 1}]));
    assert_eq!(payloads[0]["hasNext"], json!(true));
    assert_eq!(payloads[1]["incremental"][0]["data"], json!({"label": "one"}));
    assert_eq!(payloads[1]["incremental"][0]["path"], json!(["items", 0]));
    assert_eq!(payloads[1]["hasNext"], json!(false));
}

/// `Accept` listing both framings gets SSE — the one this transport shipped first.
/// A client naming both is saying "either", and answering it with neither would be
/// the only wrong reply.
#[tokio::test]
async fn accept_listing_both_framings_resolves_to_sse() {
    if database_url_or_skip("multipart_accept_both").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let resp = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "multipart/mixed; deferSpec=20220824, text/event-stream")
        .header("content-type", "application/json")
        .json(&json!({"query": "{ items(limit: 1) { id } }"}))
        .send()
        .await
        .expect("request");
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(content_type.starts_with("text/event-stream"), "got {content_type}");
}

/// Multipart is the same opt-in as SSE. With incremental delivery disabled, the
/// `Accept` header is ignored and the ordinary buffered JSON response is served —
/// answering a multipart body here would mean the operator's opt-out applied to one
/// framing and not the other.
#[tokio::test]
async fn multipart_is_ignored_when_incremental_delivery_is_disabled() {
    if database_url_or_skip("multipart_disabled").is_none() {
        return;
    }
    let server = Box::pin(boot(ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    }))
    .await
    .unwrap();

    let resp = multipart_post(&server, &json!({"query": "{ items(limit: 1) { id } }"})).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(content_type.starts_with("application/json"), "got {content_type}");
}

// ── @defer on fragments (#958) ───────────────────────────────────────────────

/// `@defer` on a fragment spread: the immediate payload carries the fields the
/// client did not defer, then the deferred fragment arrives as an `incremental`
/// entry addressed by its path.
#[tokio::test]
async fn defer_on_a_fragment_splits_the_response_into_two_payloads() {
    if database_url_or_skip("sse_defer_fragment").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let query = r#"
        { items(limit: 2, orderBy: {id: ASC}) { id ...Detail @defer(label: "detail") } }
        fragment Detail on SseItem { label }
    "#;
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    assert_eq!(resp.status(), 200);
    let payloads = next_payloads(&parse_sse(&resp.text().await.unwrap()));

    // Immediate payload: the undeferred field only, and hasNext announcing more.
    assert_eq!(
        payloads[0]["data"]["items"],
        json!([{"id": 1}, {"id": 2}]),
        "the deferred fragment's field must not be in the immediate payload: {payloads:?}"
    );
    assert_eq!(payloads[0]["hasNext"], json!(true));

    // One incremental entry per list element, each addressed by its own index.
    let entries: Vec<&Value> = payloads[1..]
        .iter()
        .filter_map(|p| p.get("incremental").and_then(Value::as_array))
        .flatten()
        .collect();
    assert_eq!(entries.len(), 2, "one deferred payload per element: {payloads:?}");
    assert_eq!(entries[0]["path"], json!(["items", 0]));
    assert_eq!(entries[0]["data"], json!({"label": "one"}));
    assert_eq!(entries[0]["label"], json!("detail"));
    assert_eq!(entries[1]["path"], json!(["items", 1]));
    assert_eq!(entries[1]["data"], json!({"label": "two"}));

    assert_eq!(
        payloads.last().unwrap()["hasNext"],
        json!(false),
        "the final payload closes the delivery: {payloads:?}"
    );
}

/// `@defer(if: false)` is not a defer: the response must keep its ordinary
/// single-payload shape, with no `hasNext` promising a payload that never comes.
#[tokio::test]
async fn defer_if_false_delivers_one_ordinary_payload() {
    if database_url_or_skip("sse_defer_if_false").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let query = r"
        { items(limit: 1) { id ...Detail @defer(if: false) } }
        fragment Detail on SseItem { label }
    ";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    assert_eq!(payloads.len(), 1, "a disabled @defer must not split the response: {payloads:?}");
    assert_eq!(payloads[0]["data"]["items"][0]["label"], json!("one"));
    assert!(
        payloads[0].get("hasNext").is_none(),
        "an unsplit response must not announce a continuation: {payloads:?}"
    );
    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));
}

/// `@defer` and `@stream` in one operation order the same response differently.
/// Interleaving them is not defined here, so the combination is refused rather
/// than silently resolved one way.
#[tokio::test]
async fn defer_combined_with_stream_is_refused() {
    if database_url_or_skip("sse_defer_with_stream").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    let query = r"
        { items(orderBy: {id: ASC}) @stream(initialCount: 1) { id ...Detail @defer } }
        fragment Detail on SseItem { label }
    ";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "the refusal must be a buffered error response, never a stream; got {content_type}"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["errors"][0]["message"].as_str().unwrap_or("").contains("@defer"),
        "the combination must be refused with the reason: {body}"
    );
}

/// The buffered transport cannot deliver incrementally, so `@defer` stays what the
/// spec permits there: the full result in one response. A split payload over
/// `application/json` would be a response no GraphQL client could read.
#[tokio::test]
async fn defer_over_the_buffered_transport_delivers_the_full_result() {
    if database_url_or_skip("sse_defer_buffered").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("content-type", "application/json")
        .json(&json!({"query": r"
            { items(limit: 1) { id ...Detail @defer } }
            fragment Detail on SseItem { label }
        "}))
        .send()
        .await
        .expect("request")
        .json()
        .await
        .unwrap();

    assert_eq!(
        body["data"]["items"][0],
        json!({"id": 1, "label": "one"}),
        "without an incremental transport the deferred field must still be delivered: {body}"
    );
}

// ── Resumption via Last-Event-ID (#958) ──────────────────────────────────────

/// Every `next` event carries `id:` = the absolute offset of the first row it has
/// *not* delivered, so a client that reconnects with `Last-Event-ID` continues
/// exactly where it stopped: no row repeated, none skipped.
#[tokio::test]
async fn last_event_id_resumes_the_delivery_without_repeating_or_skipping_rows() {
    if database_url_or_skip("sse_resume").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    let query = r"{ items(orderBy: {id: ASC}) @stream(initialCount: 2) { id } }";

    // First connection, read whole: ids are 2, 3, 4, 5, 5 (initial delivers 2 rows,
    // then one per batch, and the terminal payload repeats the final offset).
    let first =
        parse_sse(&sse_post(&server, &json!({"query": query}), None).await.text().await.unwrap());
    let ids: Vec<&str> = first
        .iter()
        .filter(|e| e.name == "next")
        .map(|e| e.id.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        ids.first().copied(),
        Some("2"),
        "the initial payload's id is the offset after its initialCount rows: {ids:?}"
    );
    assert!(
        ids.iter().all(|id| !id.is_empty()),
        "every next event must carry a resume id: {ids:?}"
    );

    // A client that got as far as id "2" reconnects from there.
    let resumed = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .header("last-event-id", "2")
        .json(&json!({"query": query}))
        .send()
        .await
        .expect("request");
    assert_eq!(resumed.status(), 200);
    let payloads = next_payloads(&parse_sse(&resumed.text().await.unwrap()));
    assert_eq!(
        streamed_ids(&payloads, "items"),
        vec![3, 4, 5],
        "resuming at offset 2 must deliver rows 3..5 and nothing already seen"
    );
}

/// Resuming must not restart the client's own row budget: `limit: 3` means three
/// rows across the whole logical delivery, not three more per reconnect.
#[tokio::test]
async fn resuming_charges_already_delivered_rows_against_the_client_limit() {
    if database_url_or_skip("sse_resume_budget").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    let query = r"{ items(limit: 3, orderBy: {id: ASC}) @stream(initialCount: 1) { id } }";
    let resumed = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "text/event-stream")
        .header("content-type", "application/json")
        .header("last-event-id", "2")
        .json(&json!({"query": query}))
        .send()
        .await
        .expect("request");
    assert_eq!(resumed.status(), 200);
    let payloads = next_payloads(&parse_sse(&resumed.text().await.unwrap()));
    assert_eq!(
        streamed_ids(&payloads, "items"),
        vec![3],
        "two rows were already delivered before the reconnect, so a limit of 3 leaves \
         exactly one — a resumed delivery that restarted the budget would return three"
    );
}

/// A `Last-Event-ID` that is not an offset, or that points before the document's own
/// `offset` argument, is refused loudly. Clamping it silently would deliver a wrong
/// result set that looks like a right one — and pointing it *backwards* would let a
/// header override a query argument.
#[tokio::test]
async fn a_malformed_or_backwards_last_event_id_is_refused() {
    if database_url_or_skip("sse_resume_refusals").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(1))).await.unwrap();

    for (header, expect) in [("not-an-offset", "Last-Event-ID"), ("1", "precedes")] {
        let resp = reqwest::Client::new()
            .post(format!("{}/graphql", server.url))
            .header("accept", "text/event-stream")
            .header("content-type", "application/json")
            .header("last-event-id", header)
            .json(&json!({
                "query": r"{ items(offset: 2, orderBy: {id: ASC}) @stream(initialCount: 1) { id } }"
            }))
            .send()
            .await
            .expect("request");
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert!(
            content_type.starts_with("application/json"),
            "the refusal must be a buffered error response, never a stream; got {content_type}"
        );
        let body: Value = resp.json().await.unwrap();
        assert!(
            body["errors"][0]["message"].as_str().unwrap_or("").contains(expect),
            "Last-Event-ID {header:?} must be refused naming {expect:?}: {body}"
        );
    }
}

// ── Refusals: loud, before any event ─────────────────────────────────────────

#[tokio::test]
async fn stream_on_a_single_item_query_is_refused() {
    if database_url_or_skip("stream_single_item").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let resp = sse_post(&server, &json!({"query": "{ item @stream { id } }"}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "the refusal must be a buffered error response, never a stream; got {content_type}"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["errors"][0]["message"].as_str().unwrap_or("").contains("list"),
        "@stream on a non-list query must be refused loudly with the reason: {body}"
    );
}

#[tokio::test]
async fn stream_with_conflicting_pagination_variables_is_refused() {
    if database_url_or_skip("stream_var_conflict").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let query = "query($limit: Int) { items(limit: $limit) @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query, "variables": {"limit": 4}}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "the refusal must be a buffered error response, never a stream; got {content_type}"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["errors"][0]["message"].as_str().unwrap_or("").contains("$limit"),
        "a document declaring $limit would be silently rewritten by batch pagination — \
         it must be refused with the reason: {body}"
    );
}

// ── Auth: inherited at entry, re-checked mid-stream (P18) ────────────────────

fn hs256_config(batch_size: u32) -> ServerConfig {
    std::env::set_var(SECRET_ENV, SECRET);
    ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        ..sse_config(batch_size)
    }
}

fn mint_token(ttl_secs: i64) -> String {
    mint_token_with_jti(ttl_secs, None)
}

fn mint_token_with_jti(ttl_secs: i64, jti: Option<&str>) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let mut claims = json!({
        "sub": "sse-user",
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + ttl_secs,
    });
    if let Some(jti) = jti {
        claims["jti"] = json!(jti);
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

#[tokio::test]
async fn unauthenticated_sse_is_refused_before_any_event() {
    if database_url_or_skip("sse_unauthenticated").is_none() {
        return;
    }
    let server = Box::pin(boot(hs256_config(10))).await.unwrap();

    let resp = sse_post(&server, &json!({"query": "{ items { id } }"}), None).await;
    assert_eq!(
        resp.status(),
        401,
        "the SSE branch sits behind the same auth route_layer as the buffered \
         transport — an unauthenticated request must be refused before any stream opens"
    );
}

#[tokio::test]
async fn expired_token_terminates_the_stream_mid_delivery() {
    if database_url_or_skip("sse_expiry_mid_stream").is_none() {
        return;
    }
    let server = Box::pin(boot(hs256_config(1))).await.unwrap();

    // Full delivery takes ~2 s (5 rows x 400 ms, one per batch); the token
    // expires after 1 s, so the per-batch expiry re-check MUST fire mid-stream.
    let token = mint_token(1);
    let query = "{ slowItems @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query}), Some(&token)).await;
    assert_eq!(resp.status(), 200, "the stream starts while the token is valid");

    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    let ids = streamed_ids(&payloads, "slowItems");
    assert!(
        ids.len() < 5,
        "the delivery must terminate before completing all rows; got {ids:?}"
    );
    let error_payload = payloads
        .iter()
        .find(|p| p.get("errors").is_some())
        .unwrap_or_else(|| panic!("an UNAUTHENTICATED error event must be emitted: {payloads:?}"));
    assert!(
        error_payload["errors"][0]["extensions"]["code"]
            .as_str()
            .unwrap_or("")
            .contains("UNAUTHENTICATED"),
        "the termination names token expiry: {error_payload}"
    );
    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));
}

/// The `jti` the mid-delivery test revokes.
const JTI: &str = "sse-stream-jti";

/// #958: expiry is not the only way a principal stops being one. A `revoke-all`
/// ("log out everywhere") or a single-token revocation lands *while* the delivery is
/// running, and before this the batch loop checked only `expires_at` — so a revoked
/// principal kept being served for the whole remaining life of the delivery, which on
/// a large result set is unbounded.
///
/// The token here is valid for an hour, so **expiry cannot be what terminates it**;
/// the assertion on the reason string is what keeps that honest.
#[tokio::test]
async fn revoked_token_terminates_the_stream_mid_delivery() {
    use fraiseql_server::token_revocation::{
        InMemoryRevocationStore, RevocationStore, TokenRevocationManager,
    };

    let Some(url) = database_url_or_skip("sse_revocation_mid_stream") else {
        return;
    };

    let store: Arc<dyn RevocationStore> = Arc::new(InMemoryRevocationStore::new());
    let revocation = Arc::new(TokenRevocationManager::new(store, false, false, 3600));

    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("adapter"));
    seed(&adapter).await;
    let server = Box::pin(TestServer::start_with_revocation(
        hs256_config(1),
        schema(),
        adapter,
        Arc::clone(&revocation),
    ))
    .await;

    let token = mint_token_with_jti(3600, Some(JTI));

    // Revoke ~600 ms in: the delivery is ~2 s (5 rows x 400 ms, one row per batch),
    // so this lands mid-stream.
    let revoker = Arc::clone(&revocation);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        revoker.revoke(JTI, 3600).await.expect("revoke mid-stream");
    });

    let query = "{ slowItems @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query}), Some(&token)).await;
    assert_eq!(resp.status(), 200, "the stream starts while the token is valid");

    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    let ids = streamed_ids(&payloads, "slowItems");
    assert!(
        ids.len() < 5,
        "the delivery must terminate before completing all rows; got {ids:?}"
    );
    let error_payload = payloads
        .iter()
        .find(|p| p.get("errors").is_some())
        .unwrap_or_else(|| panic!("an UNAUTHENTICATED error event must be emitted: {payloads:?}"));
    let message = error_payload["errors"][0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("revoked"),
        "the termination must name revocation, not expiry — an hour-long token cannot \
         have expired, so an expiry message would mean the revocation check never ran: \
         {error_payload}"
    );
    assert_eq!(
        error_payload["errors"][0]["extensions"]["code"].as_str().unwrap_or(""),
        "UNAUTHENTICATED"
    );
    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));
}

// ── Middleware interactions ──────────────────────────────────────────────────

#[tokio::test]
async fn stream_survives_the_global_request_timeout() {
    if database_url_or_skip("sse_timeout_survival").is_none() {
        return;
    }
    let server = Box::pin(boot(ServerConfig {
        request_timeout_secs: Some(1),
        ..sse_config(1)
    }))
    .await
    .unwrap();

    // ~2 s of delivery against a 1 s request timeout: the timeout bounds
    // response-head production, not the streaming body.
    let query = "{ slowItems @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    assert_eq!(resp.status(), 200);
    let events = parse_sse(&resp.text().await.unwrap());
    assert_eq!(
        streamed_ids(&next_payloads(&events), "slowItems").len(),
        5,
        "a streaming delivery longer than request_timeout_secs must not be truncated: \
         {events:?}"
    );
    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));
}

#[tokio::test]
async fn sse_responses_are_not_compressed() {
    if database_url_or_skip("sse_not_compressed").is_none() {
        return;
    }
    let server = Box::pin(boot(ServerConfig {
        compression_enabled: true,
        ..sse_config(1)
    }))
    .await
    .unwrap();

    let resp = reqwest::Client::new()
        .post(format!("{}/graphql", server.url))
        .header("accept", "text/event-stream")
        .header("accept-encoding", "gzip")
        .header("content-type", "application/json")
        .json(&json!({"query": "{ items @stream(initialCount: 1) { id label } }"}))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status(), 200);
    assert!(
        resp.headers().get("content-encoding").is_none(),
        "the compression predicate must exempt text/event-stream — a buffered gzip \
         encoder defeats incremental flushing"
    );
    let events = parse_sse(&resp.text().await.unwrap());
    assert_eq!(streamed_ids(&next_payloads(&events), "items"), vec![1, 2, 3, 4, 5]);
}

// ---------------------------------------------------------------------------
// Nested `@stream` (#958)
// ---------------------------------------------------------------------------

/// A nested `@stream` splits the delivery of a list the single statement already
/// produced: the initial payload carries `initialCount` items and each chunk arrives
/// addressed by the index of its first item.
///
/// It is deliberately **not** database paging. The list lives inside the row's own
/// JSONB document, so there is nothing to page — see
/// `fraiseql_core::graphql::stream_split` for why a second statement would be a
/// second snapshot with no sound alignment.
#[tokio::test]
async fn nested_stream_splits_the_delivery_of_an_inner_list() {
    if database_url_or_skip("sse_nested_stream").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(2))).await.unwrap();

    let query = r"{ taggedItems(limit: 1) { id tags @stream(initialCount: 1) } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    assert_eq!(
        payloads.len(),
        3,
        "one immediate payload plus two chunks of the four-item list: {payloads:?}"
    );

    assert_eq!(
        payloads[0]["data"]["taggedItems"][0]["tags"],
        json!(["t1"]),
        "the immediate payload keeps exactly initialCount items: {payloads:?}"
    );
    assert_eq!(payloads[0]["hasNext"], json!(true));

    assert_eq!(payloads[1]["incremental"][0]["items"], json!(["t2", "t3"]));
    assert_eq!(
        payloads[1]["incremental"][0]["path"],
        json!(["taggedItems", 0, "tags", 1]),
        "a chunk is addressed by the response path of its first item: {payloads:?}"
    );
    assert_eq!(payloads[1]["hasNext"], json!(true));

    assert_eq!(payloads[2]["incremental"][0]["items"], json!(["t4"]));
    assert_eq!(payloads[2]["incremental"][0]["path"], json!(["taggedItems", 0, "tags", 3]));
    assert_eq!(
        payloads[2]["hasNext"],
        json!(false),
        "the last chunk ends the delivery: {payloads:?}"
    );

    assert_eq!(events.last().map(|e| e.name.as_str()), Some("complete"));
}

/// Each element of the enclosing list is its own path.
///
/// Without that, every row's inner list would be addressed as the same list and a
/// client would splice all of them into the first row — a corruption that looks like
/// a working stream, which is why it is asserted rather than assumed.
#[tokio::test]
async fn nested_stream_addresses_each_enclosing_row_separately() {
    if database_url_or_skip("sse_nested_stream_rows").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(10))).await.unwrap();

    let query = r"{ taggedItems(limit: 2) { id tags @stream(initialCount: 0) } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    assert_eq!(payloads.len(), 3, "one immediate payload plus one chunk per row: {payloads:?}");
    assert_eq!(payloads[1]["incremental"][0]["path"], json!(["taggedItems", 0, "tags", 0]));
    assert_eq!(payloads[2]["incremental"][0]["path"], json!(["taggedItems", 1, "tags", 0]));
}

/// A root `@stream` still pages the database, and is not turned into a delivery
/// split by the nested support: the two are different features over different things.
#[tokio::test]
async fn a_root_stream_still_pages_the_database() {
    if database_url_or_skip("sse_root_stream_unchanged").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(2))).await.unwrap();

    let query = r"{ items(orderBy: {id: ASC}) @stream(initialCount: 1) { id } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let events = parse_sse(&resp.text().await.unwrap());
    let payloads = next_payloads(&events);

    assert_eq!(streamed_ids(&payloads, "items"), vec![1, 2, 3, 4, 5]);
    assert!(
        events.iter().any(|e| e.id.is_some()),
        "a root @stream is resumable and stamps its resume point: {events:?}"
    );
}

/// A nested `@stream` on a field that did not resolve to a list is refused, and
/// refused as an **HTTP error** — the split happens before any byte is written, so
/// the transport can still say no. A directive that silently did nothing on a
/// negotiated incremental transport would read to the client as "streaming worked".
#[tokio::test]
async fn nested_stream_on_a_non_list_field_is_refused() {
    if database_url_or_skip("sse_nested_stream_non_list").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(2))).await.unwrap();

    let query = r"{ taggedItems(limit: 1) { id label @stream } }";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "the refusal must be a buffered error response, never a stream; got {content_type}"
    );
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["errors"][0]["message"].as_str().unwrap_or("").contains("list"),
        "the refusal must name the reason: {body}"
    );
}

/// A nested `@stream` and a `@defer` both split the delivery of one result, and
/// their payload order is not defined here. Refused rather than resolved one way.
#[tokio::test]
async fn nested_stream_combined_with_defer_is_refused() {
    if database_url_or_skip("sse_nested_stream_defer").is_none() {
        return;
    }
    let server = Box::pin(boot(sse_config(2))).await.unwrap();

    let query = r"
        { taggedItems(limit: 1) { id tags @stream ...Detail @defer } }
        fragment Detail on SseTaggedItem { label }
    ";
    let resp = sse_post(&server, &json!({"query": query}), None).await;
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["errors"][0]["message"].as_str().unwrap_or("").contains("@defer"),
        "the combination must be refused with the reason: {body}"
    );
}
