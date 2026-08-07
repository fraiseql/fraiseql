#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
//! #941 — the admin API's three cache endpoints must agree with each other.
//!
//! The issue is a three-curl repro against a live server with `cache_enabled = true`
//! and no Arrow Flight cache:
//!
//! ```text
//! GET  /api/v1/admin/config       → {"cache_enabled":"true","cache_status":"active"}
//! GET  /api/v1/admin/cache/stats  → {"cache_enabled":false,"message":"Cache is not configured"}
//! POST /api/v1/admin/cache/clear  → 500 {"error":"… Cache not configured"}
//! ```
//!
//! Two caches shared one vocabulary: `config` reported the query result cache (the one
//! serving GraphQL), while `stats` and `clear` could see only the Arrow Flight cache.
//! An operator following runbook 04 got a 500 from an endpoint whose sibling said the
//! cache was active.
//!
//! This runs the same three requests through `serve_on_listener` — the shipped entry
//! point — and additionally proves the clear is *real*: a warmed entry is gone
//! afterwards, observed as an adapter call, not as a success message.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe (ephemeral port per test)

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as FraiseQLResult,
    schema::{CompiledSchema, QueryDefinition, SqlProjectionHint, TypeDefinition},
};
use fraiseql_server::{Server, server_config::ServerConfig};

const ADMIN_TOKEN: &str = "an-admin-token-that-is-long-enough";

/// Counts every query that reaches the database.
#[derive(Debug, Clone)]
struct CountingAdapter {
    calls: Arc<AtomicU64>,
}

#[async_trait]
impl DatabaseAdapter for CountingAdapter {
    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(serde_json::json!({"id": 1}))])
    }

    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> FraiseQLResult<Vec<JsonbValue>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(serde_json::json!({"id": 1}))])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> FraiseQLResult<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> FraiseQLResult<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl SupportsMutations for CountingAdapter {}

/// One cacheable query over `v_price`, backing the `Price` type.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    schema.types.push(TypeDefinition::new("Price", "v_price"));
    let mut query = QueryDefinition::new("prices", "Price");
    query.sql_source = Some("v_price".to_string());
    query.cache_ttl_seconds = Some(300);
    schema.queries.push(query);
    schema
}

fn admin_config() -> ServerConfig {
    ServerConfig {
        cache_enabled: true,
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        // #874: production validate() refuses cors_enabled = true with empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

struct Client {
    port: u16,
    http: reqwest::Client,
}

impl Client {
    async fn get_json(&self, path: &str) -> serde_json::Value {
        self.http
            .get(format!("http://127.0.0.1:{}{path}", self.port))
            .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
            .send()
            .await
            .expect("GET")
            .json()
            .await
            .expect("JSON body")
    }

    async fn post_json(&self, path: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
        let resp = self
            .http
            .post(format!("http://127.0.0.1:{}{path}", self.port))
            .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .expect("POST");
        let status = resp.status().as_u16();
        (status, resp.json().await.unwrap_or(serde_json::Value::Null))
    }

    async fn graphql(&self, query: &str) -> serde_json::Value {
        self.http
            .post(format!("http://127.0.0.1:{}/graphql", self.port))
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "query": query }).to_string())
            .send()
            .await
            .expect("POST /graphql")
            .json()
            .await
            .expect("JSON body")
    }
}

/// Find the `caches[]` entry for one cache name.
fn cache_entry<'a>(data: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    data["caches"]
        .as_array()
        .unwrap_or_else(|| panic!("response must carry a caches array; got: {data}"))
        .iter()
        .find(|c| c["cache"] == name)
        .unwrap_or_else(|| panic!("no `{name}` entry in: {data}"))
}

#[tokio::test]
async fn the_three_admin_cache_endpoints_agree() {
    let calls = Arc::new(AtomicU64::new(0));
    let adapter = CountingAdapter {
        calls: Arc::clone(&calls),
    };

    let server = Server::new(admin_config(), schema(), Arc::new(adapter), None)
        .await
        .expect("Server::new");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    let client = Client {
        port,
        http: reqwest::Client::new(),
    };

    // curl 1 — config says the cache is active.
    let config = client.get_json("/api/v1/admin/config").await;
    assert_eq!(
        config["data"]["config"]["cache_enabled"], "true",
        "precondition: config must report the cache active; got: {config}"
    );
    assert_eq!(config["data"]["config"]["cache_status"], "active");

    // curl 2 — stats must agree. This is the assertion the issue is about: it used to
    // answer `cache_enabled: false, "Cache is not configured"` on this exact server.
    let stats = client.get_json("/api/v1/admin/cache/stats").await;
    let result_cache = cache_entry(&stats["data"], "query_result");
    assert_eq!(
        result_cache["configured"], true,
        "stats must see the cache config reports active (#941); got: {stats}"
    );

    // Warm one entry, and prove it is warm by the adapter count.
    let body = client.graphql("query { prices { id } }").await;
    assert!(body.get("data").is_some(), "the query must succeed; got: {body}");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "first query reaches the adapter");
    let _ = client.graphql("query { prices { id } }").await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "second query is served from cache");

    let stats = client.get_json("/api/v1/admin/cache/stats").await;
    assert_eq!(
        cache_entry(&stats["data"], "query_result")["entries_count"],
        1,
        "stats must count the warmed entry; got: {stats}"
    );

    // curl 3 — clear must succeed AND actually clear. `entries_cleared` is a claim;
    // the adapter count is the evidence.
    let (status, cleared) = client
        .post_json("/api/v1/admin/cache/clear", serde_json::json!({"scope": "all"}))
        .await;
    assert_eq!(
        status, 200,
        "clear must not 500 on a server whose cache is active; got: {cleared}"
    );
    assert_eq!(
        cache_entry(&cleared["data"], "query_result")["entries_cleared"],
        1,
        "clear must report the entry it dropped; got: {cleared}"
    );

    let _ = client.graphql("query { prices { id } }").await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "after a clear the next query must reach the adapter again — otherwise the \
         endpoint reported a success it did not perform"
    );

    let _ = tx.send(());
    let _ = handle.await;
}

/// The other half of the vocabulary: with the cache off, every endpoint says so.
#[tokio::test]
async fn with_the_cache_off_every_endpoint_says_so() {
    let calls = Arc::new(AtomicU64::new(0));
    let config = ServerConfig {
        cache_enabled: false,
        ..admin_config()
    };
    let server = Server::new(
        config,
        schema(),
        Arc::new(CountingAdapter {
            calls: Arc::clone(&calls),
        }),
        None,
    )
    .await
    .expect("Server::new");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    let client = Client {
        port,
        http: reqwest::Client::new(),
    };

    let config = client.get_json("/api/v1/admin/config").await;
    assert_eq!(config["data"]["config"]["cache_enabled"], "false");

    let stats = client.get_json("/api/v1/admin/cache/stats").await;
    assert_eq!(
        cache_entry(&stats["data"], "query_result")["configured"],
        false,
        "stats must agree with config that there is no cache; got: {stats}"
    );

    // A clear against an absent cache is not an error — it is an honest "nothing here".
    let (status, cleared) = client
        .post_json("/api/v1/admin/cache/clear", serde_json::json!({"scope": "all"}))
        .await;
    assert_eq!(status, 200, "clearing an absent cache is not a server error; got: {cleared}");
    let entry = cache_entry(&cleared["data"], "query_result");
    assert_eq!(entry["configured"], false);
    assert!(
        entry["entries_cleared"].is_null(),
        "an absent cache must report no count rather than a successful clear of zero; got: {cleared}"
    );

    let _ = tx.send(());
    let _ = handle.await;
}
