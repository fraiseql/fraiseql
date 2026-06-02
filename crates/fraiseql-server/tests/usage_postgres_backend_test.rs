//! PostgreSQL persistence backend tests for `UsageAggregator`.
//!
//! Requires a live PostgreSQL instance (spun up automatically via testcontainers).
//! No environment variables or external infrastructure needed.
//!
//! # Running
//!
//! ```bash
//! cargo test --test usage_postgres_backend_test
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

use std::sync::Arc;

use fraiseql_server::usage::{
    aggregator::{PostgresBackend, UsageAggregator},
    events::MutationAuditEvent,
};
use sqlx::PgPool;

// ── Harness setup ────────────────────────────────────────────────────────────

/// Connect to the harness Postgres (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature) and DROP the usage-counter table so the
/// `PostgresBackend::new` CREATE-IF-NOT-EXISTS recreates it empty for each test. The
/// server suite runs these with --test-threads=1, so the shared bound database gives
/// per-test isolation without per-test DBs. Returns the pool plus the service guard.
async fn setup_pg() -> (PgPool, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS fraiseql_usage_counters CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    (pool, svc)
}

fn event(tenant: &str, period: &str, entity: &str) -> MutationAuditEvent {
    MutationAuditEvent::new(format!("create_{entity}"), entity, "create", tenant, period)
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// `PostgresBackend` creates its schema and persists counters across aggregator instances.
#[tokio::test]
async fn test_postgres_backend_flush_and_load_round_trip() {
    let (pool, _container) = setup_pg().await;
    let backend = Arc::new(PostgresBackend::new(pool.clone()).await.unwrap());

    // Record and flush
    let agg = UsageAggregator::new_with_backend(backend.clone());
    agg.record(&event("acme", "2026-05", "User"));
    agg.record(&event("acme", "2026-05", "User"));
    agg.record(&event("acme", "2026-05", "Order"));
    agg.flush_to_backend().await.unwrap();

    // New aggregator (simulates restart) loads persisted state
    let new_agg = UsageAggregator::new_with_backend(backend.clone());
    new_agg.load_from_backend().await.unwrap();

    assert_eq!(new_agg.query("acme", "2026-05").mutations["User"], 2);
    assert_eq!(new_agg.query("acme", "2026-05").mutations["Order"], 1);
}

/// Flush is idempotent: flushing the same counts twice does not double them.
#[tokio::test]
async fn test_postgres_backend_flush_is_idempotent() {
    let (pool, _container) = setup_pg().await;
    let backend = Arc::new(PostgresBackend::new(pool.clone()).await.unwrap());

    let agg = UsageAggregator::new_with_backend(backend.clone());
    agg.record(&event("t1", "2026-05", "Widget"));
    agg.record(&event("t1", "2026-05", "Widget"));
    agg.flush_to_backend().await.unwrap();
    agg.flush_to_backend().await.unwrap(); // second flush — same count, not doubled

    let new_agg = UsageAggregator::new_with_backend(backend.clone());
    new_agg.load_from_backend().await.unwrap();
    assert_eq!(new_agg.query("t1", "2026-05").mutations["Widget"], 2);
}

/// Load merges persisted state with in-flight events (no lost events on restart).
#[tokio::test]
async fn test_postgres_backend_load_merges_with_inflight() {
    let (pool, _container) = setup_pg().await;
    let backend = Arc::new(PostgresBackend::new(pool.clone()).await.unwrap());

    // First aggregator: record 3, flush
    let agg = UsageAggregator::new_with_backend(backend.clone());
    for _ in 0..3 {
        agg.record(&event("tenant", "2026-05", "Thing"));
    }
    agg.flush_to_backend().await.unwrap();

    // Second aggregator: record 2 in-flight, then load → total must be 5
    let new_agg = UsageAggregator::new_with_backend(backend.clone());
    new_agg.record(&event("tenant", "2026-05", "Thing"));
    new_agg.record(&event("tenant", "2026-05", "Thing"));
    new_agg.load_from_backend().await.unwrap();

    assert_eq!(new_agg.query("tenant", "2026-05").mutations["Thing"], 5);
}

/// Multiple tenants are stored independently.
#[tokio::test]
async fn test_postgres_backend_tenant_isolation() {
    let (pool, _container) = setup_pg().await;
    let backend = Arc::new(PostgresBackend::new(pool.clone()).await.unwrap());

    let agg = UsageAggregator::new_with_backend(backend.clone());
    agg.record(&event("tenant_a", "2026-05", "User"));
    agg.record(&event("tenant_b", "2026-05", "User"));
    agg.record(&event("tenant_b", "2026-05", "User"));
    agg.flush_to_backend().await.unwrap();

    let new_agg = UsageAggregator::new_with_backend(backend.clone());
    new_agg.load_from_backend().await.unwrap();

    assert_eq!(new_agg.query("tenant_a", "2026-05").mutations["User"], 1);
    assert_eq!(new_agg.query("tenant_b", "2026-05").mutations["User"], 2);
}

/// Empty backend load is a no-op (first boot).
#[tokio::test]
async fn test_postgres_backend_empty_load_is_noop() {
    let (pool, _container) = setup_pg().await;
    let backend = Arc::new(PostgresBackend::new(pool.clone()).await.unwrap());

    let agg = UsageAggregator::new_with_backend(backend);
    agg.load_from_backend().await.unwrap(); // should not error

    assert_eq!(agg.entry_count(), 0);
}
