//! The per-dispatch idempotency token, end-to-end through the Deno runtime.
//!
//! Drives a minimal guest that returns `fraiseql_idempotency_token()` through the
//! real `DenoRuntime`, on a real `LiveHostContext`, to prove the op surfaces the
//! host-carried token (and `null` when none is set). Each test runs exactly one
//! guest → one V8 isolate per test process, which is required (two isolates in one
//! process SIGSEGV even under nextest).

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use chrono::Utc;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{
        dyn_context::DynHostContext,
        live::{HostContextConfig, LiveHostContext},
    },
    runtime::deno::{DenoConfig, DenoRuntime},
};

/// A guest that echoes the host-provided idempotency token straight back.
const ECHO_TOKEN_TS: &str = r"
export default async () => ({ token: Deno.core.ops.fraiseql_idempotency_token() });
";

fn event() -> EventPayload {
    EventPayload {
        trigger_type: "after:mutation".to_string(),
        entity:       "Deal".to_string(),
        event_kind:   "updated".to_string(),
        data:         serde_json::json!({ "id": "deal-1" }),
        timestamp:    Utc::now(),
    }
}

async fn run_echo(host: Arc<dyn DynHostContext>) -> serde_json::Value {
    let module = FunctionModule::from_source(
        "echo-token".to_string(),
        ECHO_TOKEN_TS.to_string(),
        RuntimeType::Deno,
    );
    let runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    runtime
        .invoke_with_context(&module, event(), host, ResourceLimits::default())
        .await
        .expect("guest runs")
        .value
        .expect("returns a value")
}

#[tokio::test]
async fn token_is_exposed_to_the_guest() {
    let token = "1a2b3c4d5e6f70818283848586878889";
    let host: Arc<dyn DynHostContext> = Arc::new(
        LiveHostContext::new(event(), HostContextConfig::default()).with_idempotency_token(token),
    );

    let value = run_echo(host).await;

    assert_eq!(value["token"], token, "the guest reads the host-carried token verbatim");
}

#[tokio::test]
async fn absent_token_is_null_to_the_guest() {
    // A host that carries no token (the default) surfaces `null`, so a guest can
    // branch on it (e.g. fall back to a hand-derived key on a non-dispatched path).
    let host: Arc<dyn DynHostContext> =
        Arc::new(LiveHostContext::new(event(), HostContextConfig::default()));

    let value = run_echo(host).await;

    assert!(value["token"].is_null(), "no token set → null, got {:?}", value["token"]);
}
