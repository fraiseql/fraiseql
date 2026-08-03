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

    schema.queries.push(list_query("items", "v_item"));
    schema.queries.push(list_query("slowItems", "v_slow_item"));
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
        enable_graphql_sse: true,
        graphql_sse_stream_batch_size: Some(batch_size),
        ..ServerConfig::default()
    }
}

/// One parsed SSE event: `event:` name plus concatenated `data:` payload.
#[derive(Debug)]
struct SseEvent {
    name: String,
    data: String,
}

fn parse_sse(body: &str) -> Vec<SseEvent> {
    body.split("\n\n")
        .filter_map(|block| {
            let mut name = String::from("message");
            let mut data = String::new();
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    name = v.trim().to_string();
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
                Some(SseEvent { name, data })
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
        "with enable_graphql_sse=false the Accept header must be ignored; got {content_type}"
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
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let claims = json!({
        "sub": "sse-user",
        "iss": ISSUER,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + ttl_secs,
    });
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
