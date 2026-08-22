//! Tests for `DenoRuntime` — `JavaScript`/`TypeScript` function execution via V8

#![cfg(feature = "runtime-deno")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(unused_imports)] // Reason: used in test functions that may not be compiled in some configurations

use chrono::Utc;

use crate::{EventPayload, FunctionModule, FunctionRuntime, ResourceLimits, RuntimeType};

/// Helper to create a test event payload
#[allow(dead_code)] // Reason: used in failing tests, will be used when GREEN phase implemented
fn test_event() -> EventPayload {
    EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({"value": 42}),
        timestamp:    Utc::now(),
    }
}

/// #970 regression test host: a host op whose network resource is CREATED during
/// one invocation and REUSED by the next — the shape of every shared pool (sqlx,
/// reqwest keep-alive). The first `sql_query` call connects a `TcpStream` and
/// stores it; every later call does a 1-byte echo round-trip on the stored stream.
#[allow(dead_code)] // Reason: constructed only by #[tokio::test] fns, which compile out of dependency builds (this module is `pub mod tests`, not cfg(test))
struct ReusedStreamHost {
    payload: crate::EventPayload,
    addr:    std::net::SocketAddr,
    stream:  tokio::sync::Mutex<Option<tokio::net::TcpStream>>,
}

impl crate::host::dyn_context::DynHostContext for ReusedStreamHost {
    fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<serde_json::Value>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<Vec<serde_json::Value>>>
    {
        Box::pin(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let io_err = |e: std::io::Error| fraiseql_error::FraiseQLError::Internal {
                message: format!("echo stream: {e}"),
                source:  None,
            };
            let mut guard = self.stream.lock().await;
            if guard.is_none() {
                *guard = Some(tokio::net::TcpStream::connect(self.addr).await.map_err(io_err)?);
            }
            let stream = guard.as_mut().expect("just set");
            stream.write_all(b"p").await.map_err(io_err)?;
            let mut buf = [0_u8; 1];
            stream.read_exact(&mut buf).await.map_err(io_err)?;
            Ok(vec![serde_json::Value::from(u32::from(buf[0]))])
        })
    }

    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<crate::host::HttpResponse>>
    {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<Vec<u8>>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<()>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn send_email<'a>(
        &'a self,
        _request: &'a crate::outbound::SendEmailRequest,
    ) -> crate::host::dyn_context::BoxFuture<
        'a,
        fraiseql_error::Result<crate::outbound::SendEmailResponse>,
    > {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Err(fraiseql_error::FraiseQLError::validation("not wired"))
    }

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
        Ok(None)
    }

    fn event_payload(&self) -> &crate::EventPayload {
        &self.payload
    }

    fn log(&self, _level: crate::types::LogLevel, _message: &str) {}
}

/// #970 regression: a network resource created inside one invocation's host op
/// must remain usable in the next invocation's host op.
///
/// Every invocation runs on its own thread with its own throwaway Tokio runtime.
/// Before the fix, host-op futures executed on that runtime, so a socket opened
/// during invocation 1 registered with invocation 1's I/O reactor — which died
/// with its thread. Invocation 2 then awaited a read on a reactor that no longer
/// existed and burned its entire time budget ("event loop exceeded time limit").
/// This is exactly what a shared sqlx pool or reqwest keep-alive connection does
/// across firings, which alternately failed every second scheduled source firing
/// and poisoned the server's main DB pool. The fix pins all host I/O to the
/// owner runtime, where the reactor outlives every invocation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_op_resources_survive_across_invocations() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // 1-byte echo server on the OWNER runtime (alive for the whole test).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0_u8; 1];
                while sock.read_exact(&mut buf).await.is_ok() {
                    if sock.write_all(&buf).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    let event = test_event();
    let host = std::sync::Arc::new(ReusedStreamHost {
        payload: event.clone(),
        addr,
        stream: tokio::sync::Mutex::new(None),
    });
    let source = r#"
export default async () => {
  await Deno.core.ops.fraiseql_sql_query("ping", "[]");
  return { ok: true };
};
"#
    .to_string();
    let module = FunctionModule::from_source("reuse".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    for i in 0..2 {
        let result = runtime
            .invoke_with_context(&module, event.clone(), host.clone(), ResourceLimits::default())
            .await;
        assert!(
            result.is_ok(),
            "invocation {i} must succeed — a failure here means the stored stream's \
             reactor died with a previous invocation's runtime (#970): {result:?}"
        );
    }
}

/// #969 regression: the second V8 isolate in one process must not SIGSEGV.
///
/// Every Deno invocation runs on its own short-lived thread. Before the fix,
/// the V8 platform was lazily initialized as the PKU-protected default on the
/// FIRST invocation's thread; pkey rights on the JIT pages are inherited only
/// by that thread's descendants, so the second invocation in any process
/// faulted on the first JIT page it touched — crashing the whole server. #796
/// masked this in production (nothing ever fired twice per process) and
/// nextest's process-per-test model masked it locally: this test only means
/// something under plain `cargo test`, where the process is shared, and even
/// here only when it is not the sole test run. Three sequential invocations
/// still assert the per-invocation threads' isolates coexist with any isolate
/// another test in this binary already created.
#[tokio::test]
async fn two_sequential_invocations_in_one_process() {
    let source = "export default async (event) => event;".to_string();
    let module = FunctionModule::from_source("repeat".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");
    for i in 0..3 {
        let event = test_event();
        let event_data = event.data.clone();
        let result = runtime
            .invoke(
                &module,
                event.clone(),
                &crate::host::NoopHostContext::new(event),
                ResourceLimits::default(),
            )
            .await;
        assert!(result.is_ok(), "invocation {i} must succeed: {result:?}");
        assert_eq!(result.unwrap().value, Some(event_data), "invocation {i} returns the event");
    }
}

#[tokio::test]
async fn test_deno_execute_identity_js() {
    // JS module: `export default async (event) => event;`
    // Input: {"value": 42}
    // Expected: same JSON returned
    let source = r"
export default async (event) => {
    return event;
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_identity".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let event_data = event.data.clone();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should succeed and return the event as-is
    assert!(result.is_ok(), "Identity function should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.value, Some(event_data));
}

#[tokio::test]
async fn test_deno_execute_transform_js() {
    // JS module that adds a field — event is the entity data directly (event.data from the trigger)
    let source = r"
export default async (event) => {
    return { ...event, processed: true };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_transform".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Transform function should execute successfully");
    let result = result.unwrap();
    // Result should have the original data plus the new field
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert!(obj.contains_key("processed"), "Result should have 'processed' field");
        assert_eq!(obj["processed"], true);
        assert_eq!(obj["value"], 42);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_execute_typescript() {
    // TypeScript-style module: the host strips TS syntax before execution.
    // Event is the entity data directly (no nested .data field).
    let source = r"
export default async (event) => {
    return { result: event.value + 1 };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_typescript".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "TypeScript function should execute successfully");
    let result = result.unwrap();
    // Result should be { result: 43 }
    if let Some(serde_json::Value::Object(obj)) = result.value {
        assert_eq!(obj["result"], 43);
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_typescript_annotations_are_stripped() {
    // Real TypeScript — interface, `: Type` annotations, a constrained generic, an
    // `as` assertion — runs end-to-end because the runtime strips the types first.
    let source = r"
interface Deal { value: number; label: string }
function bump<T extends { value: number }>(d: T): number {
    return d.value + 1;
}
export default async (deal: Deal): Promise<{ result: number; label: string }> => {
    const next: number = bump(deal);
    return { result: next, label: deal.label as string };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_ts_annotations".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = EventPayload {
        data: serde_json::json!({"value": 42, "label": "acme"}),
        ..test_event()
    };
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "annotated TypeScript should run: {result:?}");
    if let Some(serde_json::Value::Object(obj)) = result.unwrap().value {
        assert_eq!(obj["result"], 43);
        assert_eq!(obj["label"], "acme");
    } else {
        panic!("Expected object result");
    }
}

#[tokio::test]
async fn test_deno_typescript_disabled_rejects_annotations() {
    // With type-stripping off, the same annotated source reaches V8 verbatim and is
    // a SyntaxError — proving `enable_typescript` actually gates the pass.
    let source = "export default async (deal: { value: number }): Promise<number> => deal.value;"
        .to_string();

    let module =
        FunctionModule::from_source("test_ts_disabled".to_string(), source, RuntimeType::Deno);
    let config = super::DenoConfig {
        enable_typescript: false,
        v8_flags:          vec![],
    };
    let runtime = super::DenoRuntime::new(&config).expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_err(), "annotated TS must fail when type-stripping is disabled");
}

#[tokio::test]
async fn test_deno_syntax_error_returns_validation() {
    // Invalid JavaScript
    let source = "export default async (event) => { broken syntax here }".to_string();

    let module =
        FunctionModule::from_source("test_syntax_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should return an error (either Validation for syntax error or Unsupported for stub)
    assert!(result.is_err(), "Syntax error should result in error");
    // When fully implemented, should be Validation error for SyntaxError
    // For now, stub returns Unsupported
}

#[tokio::test]
async fn test_deno_runtime_error_returns_internal() {
    // JavaScript that throws at runtime
    let source = r#"
export default async (event) => {
    throw new Error("Something went wrong");
};
"#
    .to_string();

    let module =
        FunctionModule::from_source("test_runtime_error".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should return an internal error
    assert!(result.is_err(), "Runtime error should result in error");
}

#[tokio::test]
async fn test_deno_name_returns_deno() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    assert_eq!(runtime.name(), "deno");
}

#[tokio::test]
async fn test_deno_supported_extensions() {
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let exts = runtime.supported_extensions();
    assert!(exts.contains(&".js"));
    assert!(exts.contains(&".ts"));
    assert!(exts.contains(&".mjs"));
    assert!(exts.contains(&".mts"));
}

// Cycle 2: V8 Resource Limits (Memory & CPU Timeouts)

#[tokio::test]
async fn test_deno_memory_limit_terminates() {
    // JS that allocates memory until limit is exceeded
    let source = r"
export default async (event) => {
    const arr = [];
    while (true) {
        arr.push(new ArrayBuffer(1024 * 1024)); // 1MB allocations
    }
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_memory_limit".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 64 * 1024 * 1024, // 64MB hard limit
        max_duration:     std::time::Duration::from_secs(5),
        max_log_entries:  10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return an error when memory limit is exceeded
    assert!(result.is_err(), "Memory limit exceeded should result in error");
}

#[tokio::test]
async fn test_deno_timeout_aborts() {
    // JS with infinite loop
    let source = r"
export default async (event) => {
    while (true) {}
};
"
    .to_string();

    let module = FunctionModule::from_source("test_timeout".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration:     std::time::Duration::from_millis(100), // 100ms timeout
        max_log_entries:  10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return a timeout error
    assert!(result.is_err(), "Infinite loop should timeout and return error");
    // When fully implemented, should be Timeout error
    // For now, stub will return some error
}

#[tokio::test]
async fn test_deno_within_limits_succeeds() {
    // Lightweight function that completes quickly
    let source = r"
export default async (event) => {
    return { result: 'success' };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_within_limits".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 256 * 1024 * 1024,                 // 256MB
        max_duration:     std::time::Duration::from_secs(5), // 5 second timeout
        max_log_entries:  10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should succeed
    assert!(result.is_ok(), "Function within limits should succeed");
}

#[tokio::test]
async fn test_deno_memory_peak_reported() {
    // Function that allocates some memory but stays within limit
    let source = r"
export default async (event) => {
    const arr = [];
    for (let i = 0; i < 100; i++) {
        arr.push(new ArrayBuffer(1024)); // 100KB total
    }
    return { allocated: true };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_memory_peak".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should succeed
    assert!(result.is_ok(), "Memory allocation within bounds should succeed");
    let result = result.unwrap();
    // Memory peak should be reported (non-zero if successfully measured)
    // Current stub returns 0, but when fully implemented should be > 0
    let _memory_peak = result.memory_peak_bytes;
}

#[tokio::test]
async fn test_deno_async_timeout() {
    // Async function that never resolves (hangs forever)
    let source = r"
export default async (event) => {
    return new Promise(() => {
        // Never resolves — intentional hang
    });
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_async_timeout".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration:     std::time::Duration::from_millis(200), // 200ms timeout
        max_log_entries:  10_000,
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should return a timeout error
    assert!(result.is_err(), "Unresolved promise with timeout should error");
    // When fully implemented, should be Timeout error
}

/// #804: a host whose `http_request` is genuinely pending (awaits a timer), so
/// the guest's continuation after `await` provably resumes inside
/// `run_event_loop`, not during `execute_script`'s microtask drain.
#[allow(dead_code)] // Reason: constructed only by the #[tokio::test] harness build
struct PendingOpHost {
    event: EventPayload,
}

impl crate::host::dyn_context::DynHostContext for PendingOpHost {
    fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<serde_json::Value>> {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }

    fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<Vec<serde_json::Value>>>
    {
        Box::pin(async { Ok(vec![]) })
    }

    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<crate::host::HttpResponse>>
    {
        Box::pin(async {
            // Genuinely pending: the promise resolves via the event loop.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(crate::host::HttpResponse {
                status:  200,
                headers: vec![],
                body:    b"{}".to_vec(),
            })
        })
    }

    fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<Vec<u8>>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> crate::host::dyn_context::BoxFuture<'_, fraiseql_error::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn send_email<'a>(
        &'a self,
        _request: &'a crate::outbound::SendEmailRequest,
    ) -> crate::host::dyn_context::BoxFuture<
        'a,
        fraiseql_error::Result<crate::outbound::SendEmailResponse>,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "not configured".to_string(),
            })
        })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event
    }

    fn log(&self, _level: crate::types::LogLevel, _message: &str) {}
}

/// #804: a guest that spins *after* awaiting a real async host op must still be
/// terminated at `max_duration`. Pre-fix, `watchdog_done` was stored `true`
/// immediately after `execute_script` returned — before `run_event_loop` ran —
/// so the continuation's `while (true) {}` had no armed watchdog, the
/// event-loop `tokio::time::timeout` future was never polled again, and the
/// executor thread + isolate wedged at 100% CPU forever.
#[tokio::test]
async fn test_deno_sync_spin_after_await_times_out() {
    let source = r#"
export default async (event) => {
    try { await Deno.core.ops.fraiseql_http_request("GET", "http://pending.test/", [], null); } catch (e) {}
    while (true) {}
};
"#
    .to_string();

    let module =
        FunctionModule::from_source("spin_after_await".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration:     std::time::Duration::from_millis(200),
        max_log_entries:  10_000,
    };

    let host: std::sync::Arc<dyn crate::host::dyn_context::DynHostContext> =
        std::sync::Arc::new(PendingOpHost {
            event: event.clone(),
        });
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        runtime.invoke_with_context(&module, event, host, limits),
    )
    .await;

    let inner = result.expect("executor hung past 25x max_duration — the watchdog is disarmed");
    let error = inner.expect_err("a spinning guest must be a timeout error, not a result");
    assert!(
        error.to_string().to_lowercase().contains("timeout"),
        "expected a timeout classification, got: {error}"
    );
}

// Cycle 3: Deno Host Ops (Structured Logging)

#[tokio::test]
async fn test_deno_guest_can_call_log() {
    // JavaScript that calls the fraiseql_log host op
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(1, 'hello from deno');
    return { logged: true };
};
"
    .to_string();

    let module = FunctionModule::from_source("test_log".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    // Should succeed and have a log entry
    assert!(result.is_ok(), "Log call should execute successfully");
    let result = result.unwrap();
    assert!(!result.logs.is_empty(), "Result should have at least one log entry");

    // Check the log entry
    let log = &result.logs[0];
    assert_eq!(log.level, crate::LogLevel::Info);
    assert_eq!(log.message, "hello from deno");
}

#[tokio::test]
async fn test_deno_guest_log_levels() {
    // JavaScript that logs at different levels
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(0, 'debug message');
    Deno.core.ops.fraiseql_log(1, 'info message');
    Deno.core.ops.fraiseql_log(2, 'warn message');
    Deno.core.ops.fraiseql_log(3, 'error message');
    return { logged: true };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_log_levels".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_ok(), "Log calls should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 4, "Should have 4 log entries");

    // Verify each log level
    assert_eq!(result.logs[0].level, crate::LogLevel::Debug);
    assert_eq!(result.logs[0].message, "debug message");

    assert_eq!(result.logs[1].level, crate::LogLevel::Info);
    assert_eq!(result.logs[1].message, "info message");

    assert_eq!(result.logs[2].level, crate::LogLevel::Warn);
    assert_eq!(result.logs[2].message, "warn message");

    assert_eq!(result.logs[3].level, crate::LogLevel::Error);
    assert_eq!(result.logs[3].message, "error message");
}

#[tokio::test]
async fn test_deno_guest_log_with_timestamp() {
    // JavaScript that logs a message
    // Verify that each log entry has a proper timestamp
    let source = r"
export default async (event) => {
    Deno.core.ops.fraiseql_log(1, 'test message with timestamp');
    return { done: true };
};
"
    .to_string();

    let module =
        FunctionModule::from_source("test_log_timestamp".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let before = chrono::Utc::now();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;
    let after = chrono::Utc::now();

    // Should succeed and have captured log with proper timestamp
    assert!(result.is_ok(), "Log call should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 1);
    assert_eq!(result.logs[0].message, "test message with timestamp");

    // Verify timestamp is within the invocation period
    assert!(result.logs[0].timestamp >= before);
    assert!(result.logs[0].timestamp <= after);
}

#[tokio::test]
async fn test_deno_guest_log_limit_enforced() {
    // Generate JavaScript with many individual log calls (can't use a loop in stub)
    use std::fmt::Write;
    let mut source = String::from("export default async (event) => {\n");
    for i in 0..1500 {
        writeln!(source, "    Deno.core.ops.fraiseql_log(1, 'log entry {}');", i)
            .expect("write to string");
    }
    source.push_str("    return { logged: true };\n};\n");

    let module =
        FunctionModule::from_source("test_log_limit".to_string(), source, RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");

    let event = test_event();
    let limits = ResourceLimits {
        max_memory_bytes: 128 * 1024 * 1024,
        max_duration:     std::time::Duration::from_secs(5),
        max_log_entries:  1000, // Only allow 1000 logs
    };

    let result = runtime
        .invoke(&module, event.clone(), &crate::host::NoopHostContext::new(event), limits)
        .await;

    // Should succeed but with limited logs
    assert!(result.is_ok(), "Log calls should execute successfully");
    let result = result.unwrap();
    assert_eq!(result.logs.len(), 1000, "Should cap logs at max_log_entries");

    // Verify the last captured log
    assert_eq!(result.logs[999].message, "log entry 999");
}

// ========== Cycle 2: Deno host-op bridge (async I/O → HostContext) ==========
//
// These exercise the real `Deno.core.ops.fraiseql_*` bridge through
// `DenoRuntime::invoke_with_context`, using a self-contained mock host so they run
// in the plain `runtime-deno` leg (no `host-live`/network needed).

/// A canned [`HostContext`](crate::HostContext) that echoes its inputs back as
/// deterministic outputs, so a guest can prove it reached the host.
#[allow(dead_code)] // Reason: used only by #[test] fns, which are stripped from the lib build
struct MockHostContext {
    event_payload: crate::EventPayload,
}

impl MockHostContext {
    #[allow(dead_code)] // Reason: used only by #[test] fns, which are stripped from the lib build
    fn new() -> Self {
        Self {
            event_payload: test_event(),
        }
    }
}

// Reason: `HostContext` is async because the live implementation performs I/O —
// GraphQL queries, HTTP calls, secret lookups. Impls whose methods refuse or
// answer from memory still have to present the awaited signature the trait
// defines and every guest call site uses.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl crate::HostContext for MockHostContext {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "graphql": graphql,
            "variables": variables,
            "data": { "ok": true },
        }))
    }

    async fn sql_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> fraiseql_error::Result<Vec<serde_json::Value>> {
        Ok(vec![serde_json::json!({ "sql": sql, "params": params })])
    }

    async fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> fraiseql_error::Result<crate::host::HttpResponse> {
        let body_len = body.map_or(0, <[u8]>::len);
        Ok(crate::host::HttpResponse {
            status:  200,
            headers: vec![("x-mock".to_string(), "true".to_string())],
            body:    format!("{method} {url} headers={} body={body_len}", headers.len())
                .into_bytes(),
        })
    }

    async fn storage_get(&self, bucket: &str, key: &str) -> fraiseql_error::Result<Vec<u8>> {
        Ok(format!("stored:{bucket}/{key}").into_bytes())
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

    async fn send_email(
        &self,
        request: &crate::outbound::SendEmailRequest,
    ) -> fraiseql_error::Result<crate::outbound::SendEmailResponse> {
        // Echo the recipient so a guest test can prove the request JSON marshaled
        // through the op intact.
        Ok(crate::outbound::SendEmailResponse {
            message_id: Some(format!("sent:{}", request.to)),
            accepted:   true,
        })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({ "user_id": "u123", "roles": ["admin"] }))
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        if name == "MODEL_KEY" {
            Ok(Some("sk-test".to_string()))
        } else {
            Ok(None)
        }
    }

    fn event_payload(&self) -> &crate::EventPayload {
        &self.event_payload
    }

    fn log(&self, _level: crate::LogLevel, _message: &str) {}
}

/// Run a guest through `invoke_with_context` with the mock host and return its value.
#[allow(dead_code)] // Reason: used only by #[test] fns, which are stripped from the lib build
async fn run_with_mock(source: &str) -> fraiseql_error::Result<serde_json::Value> {
    let module =
        FunctionModule::from_source("mock_op".to_string(), source.to_string(), RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");
    let host: std::sync::Arc<dyn crate::host::dyn_context::DynHostContext> =
        std::sync::Arc::new(MockHostContext::new());
    runtime
        .invoke_with_context(&module, test_event(), host, ResourceLimits::default())
        .await
        .map(|r| r.value.unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn test_deno_op_http_request_reaches_host() {
    // GET, no body — asserts the mock's echo comes back and the Uint8Array body
    // decodes (String.fromCharCode avoids needing the deno_web TextDecoder).
    let value = run_with_mock(
        r"
export default async () => {
    const resp = await Deno.core.ops.fraiseql_http_request(
        'GET', 'https://api.test/x', [['accept', 'application/json']], null);
    return { status: resp.status, body: String.fromCharCode(...resp.body) };
};
",
    )
    .await
    .expect("http_request op should succeed");

    assert_eq!(value["status"], 200);
    assert_eq!(value["body"], "GET https://api.test/x headers=1 body=0");
}

#[tokio::test]
async fn test_deno_op_http_request_sends_body() {
    let value = run_with_mock(
        r"
export default async () => {
    const resp = await Deno.core.ops.fraiseql_http_request(
        'POST', 'https://api.test/x', [], new Uint8Array([1, 2, 3, 4, 5]));
    return { body: String.fromCharCode(...resp.body) };
};
",
    )
    .await
    .expect("http_request op with body should succeed");

    assert_eq!(value["body"], "POST https://api.test/x headers=0 body=5");
}

#[tokio::test]
async fn test_deno_op_query_reaches_host() {
    let value = run_with_mock(
        r"
export default async () => {
    const raw = await Deno.core.ops.fraiseql_query('{ users { id } }', JSON.stringify({ limit: 5 }));
    return JSON.parse(raw);
};
",
    )
    .await
    .expect("query op should succeed");

    assert_eq!(value["graphql"], "{ users { id } }");
    assert_eq!(value["variables"]["limit"], 5);
    assert_eq!(value["data"]["ok"], true);
}

#[tokio::test]
async fn test_deno_op_storage_round_trip() {
    let value = run_with_mock(
        r"
export default async () => {
    await Deno.core.ops.fraiseql_storage_put('b', 'k', new Uint8Array([104, 105]), 'text/plain');
    const data = await Deno.core.ops.fraiseql_storage_get('bucket', 'key');
    return { got: String.fromCharCode(...data) };
};
",
    )
    .await
    .expect("storage ops should succeed");

    assert_eq!(value["got"], "stored:bucket/key");
}

#[tokio::test]
async fn test_deno_op_send_email_reaches_host() {
    let value = run_with_mock(
        r"
export default async () => {
    const raw = await Deno.core.ops.fraiseql_send_email(
        JSON.stringify({ to: 'bob@example.com', subject: 'hi', text: 'body' }),
    );
    return JSON.parse(raw);
};
",
    )
    .await
    .expect("send_email op should succeed");

    // The op reached the host and the request JSON marshaled through intact.
    assert_eq!(value["accepted"], true);
    assert_eq!(value["message_id"], "sent:bob@example.com");
}

#[tokio::test]
async fn test_deno_guest_can_tag_an_error_permanent() {
    // A guest that structurally tags its throw as permanent crosses the exception
    // boundary as a 4xx client error → durable dispatch dead-letters immediately.
    let error = run_with_mock(
        r"
export default async () => {
    throw Object.assign(new Error('nope'), { fraiseqlPermanent: true });
};
",
    )
    .await
    .expect_err("guest threw");
    assert!(error.is_client_error(), "a permanent-tagged guest error is a 4xx");
}

#[tokio::test]
async fn test_deno_untagged_guest_error_stays_transient() {
    // Default behaviour is unchanged: an untagged throw is transient (501), retried.
    let error = run_with_mock(
        r"
export default async () => { throw new Error('boom'); };
",
    )
    .await
    .expect_err("guest threw");
    assert!(!error.is_client_error(), "an untagged guest error stays transient");
    assert_eq!(error.status_code(), 501);
}

#[tokio::test]
async fn test_deno_op_auth_context_and_env_var() {
    let value = run_with_mock(
        r"
export default async () => {
    const auth = JSON.parse(Deno.core.ops.fraiseql_auth_context());
    const key = Deno.core.ops.fraiseql_env_var('MODEL_KEY');
    const missing = Deno.core.ops.fraiseql_env_var('DOES_NOT_EXIST');
    return { user: auth.user_id, key, missing };
};
",
    )
    .await
    .expect("auth/env ops should succeed");

    assert_eq!(value["user"], "u123");
    assert_eq!(value["key"], "sk-test");
    assert_eq!(value["missing"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_deno_op_without_host_fails_loud() {
    // On the sync `invoke` path there is no host: an I/O op must fail loud, not
    // silently return empty data.
    let source = r"
export default async () => {
    const resp = await Deno.core.ops.fraiseql_http_request('GET', 'https://x', [], null);
    return { status: resp.status };
};
";
    let module =
        FunctionModule::from_source("no_host".to_string(), source.to_string(), RuntimeType::Deno);
    let runtime = super::DenoRuntime::new(&super::DenoConfig::default())
        .expect("Failed to create DenoRuntime");
    let event = test_event();
    let result = runtime
        .invoke(
            &module,
            event.clone(),
            &crate::host::NoopHostContext::new(event),
            ResourceLimits::default(),
        )
        .await;

    assert!(result.is_err(), "host op on the no-host path must fail loud");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("host context unavailable"),
        "error should explain the missing host context, got: {msg}"
    );
}
