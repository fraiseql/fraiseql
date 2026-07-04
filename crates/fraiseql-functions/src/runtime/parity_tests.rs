//! Cross-runtime host-surface parity.
//!
//! The whole point of the shared [`DynHostContext`](crate::host::dyn_context::DynHostContext)
//! bridge is that a function sees the *same* host semantics whether it runs on the
//! WASM backend or the Deno backend. This test pins that: it drives the
//! `guest-full-bridge.wasm` fixture (which exercises every host op) and an
//! equivalent `TypeScript` guest through the **same** mock host, then asserts the
//! host-observable outcome is identical. The WASM leg is the golden reference.
//!
//! Gated on both runtime features; it runs in a local `--all-features` /
//! `runtime-wasm,runtime-deno` nextest run (each test in its own process, so V8
//! and wasmtime don't share isolate state).

#![cfg(all(feature = "runtime-wasm", feature = "runtime-deno"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use std::{path::PathBuf, sync::Arc};

use chrono::Utc;

use crate::{
    EventPayload, FunctionModule, ResourceLimits, RuntimeType,
    host::{HttpResponse, dyn_context::DynHostContext},
    runtime::{
        deno::{DenoConfig, DenoRuntime},
        wasm::{WasmConfig, WasmRuntime},
    },
};

/// Canned host shared by both runtimes. Returns values the guests can assert on.
struct ParityHost {
    event: EventPayload,
}

impl crate::HostContext for ParityHost {
    async fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({ "data": { "users": [{ "id": 1 }] } }))
    }

    async fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> fraiseql_error::Result<Vec<serde_json::Value>> {
        Ok(vec![serde_json::json!({ "id": 1 })])
    }

    async fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> fraiseql_error::Result<HttpResponse> {
        Ok(HttpResponse {
            status:  200,
            headers: vec![],
            body:    b"ok".to_vec(),
        })
    }

    async fn storage_get(&self, _bucket: &str, _key: &str) -> fraiseql_error::Result<Vec<u8>> {
        Ok(b"hello world".to_vec())
    }

    async fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> fraiseql_error::Result<()> {
        Ok(())
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({ "user_id": "u123" }))
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        if name == "FRAISEQL_TEST_VAR" {
            Ok(Some("test-value".to_string()))
        } else {
            Ok(None)
        }
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::LogLevel, _message: &str) {}
}

fn test_event() -> EventPayload {
    EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({ "value": 42 }),
        timestamp:    Utc::now(),
    }
}

fn mock_host() -> Arc<dyn DynHostContext> {
    Arc::new(ParityHost {
        event: test_event(),
    })
}

fn wasm_fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/functions")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// The `TypeScript` equivalent of `guest-full-bridge.wasm`: exercise every host
/// op and emit the same nested summary shape the WASM guest emits.
const DENO_FULL_BRIDGE: &str = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(1, 'info message');

    const auth = Deno.core.ops.fraiseql_auth_context();
    const envVal = Deno.core.ops.fraiseql_env_var('FRAISEQL_TEST_VAR');

    const resp = await Deno.core.ops.fraiseql_http_request('GET', 'https://mock.test/api', [], null);
    const query = await Deno.core.ops.fraiseql_query('{ users { id } }', '{}');
    await Deno.core.ops.fraiseql_storage_put('test-bucket', 'test-key',
        new Uint8Array([104, 101, 108, 108, 111]), 'text/plain');
    const got = await Deno.core.ops.fraiseql_storage_get('test-bucket', 'test-key');

    return {
        logging: 'ok',
        event_payload: event !== null && event !== undefined,
        auth_context: { ok: auth.length > 0 },
        env_var: envVal === null ? { found: false } : { found: true, value: envVal },
        http_request: { ok: true, status: resp.status },
        query: { ok: query.length > 0 },
        storage_put: { ok: true },
        storage_get: { ok: got.length > 0 },
    };
};
";

#[tokio::test]
async fn test_host_surface_parity_wasm_and_deno() {
    let limits = ResourceLimits::default();

    // ── WASM leg (golden reference) ──────────────────────────────────────────
    let wasm_runtime = WasmRuntime::new(&WasmConfig::default()).unwrap();
    let wasm_module = FunctionModule::from_bytecode(
        "full-bridge".to_string(),
        bytes::Bytes::from(wasm_fixture("guest-full-bridge.wasm")),
    );
    let wasm_out = wasm_runtime
        .invoke_with_context(&wasm_module, test_event(), mock_host(), limits.clone())
        .await
        .expect("WASM full-bridge should run")
        .value
        .expect("WASM guest returns a value");

    // ── Deno leg ─────────────────────────────────────────────────────────────
    let deno_runtime = DenoRuntime::new(&DenoConfig::default()).unwrap();
    let deno_module = FunctionModule::from_source(
        "full-bridge-ts".to_string(),
        DENO_FULL_BRIDGE.to_string(),
        RuntimeType::Deno,
    );
    let deno_out = deno_runtime
        .invoke_with_context(&deno_module, test_event(), mock_host(), limits)
        .await
        .expect("Deno full-bridge should run")
        .value
        .expect("Deno guest returns a value");

    // ── Parity: the host-observable fields must match across runtimes ────────
    // (the WASM guest also emits serialization-dependent `len` fields; we compare
    // only the semantic fields, which must be identical.)
    assert_eq!(wasm_out["event_payload"], deno_out["event_payload"], "event_payload parity");
    assert_eq!(wasm_out["http_request"]["ok"], deno_out["http_request"]["ok"], "http ok parity");
    assert_eq!(
        wasm_out["http_request"]["status"], deno_out["http_request"]["status"],
        "http status parity"
    );
    assert_eq!(wasm_out["query"]["ok"], deno_out["query"]["ok"], "query ok parity");
    assert_eq!(
        wasm_out["storage_put"]["ok"], deno_out["storage_put"]["ok"],
        "storage_put parity"
    );
    assert_eq!(
        wasm_out["storage_get"]["ok"], deno_out["storage_get"]["ok"],
        "storage_get parity"
    );
    assert_eq!(wasm_out["auth_context"]["ok"], deno_out["auth_context"]["ok"], "auth parity");
    assert_eq!(wasm_out["env_var"]["found"], deno_out["env_var"]["found"], "env found parity");
    assert_eq!(wasm_out["env_var"]["value"], deno_out["env_var"]["value"], "env value parity");

    // ── And the values are what the mock actually returned ───────────────────
    assert_eq!(deno_out["http_request"]["status"], 200);
    assert_eq!(deno_out["env_var"]["value"], "test-value");
    assert_eq!(deno_out["auth_context"]["ok"], true);
    assert_eq!(deno_out["storage_get"]["ok"], true);
}
