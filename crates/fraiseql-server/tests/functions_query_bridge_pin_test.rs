//! Baseline pin for #594: the `fraiseql_query` host bridge is wired for sources
//! but NOT for after:mutation dispatch (phase 00).
//!
//! Two hosts, one seam, documenting the asymmetry:
//! - the host an after:mutation dispatch builds today (`LiveHostContext::new`, exactly as
//!   `DurableDispatcher::invoke_once` constructs it) has no query executor, so a function calling
//!   `fraiseql_query` fails with "query executor not configured";
//! - a sources-style host (`.with_executor(..)`, as `SourceQueryExecutor` wires it) runs the same
//!   call successfully.
//!
//! Phase 02 extracts the shared executor and wires it onto the after:mutation /
//! after:ingest host contexts under a `run_as` ceiling; when it lands, the first
//! assertion flips (the after:mutation host gains an executor).
//!
//! Gated on `functions-runtime` because `LiveHostContext` lives behind
//! `fraiseql-functions/host-live`. No V8 isolate is spun (the mock executor
//! returns a canned value), so the test is safe under plain `cargo test
//! --features functions-runtime`.

#![cfg(feature = "functions-runtime")]
#![allow(clippy::unwrap_used)] // Reason: test code

use std::{future::Future, pin::Pin, sync::Arc};

use fraiseql_functions::{
    HostContext,
    host::live::{HostContextConfig, LiveHostContext, QueryExecutor},
    types::EventPayload,
};
use serde_json::Value;

/// A stand-in for the real query executor: records the call and returns a canned
/// result. Mirrors what `SourceQueryExecutor` provides for sources.
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

/// The event payload an after:mutation dispatch hands to the host.
fn after_mutation_payload() -> EventPayload {
    EventPayload {
        trigger_type: "after:mutation:notify_approved".to_string(),
        entity:       "Order".to_string(),
        event_kind:   "update".to_string(),
        data:         serde_json::json!({ "new": { "id": 1 } }),
        timestamp:    chrono::Utc::now(),
    }
}

#[tokio::test]
async fn pin_594_after_mutation_host_has_no_query_executor() {
    // Built exactly as `DurableDispatcher::invoke_once` builds it: `new(..)` with
    // no `.with_executor(..)`. This is the #594 gap.
    let host = LiveHostContext::new(after_mutation_payload(), HostContextConfig::default());

    let result = host
        .query("mutation { recordApproval(id: 1) { id } }", serde_json::json!({}))
        .await;

    let err = result.expect_err(
        "M-594: an after:mutation function's `fraiseql_query` must fail today — no executor is \
         wired onto the dispatch host. Phase 02 wires it; flip this to `expect(..)` then.",
    );
    assert!(
        err.to_string().contains("query executor not configured"),
        "M-594: expected the unconfigured-executor error, got: {err}"
    );
}

#[tokio::test]
async fn pin_594_sources_style_host_with_executor_succeeds() {
    // The sources path DOES wire an executor. The same guest call succeeds — proof
    // the bridge is release-grade and merely unwired on the after:mutation path.
    let host = LiveHostContext::new(after_mutation_payload(), HostContextConfig::default())
        .with_executor(Arc::new(MockQueryExecutor));

    let result = host
        .query("mutation { recordApproval(id: 1) { id } }", serde_json::json!({}))
        .await;

    let value = result.expect("a host with an executor runs the query bridge");
    assert_eq!(value, serde_json::json!({ "data": { "ok": true } }));
}
