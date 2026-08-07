//! Cache wiring integration tests (issue #184).
//!
//! Verifies that:
//! 1. `Server::new` accepts an inner adapter and builds successfully.
//! 2. `CachedDatabaseAdapter` (the wrapper that `Server::new` now uses internally) correctly caches
//!    results: the second identical call is a cache hit and does not reach the DB.
//! 3. When the cache is disabled (`CacheConfig::disabled()`), every call reaches the DB.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** safe
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(clippy::missing_errors_doc)] // Reason: test functions
#![allow(missing_docs)] // Reason: test code does not require documentation

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{
        DatabaseAdapter, DatabaseType, SupportsMutations, WhereClause,
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::Result as FraiseQLResult,
    schema::{CompiledSchema, SqlProjectionHint},
};
use fraiseql_server::{Server, server_config::ServerConfig};

/// A minimal database adapter that counts `execute_where_query` calls.
#[derive(Debug)]
struct CountingAdapter {
    call_count: Arc<AtomicU64>,
}

impl CountingAdapter {
    fn new() -> (Self, Arc<AtomicU64>) {
        let counter = Arc::new(AtomicU64::new(0));
        (
            Self {
                call_count: Arc::clone(&counter),
            },
            counter,
        )
    }
}

impl Clone for CountingAdapter {
    fn clone(&self) -> Self {
        Self {
            call_count: Arc::clone(&self.call_count),
        }
    }
}

// Reason: async_trait required by DatabaseAdapter trait definition
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
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "test"}),
        )])
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
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "test"}),
        )])
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

// ── Test 1: CachedDatabaseAdapter caches results ───────────────────────────

/// With caching enabled, issuing the same query twice calls the underlying adapter once.
#[tokio::test]
async fn test_cached_adapter_cache_hit_on_second_query() {
    let (inner, counter) = CountingAdapter::new();

    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(inner, cache, "test-schema-v1".to_string());

    // First call — cache miss.
    let _ = adapter.execute_where_query("v_item", None, None, None, None).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "first call must reach the underlying adapter"
    );

    // Second identical call — cache hit; underlying adapter NOT called again.
    let _ = adapter.execute_where_query("v_item", None, None, None, None).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "second identical call must be served from cache (adapter count unchanged)"
    );
}

// ── Test 2: CachedDatabaseAdapter with cache disabled is a passthrough ──────

/// With caching disabled, every call goes to the underlying adapter.
#[tokio::test]
async fn test_cached_adapter_disabled_is_passthrough() {
    let (inner, counter) = CountingAdapter::new();

    let cache = QueryResultCache::new(CacheConfig::disabled());
    let adapter = CachedDatabaseAdapter::new(inner, cache, "test-schema-v1".to_string());

    // First call.
    let _ = adapter.execute_where_query("v_item", None, None, None, None).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "first call must reach the underlying adapter"
    );

    // Second call — cache disabled so adapter is hit again.
    let _ = adapter.execute_where_query("v_item", None, None, None, None).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "second call must also reach the adapter when cache is disabled"
    );
}

// ── Test 3: Server::new accepts an inner adapter and wraps it ──────────────

/// `Server::new` should successfully build with any `DatabaseAdapter + Clone + Send + Sync`.
#[tokio::test]
async fn test_server_new_wraps_adapter_successfully() {
    let (adapter, _counter) = CountingAdapter::new();
    let mut schema = CompiledSchema::default();
    schema
        .queries
        .push(fraiseql_core::schema::QueryDefinition::new("items", "Item"));

    let config = ServerConfig {
        cache_enabled: true,
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    };

    // This compiles and runs only if Server::new correctly returns
    // Server<CachedDatabaseAdapter<CountingAdapter>>.
    let _server = Server::new(config, schema, Arc::new(adapter), None)
        .await
        .expect("Server::new must succeed when adapter satisfies bounds");
}

// ── Test 4: Server::new with cache_disabled also builds ────────────────────

#[tokio::test]
async fn test_server_new_cache_disabled_also_builds() {
    let (adapter, _counter) = CountingAdapter::new();
    let schema = CompiledSchema::default();

    let config = ServerConfig {
        cache_enabled: false,
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    };

    let _server = Server::new(config, schema, Arc::new(adapter), None)
        .await
        .expect("Server::new must succeed when cache_enabled = false");
}

// ── Test 5: the Arrow/Flight constructor caches too (#889) ─────────────────
//
// `with_flight_service` used to hand the raw adapter straight to the executor, so
// `cache_enabled = true` did nothing on the arrow boot path — the same TOML behaved
// completely differently depending on which feature the binary was built with. The
// assertion is a *hit count* through the shipped HTTP entry point, not a log line:
// two identical queries must reach the adapter once.

/// A schema whose one query is annotated cacheable. The annotation is required:
/// `with_cache_metadata_from_schema` puts the adapter in opt-in mode, so a view with
/// no `cache_ttl_seconds` bypasses the cache on *every* constructor.
#[cfg(feature = "arrow")]
fn cacheable_schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    let mut query = fraiseql_core::schema::QueryDefinition::new("items", "Item");
    query.sql_source = Some("v_item".to_string());
    query.cache_ttl_seconds = Some(60);
    schema.queries.push(query);
    schema
}

#[cfg(feature = "arrow")]
async fn post_graphql(port: u16, body: &str) -> String {
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("POST /graphql")
        .text()
        .await
        .expect("body")
}

/// Serve `server` on an ephemeral port, issue the same query twice, and return the
/// adapter call count plus the first response body.
#[cfg(feature = "arrow")]
async fn count_adapter_calls_for_two_identical_queries<A>(
    server: Server<A>,
    counter: &Arc<AtomicU64>,
) -> (u64, String)
where
    A: fraiseql_core::db::DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });

    let query = r#"{"query":"query { items { id name } }"}"#;
    let first = post_graphql(port, query).await;
    let _ = post_graphql(port, query).await;

    let _ = tx.send(());
    let _ = handle.await;
    (counter.load(Ordering::SeqCst), first)
}

#[cfg(feature = "arrow")]
fn caching_config() -> ServerConfig {
    ServerConfig {
        cache_enabled: true,
        // #874: production validate() refuses cors_enabled=true + empty origins
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

/// The headline: the arrow boot path caches, and it caches the *same way*
/// `Server::new` does. Both halves run against the same schema and the same query,
/// so a difference is a constructor difference and nothing else.
#[cfg(feature = "arrow")]
#[tokio::test]
async fn with_flight_service_caches_exactly_as_server_new_does() {
    let (adapter, counter) = CountingAdapter::new();
    let server = Server::new(caching_config(), cacheable_schema(), Arc::new(adapter), None)
        .await
        .expect("Server::new must boot with cache_enabled = true");
    let (baseline, body) = count_adapter_calls_for_two_identical_queries(server, &counter).await;
    assert!(
        body.contains("\"data\""),
        "the query must succeed before any cache assertion means anything; got: {body}"
    );
    assert_eq!(
        baseline, 1,
        "reference: with Server::new, the second identical query is served from cache"
    );

    let (adapter, counter) = CountingAdapter::new();
    let server = Server::with_flight_service(
        caching_config(),
        cacheable_schema(),
        Arc::new(adapter),
        None,
        None,
    )
    .await
    .expect("with_flight_service must boot with cache_enabled = true");
    let (arrow_calls, body) = count_adapter_calls_for_two_identical_queries(server, &counter).await;
    assert!(
        body.contains("\"data\""),
        "the query must succeed on the arrow path too; got: {body}"
    );
    assert_eq!(
        arrow_calls, baseline,
        "the arrow boot path must build the same result cache as Server::new: the second \
         identical query must not reach the adapter (#889)"
    );
}

/// The one combination the cache cannot be made honest for: an allow-listed Flight
/// Upload writes rows without passing the mutation runner, so nothing invalidates.
/// Refused at boot rather than served stale.
#[cfg(feature = "arrow")]
#[tokio::test]
async fn cache_plus_flight_upload_is_refused_at_boot() {
    let (adapter, _counter) = CountingAdapter::new();
    let config = ServerConfig {
        flight_upload_tables: vec!["ta_measurements".to_string()],
        ..caching_config()
    };

    let err =
        Server::with_flight_service(config, cacheable_schema(), Arc::new(adapter), None, None)
            .await
            .err()
            .expect("cache_enabled + a non-empty Upload allow-list must refuse to boot (#889)");

    let msg = err.to_string();
    assert!(
        msg.contains("flight_upload_tables") && msg.contains("cache_enabled"),
        "the refusal must name both knobs so the operator knows which to change; got: {msg}"
    );
}

/// Guard against over-refusal: the default (Upload disabled) still boots with caching.
#[cfg(feature = "arrow")]
#[tokio::test]
async fn cache_with_upload_disabled_still_boots() {
    let (adapter, _counter) = CountingAdapter::new();
    let result = Server::with_flight_service(
        caching_config(),
        cacheable_schema(),
        Arc::new(adapter),
        None,
        None,
    )
    .await;
    assert!(
        result.is_ok(),
        "an empty flight_upload_tables leaves Upload disabled, so caching is safe"
    );
}
