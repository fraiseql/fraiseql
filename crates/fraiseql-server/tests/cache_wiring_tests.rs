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
        _order_by: Option<&[OrderByClause]>,        _session_vars: &[(&str, &str)],

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
        _order_by: Option<&[OrderByClause]>,        _session_vars: &[(&str, &str)],

    ) -> FraiseQLResult<Vec<JsonbValue>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "test"}),
        )])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
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
        _params: &[serde_json::Value],        _session_vars: &[(&str, &str)],

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
    let _ = adapter.execute_where_query("v_item", None, None, None, None, &[]).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "first call must reach the underlying adapter"
    );

    // Second identical call — cache hit; underlying adapter NOT called again.
    let _ = adapter.execute_where_query("v_item", None, None, None, None, &[]).await.unwrap();
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
    let _ = adapter.execute_where_query("v_item", None, None, None, None, &[]).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "first call must reach the underlying adapter"
    );

    // Second call — cache disabled so adapter is hit again.
    let _ = adapter.execute_where_query("v_item", None, None, None, None, &[]).await.unwrap();
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
        ..ServerConfig::default()
    };

    let _server = Server::new(config, schema, Arc::new(adapter), None)
        .await
        .expect("Server::new must succeed when cache_enabled = false");
}
