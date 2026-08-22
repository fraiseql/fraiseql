//! Tests for the source poller.
//!
//! - [`source_payload_carries_the_trigger_context`] is pure.
//! - [`build_host_binds_both_the_cursor_and_the_executor`] proves the poller's novel composition —
//!   that one firing's host reaches *both* the durable cursor (vs real `PostgreSQL`) *and* the
//!   `run_as` executor — without a V8 guest, so it runs in the PG integration leg. The full Model B
//!   guest-through-poller round-trip (a Deno connector reading its cursor, mutating via
//!   `fraiseql_query`, advancing) is a local-only V8 test that ships with the runnable example.
#![allow(clippy::unwrap_used)] // Reason: test module
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres
#![allow(clippy::large_futures)] // Reason: a test future holds a poller + runtime; stack size is irrelevant in a #[tokio::test]

use std::{future::Future, pin::Pin, sync::Arc};

use chrono::Utc;
use fraiseql_functions::{
    FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
    host::live::{HostContextConfig, QueryExecutor},
    runtime::deno::{DenoConfig, DenoRuntime},
    triggers::CronSchedule,
};
use fraiseql_observers::{
    LeaseGuardedRunner, PostgresSourceCursorStore, RunOutcome, SourceCursorStore,
};
use serde_json::{Value, json};
use sqlx::PgPool;

use super::{SourcePoller, build_source_payload};

#[test]
fn source_payload_carries_the_trigger_context() {
    let payload = build_source_payload("orders", "*/5 * * * *", Utc::now());
    assert_eq!(payload.trigger_type, "source:orders");
    assert_eq!(payload.entity, "source");
    assert_eq!(payload.event_kind, "scheduled");
    assert_eq!(payload.data["source"], "orders");
    assert_eq!(payload.data["schedule"], "*/5 * * * *");
}

/// The per-firing idempotency token is signed with the server HMAC subkey when one
/// is configured — the poller threads `idempotency_key` through rather than
/// hard-coding the unsigned digest, so a source's token is as unforgeable as every
/// other dispatch path's. A lazy pool never connects (this derives tokens, it does
/// not query) — `#[tokio::test]` only because `connect_lazy` needs a runtime handle.
#[tokio::test]
async fn idempotency_token_is_signed_when_a_key_is_configured() {
    let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let build = |key: Option<Arc<[u8]>>| {
        SourcePoller::new(
            "orders",
            "orders",
            CronSchedule::parse("*/5 * * * *").unwrap(),
            FunctionModule::from_source("connector".to_string(), String::new(), RuntimeType::Deno),
            Arc::new(FunctionObserver::new()),
            PostgresSourceCursorStore::new(pool.clone()),
            StubExecutor::new(json!(null)),
            fraiseql_core::security::SecurityContext::system_job(
                "test-source",
                "test-request",
                vec![],
                vec![],
                None,
            ),
            LeaseGuardedRunner::in_process("orders"),
            HostContextConfig::default(),
            ResourceLimits::default(),
            key,
            false,
        )
    };
    let payload = build_source_payload("orders", "*/5 * * * *", Utc::now());

    let unsigned = build(None).idempotency_token(&payload);
    let secret: Arc<[u8]> = Arc::from(b"server-hmac-secret".as_slice());
    let signed = build(Some(Arc::clone(&secret))).idempotency_token(&payload);

    // The key changes the token: it is threaded through, not ignored.
    assert_ne!(unsigned, signed);
    // The signed form is exactly the keyed HMAC over the firing's stable identity.
    let expected = fraiseql_observers::derive_idempotency_token(
        Some(&secret),
        fraiseql_observers::DispatchSource::Source,
        "connector",
        &payload.trigger_type,
        &payload.data,
    );
    assert_eq!(signed, expected);
    // Both forms honour the 32-hex-char (128-bit) token contract.
    assert_eq!(unsigned.len(), 32);
}

/// A query executor that returns a canned response and records the query it saw, so
/// a test can prove the host reached *an* executor (the poller wired one on).
struct StubExecutor {
    response: Value,
    seen:     std::sync::Mutex<Vec<String>>,
}

impl StubExecutor {
    fn new(response: Value) -> Arc<Self> {
        Arc::new(Self {
            response,
            seen: std::sync::Mutex::new(Vec::new()),
        })
    }
}

impl QueryExecutor for StubExecutor {
    fn execute_query(
        &self,
        query: &str,
        _variables: Option<&Value>,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Value>> + Send + '_>> {
        self.seen.lock().unwrap().push(query.to_string());
        let response = self.response.clone();
        Box::pin(async move { Ok(response) })
    }
}

async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// Build a poller whose collaborators are all scoped to `source`, with `executor`
/// as its query bridge. The module/observer are inert here — `build_host` does not
/// invoke the guest.
fn poller(pool: &PgPool, source: &str, executor: Arc<dyn QueryExecutor>) -> SourcePoller {
    SourcePoller::new(
        source,
        source,
        CronSchedule::parse("*/5 * * * *").unwrap(),
        FunctionModule::from_source("noop".to_string(), String::new(), RuntimeType::Deno),
        Arc::new(FunctionObserver::new()),
        PostgresSourceCursorStore::new(pool.clone()),
        executor,
        fraiseql_core::security::SecurityContext::system_job(
            "test-source",
            "test-request",
            vec![],
            vec![],
            None,
        ),
        LeaseGuardedRunner::in_process(source),
        HostContextConfig::default(),
        ResourceLimits::default(),
        None,
        false,
    )
}

#[tokio::test]
async fn build_host_binds_both_the_cursor_and_the_executor() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP build_host_binds_both_the_cursor_and_the_executor: no postgres");
        return;
    };
    let source = "test-poller-build-host";
    // Fresh cursor row so re-runs are independent.
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("DELETE FROM _fraiseql_source_cursor WHERE source_name = $1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();

    let executor = StubExecutor::new(json!({ "data": { "createOrder": { "status": "ok" } } }));
    let poller = poller(&pool, source, executor.clone());
    let host = poller.build_host(build_source_payload(source, "*/5 * * * *", Utc::now()), "tok");

    // The cursor is bound: it round-trips through the host against real Postgres.
    assert!(host.cursor().await.unwrap().is_none(), "a fresh source has no cursor");
    host.advance_cursor(json!({ "page": 3 })).await.unwrap();
    assert_eq!(
        host.cursor().await.unwrap(),
        Some(json!({ "page": 3 })),
        "the host reads back what it advanced"
    );

    // The executor is bound: host.query reaches it (the fraiseql_query bridge that
    // production dispatch left unwired).
    let result = host.query("mutation { createOrder }", json!({})).await.unwrap();
    assert_eq!(result, json!({ "data": { "createOrder": { "status": "ok" } } }));
    assert_eq!(
        executor.seen.lock().unwrap().as_slice(),
        ["mutation { createOrder }"],
        "the query reached the bound executor"
    );

    // Durability: the advance persisted beyond the host.
    let snapshot = PostgresSourceCursorStore::new(pool.clone()).load(source).await.unwrap();
    assert_eq!(snapshot.value, Some(json!({ "page": 3 })));
}

/// A minimal Model B connector: read the cursor, mutate via `fraiseql_query`,
/// advance the cursor — the exact loop the #573 issue shows.
const CONNECTOR_TS: &str = r#"
export default async () => {
  const before = JSON.parse(await Deno.core.ops.fraiseql_cursor_get());
  const page = (before && before.page ? before.page : 0) + 1;
  await Deno.core.ops.fraiseql_query(
    "mutation { createOrder(page: " + page + ") { id } }",
    "{}"
  );
  await Deno.core.ops.fraiseql_cursor_advance(JSON.stringify({ page: page }));
  return { page: page };
};
"#;

/// The whole Model B slice end-to-end through the poller: a real Deno connector,
/// fired once under the lease, reads its (null) cursor, issues a `fraiseql_query`
/// mutation (reaching the bound executor), and advances the durable cursor.
///
/// LOCAL-ONLY: this invokes a real Deno guest (one V8 isolate), so — like every
/// `runtime-deno` test — it is excluded from CI (embedded V8 SIGSEGVs in the Dagger
/// exec sandbox); the `.dagger` source suite skips it by name. Run locally with
/// `DATABASE_URL` set, one isolate per process.
#[tokio::test]
async fn fires_a_model_b_connector_end_to_end() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP fires_a_model_b_connector_end_to_end: no postgres");
        return;
    };
    let source = "test-poller-e2e";
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("DELETE FROM _fraiseql_source_cursor WHERE source_name = $1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();

    let executor = StubExecutor::new(json!({ "data": { "createOrder": { "id": "1" } } }));
    let mut observer = FunctionObserver::new();
    observer.register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default()).unwrap());
    let module = FunctionModule::from_source(
        "connector".to_string(),
        CONNECTOR_TS.to_string(),
        RuntimeType::Deno,
    );

    let poller = SourcePoller::new(
        source,
        source,
        CronSchedule::parse("*/5 * * * *").unwrap(),
        module,
        Arc::new(observer),
        PostgresSourceCursorStore::new(pool.clone()),
        executor.clone(),
        fraiseql_core::security::SecurityContext::system_job(
            "test-source",
            "test-request",
            vec![],
            vec![],
            None,
        ),
        LeaseGuardedRunner::in_process(source),
        HostContextConfig::default(),
        ResourceLimits::default(),
        None,
        false,
    );

    let outcome = poller.fire_once(chrono::Utc::now()).await;
    assert!(
        matches!(outcome, RunOutcome::Ran(Ok(_))),
        "the connector fired and ran to completion under the lease"
    );

    // The connector's fraiseql_query reached the bound executor. Clone out of the
    // lock so no guard is held across the later await.
    let seen = executor.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "the guest issued exactly one query");
    assert!(seen[0].contains("createOrder"), "it was the connector's mutation: {}", seen[0]);

    // The connector advanced its durable cursor from null → { page: 1 }.
    let snapshot = PostgresSourceCursorStore::new(pool.clone()).load(source).await.unwrap();
    assert_eq!(snapshot.value, Some(json!({ "page": 1 })));
}

/// Poll the durable cursor until the connector's page counter reaches `target`, or
/// fail after `deadline`. Returns the page observed at (or past) the target.
async fn wait_for_cursor_page(
    pool: &PgPool,
    source: &str,
    target: i64,
    deadline: std::time::Duration,
) -> i64 {
    let store = PostgresSourceCursorStore::new(pool.clone());
    let started = std::time::Instant::now();
    loop {
        let page = store
            .load(source)
            .await
            .unwrap()
            .value
            .and_then(|v| v.get("page").and_then(Value::as_i64))
            .unwrap_or(0);
        if page >= target {
            return page;
        }
        assert!(
            started.elapsed() < deadline,
            "cursor page stuck at {page} (target {target}) after {:?} — the scheduler stopped \
             firing (the #796 failure shape)",
            started.elapsed()
        );
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// #573 re-verification (P30): the whole scheduled-ingress claim on the **real
/// clock** — a Model B connector fires in several successive schedule windows,
/// and across a poller restart it resumes from the durable cursor.
///
/// Before the #796 fix, every `cron:`-triggered firing — including every Source —
/// happened once per process start and then never again, and the unit suite stayed
/// green because zero-second synthetic timestamps made the broken window
/// computation coincide with the correct one. Real wall-clock instants are
/// therefore load-bearing: this drives the real [`SourcePoller::run_forever`]
/// ticker (60 s interval, `Utc::now()` windows) end to end, so it takes ~4 minutes.
///
/// LOCAL-ONLY and `#[ignore]`d: it invokes a real Deno guest (V8 SIGSEGVs in the
/// Dagger exec sandbox — see `.dagger/parity-notes.md`) and holds multi-minute
/// wall-clock windows. Run explicitly:
/// `cargo test -p fraiseql-server --features sources --lib
///  ingests_across_schedule_windows_and_a_restart -- --ignored --test-threads=1`
/// The fast halves stay in CI: the window-decision simulator and cross-restart
/// cron-state guard (functions leg) and the cursor round-trip (sources leg).
#[tokio::test]
#[ignore = "local-only #573 gate: real V8 guest + real multi-minute schedule windows (~4 min)"]
async fn ingests_across_schedule_windows_and_a_restart() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP ingests_across_schedule_windows_and_a_restart: no postgres");
        return;
    };
    let source = "test-poller-multi-window";
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("DELETE FROM _fraiseql_source_cursor WHERE source_name = $1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();

    let executor = StubExecutor::new(json!({ "data": { "createOrder": { "id": "1" } } }));
    // One builder, invoked once per "process": a restart is a fresh poller (fresh
    // in-memory window state, fresh V8 runtime) over the same durable cursor row.
    let build = || {
        let mut observer = FunctionObserver::new();
        observer
            .register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default()).unwrap());
        SourcePoller::new(
            source,
            source,
            CronSchedule::parse("* * * * *").unwrap(),
            FunctionModule::from_source(
                "connector".to_string(),
                CONNECTOR_TS.to_string(),
                RuntimeType::Deno,
            ),
            Arc::new(observer),
            PostgresSourceCursorStore::new(pool.clone()),
            executor.clone(),
            fraiseql_core::security::SecurityContext::system_job(
                "test-source",
                "test-request",
                vec![],
                vec![],
                None,
            ),
            LeaseGuardedRunner::in_process(source),
            HostContextConfig::default(),
            ResourceLimits::default(),
            None,
            false,
        )
    };

    // First "process": ≥2 firings means ≥2 distinct windows — the AlreadyFired
    // guard forbids a same-window repeat, and window 2 is exactly what #796 broke.
    // Worst case ~60 s to the first tick + 60 s to the next window + guest time.
    let first = tokio::spawn(build().run_forever());
    wait_for_cursor_page(&pool, source, 2, std::time::Duration::from_mins(4)).await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());

    // Re-read after the abort so a raced third firing cannot skew the baseline.
    let before = wait_for_cursor_page(&pool, source, 2, std::time::Duration::from_secs(5)).await;
    let seen_at_restart = executor.seen.lock().unwrap().clone();
    assert!(
        seen_at_restart.len() >= 2,
        "each window's firing issued its mutation: {seen_at_restart:?}"
    );
    assert!(
        seen_at_restart[0].contains("(page: 1)"),
        "the first window ingested from a fresh cursor: {}",
        seen_at_restart[0]
    );

    // "Restart": only the DB row survives. The next firing must continue at
    // page `before + 1` — a cursor reset would re-ingest from page 1, a skip
    // would gap. (At-least-once: a transient guest failure re-runs the SAME
    // page, which the contains-assert below tolerates.)
    let second = tokio::spawn(build().run_forever());
    wait_for_cursor_page(&pool, source, before + 1, std::time::Duration::from_mins(4)).await;
    second.abort();
    assert!(second.await.unwrap_err().is_cancelled());

    let seen = executor.seen.lock().unwrap().clone();
    let first_after_restart = &seen[seen_at_restart.len()];
    assert!(
        first_after_restart.contains(&format!("(page: {})", before + 1)),
        "after the restart the connector resumed from the durable cursor (expected page {}, \
         got: {first_after_restart})",
        before + 1
    );
}

/// The poller advances the **declared cursor** key, not the source name (#868 item 4).
///
/// `source_name` and `cursor_name` are distinct concepts that used to share one field: the
/// name labels the lease, metrics and logs; the cursor key names the watermark row in
/// `_fraiseql_source_cursor`. Passing the name for both made every declared `cursor` override
/// inert — accepted, validated for uniqueness, compiled, printed by `fraiseql sources`, and
/// doing nothing. An operator renaming a source from `orders` to `orders_v2` with
/// `cursor = "orders"` to preserve the watermark got a fresh cursor and a full re-ingest.
///
/// This asserts against **real Postgres**, on the row the store actually writes, because that
/// is the only place the difference is observable: a test that merely reads the poller's own
/// field back passes whether or not `build_host` uses it.
#[tokio::test]
async fn the_poller_advances_the_declared_cursor_not_the_source_name() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP the_poller_advances_the_declared_cursor_not_the_source_name: no postgres");
        return;
    };
    let source_name = "test-poller-renamed-source";
    let declared_cursor = "test-poller-original-cursor";

    let store = PostgresSourceCursorStore::new(pool.clone());
    store.init().await.unwrap();
    for key in [source_name, declared_cursor] {
        sqlx::query("DELETE FROM _fraiseql_source_cursor WHERE source_name = $1")
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
    }

    let poller = SourcePoller::new(
        source_name,
        declared_cursor,
        CronSchedule::parse("*/5 * * * *").unwrap(),
        FunctionModule::from_source("connector".to_string(), String::new(), RuntimeType::Deno),
        Arc::new(FunctionObserver::new()),
        PostgresSourceCursorStore::new(pool.clone()),
        StubExecutor::new(json!(null)),
        fraiseql_core::security::SecurityContext::system_job(
            "test-source",
            "test-request",
            vec![],
            vec![],
            None,
        ),
        LeaseGuardedRunner::in_process(source_name),
        HostContextConfig::default(),
        ResourceLimits::default(),
        None,
        false,
    );

    let host =
        poller.build_host(build_source_payload(source_name, "*/5 * * * *", Utc::now()), "tok");
    host.advance_cursor(json!({ "page": 7 })).await.unwrap();

    assert_eq!(
        store.load(declared_cursor).await.unwrap().value,
        Some(json!({ "page": 7 })),
        "the watermark must land under the declared `cursor` override"
    );
    assert_eq!(
        store.load(source_name).await.unwrap().value,
        None,
        "nothing may be written under the source name when an override is declared — that is \
         the row a rename-with-cursor-preservation was trying to avoid creating"
    );
}
