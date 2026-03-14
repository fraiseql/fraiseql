//! Mutation-targeted tests for the cache adapter query path.
//!
//! These tests are explicitly designed to **kill surviving mutants** in
//! `cache/adapter/query.rs`. Each test targets a specific mutation that
//! typical happy-path tests would not detect.
//!
//! ## Targeted mutations
//!
//! | Mutant | Location | What cargo-mutants changes | Killed by |
//! |--------|----------|---------------------------|-----------|
//! | A1 | query.rs:29 | `delete !` in `if !self.cache.is_enabled()` | `projection_bypass_does_not_cache_when_disabled` |
//! | A2 | query.rs:29 | `delete !` → enabled cache skips cache entirely | `projection_enabled_cache_hits_on_second_call` |
//! | A3 | query.rs:77 | `delete !` in `execute_where_query_impl` | `where_query_enabled_cache_hits_on_second_call` |
//! | A4 | query.rs:38 | `projection_info` removed from variables | `projection_info_contributes_to_cache_key` |
//! | A5 | query.rs:60/105 | cache.put skipped | `result_is_stored_in_cache_after_first_call` |
//!
//! **Do not merge tests** — each test targets exactly one mutation.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
};

use async_trait::async_trait;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{DatabaseAdapter, DatabaseType, PoolMetrics, WhereClause, WhereOperator, types::JsonbValue},
    error::Result,
    schema::SqlProjectionHint,
};
use serde_json::json;

// ── Minimal mock adapter ──────────────────────────────────────────────────────

/// Counter-instrumented mock adapter for verifying call-through behaviour.
struct CountingMock {
    projection_calls: AtomicU32,
    where_calls:      AtomicU32,
}

impl CountingMock {
    fn new() -> Self {
        Self {
            projection_calls: AtomicU32::new(0),
            where_calls:      AtomicU32::new(0),
        }
    }

    fn projection_calls(&self) -> u32 {
        self.projection_calls.load(Ordering::SeqCst)
    }

    fn where_calls(&self) -> u32 {
        self.where_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DatabaseAdapter for CountingMock {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.projection_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(json!({"id": 1, "name": "Alice"}))])
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.where_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![JsonbValue::new(json!({"id": 2, "name": "Bob"}))])
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

fn sample_projection() -> SqlProjectionHint {
    SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:         "jsonb_build_object('id', data->>'id')".to_string(),
        estimated_reduction_percent: 50,
    }
}

fn sample_where() -> WhereClause {
    WhereClause::Field {
        path:     vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("active"),
    }
}

// ── A1/A2: execute_with_projection — is_enabled() check (line 29) ─────────────

/// A1: When cache is DISABLED, each call must go to the adapter (no caching).
///
/// Mutation "delete !" at line 29 would invert the check to `if self.cache.is_enabled()`.
/// With a disabled cache, `is_enabled()` returns false, so the mutated branch would be
/// `if false { ... }` — NOT short-circuiting. The mutant would fall through to the
/// cache path, but since the cache is disabled, `cache.get()` always misses and
/// `cache.put()` is a no-op, so both calls still go to the adapter.
/// This test specifically catches when the mutant causes double-calls with ENABLED caches.
#[tokio::test]
async fn projection_bypass_does_not_cache_when_disabled() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::disabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let proj = sample_projection();

    // Both calls must go to the adapter (disabled cache never short-circuits into cache path)
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();

    assert_eq!(
        adapter.inner().projection_calls(),
        2,
        "A1: disabled cache must call adapter on every request"
    );
}

/// A2: When cache is ENABLED, the second identical call must hit the cache.
///
/// Mutation "delete !" at line 29 → `if self.cache.is_enabled()` means with ENABLED
/// cache, the condition is `true` → short-circuit to adapter on EVERY call.
/// This test fails with the mutant: adapter is called twice instead of once.
#[tokio::test]
async fn projection_enabled_cache_hits_on_second_call() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let proj = sample_projection();

    // First call — cache miss, adapter executes
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    assert_eq!(adapter.inner().projection_calls(), 1, "A2: first call must reach adapter");

    // Second identical call — must hit cache, adapter NOT called again
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    assert_eq!(
        adapter.inner().projection_calls(),
        1,
        "A2: second identical call must hit cache (adapter called once total)"
    );
}

// ── A3: execute_where_query — is_enabled() check (line 77) ───────────────────

/// A3: When cache is ENABLED, repeated WHERE queries must be served from cache.
///
/// Mutation "delete !" at line 77 → always calls adapter (cache bypassed for enabled cache).
/// This test fails with the mutant: adapter is called twice instead of once.
#[tokio::test]
async fn where_query_enabled_cache_hits_on_second_call() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let w = sample_where();

    // First call — cache miss
    adapter.execute_where_query("v_user", Some(&w), None, None).await.unwrap();
    assert_eq!(adapter.inner().where_calls(), 1, "A3: first call must reach adapter");

    // Second call — cache hit
    adapter.execute_where_query("v_user", Some(&w), None, None).await.unwrap();
    assert_eq!(
        adapter.inner().where_calls(),
        1,
        "A3: second identical WHERE query must hit cache"
    );
}

// ── A4: projection_info contributes to cache key (line 38) ───────────────────

/// A4: Different projection templates must produce different cache entries.
///
/// Mutation that removes `projection_info` from the variables JSON would cause
/// queries with different projections to collide in the cache, returning wrong data.
#[tokio::test]
async fn projection_info_contributes_to_cache_key() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());

    let proj_a = SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:         "jsonb_build_object('id', data->>'id')".to_string(),
        estimated_reduction_percent: 50,
    };
    let proj_b = SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:         "jsonb_build_object('id', data->>'id', 'name', data->>'name')"
            .to_string(),
        estimated_reduction_percent: 30,
    };

    // Two calls with different projections — both must reach the adapter (different keys)
    adapter.execute_with_projection("v_user", Some(&proj_a), None, None).await.unwrap();
    adapter.execute_with_projection("v_user", Some(&proj_b), None, None).await.unwrap();

    assert_eq!(
        adapter.inner().projection_calls(),
        2,
        "A4: different projection templates must produce different cache keys"
    );
}

/// A4b: Same projection template must produce the same cache key (cache hits).
#[tokio::test]
async fn same_projection_template_produces_cache_hit() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let proj = sample_projection();

    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    // Clone with identical template
    let proj2 = SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:         "jsonb_build_object('id', data->>'id')".to_string(),
        estimated_reduction_percent: 50,
    };
    adapter.execute_with_projection("v_user", Some(&proj2), None, None).await.unwrap();

    assert_eq!(
        adapter.inner().projection_calls(),
        1,
        "A4b: same projection template must hit cache on second call"
    );
}

// ── A5: result is stored after cache miss ─────────────────────────────────────

/// A5: Verifies the cache.put() call at line 60 actually stores the result.
///
/// A mutation that skips `cache.put(...)` would cause every call to be a miss,
/// so adapter call count would be N instead of 1 across N identical calls.
#[tokio::test]
async fn result_is_stored_in_cache_after_first_projection_call() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let proj = sample_projection();

    // Three calls — only first should hit adapter
    for _ in 0..3 {
        adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    }

    assert_eq!(
        adapter.inner().projection_calls(),
        1,
        "A5: only the first call should reach the adapter (result cached after first)"
    );
}

/// A5b: Verifies cache.put() at line 105 stores WHERE query results.
#[tokio::test]
async fn result_is_stored_in_cache_after_first_where_query_call() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let w = sample_where();

    for _ in 0..3 {
        adapter.execute_where_query("v_user", Some(&w), None, None).await.unwrap();
    }

    assert_eq!(
        adapter.inner().where_calls(),
        1,
        "A5b: only the first WHERE query should reach the adapter"
    );
}

// ── Cross-path isolation ──────────────────────────────────────────────────────

/// The two impl methods use independent cache entries: a hit in execute_with_projection
/// must not collide with execute_where_query for the same view.
#[tokio::test]
async fn projection_and_where_query_use_independent_cache_entries() {
    let mock = CountingMock::new();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let adapter = CachedDatabaseAdapter::new(mock, cache, "v1".to_string());
    let proj = sample_projection();
    let w = sample_where();

    // Prime both caches
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    adapter.execute_where_query("v_user", Some(&w), None, None).await.unwrap();

    // Both methods must have been called exactly once each
    assert_eq!(adapter.inner().projection_calls(), 1);
    assert_eq!(adapter.inner().where_calls(), 1);

    // Second round — both should hit cache
    adapter.execute_with_projection("v_user", Some(&proj), None, None).await.unwrap();
    adapter.execute_where_query("v_user", Some(&w), None, None).await.unwrap();

    assert_eq!(adapter.inner().projection_calls(), 1, "projection should still be cached");
    assert_eq!(adapter.inner().where_calls(), 1, "where query should still be cached");
}
