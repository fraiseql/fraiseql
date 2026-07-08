//! Deno (`JavaScript`/`TypeScript`) runtime for function execution via V8.
//!
//! This module provides `DenoRuntime`, which executes `JavaScript` and `TypeScript` functions
//! using the Deno core runtime with embedded V8 isolates.
//!
//! # Architecture
//!
//! The Deno runtime is an opt-in feature (`runtime-deno`) due to V8's binary size (~30MB)
//! and compile-time impact. When disabled, there is zero impact on compilation time or binary size.
//!
//! Each execution:
//! 1. Creates a new V8 isolate with memory and timeout limits
//! 2. Loads the function source. When [`DenoConfig::enable_typescript`] is set (the default),
//!    `TypeScript` types are stripped to executable `JavaScript` first (see [`transpile`]); with it
//!    off the isolate executes the `JavaScript` unchanged.
//! 3. Calls the default export with the event as a JS object
//! 4. Captures logs and enforces resource limits throughout
//! 5. Properly cleans up the isolate after execution

pub mod executor;
pub mod ops;
pub mod tests;
pub mod transpile;

#[cfg(test)]
mod follow_up_tests;
#[cfg(test)]
mod transpile_tests;
// Uses the real `LiveHostContext` (host-live) to prove the op end-to-end, so it is
// gated on host-live as well — the `runtime-deno`-only clippy combo has no live host.
#[cfg(all(test, feature = "host-live"))]
mod idempotency_tests;
// Model B cursor ops end-to-end on a source-bound `LiveHostContext` (host-live) +
// real Postgres; local-only like every deno test (V8 SIGSEGVs in the CI sandbox).
#[cfg(all(test, feature = "host-live"))]
mod cursor_tests;
#[cfg(test)]
mod qonto_tests;
#[cfg(test)]
mod reply_awareness_tests;
#[cfg(test)]
mod scoring_tests;

use std::sync::Arc;

use fraiseql_error::Result;

use crate::{
    HostContext,
    host::dyn_context::DynHostContext,
    runtime::FunctionRuntime,
    types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits},
};

/// `TypeScript` ambient declarations for the FraiseQL native-functions host surface.
///
/// The host operations are reached as `Deno.core.ops.fraiseql_*`, with
/// `Deno.core.encode` / `Deno.core.decode` bridging strings and byte buffers. This
/// is the canonical shape shipped to authors as `examples/native-functions/fraiseql-host.d.ts`
/// (kept in sync with this constant); reference it from a function to get
/// type-checking and editor autocomplete.
pub const FRAISEQL_HOST_TYPES: &str = r"
// FraiseQL native-functions host surface (Deno.core.ops.fraiseql_*).

interface FraiseqlHttpResponse {
  status: number;
  headers: Array<[string, string]>;
  body: Uint8Array;
}

interface FraiseqlHostOps {
  // Execute a GraphQL query/mutation. `variables` is a JSON string; returns a JSON string.
  fraiseql_query(graphql: string, variables: string): Promise<string>;
  // Execute a raw SQL query. `params` is a JSON array string; returns a JSON array string.
  fraiseql_sql_query(sql: string, params: string): Promise<string>;
  // Make an outbound HTTP request (SSRF-allowlisted by the host).
  fraiseql_http_request(
    method: string,
    url: string,
    headers: Array<[string, string]>,
    body: Uint8Array | null,
  ): Promise<FraiseqlHttpResponse>;
  // Object storage.
  fraiseql_storage_get(bucket: string, key: string): Promise<Uint8Array>;
  fraiseql_storage_put(
    bucket: string,
    key: string,
    body: Uint8Array,
    contentType: string,
  ): Promise<void>;
  // Send an email. `from` is host-owned; the request JSON carries only
  // { to, subject, text?, html?, reply_to? }.
  fraiseql_send_email(request: string): Promise<string>;
  // The authenticated caller's context, as a JSON string.
  fraiseql_auth_context(): string;
  // Read a host-allowlisted environment variable, or null.
  fraiseql_env_var(name: string): string | null;
  // Per-dispatch idempotency token, or null on a non-durably-dispatched invocation.
  fraiseql_idempotency_token(): string | null;
  // Scheduled-source cursor (Model B). `fraiseql_cursor_get` returns the JSON string
  // the source last advanced to, or the string 'null'; `fraiseql_cursor_advance`
  // persists any JSON value as the new cursor. Both fail on a non-source invocation.
  fraiseql_cursor_get(): Promise<string>;
  fraiseql_cursor_advance(valueJson: string): Promise<void>;
  // Structured log. Levels: 0=debug, 1=info, 2=warn, 3=error.
  fraiseql_log(level: number, message: string): void;
}

declare namespace Deno {
  namespace core {
    const ops: FraiseqlHostOps;
    function encode(text: string): Uint8Array;
    function decode(bytes: Uint8Array): string;
  }
}
";

/// Configuration for the Deno runtime.
///
/// Allows tuning of the V8 engine for performance and feature support.
#[derive(Debug, Clone)]
pub struct DenoConfig {
    /// Strip `TypeScript` types to executable `JavaScript` before execution (see
    /// [`transpile::transpile_typescript`]). On by default; with it off the source
    /// is executed as-is, so only the type-annotation-free `TypeScript` subset runs.
    pub enable_typescript: bool,
    /// Additional V8 flags (e.g., "--expose-gc").
    pub v8_flags:          Vec<String>,
}

impl Default for DenoConfig {
    fn default() -> Self {
        Self {
            enable_typescript: true,
            v8_flags:          vec![],
        }
    }
}

/// Deno runtime using V8 isolates for JavaScript/TypeScript execution.
pub struct DenoRuntime {
    config: DenoConfig,
}

impl std::fmt::Debug for DenoRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DenoRuntime").field("config", &self.config).finish()
    }
}

impl Clone for DenoRuntime {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}

impl DenoRuntime {
    /// Create a new Deno runtime with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns `Err` if runtime initialization fails.
    pub fn new(config: &DenoConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
        })
    }

    /// Execute a `JavaScript`/`TypeScript` module with a full I/O-capable host context.
    ///
    /// Unlike [`FunctionRuntime::invoke`] — which runs the guest with no host, so
    /// the `Deno.core.ops.fraiseql_*` I/O ops fail loud — this threads a live
    /// [`DynHostContext`] into the V8 op-state, giving the guest outbound HTTP,
    /// GraphQL `query`, storage, `env_var`, and `auth_context` at parity with the
    /// WASM [`invoke_with_context`](crate::runtime::wasm::WasmRuntime::invoke_with_context)
    /// path. The after:mutation dispatcher uses this so side-effecting `TypeScript`
    /// functions can reach the network.
    ///
    /// # Errors
    ///
    /// Returns `Err` on syntax errors, runtime exceptions, resource-limit
    /// violations, or a host op that fails (e.g. an SSRF-blocked request).
    pub async fn invoke_with_context(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        host_context: Arc<dyn DynHostContext>,
        limits: ResourceLimits,
    ) -> Result<FunctionResult> {
        run_guest(module, event, limits, Some(host_context), self.config.enable_typescript).await
    }
}

/// Run a guest module on a dedicated OS thread (with its own single-threaded Tokio
/// runtime for deno's event loop), optionally wiring a live host context into the
/// op-state. Shared by [`FunctionRuntime::invoke`] (no host) and
/// [`DenoRuntime::invoke_with_context`] (live host).
fn run_guest(
    module: &FunctionModule,
    event: EventPayload,
    limits: ResourceLimits,
    host: Option<Arc<dyn DynHostContext>>,
    enable_typescript: bool,
) -> impl std::future::Future<Output = Result<FunctionResult>> + Send {
    let raw = String::from_utf8_lossy(&module.bytecode).to_string();
    // With `enable_typescript` (the default) strip `TypeScript` types to executable
    // `JavaScript` before the isolate sees the source; with it off the `JavaScript`
    // runs byte-for-byte unchanged. A transpile failure is a located `SyntaxError`,
    // mapped below to the same permanent 4xx a malformed guest already gets.
    let source = if enable_typescript {
        transpile::transpile_typescript(&raw)
    } else {
        Ok(raw)
    };
    // Functions receive the entity data, not the full internal EventPayload.
    let event_data = event.data;

    async move {
        let source = source.map_err(|message| fraiseql_error::FraiseQLError::Validation {
            message,
            path: None,
        })?;
        let start = std::time::Instant::now();

        let (tx, rx) = tokio::sync::oneshot::channel::<
            std::result::Result<executor::ExecutionResult, String>,
        >();

        std::thread::spawn(move || {
            let result = executor::run_in_dedicated_thread(&source, &event_data, &limits, host);
            let _ = tx.send(result);
        });

        let exec_result = rx.await.map_err(|_| fraiseql_error::FraiseQLError::Internal {
            message: "Deno executor thread crashed".to_string(),
            source:  None,
        })?;

        // Measure AFTER the executor thread completes (M-deno-duration). Taking
        // the elapsed time right after spawning measured only channel setup,
        // not the actual script execution awaited on `rx`.
        let duration = start.elapsed();

        match exec_result {
            Ok(execution_result) => Ok(FunctionResult {
                value: Some(execution_result.value),
                logs: execution_result.logs,
                duration,
                memory_peak_bytes: 0,
            }),
            Err(e) if e.starts_with("SyntaxError") => {
                Err(fraiseql_error::FraiseQLError::Validation {
                    message: e,
                    path:    None,
                })
            },
            // A permanent-tagged failure (a host op that returned a 4xx, or a guest
            // that tagged its throw) maps to a 4xx so durable dispatch dead-letters
            // immediately instead of retrying. Untagged failures stay `Unsupported`
            // (501, transient).
            Err(e) if e.contains(crate::types::PERMANENT_ERROR_MARKER) => {
                Err(fraiseql_error::FraiseQLError::Validation {
                    message: e,
                    path:    None,
                })
            },
            Err(e) => Err(fraiseql_error::FraiseQLError::Unsupported { message: e }),
        }
    }
}

impl FunctionRuntime for DenoRuntime {
    /// Execute a `JavaScript` module with the given event and host context.
    ///
    /// # Implementation
    ///
    /// 1. Spawns a fresh OS thread (avoids tokio `block_on` nesting).
    /// 2. That thread creates its own single-threaded Tokio runtime for deno's event loop.
    /// 3. The `export default` function is called with `event.data` as the argument.
    /// 4. The result and captured logs are returned via a `oneshot` channel.
    ///
    /// # Errors
    ///
    /// Returns `Err` on syntax errors, runtime exceptions, or resource-limit violations.
    #[allow(clippy::manual_async_fn)] // Reason: impl Future syntax for trait compatibility
    fn invoke<H>(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        _host: &H,
        limits: ResourceLimits,
    ) -> impl std::future::Future<Output = Result<FunctionResult>> + Send
    where
        H: HostContext + ?Sized,
    {
        // The sync `invoke` path provides no live host: the guest may log and
        // transform data, but any `fraiseql_*` I/O op fails loud. Use
        // `invoke_with_context` for outbound HTTP / query / storage.
        run_guest(module, event, limits, None, self.config.enable_typescript)
    }

    fn supported_extensions(&self) -> &[&str] {
        &[".js", ".ts", ".mjs", ".mts"]
    }

    fn supports_hot_reload(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "deno"
    }
}
