//! Tests for the source poller.
//!
//! - [`source_payload_carries_the_trigger_context`] is pure.
//! - [`build_host_binds_both_the_cursor_and_the_executor`] proves the poller's novel composition —
//!   that one firing's host reaches *both* the durable cursor (vs real PostgreSQL) *and* the
//!   `run_as` executor — without a V8 guest, so it runs in the PG integration leg. The full Model B
//!   guest-through-poller round-trip (a Deno connector reading its cursor, mutating via
//!   `fraiseql_query`, advancing) is a local-only V8 test landing with the runnable example in
//!   Phase 07.
#![allow(clippy::unwrap_used)] // Reason: test module

use std::{future::Future, pin::Pin, sync::Arc};

use chrono::Utc;
use fraiseql_functions::{
    FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::CronSchedule,
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore, SourceCursorStore};
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
        CronSchedule::parse("*/5 * * * *").unwrap(),
        FunctionModule::from_source("noop".to_string(), String::new(), RuntimeType::Deno),
        Arc::new(FunctionObserver::new()),
        PostgresSourceCursorStore::new(pool.clone()),
        executor,
        LeaseGuardedRunner::in_process(source),
        HostContextConfig::default(),
        ResourceLimits::default(),
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
    let host = poller.build_host(build_source_payload(source, "*/5 * * * *", Utc::now()));

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
