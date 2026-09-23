//! Tests for the Postgres-backed function-dispatch DLQ store (#598).
//!
//! These are the durability flip of the phase-00 in-memory-loss pin
//! (`observers::runtime::tests::function_dlq::pin_598_in_memory_dlq_loses_function_entries_on_restart`):
//! where the in-memory store loses a dead-lettered dispatch when the process
//! restarts, this store persists it. A "restart" is modelled by dropping the store
//! (and its pool) and constructing a fresh one against the same database.
//!
//! **Each test owns its own copy of the table.** The store names
//! `_fraiseql_function_dlq` unqualified, so a pool whose `search_path` names a
//! per-test schema gets a private table under the same name and the store needs no
//! test hook to say so.
//!
//! These tests used to share one table in `public` and `TRUNCATE` it on entry. All
//! three then failed whenever they ran together — deterministically, three of three
//! on every run, not as an intermittent flake: each truncated the rows the others
//! had just written, and each asserts an absolute row count. Two of them cannot be
//! written to tolerate a neighbour's rows even in principle, because the quantity
//! under test *is* the whole table's contents: `get_pending_functions` has no
//! per-caller filter, and the capacity cap is enforced against the table's total
//! count. The module doc asked for `--test-threads=1` instead, which is a note
//! asking every future runner of the suite to know something, in place of a suite
//! that is correct.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code — fail-loud.
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use fraiseql_observers::{DeadLetterQueue, DispatchSource, FunctionDispatchRecord};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

use super::PgFunctionDlq;

/// A pool that resolves unqualified names in `schema` first.
///
/// Built from the URL rather than from an existing pool because the `search_path` is a
/// connection startup parameter: it has to be set when the connection is made, so the
/// "restart" test can rebuild a pool that still sees the same private table.
async fn isolated_pool(url: &str, schema: &str) -> PgPool {
    let options: PgConnectOptions = url.parse().unwrap();
    // `public` stays on the path behind the private schema: the private table shadows
    // any same-named one in `public`, and anything else resolves as it always did.
    let options = options.options([("search_path", format!("{schema},public"))]);
    PgPoolOptions::new().connect_with(options).await.unwrap()
}

/// Connect to the harness-provided Postgres (Dagger-bound in CI; a local spawn with
/// the `local-testcontainers` feature), with `schema` created and on the search path.
/// Returns the pool plus the service guard. `None` when no service is available so the
/// test skips cleanly.
async fn connect_pool(schema: &str) -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    // The schema itself is created on a default connection — it cannot be created
    // through a `search_path` that names it before it exists.
    let admin = PgPool::connect(svc.url()).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    Some((isolated_pool(svc.url(), schema).await, svc))
}

/// A dead-letter record with a distinctive error message, for assertions.
fn record(error: &str) -> FunctionDispatchRecord {
    FunctionDispatchRecord::new(
        DispatchSource::AfterMutation,
        "notify_ops",
        "after:mutation:createOrder",
        "idem-tok-abc123",
        serde_json::json!({ "order_id": 42, "total": "19.99" }),
        error,
        3,
    )
}

/// Start from an empty table. Safe now that the table is the test's own — this is the
/// statement that used to destroy the other two tests' fixtures.
async fn truncate(pool: &PgPool) {
    sqlx::query("TRUNCATE _fraiseql_function_dlq").execute(pool).await.unwrap();
}

#[tokio::test]
async fn dead_lettered_dispatch_survives_a_restart() {
    const SCHEMA: &str = "dlq_test_restart";
    let Some((pool, svc)) = connect_pool(SCHEMA).await else {
        eprintln!("SKIP dead_lettered_dispatch_survives_a_restart: no postgres");
        return;
    };

    // ── Before restart: store A dead-letters one dispatch. ───────────────────
    let store_a = PgFunctionDlq::new(pool.clone(), None);
    store_a.init().await.unwrap();
    truncate(&pool).await;

    let rec = record("upstream 503");
    let want_id = rec.id;
    store_a.push_function(rec).await.unwrap();
    assert_eq!(store_a.get_pending_functions(10).await.unwrap().len(), 1);

    // ── "Restart": drop store A and its pool, reconnect a fresh store B. ─────
    // The new pool is built the same way, so it lands on the same private schema; a
    // plain `PgPool::connect` here would look at `public` and find another table.
    drop(store_a);
    drop(pool);
    let pool_b = isolated_pool(svc.url(), SCHEMA).await;
    let store_b = PgFunctionDlq::new(pool_b, None);
    // init() is idempotent — a real restart re-runs it; the row must remain.
    store_b.init().await.unwrap();

    let pending = store_b.get_pending_functions(10).await.unwrap();
    assert_eq!(
        pending.len(),
        1,
        "M-598: the Postgres-backed DLQ must survive a restart (the in-memory store loses it)"
    );
    let reloaded = &pending[0];
    assert_eq!(reloaded.id, want_id, "the stable dead-letter id round-trips");
    assert_eq!(reloaded.function_name, "notify_ops");
    assert_eq!(reloaded.trigger_type, "after:mutation:createOrder");
    // The idempotency token an operator needs to trace/replay the dispatch survives.
    assert_eq!(reloaded.idempotency_token, "idem-tok-abc123");
    assert_eq!(reloaded.source, DispatchSource::AfterMutation, "the source label round-trips");
    assert_eq!(reloaded.payload, serde_json::json!({ "order_id": 42, "total": "19.99" }));
    assert_eq!(reloaded.attempts, 3);
}

#[tokio::test]
async fn get_pending_returns_records_oldest_first() {
    const SCHEMA: &str = "dlq_test_oldest_first";
    let Some((pool, _svc)) = connect_pool(SCHEMA).await else {
        eprintln!("SKIP get_pending_returns_records_oldest_first: no postgres");
        return;
    };
    let store = PgFunctionDlq::new(pool.clone(), None);
    store.init().await.unwrap();
    truncate(&pool).await;

    store.push_function(record("first")).await.unwrap();
    store.push_function(record("second")).await.unwrap();
    store.push_function(record("third")).await.unwrap();

    let pending = store.get_pending_functions(10).await.unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(pending[0].error_message, "first", "oldest drains first");
    assert_eq!(pending[2].error_message, "third");

    // The list honours the limit.
    assert_eq!(store.get_pending_functions(2).await.unwrap().len(), 2);
}

#[tokio::test]
async fn capacity_drops_newest_and_holds_the_cap() {
    const SCHEMA: &str = "dlq_test_capacity";
    let Some((pool, _svc)) = connect_pool(SCHEMA).await else {
        eprintln!("SKIP capacity_drops_newest_and_holds_the_cap: no postgres");
        return;
    };
    let store = PgFunctionDlq::new(pool.clone(), Some(2));
    store.init().await.unwrap();
    truncate(&pool).await;

    store.push_function(record("e1")).await.unwrap();
    store.push_function(record("e2")).await.unwrap();
    // Third is at capacity → dropped (drop-newest), mirroring the in-memory store.
    store.push_function(record("e3")).await.unwrap();

    let pending = store.get_pending_functions(10).await.unwrap();
    assert_eq!(pending.len(), 2, "the cap holds the durable queue at 2");
    assert_eq!(pending[0].error_message, "e1", "the two oldest survive; the newest is dropped");
    assert_eq!(pending[1].error_message, "e2");
}
