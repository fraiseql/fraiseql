//! Live-PostgreSQL characterization of [`CheckpointLease::postgres`] — the
//! session-advisory-lock lease that #573's single-firing runner will wrap.
//!
//! The lease (`listener/lease.rs`) is built but wired nowhere and, until now,
//! untested. Before `LeaseGuardedRunner` (Phase 01) depends on it, these tests
//! pin the two properties the source scheduler leans on:
//!
//! * **Mutual exclusion** — two leases on one key, from two independent pools (two replicas), never
//!   both hold: the first `acquire()` wins, the second is refused until the holder `release()`s.
//!   This is what makes a source fire on exactly one replica.
//! * **Crash safety, and its exact boundary** — a `pg_try_advisory_lock` is a *session* lock,
//!   released only when the session ends or `pg_advisory_unlock` runs. Merely dropping the lease
//!   returns its `PoolConnection` to the pool **still holding the lock**; the lock is freed only
//!   when the connection actually dies (process crash / `pool.close()`). So the happy path *must*
//!   call `release()` explicitly — drop alone is not enough. Phase 01's `LeaseGuardedRunner` relies
//!   on this exact boundary; it is asserted here.
//!
//! Self-skips when no Postgres is available (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: postgres`
//! suite. Advisory locks need no tables or migrations; each test uses a fresh
//! random lock key so concurrent runs never collide.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** independent lock keys → safe to run in parallel.
#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used)] // Reason: integration test file — panics are acceptable
#![allow(clippy::panic)] // Reason: integration test file
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::time::Duration;

use fraiseql_observers::listener::CheckpointLease;
use sqlx::{PgPool, postgres::PgPoolOptions};

/// Connect a pool to the harness-provided Postgres (Dagger-bound in CI; a local
/// spawn under the `local-testcontainers` feature). Returns `None` — so the test
/// skips cleanly — when no service is available. The [`Service`] guard is
/// returned so the caller keeps any spawned container alive for the test.
///
/// [`Service`]: fraiseql_test_support::Service
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// A fresh random advisory-lock key, so parallel runs never contend on the same
/// lock by accident.
fn random_lock_key() -> i64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    i64::from_le_bytes(bytes[..8].try_into().unwrap())
}

/// Poll `acquire()` until it succeeds or the attempts run out. Used only where a
/// prior connection is being torn down asynchronously (pool close), so the lock
/// release races the next acquire; the mutual-exclusion assertions never retry.
async fn acquire_within(lease: &CheckpointLease, attempts: u32) -> bool {
    for _ in 0..attempts {
        if lease.acquire().await.unwrap() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Two replicas, one lock: the first holder wins and the second is refused until
/// the first explicitly releases. This is the single-firing guarantee the source
/// scheduler is built on.
#[tokio::test]
async fn two_leases_on_one_key_are_mutually_exclusive() {
    let Some((pool_a, svc)) = connect_pool().await else {
        eprintln!(
            "SKIP two_leases_on_one_key_are_mutually_exclusive: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };
    // A second, independent pool models a second replica with its own sessions.
    // `svc` is kept bound for the whole test so its liveness guard survives.
    let pool_b = PgPool::connect(svc.url()).await.unwrap();
    let key = random_lock_key();

    let lease_a = CheckpointLease::postgres(pool_a, "replica-a".to_string(), key);
    let lease_b = CheckpointLease::postgres(pool_b, "replica-b".to_string(), key);

    // Replica A wins the lock.
    assert!(lease_a.acquire().await.unwrap(), "first acquirer must win the lock");
    // Replica B is refused while A holds it — exactly one replica fires.
    assert!(
        !lease_b.acquire().await.unwrap(),
        "a second replica must be refused the same lock"
    );

    // A re-acquire by the holder is idempotent (still held, still true).
    assert!(lease_a.acquire().await.unwrap(), "holder re-acquire is idempotent");

    // Once A releases, B can take over.
    lease_a.release().await.unwrap();
    assert!(
        lease_b.acquire().await.unwrap(),
        "released lock becomes available to the other replica"
    );

    lease_b.release().await.unwrap();
}

/// The crash-safety boundary, pinned exactly: dropping the lease does **not**
/// free the lock (its connection returns to the pool still holding it), but the
/// connection actually dying — `pool.close()`, standing in for a process crash —
/// does. The happy path must therefore `release()` explicitly.
#[tokio::test]
async fn dropped_lease_holds_the_lock_until_the_connection_dies() {
    // Only the service (URL + liveness guard) is needed; the probe pool is
    // discarded — the holder gets a dedicated single-connection pool below.
    let Some((_, svc)) = connect_pool().await else {
        eprintln!(
            "SKIP dropped_lease_holds_the_lock_until_the_connection_dies: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };
    // The holder gets a single-connection pool so closing it deterministically
    // ends the exact session that owns the lock.
    let holder_pool = PgPoolOptions::new().max_connections(1).connect(svc.url()).await.unwrap();
    // A separate pool for the contender (a second replica).
    let contender_pool = PgPool::connect(svc.url()).await.unwrap();
    let key = random_lock_key();

    let contender = CheckpointLease::postgres(contender_pool, "contender".to_string(), key);

    // Holder acquires; contender is refused.
    let holder = CheckpointLease::postgres(holder_pool.clone(), "holder".to_string(), key);
    assert!(holder.acquire().await.unwrap(), "holder acquires the lock");
    assert!(!contender.acquire().await.unwrap(), "contender is refused while held");

    // Drop the lease WITHOUT releasing. The pooled connection returns to
    // `holder_pool` alive, so the session — and the lock — persist.
    drop(holder);
    assert!(
        !contender.acquire().await.unwrap(),
        "dropping the lease alone must NOT release the lock: the connection is still alive in the pool"
    );

    // Kill the connection (crash simulation). Now the session ends and the lock
    // is released; the contender takes over. Retry to absorb the async teardown.
    holder_pool.close().await;
    assert!(
        acquire_within(&contender, 40).await,
        "a dead holder connection releases the lock (crash safety)"
    );

    contender.release().await.unwrap();
}
