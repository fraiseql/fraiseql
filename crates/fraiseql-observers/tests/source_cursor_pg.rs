//! Live-PostgreSQL tests for the #573 source coordination primitives: the durable
//! [`PostgresSourceCursorStore`] and the single-firing [`LeaseGuardedRunner`].
//!
//! Proves the parts only a real database can:
//!
//! * **compare-and-swap advance** — a first advance inserts at version 1; each subsequent advance
//!   from the current snapshot bumps the version; a *stale* advance (wrong version) is rejected and
//!   never regresses the watermark;
//! * **transactional advance** — `advance_in_tx` commits or rolls back atomically with the caller's
//!   transaction (Model A's no-reprocess guarantee);
//! * **deny-by-default RLS** — a NOBYPASSRLS role with no `fraiseql.tenant_id` GUC reads zero
//!   cursor rows (mirrors `rls_isolation.rs`);
//! * **single-firing** — two runners (two replicas) on one source: one runs, the other skips,
//!   counting `skips_not_leader`.
//!
//! Self-skips when no Postgres is available (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: observers` suite.
//! Each test uses a fresh random source name so runs never collide on cursor rows.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates a shared subordinate role → run `--test-threads=1`.
#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used)] // Reason: integration test file — panics are acceptable
#![allow(clippy::panic)] // Reason: integration test file
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::str::FromStr;

use fraiseql_observers::{
    CursorSnapshot, LeaseGuardedRunner, PostgresSourceCursorStore, RunOutcome, SourceCursorStore,
};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// NOBYPASSRLS reader — the RLS-subject role the deny-by-default policy must bind.
const RLS_ROLE: &str = "fraiseql_source_cursor_rls_reader";
const ROLE_PASSWORD: &str = "source_cursor_rls_test_password";

/// Connect a pool to the harness-provided Postgres, or `None` to skip cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// A pool connected as `role` — same host/port/database as the given URL, with the
/// credentials swapped (mirrors `rls_isolation.rs::role_pool`).
async fn role_pool(url: &str, role: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(url)
        .expect("parse DATABASE_URL")
        .username(role)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .unwrap_or_else(|e| panic!("connect as {role}: {e}"))
}

/// A fresh source name so parallel/repeat runs never share a cursor row.
fn unique_source() -> String {
    format!("test-src-{}", uuid::Uuid::new_v4())
}

#[tokio::test]
async fn load_missing_is_the_empty_snapshot() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP load_missing_is_the_empty_snapshot: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();

    let snap = store.load(&unique_source()).await.unwrap();
    assert_eq!(snap, CursorSnapshot::empty(), "an unseen source loads as empty");
    assert!(snap.is_unset());
}

#[tokio::test]
async fn advances_are_monotonic_and_readable() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP advances_are_monotonic_and_readable: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();
    let source = unique_source();

    // First advance inserts at version 1.
    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!({"last_uid": 100})).await.unwrap());
    let v1 = store.load(&source).await.unwrap();
    assert_eq!(v1.value, Some(json!({"last_uid": 100})));
    assert_eq!(v1.version, 1);

    // A second advance from the current snapshot bumps to version 2.
    assert!(store.advance(&source, &v1, json!({"last_uid": 250})).await.unwrap());
    let v2 = store.load(&source).await.unwrap();
    assert_eq!(v2.value, Some(json!({"last_uid": 250})));
    assert_eq!(v2.version, 2);
}

#[tokio::test]
async fn stale_advance_is_rejected_and_does_not_regress() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP stale_advance_is_rejected_and_does_not_regress: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();
    let source = unique_source();

    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!("first")).await.unwrap()); // → v1

    // A stale writer still holding the empty (v0) snapshot: the first-write INSERT
    // loses on ON CONFLICT, so the advance is rejected.
    assert!(
        !store.advance(&source, &empty, json!("stale-from-v0")).await.unwrap(),
        "a v0 advance against an existing row must be rejected"
    );

    let v1 = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &v1, json!("second")).await.unwrap()); // → v2

    // A stale writer still holding the v1 snapshot: the UPDATE matches no row.
    assert!(
        !store.advance(&source, &v1, json!("stale-from-v1")).await.unwrap(),
        "a v1 advance against a v2 row must be rejected"
    );

    // The watermark is exactly the last winning advance — never regressed.
    let current = store.load(&source).await.unwrap();
    assert_eq!(current.value, Some(json!("second")));
    assert_eq!(current.version, 2);
}

#[tokio::test]
async fn advance_in_tx_is_atomic_with_the_caller_transaction() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP advance_in_tx_is_atomic_with_the_caller_transaction: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool.clone());
    store.init().await.unwrap();
    let source = unique_source();
    let empty = store.load(&source).await.unwrap();

    // A rolled-back transaction leaves no watermark (re-run is safe).
    let mut tx = pool.begin().await.unwrap();
    assert!(store.advance_in_tx(&mut tx, &source, &empty, json!({"n": 5})).await.unwrap());
    tx.rollback().await.unwrap();
    assert_eq!(
        store.load(&source).await.unwrap(),
        CursorSnapshot::empty(),
        "a rolled-back advance must leave no watermark"
    );

    // A committed transaction persists the watermark.
    let mut tx = pool.begin().await.unwrap();
    assert!(store.advance_in_tx(&mut tx, &source, &empty, json!({"n": 9})).await.unwrap());
    tx.commit().await.unwrap();
    let snap = store.load(&source).await.unwrap();
    assert_eq!(snap.value, Some(json!({"n": 9})));
    assert_eq!(snap.version, 1);
}

#[tokio::test]
async fn rls_denies_reads_to_a_role_without_the_tenant_guc() {
    let Some((admin, svc)) = connect_pool().await else {
        eprintln!("SKIP rls_denies_reads_to_a_role_without_the_tenant_guc: no postgres");
        return;
    };

    // Create the NOBYPASSRLS reader (idempotent), re-asserting its attributes in
    // case it survived an earlier run.
    sqlx::query(&format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{RLS_ROLE}') THEN
                 CREATE ROLE {RLS_ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
             END IF;
         END $$"
    ))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!(
        "ALTER ROLE {RLS_ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"
    ))
    .execute(&admin)
    .await
    .unwrap();

    let store = PostgresSourceCursorStore::new(admin.clone());
    store.init().await.unwrap();
    sqlx::query(&format!("GRANT SELECT ON _fraiseql_source_cursor TO {RLS_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();

    // Seed a (global, tenant_id NULL) cursor as the superuser.
    let source = unique_source();
    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!({"secret": true})).await.unwrap());

    // The NOBYPASSRLS reader, with no tenant GUC, sees zero rows (deny-by-default).
    let reader = role_pool(svc.url(), RLS_ROLE).await;
    let (denied,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_source_cursor WHERE source_name = $1")
            .bind(&source)
            .fetch_one(&reader)
            .await
            .unwrap();
    assert_eq!(denied, 0, "a NOBYPASSRLS role with no tenant GUC must read zero cursor rows");

    // The superuser (bypasses RLS) still sees the row — proving the deny is RLS,
    // not a missing row.
    let (seen,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM _fraiseql_source_cursor WHERE source_name = $1")
            .bind(&source)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(seen, 1, "the trusted superuser reads the seeded cursor row");
}

#[tokio::test]
async fn two_runners_on_one_source_fire_once() {
    let Some((pool_a, svc)) = connect_pool().await else {
        eprintln!("SKIP two_runners_on_one_source_fire_once: no postgres");
        return;
    };
    // A second pool models a second replica with its own sessions.
    let pool_b = PgPool::connect(svc.url()).await.unwrap();
    let source = unique_source();

    let runner_a = LeaseGuardedRunner::postgres(pool_a, source.clone());
    let runner_b = LeaseGuardedRunner::postgres(pool_b, source);

    // Replica A grabs the lease and holds it until told to release; replica B must
    // attempt while A holds it, so the barrier makes the race deterministic.
    let (holding_tx, holding_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let a = tokio::spawn(async move {
        runner_a
            .run(|| async move {
                holding_tx.send(()).unwrap();
                release_rx.await.unwrap();
                "a-ran"
            })
            .await
    });

    // Wait until A holds the lease, then B attempts and must skip.
    holding_rx.await.unwrap();
    let b_outcome = runner_b.run(|| async { "b-ran" }).await.unwrap();
    assert_eq!(b_outcome, RunOutcome::SkippedNotLeader, "the second replica must skip");
    assert_eq!(runner_b.skips_not_leader(), 1);

    // Let A finish; it ran exactly once.
    release_tx.send(()).unwrap();
    let a_outcome = a.await.unwrap().unwrap();
    assert_eq!(a_outcome, RunOutcome::Ran("a-ran"));
}
