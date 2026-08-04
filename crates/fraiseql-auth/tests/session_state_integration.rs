//! Live-PostgreSQL integration tests for the session-state subsystem (#389).
//!
//! Runs the same policy layer the unit tests exercise in memory — TTL
//! visibility, the summarisation collapse, eviction — against the durable
//! `_system.session_state` table, plus the Postgres-only properties: upsert
//! semantics on the composite PK, collapse atomicity in a transaction, and
//! `init()` idempotency.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: postgres`
//! suite, which binds Postgres and injects `DATABASE_URL`.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `_system.session_state` on setup → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_auth::session_state::{
    MAX_SESSION_VALUE_BYTES, PostgresSessionStateStore, SUMMARY_KEY, SessionState,
    SessionStateBackend, SessionStateEntry, SummarizeFuture, Summarizer,
};
use fraiseql_test_support::try_database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

/// Connect, run `init()` twice (idempotency is part of the contract), and
/// truncate so each test starts clean. Returns `None` (skip) when unconfigured.
async fn fresh() -> Option<(SessionState, PgPool)> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    let store = PostgresSessionStateStore::new(pool.clone());
    store.init().await.expect("first init");
    store.init().await.expect("init is idempotent");
    sqlx::query("TRUNCATE _system.session_state").execute(&pool).await.unwrap();
    Some((SessionState::new(SessionStateBackend::Postgres(store), 3600), pool))
}

struct RollupSummarizer;

impl Summarizer for RollupSummarizer {
    fn summarize(&self, entries: Vec<SessionStateEntry>) -> SummarizeFuture<'_> {
        let keys: Vec<String> = entries.iter().map(|e| e.key.clone()).collect();
        Box::pin(async move { Ok(serde_json::json!({ "rolled_up": keys })) })
    }
}

#[tokio::test]
async fn roundtrip_upsert_and_isolation() {
    let Some((state, _pool)) = fresh().await else {
        eprintln!("SKIP session_state roundtrip: no DATABASE_URL");
        return;
    };
    let (alice, mallory) = (Uuid::new_v4(), Uuid::new_v4());

    state.set(alice, "t1", "context", serde_json::json!({"step": 1})).await.unwrap();
    // Upsert on the composite PK: a second write overwrites, not duplicates.
    state.set(alice, "t1", "context", serde_json::json!({"step": 2})).await.unwrap();

    let entry = state.get(alice, "t1", "context").await.unwrap().expect("present");
    assert_eq!(entry.value, serde_json::json!({"step": 2}), "second write wins");
    assert_eq!(state.list_thread(alice, "t1").await.unwrap().len(), 1, "upsert, no duplicate");

    assert!(
        state.get(mallory, "t1", "context").await.unwrap().is_none(),
        "an entry is only visible to its owning session_id"
    );
}

#[tokio::test]
async fn expired_entries_are_invisible_and_evicted() {
    let Some((state, pool)) = fresh().await else {
        eprintln!("SKIP session_state expiry: no DATABASE_URL");
        return;
    };
    let session = Uuid::new_v4();
    state.set(session, "t1", "live", serde_json::json!(1)).await.unwrap();

    // Backdate one entry into the past — the passage of time, without sleeping.
    sqlx::query(
        "INSERT INTO _system.session_state (session_id, thread_id, key, value, updated_at, \
         expires_at) VALUES ($1, 't1', 'dead', '2', now() - interval '2 hours', now() - \
         interval '1 hour')",
    )
    .bind(session)
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        state.get(session, "t1", "dead").await.unwrap().is_none(),
        "an expired entry is invisible to reads even before the sweep runs"
    );
    assert_eq!(
        state.list_thread(session, "t1").await.unwrap().len(),
        1,
        "list excludes the expired entry"
    );

    assert_eq!(state.evict_expired().await.unwrap(), 1, "the sweep removes exactly it");
    let remaining: i64 =
        sqlx::query("SELECT count(*) AS n FROM _system.session_state WHERE session_id = $1")
            .bind(session)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("n");
    assert_eq!(remaining, 1, "the live row survives the sweep");
}

#[tokio::test]
async fn threshold_collapse_is_atomic_and_durable() {
    let Some((state, pool)) = fresh().await else {
        eprintln!("SKIP session_state collapse: no DATABASE_URL");
        return;
    };
    let state = state.with_summarizer(Arc::new(RollupSummarizer), 2);
    let session = Uuid::new_v4();

    state.set(session, "t1", "k0", serde_json::json!(0)).await.unwrap();
    state.set(session, "t1", "k1", serde_json::json!(1)).await.unwrap();
    state.set(session, "t1", "k2", serde_json::json!(2)).await.unwrap();

    let entries = state.list_thread(session, "t1").await.unwrap();
    assert_eq!(entries.len(), 1, "collapsed to the summary alone: {entries:?}");
    assert_eq!(entries[0].key, SUMMARY_KEY);
    assert_eq!(entries[0].value, serde_json::json!({"rolled_up": ["k0", "k1", "k2"]}));

    // Durable: visible on the raw table, not just through the store.
    let n: i64 = sqlx::query(
        "SELECT count(*) AS n FROM _system.session_state WHERE session_id = $1 AND thread_id = \
         't1'",
    )
    .bind(session)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("n");
    assert_eq!(n, 1, "exactly one physical row after the collapse");
}

#[tokio::test]
async fn size_cap_and_reserved_key_hold_on_postgres_too() {
    let Some((state, _pool)) = fresh().await else {
        eprintln!("SKIP session_state caps: no DATABASE_URL");
        return;
    };
    let session = Uuid::new_v4();

    let big = serde_json::json!("x".repeat(MAX_SESSION_VALUE_BYTES + 1));
    assert!(state.set(session, "t1", "blob", big).await.is_err(), "size cap enforced");
    assert!(
        state.set(session, "t1", SUMMARY_KEY, serde_json::json!("spoof")).await.is_err(),
        "reserved key refused"
    );
    assert!(
        state.list_thread(session, "t1").await.unwrap().is_empty(),
        "nothing was written"
    );
}

#[tokio::test]
async fn expire_thread_scopes_to_one_thread() {
    let Some((state, _pool)) = fresh().await else {
        eprintln!("SKIP session_state expire_thread: no DATABASE_URL");
        return;
    };
    let session = Uuid::new_v4();
    state.set(session, "t1", "a", serde_json::json!(1)).await.unwrap();
    state.set(session, "t2", "b", serde_json::json!(2)).await.unwrap();

    state.expire_thread(session, "t1").await.unwrap();
    assert!(state.list_thread(session, "t1").await.unwrap().is_empty());
    assert_eq!(state.list_thread(session, "t2").await.unwrap().len(), 1);
}
