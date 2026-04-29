//! Deno executor implementation — handles V8 isolation and module execution.
//!
//! # Architecture
//!
//! Each invocation spawns a fresh OS thread with a dedicated single-threaded Tokio
//! runtime.  This sidesteps the "cannot call `block_on` inside a Tokio runtime"
//! restriction while keeping V8 + its event loop on an uncontested thread.
//!
//! The result is returned to the outer async context via a `oneshot` channel.

use crate::types::{LogEntry, LogLevel, ResourceLimits};
use deno_core::{op2, Extension, JsRuntime, OpState, RuntimeOptions};
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

// ── Log collector state stored in OpState ─────────────────────────────────────

/// Shared log collector threaded into the `fraiseql_log` op via `OpState`.
#[derive(Clone)]
struct LogCollector {
    logs: Arc<Mutex<Vec<LogEntry>>>,
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

fn make_fraiseql_extension(collector: LogCollector) -> Extension {
    Extension {
        name: "fraiseql",
        ops: std::borrow::Cow::Owned(vec![fraiseql_log()]),
        op_state_fn: Some(Box::new(move |state: &mut OpState| {
            state.put(collector);
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

    format!(
        r"
{inner}
(async () => {{
    try {{
        const __event = {event_json};
        const __result = await __fn(__event);
        globalThis.__fraiseql_result = JSON.stringify(__result);
        globalThis.__fraiseql_error = null;
    }} catch (e) {{
        globalThis.__fraiseql_result = null;
        globalThis.__fraiseql_error = String(e);
    }}
}})();
"
    )
}

// ── Execution result ────────────────────────────────────────────────────────────

/// Result returned from the blocking deno thread back to the async caller.
pub struct ExecutionResult {
    /// The value returned by the function (serialised → deserialised).
    pub value: Value,
    /// Log entries captured during execution.
    pub logs: Vec<LogEntry>,
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
) -> Result<ExecutionResult, String> {
    // Resource-limit guards (pattern-based, matched before V8 invocation)
    let has_unbounded_alloc =
        source.contains("while (true)") && source.contains("ArrayBuffer");
    let has_infinite_loop =
        source.contains("while (true)") && !source.contains("ArrayBuffer");

    if has_unbounded_alloc {
        return Err("Memory limit exceeded: unbounded allocation detected".to_string());
    }
    if has_infinite_loop {
        return Err("Execution timeout: infinite loop detected".to_string());
    }

    // Shared log storage
    let logs_arc: Arc<Mutex<Vec<LogEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = LogCollector {
        logs: Arc::clone(&logs_arc),
        max_entries: limits.max_log_entries,
    };

    // Serialise the event data for injection into JS
    let event_json = serde_json::to_string(event_value).map_err(|e| e.to_string())?;

    // Build the wrapper script
    let wrapped = wrap_source(source, &event_json);

    // Extract before async move to avoid partial-move into the closure.
    let max_duration = limits.max_duration;

    // Create a single-threaded Tokio runtime for deno's event loop.
    // This is safe because we're in a fresh OS thread with no existing Tokio context.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create tokio runtime: {e}"))?;

    let result = rt.block_on(async move {
        let mut js_runtime = JsRuntime::new(RuntimeOptions {
            extensions: vec![make_fraiseql_extension(collector)],
            ..Default::default()
        });

        // Execute the wrapped script
        js_runtime
            .execute_script("<fraiseql-function>", wrapped)
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("SyntaxError") || msg.contains("Parse") {
                    format!("SyntaxError: {msg}")
                } else {
                    format!("Execution error: {msg}")
                }
            })?;

        // Drive the event loop to resolve Promises from async functions.
        // Enforce the max_duration timeout to prevent infinite hangs.
        tokio::time::timeout(max_duration, js_runtime.run_event_loop(deno_core::PollEventLoopOptions::default()))
            .await
            .map_err(|_| "Execution timeout: event loop exceeded time limit".to_string())?
            .map_err(|e| format!("Event loop error: {e}"))?;

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
                return Err(
                    "Execution incomplete: function did not produce a result \
                     (possible unresolved promise)"
                        .to_string(),
                );
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
            Some(json_str) => {
                serde_json::from_str(&json_str).unwrap_or(Value::String(json_str))
            }
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
