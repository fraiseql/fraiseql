//! Live-PostgreSQL tests for cross-replica SAML replay protection (#949).
//!
//! The assertion that matters here is the one a single-process suite structurally cannot
//! make, which is why the gap survived: **an assertion accepted by one server must be
//! refused by a second, independently-constructed one over the same database.** With the
//! in-process `DashMap` the second store had never seen the ID, so it accepted — the
//! signature is valid, and the replay cache was the only thing that would have stopped it.
//!
//! Two `PgSamlReplayStore` values built from two separate pools stand in for two replicas.
//! They share nothing but the database, which is exactly the property under test; going
//! through the full HTTP surface would add a signed-assertion fixture and two bound ports
//! without changing what is being asserted.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: saml` suite — not the
//! `postgres` one, which builds `fraiseql-auth` with default features and so cannot see
//! `PgSamlReplayStore` at all. `required-features = ["auth-saml"]` in `Cargo.toml` is what
//! holds that: cargo refuses the target by name in a leg that forgets the feature, and
//! `tools/check-suite-coverage.py` reads the same key and reports an ORPHAN unless some
//! leg enables it.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` + `auth-saml` ·
//! **Parallelism:** truncates `core.tb_saml_replay` on setup → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use chrono::{Duration, Utc};
use fraiseql_auth::{PgSamlReplayStore, SamlReplayCache, SamlReplayStore};
use fraiseql_test_support::try_database_url;
use sqlx::postgres::PgPoolOptions;

/// Build `n` stores, each over its **own** pool — two replicas, one database.
async fn replicas(n: usize) -> Option<Vec<PgSamlReplayStore>> {
    let url = try_database_url()?;
    let mut stores = Vec::with_capacity(n);
    for _ in 0..n {
        let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let store = PgSamlReplayStore::new(pool);
        store.init().await.unwrap();
        stores.push(store);
    }
    sqlx::query("TRUNCATE core.tb_saml_replay")
        .execute(&PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap())
        .await
        .unwrap();
    Some(stores)
}

#[tokio::test]
async fn an_assertion_accepted_by_one_replica_is_refused_by_another() {
    let Some(stores) = replicas(2).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let (first, second) = (&stores[0], &stores[1]);

    let now = Utc::now();
    let expires = now + Duration::minutes(5);

    assert!(
        first.check_and_record("assertion-1", expires, now).await.unwrap(),
        "the first presentation is fresh"
    );
    assert!(
        !second.check_and_record("assertion-1", expires, now).await.unwrap(),
        "a second replica must refuse an assertion the first already consumed — this is \
         the whole of #949, and the in-process cache accepted it"
    );
}

#[tokio::test]
async fn the_in_process_cache_does_not_hold_across_instances() {
    // The negative control. Without it, the test above could pass for the wrong reason —
    // e.g. if both "replicas" shared one store — and would no longer be evidence that the
    // Postgres store is what closes the gap.
    let one = SamlReplayCache::new();
    let two = SamlReplayCache::new();

    let now = Utc::now();
    let expires = now + Duration::minutes(5);

    assert!(one.check_and_record("assertion-1", expires, now).await.unwrap());
    assert!(
        two.check_and_record("assertion-1", expires, now).await.unwrap(),
        "an in-process cache cannot see another instance's consumption — the defect"
    );
}

#[tokio::test]
async fn a_replica_refuses_an_assertion_it_consumed_itself() {
    let Some(stores) = replicas(1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let store = &stores[0];

    let now = Utc::now();
    let expires = now + Duration::minutes(5);

    assert!(store.check_and_record("assertion-2", expires, now).await.unwrap());
    assert!(
        !store.check_and_record("assertion-2", expires, now).await.unwrap(),
        "single-node replay protection must not regress"
    );
}

#[tokio::test]
async fn an_id_becomes_reusable_once_its_window_has_closed() {
    let Some(stores) = replicas(1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let store = &stores[0];

    let then = Utc::now() - Duration::hours(2);
    assert!(
        store
            .check_and_record("assertion-3", then + Duration::minutes(5), then)
            .await
            .unwrap()
    );

    // Past its `NotOnOrAfter` the assertion is refused by the signature time-check, so
    // treating the ID as fresh again cannot enable a replay — and it stops a stale row
    // from rejecting a legitimately new assertion that happens to reuse the ID.
    let now = Utc::now();
    assert!(
        store
            .check_and_record("assertion-3", now + Duration::minutes(5), now)
            .await
            .unwrap(),
        "an expired entry must not permanently burn its assertion ID"
    );
}

#[tokio::test]
async fn the_sweep_removes_closed_windows_and_keeps_live_ones() {
    let Some(stores) = replicas(1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let store = &stores[0];

    let then = Utc::now() - Duration::hours(2);
    assert!(
        store
            .check_and_record("stale", then + Duration::minutes(5), then)
            .await
            .unwrap()
    );

    let now = Utc::now();
    assert!(store.check_and_record("live", now + Duration::minutes(5), now).await.unwrap());

    assert_eq!(store.sweep_expired(now).await.unwrap(), 1, "only the closed window is swept");
    assert!(
        !store.check_and_record("live", now + Duration::minutes(5), now).await.unwrap(),
        "a live entry must survive the sweep and still refuse a replay"
    );
}

#[tokio::test]
async fn distributed_posture_is_reported_honestly() {
    // The server refuses a multi-replica posture on a single-process store; that check is
    // only as good as what the stores report about themselves.
    assert!(!SamlReplayCache::new().is_distributed());

    let Some(stores) = replicas(1).await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    assert!(stores[0].is_distributed());
}
