//! The Model B source cursor ops, end-to-end through the Deno runtime (#573).
//!
//! Drives a minimal guest that reads (`fraiseql_cursor_get`) and advances
//! (`fraiseql_cursor_advance`) its source's durable cursor through the real
//! `DenoRuntime`, on a real `LiveHostContext` bound to a source, against real
//! Postgres. Proves the cursor round-trips through the guest and that advancing is
//! scoped to the bound source (no cross-source bleed) — and that a non-source host
//! refuses the advance.
//!
//! Each test runs exactly one guest → one V8 isolate per test process, which is
//! required (two isolates in one process SIGSEGV). Like every `runtime-deno` test
//! this runs locally only — CI compiles the path but excludes execution (embedded
//! V8 SIGSEGVs inside the Dagger exec sandbox). The Postgres-backed tests self-skip
//! when no `DATABASE_URL` is set.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use chrono::Utc;
use fraiseql_observers::{PostgresSourceCursorStore, SourceCursorStore};
use sqlx::PgPool;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{
        dyn_context::DynHostContext,
        live::{HostContextConfig, LiveHostContext},
    },
    runtime::deno::{DenoConfig, DenoRuntime},
};

fn event() -> EventPayload {
    EventPayload {
        trigger_type: "cron:poll".to_string(),
        entity:       "cron".to_string(),
        event_kind:   "scheduled".to_string(),
        data:         serde_json::json!({}),
        timestamp:    Utc::now(),
    }
}

async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// Ensure the cursor table exists and this source starts with no row (so re-runs of
/// a fixed-name test are independent).
async fn reset(pool: &PgPool, source: &str) {
    PostgresSourceCursorStore::new(pool.clone()).init().await.unwrap();
    sqlx::query("DELETE FROM _fraiseql_source_cursor WHERE source_name = $1")
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
}

async fn run_guest(host: Arc<dyn DynHostContext>, ts: &str) -> serde_json::Value {
    let module =
        FunctionModule::from_source("cursor-guest".to_string(), ts.to_string(), RuntimeType::Deno);
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event(), host, ResourceLimits::default())
        .await
        .expect("guest runs")
        .value
        .expect("returns a value")
}

fn source_host(pool: &PgPool, source: &str) -> Arc<dyn DynHostContext> {
    Arc::new(
        LiveHostContext::new(event(), HostContextConfig::default())
            .with_source_cursor(source.to_string(), PostgresSourceCursorStore::new(pool.clone())),
    )
}

/// A guest that reads the cursor, advances it, then reads it back — all in one
/// invocation (a second invocation would need a second isolate).
const ROUNDTRIP_TS: &str = r"
export default async () => {
  const before = await Deno.core.ops.fraiseql_cursor_get();
  await Deno.core.ops.fraiseql_cursor_advance(JSON.stringify({ page: 5 }));
  const after = await Deno.core.ops.fraiseql_cursor_get();
  return { before: JSON.parse(before), after: JSON.parse(after) };
};
";

#[tokio::test]
async fn guest_reads_and_advances_its_cursor() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP guest_reads_and_advances_its_cursor: no postgres");
        return;
    };
    let source = "test-cursor-roundtrip";
    reset(&pool, source).await;

    let value = run_guest(source_host(&pool, source), ROUNDTRIP_TS).await;

    assert!(
        value["before"].is_null(),
        "the first read is null (never advanced), got {:?}",
        value["before"]
    );
    assert_eq!(
        value["after"],
        serde_json::json!({ "page": 5 }),
        "the guest reads back what it advanced"
    );

    // Durability: the advance persisted to the store beyond the guest invocation.
    let snapshot = PostgresSourceCursorStore::new(pool.clone()).load(source).await.unwrap();
    assert_eq!(snapshot.value, Some(serde_json::json!({ "page": 5 })));
}

/// A guest that advances its (bound) source's cursor.
const ADVANCE_TS: &str = r"
export default async () => {
  await Deno.core.ops.fraiseql_cursor_advance(JSON.stringify({ a: 1 }));
  return { ok: true };
};
";

#[tokio::test]
async fn advance_is_scoped_to_the_bound_source() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP advance_is_scoped_to_the_bound_source: no postgres");
        return;
    };
    let source_a = "test-cursor-scope-a";
    let source_b = "test-cursor-scope-b";
    reset(&pool, source_a).await;
    reset(&pool, source_b).await;

    // The guest is bound to source A only.
    run_guest(source_host(&pool, source_a), ADVANCE_TS).await;

    let a = PostgresSourceCursorStore::new(pool.clone()).load(source_a).await.unwrap();
    let b = PostgresSourceCursorStore::new(pool.clone()).load(source_b).await.unwrap();
    assert_eq!(a.value, Some(serde_json::json!({ "a": 1 })), "source A advanced");
    assert!(
        b.value.is_none(),
        "advancing source A must not touch source B (no cross-source bleed)"
    );
}

/// A guest that catches the error from advancing on a non-source host.
const REJECT_TS: &str = r"
export default async () => {
  try {
    await Deno.core.ops.fraiseql_cursor_advance('{}');
    return { errored: false };
  } catch (e) {
    return { errored: true, message: String(e) };
  }
};
";

#[tokio::test]
async fn non_source_host_refuses_to_advance() {
    // No cursor binding (the default host): a guest that is not a scheduled source
    // cannot move a cursor. Needs no database — the default fails before any I/O.
    let host: Arc<dyn DynHostContext> =
        Arc::new(LiveHostContext::new(event(), HostContextConfig::default()));

    let value = run_guest(host, REJECT_TS).await;

    assert_eq!(
        value["errored"],
        serde_json::json!(true),
        "advance on a non-source host must reject"
    );
}
