//! Deno executor implementation — handles V8 isolation and module execution.
//!
//! # Architecture
//!
//! Each invocation spawns a fresh OS thread with a dedicated single-threaded Tokio
//! runtime.  This sidesteps the "cannot call `block_on` inside a Tokio runtime"
//! restriction while keeping V8 + its event loop on an uncontested thread.
//!
//! The result is returned to the outer async context via a `oneshot` channel.

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use deno_core::{Extension, JsRuntime, OpState, RuntimeOptions, op2, v8};
use serde_json::Value;

use super::ops::DenoHostContext;
use crate::{
    host::dyn_context::DynHostContext,
    types::{LogEntry, LogLevel, ResourceLimits},
};

// ── Log collector state stored in OpState ─────────────────────────────────────

/// Shared log collector threaded into the `fraiseql_log` op via `OpState`.
#[derive(Clone)]
struct LogCollector {
    logs:        Arc<Mutex<Vec<LogEntry>>>,
    max_entries: usize,
}

// ── Op definitions ─────────────────────────────────────────────────────────────

/// Log a message from Deno guest code.
///
/// Called by guests as `Deno.core.ops.fraiseql_log(level, message)`.
/// Levels: 0 = debug, 1 = info, 2 = warn, 3 = error.
#[op2(fast)]
#[allow(clippy::inline_always)] // Reason: emitted by the `#[op2]` proc-macro, not our code
#[allow(clippy::needless_pass_by_value)] // Reason: `#[op2]` requires owned `String` for `#[string]`
fn fraiseql_log(state: Rc<RefCell<OpState>>, #[smi] level: u8, #[string] message: String) {
    let state = state.borrow();
    let collector = state.borrow::<LogCollector>();
    let mut logs = collector.logs.lock().expect("log mutex poisoned");
    if logs.len() < collector.max_entries {
        let log_level = match level {
            0 => LogLevel::Debug,
            2 => LogLevel::Warn,
            3 => LogLevel::Error,
            _ => LogLevel::Info,
        };
        logs.push(LogEntry {
            level: log_level,
            message,
            timestamp: chrono::Utc::now(),
        });
    }
}

// ── Extension builder ──────────────────────────────────────────────────────────

fn make_fraiseql_extension(
    collector: LogCollector,
    host: Option<Arc<dyn DynHostContext>>,
) -> Extension {
    use super::ops;
    Extension {
        name: "fraiseql",
        ops: std::borrow::Cow::Owned(vec![
            fraiseql_log(),
            ops::fraiseql_query(),
            ops::fraiseql_sql_query(),
            ops::fraiseql_http_request(),
            ops::fraiseql_storage_get(),
            ops::fraiseql_storage_put(),
            ops::fraiseql_send_email(),
            ops::fraiseql_auth_context(),
            ops::fraiseql_env_var(),
            ops::fraiseql_idempotency_token(),
            ops::fraiseql_cursor_get(),
            ops::fraiseql_cursor_advance(),
        ]),
        op_state_fn: Some(Box::new(move |state: &mut OpState| {
            state.put(collector);
            if let Some(host) = host {
                state.put(DenoHostContext(host));
            }
        })),
        ..Default::default()
    }
}

// ── Source preprocessing ────────────────────────────────────────────────────────

/// Wrap the user-provided source so the default-exported function is called with
/// the event data and the result stored in `globalThis.__fraiseql_result`.
///
/// Handles both:
///   `export default async (event) => { ... };`
///   `export default async function(event) { ... }`
///
/// The wrapped code also catches any thrown exception and stores it in
/// `globalThis.__fraiseql_error`.
fn wrap_source(source: &str, event_json: &str) -> String {
    // Convert "export default <expr>" to "const __fn = <expr>"
    let inner = source
        .replace("export default async function", "const __fn = async function")
        .replace("export default async", "const __fn = async")
        .replace("export default function", "const __fn = function")
        .replace("export default", "const __fn =");

    // Injected so a guest can mark a thrown error permanent structurally
    // (`throw Object.assign(new Error(msg), {{ fraiseqlPermanent: true }})`) and the
    // runtime dead-letters it immediately. Host-op errors already carry the marker
    // in their message; this folds the property form into the same signal.
    let marker = crate::types::PERMANENT_ERROR_MARKER;
    format!(
        r#"
{inner}
(async () => {{
    try {{
        const __event = {event_json};
        const __result = await __fn(__event);
        globalThis.__fraiseql_result = JSON.stringify(__result);
        globalThis.__fraiseql_error = null;
    }} catch (e) {{
        globalThis.__fraiseql_result = null;
        const __permanent = e && e.fraiseqlPermanent === true;
        globalThis.__fraiseql_error = (__permanent ? "{marker} " : "") + String(e);
    }}
}})();
"#
    )
}

// ── Execution result ────────────────────────────────────────────────────────────

/// Result returned from the blocking deno thread back to the async caller.
pub struct ExecutionResult {
    /// The value returned by the function (serialised → deserialised).
    pub value: Value,
    /// Log entries captured during execution.
    pub logs:  Vec<LogEntry>,
}

// ── Core execution (runs on a dedicated thread with its own Tokio runtime) ─────

/// Execute guest `JavaScript` inside a fresh `JsRuntime` + dedicated Tokio runtime.
///
/// This function is called from inside `std::thread::spawn`, so it is free to
/// create its own `current_thread` Tokio runtime and call `block_on` without
/// conflict.
///
/// # Errors
///
/// Returns an error string on syntax errors, runtime exceptions, timeout, or
/// resource-limit violations.
///
/// # Panics
///
/// Panics if the internal log mutex is poisoned (only possible if another thread
/// panicked while holding the lock, which cannot happen in normal operation).
pub fn run_in_dedicated_thread(
    source: &str,
    event_value: &Value,
    limits: &ResourceLimits,
    host: Option<Arc<dyn DynHostContext>>,
) -> Result<ExecutionResult, String> {
    // Resource limits are enforced for real against the V8 isolate (M-deno-limits):
    //
    //   * Memory: the isolate is created with a hard heap limit (`max_memory_bytes`) and a
    //     near-heap-limit callback that terminates execution when V8 approaches that limit.
    //   * CPU/time: a watchdog thread terminates the isolate after `max_duration`, catching tight
    //     synchronous loops (`while (true) {}`) that never yield to the async event loop, in
    //     addition to the event-loop `tokio::time::timeout` guard that catches async hangs
    //     (unresolved promises).
    //
    // The previous substring heuristics (matching `while (true)` / `ArrayBuffer` in
    // the source) were removed: they false-positived on those substrings appearing
    // in comments or string literals and missed every other form of DoS.

    // Shared log storage
    let logs_arc: Arc<Mutex<Vec<LogEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = LogCollector {
        logs:        Arc::clone(&logs_arc),
        max_entries: limits.max_log_entries,
    };

    // Serialise the event data for injection into JS
    let event_json = serde_json::to_string(event_value).map_err(|e| e.to_string())?;

    // Build the wrapper script
    let wrapped = wrap_source(source, &event_json);

    // Extract before async move to avoid partial-move into the closure.
    let max_duration = limits.max_duration;
    // V8's heap limit is expressed in `usize`; saturate on 32-bit targets where the
    // configured `u64` limit could exceed the address space.
    let max_memory_bytes = usize::try_from(limits.max_memory_bytes).unwrap_or(usize::MAX);

    // Flags shared with the heap-limit callback and the watchdog thread so that,
    // once V8 execution is terminated, we can report *why* (memory vs. timeout)
    // rather than surfacing the opaque "execution terminated" V8 error.
    let mem_exceeded_run = Arc::new(AtomicBool::new(false));
    let timed_out_run = Arc::new(AtomicBool::new(false));

    // Create a single-threaded Tokio runtime for deno's event loop.
    // This is safe because we're in a fresh OS thread with no existing Tokio context.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create tokio runtime: {e}"))?;

    let result = rt.block_on(async move {
        // Hard heap limit enforced by V8: the isolate may not grow past
        // `max_memory_bytes`. `0` initial lets V8 pick a sane starting size.
        let create_params = v8::CreateParams::default().heap_limits(0, max_memory_bytes);

        let mut js_runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![make_fraiseql_extension(collector, host)],
            create_params: Some(create_params),
            ..Default::default()
        });

        // Watchdog: terminate the isolate after `max_duration`. This catches tight
        // *synchronous* loops (`while (true) {}`) that never yield to the async
        // event loop, so the `tokio::time::timeout` below cannot fire. The handle
        // is thread-safe (Send + Sync) and may be invoked from any thread.
        let isolate_handle = js_runtime.v8_isolate().thread_safe_handle();
        let watchdog_done = Arc::new(AtomicBool::new(false));
        let watchdog_done_thread = Arc::clone(&watchdog_done);
        let timed_out_watchdog = Arc::clone(&timed_out_run);
        let watchdog = std::thread::spawn(move || {
            // Poll so we can exit promptly once the script finishes, without
            // waiting out the full deadline.
            let deadline = std::time::Instant::now() + max_duration;
            let poll = std::time::Duration::from_millis(10);
            while std::time::Instant::now() < deadline {
                if watchdog_done_thread.load(Ordering::Acquire) {
                    return;
                }
                std::thread::sleep(poll);
            }
            if !watchdog_done_thread.load(Ordering::Acquire) {
                timed_out_watchdog.store(true, Ordering::Release);
                isolate_handle.terminate_execution();
            }
        });

        // Near-heap-limit callback: when V8 approaches the hard limit, flag the
        // condition and terminate execution. We bump the returned limit so V8 does
        // not `FatalProcessOutOfMemory` (which would abort the whole process)
        // before the termination exception propagates out of the running script.
        let mem_flag = Arc::clone(&mem_exceeded_run);
        let heap_handle = js_runtime.v8_isolate().thread_safe_handle();
        js_runtime.add_near_heap_limit_callback(move |current_limit, _initial| {
            mem_flag.store(true, Ordering::Release);
            heap_handle.terminate_execution();
            // Grant headroom so V8 can unwind via the termination exception instead
            // of crashing the process.
            current_limit.saturating_add(current_limit / 2).max(current_limit + 1)
        });

        // Helper to convert a raw V8/deno error into a meaningful message,
        // distinguishing limit-driven termination from ordinary failures.
        let classify = |raw: &str, mem: &Arc<AtomicBool>, time: &Arc<AtomicBool>| -> String {
            if mem.load(Ordering::Acquire) {
                "Memory limit exceeded: heap allocation exceeded the configured limit".to_string()
            } else if time.load(Ordering::Acquire) {
                "Execution timeout: script exceeded the configured time limit".to_string()
            } else if raw.contains("SyntaxError") || raw.contains("Parse") {
                format!("SyntaxError: {raw}")
            } else {
                format!("Execution error: {raw}")
            }
        };

        // Execute the wrapped script
        let exec_outcome = js_runtime.execute_script("<fraiseql-function>", wrapped);

        // Stop the watchdog as soon as the (possibly terminating) script returns.
        watchdog_done.store(true, Ordering::Release);

        if let Err(e) = exec_outcome {
            return Err(classify(&e.to_string(), &mem_exceeded_run, &timed_out_run));
        }

        // Drive the event loop to resolve Promises from async functions.
        // Enforce the max_duration timeout to prevent infinite hangs.
        let loop_outcome = tokio::time::timeout(
            max_duration,
            js_runtime.run_event_loop(deno_core::PollEventLoopOptions::default()),
        )
        .await;

        match loop_outcome {
            Err(_) => {
                return Err("Execution timeout: event loop exceeded time limit".to_string());
            },
            Ok(Err(e)) => {
                return Err(classify(&e.to_string(), &mem_exceeded_run, &timed_out_run));
            },
            Ok(Ok(())) => {},
        }

        // Script and event loop finished cleanly: stop and reap the watchdog.
        watchdog_done.store(true, Ordering::Release);
        let _ = watchdog.join();

        // Retrieve the result stored in globalThis.__fraiseql_result
        let result_global = js_runtime
            .execute_script("<get-result>", "globalThis.__fraiseql_result")
            .map_err(|e| format!("Failed to read result: {e}"))?;

        let error_global = js_runtime
            .execute_script("<get-error>", "globalThis.__fraiseql_error")
            .map_err(|e| format!("Failed to read error: {e}"))?;

        // Inspect the values via a V8 handle scope
        let (result_json, error_str) = {
            let scope = &mut js_runtime.handle_scope();

            let result_local = deno_core::v8::Local::new(scope, result_global);
            let error_local = deno_core::v8::Local::new(scope, error_global);

            // Both undefined means the IIFE wrapper never completed (e.g. an unresolvable
            // Promise caused the event loop to drain without the try/catch finalising).
            if result_local.is_undefined() && error_local.is_undefined() {
                return Err("Execution incomplete: function did not produce a result \
                     (possible unresolved promise)"
                    .to_string());
            }

            let error_str = if error_local.is_null_or_undefined() {
                None
            } else {
                Some(error_local.to_rust_string_lossy(scope))
            };

            let result_json = if result_local.is_null_or_undefined() {
                None
            } else {
                Some(result_local.to_rust_string_lossy(scope))
            };

            (result_json, error_str)
        };

        // If the guest threw an error, propagate it
        if let Some(err) = error_str {
            return Err(format!("Runtime error: {err}"));
        }

        // Parse the JSON result
        let value: Value = match result_json {
            Some(json_str) => serde_json::from_str(&json_str).unwrap_or(Value::String(json_str)),
            None => Value::Null,
        };

        Ok(value)
    });

    // Collect logs
    let logs = logs_arc.lock().expect("log mutex poisoned").clone();

    match result {
        Ok(value) => Ok(ExecutionResult { value, logs }),
        Err(e) => Err(e),
    }
}
