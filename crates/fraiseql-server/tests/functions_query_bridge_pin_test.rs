//! #594 — the `fraiseql_query` host bridge contract (was a phase-00 pin, now fixed).
//!
//! Phase 02 wired the bridge onto the after:mutation / after:ingest dispatch host
//! under a per-function `run_as` ceiling (the shared `RunAsQueryExecutor`, the same
//! seam scheduled sources use). The dispatch-level fix — that
//! `DurableDispatcher::build_host` wires an executor when a factory + `run_as` are
//! present, fail-closed otherwise — is verified in-crate at
//! `routes::after_mutation::tests::query_bridge_wiring`.
//!
//! These tests pin the underlying `LiveHostContext` *contract* the dispatcher builds
//! on: a bare host has no executor; `.with_executor(..)` supplies one. That contract
//! is permanent (it is how both sources and functions attach their bridge), so this
//! is a regression test, not a gap marker.
//!
//! Gated on `functions-runtime` because `LiveHostContext` lives behind
//! `fraiseql-functions/host-live`.

#![cfg(feature = "functions-runtime")]
#![allow(clippy::unwrap_used)] // Reason: test code

use std::{future::Future, pin::Pin, sync::Arc};

use fraiseql_functions::{
    HostContext,
    host::live::{HostContextConfig, LiveHostContext, QueryExecutor},
    types::EventPayload,
};
use serde_json::Value;

/// A stand-in for the real query executor: returns a canned result. Mirrors what
/// `RunAsQueryExecutor` provides for a dispatched host under its `run_as` identity.
struct MockQueryExecutor;

impl QueryExecutor for MockQueryExecutor {
    fn execute_query(
        &self,
        _query: &str,
        _variables: Option<&Value>,
    ) -> Pin<Box<dyn Future<Output = fraiseql_error::Result<Value>> + Send + '_>> {
        Box::pin(async { Ok(serde_json::json!({ "data": { "ok": true } })) })
    }
}

fn payload() -> EventPayload {
    EventPayload {
        trigger_type: "after:mutation:notify_approved".to_string(),
        entity:       "Order".to_string(),
        event_kind:   "update".to_string(),
        data:         serde_json::json!({ "new": { "id": 1 } }),
        timestamp:    chrono::Utc::now(),
    }
}

#[tokio::test]
async fn bare_live_host_has_no_query_executor() {
    // A host with no executor attached fails loud — this is the fail-closed contract
    // the dispatcher relies on: it only attaches an executor when a `run_as` factory
    // is configured (verified in-crate). A bare `invoke` (no dispatch) stays inert.
    let host = LiveHostContext::new(payload(), HostContextConfig::default());

    let err = host
        .query("mutation { recordApproval(id: 1) { id } }", serde_json::json!({}))
        .await
        .expect_err("a bare host has no query executor");
    assert!(
        err.to_string().contains("query executor not configured"),
        "expected the unconfigured-executor error, got: {err}"
    );
}

#[tokio::test]
async fn live_host_with_executor_runs_the_query_bridge() {
    // `.with_executor(..)` supplies the bridge — the mechanism the dispatcher and the
    // sources poller both use to give a guest a working `fraiseql_query`.
    let host = LiveHostContext::new(payload(), HostContextConfig::default())
        .with_executor(Arc::new(MockQueryExecutor));

    let value = host
        .query("mutation { recordApproval(id: 1) { id } }", serde_json::json!({}))
        .await
        .expect("a host with an executor runs the query bridge");
    assert_eq!(value, serde_json::json!({ "data": { "ok": true } }));
}
