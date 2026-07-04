//! `WebAssembly` Component Model runtime for function execution.
//!
//! This module provides `WasmRuntime`, which executes WASM components
//! using the wasmtime engine with the Component Model.
//!
//! # Architecture
//!
//! The WASM runtime uses `wasmtime` with component model enabled to safely execute
//! untrusted function modules with resource limits enforced. Each execution:
//!
//! 1. Loads a pre-compiled WASM component from bytecode
//! 2. Creates a store with resource limiting callbacks
//! 3. Instantiates the component with host-provided imports (logging, context, I/O)
//! 4. Calls the exported `handle` function with the event as JSON
//! 5. Collects logs and captures the result, enforcing resource limits throughout
//!
//! # WIT Interface
//!
//! Guest components implement the `fraiseql:host/fraiseql-function` world defined in
//! `wit/fraiseql-host.wit`. The world provides:
//! - `fraiseql:host/logging`: structured logging to the host
//! - `fraiseql:host/context`: access to event payload, auth, environment
//! - `fraiseql:host/io`: calls back to the host for queries, storage, HTTP

pub mod bindings;
pub mod host_bridge;
pub mod store;

use std::sync::Arc;

use fraiseql_error::Result;

use self::{host_bridge::DynHostContext, store::StoreData};
use crate::{
    HostContext,
    runtime::FunctionRuntime,
    types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits},
};

/// Configuration for the WASM runtime.
///
/// Allows tuning of the wasmtime engine for performance and feature support.
#[derive(Debug, Clone)]
pub struct WasmConfig {
    /// Enable SIMD (Single Instruction, Multiple Data) support in WASM modules.
    ///
    /// SIMD can improve performance for data-parallel workloads but adds compilation overhead.
    pub enable_simd:           bool,
    /// Optional directory for caching compiled components.
    ///
    /// If set, compiled modules are cached to disk to speed up subsequent loads.
    pub compilation_cache_dir: Option<std::path::PathBuf>,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            enable_simd:           true,
            compilation_cache_dir: None,
        }
    }
}

/// WASM runtime using wasmtime and the Component Model.
pub struct WasmRuntime {
    engine:       wasmtime::Engine,
    /// Handle to the background epoch ticker thread.
    /// Kept alive for the runtime's lifetime; dropped on `Drop`.
    epoch_ticker: Arc<EpochTicker>,
}

/// Background thread that increments the engine epoch at a fixed interval.
///
/// When combined with `store.set_epoch_deadline()`, this preemptively interrupts
/// runaway guests rather than relying on post-hoc duration checks.
struct EpochTicker {
    shutdown: std::sync::atomic::AtomicBool,
    handle:   std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl EpochTicker {
    /// Tick interval — each tick increments the epoch by 1.
    const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

    fn start(engine: wasmtime::Engine) -> Arc<Self> {
        let ticker = Arc::new(Self {
            shutdown: std::sync::atomic::AtomicBool::new(false),
            handle:   std::sync::Mutex::new(None),
        });

        let ticker_clone = Arc::clone(&ticker);
        let handle = std::thread::Builder::new()
            .name("wasm-epoch-ticker".to_string())
            .spawn(move || {
                while !ticker_clone.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(Self::TICK_INTERVAL);
                    engine.increment_epoch();
                }
            })
            .expect("failed to spawn epoch ticker thread");

        *ticker.handle.lock().expect("lock") = Some(handle);
        ticker
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        let handle = self.handle.lock().expect("lock").take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

impl std::fmt::Debug for WasmRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmRuntime").finish()
    }
}

impl Clone for WasmRuntime {
    fn clone(&self) -> Self {
        Self {
            engine:       self.engine.clone(),
            epoch_ticker: Arc::clone(&self.epoch_ticker),
        }
    }
}

impl WasmRuntime {
    /// Create a new WASM runtime with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns `Err` if engine initialization fails.
    pub fn new(config: &WasmConfig) -> Result<Self> {
        let mut wasm_config = wasmtime::Config::new();
        wasm_config.epoch_interruption(true);
        wasm_config.wasm_simd(config.enable_simd);
        wasm_config.wasm_relaxed_simd(false);
        wasm_config.wasm_bulk_memory(true);
        wasm_config.wasm_component_model(true);

        let engine = wasmtime::Engine::new(&wasm_config).map_err(|e| {
            fraiseql_error::FraiseQLError::Validation {
                message: format!("Failed to create WASM engine: {e}"),
                path:    None,
            }
        })?;

        let ticker = EpochTicker::start(engine.clone());

        Ok(Self {
            engine,
            epoch_ticker: ticker,
        })
    }
}

impl WasmRuntime {
    /// Execute a WASM component with a pre-built `Arc<dyn DynHostContext>`.
    ///
    /// This is the core invocation path. The generic `FunctionRuntime::invoke` wraps
    /// the host into a `HostContextSnapshot`, which only supports sync ops. Use this
    /// method directly when async IO ops (HTTP, query, storage) need to reach the
    /// real host context.
    ///
    /// # Errors
    ///
    /// Returns `Err` if component loading, instantiation, or guest execution fails.
    pub async fn invoke_with_context(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        host_context: Arc<dyn DynHostContext>,
        limits: ResourceLimits,
    ) -> Result<FunctionResult> {
        let start = std::time::Instant::now();
        let timeout = limits.max_duration;

        // Load the component from bytecode
        let component = wasmtime::component::Component::new(&self.engine, &module.bytecode)
            .map_err(|e| fraiseql_error::FraiseQLError::Validation {
                message: format!("Failed to load WASM component: {e}"),
                path:    None,
            })?;

        // Create store with host context
        let mut store_data = StoreData::new(event, limits);
        store_data.set_host_context(host_context);

        let mut store = wasmtime::Store::new(&self.engine, store_data);

        // Set up resource limiter (StoreData implements ResourceLimiter directly)
        store.limiter(|data| data);

        // Set epoch deadline for preemptive timeout.
        // Each epoch tick is 100ms, so deadline = timeout_ms / 100.
        let deadline_ticks = (timeout.as_millis() / EpochTicker::TICK_INTERVAL.as_millis()).max(1);
        #[allow(clippy::cast_possible_truncation)]
        // Reason: deadline clamped by max_duration which fits in u64
        store.set_epoch_deadline(deadline_ticks as u64);

        // Create linker and add host imports
        let mut linker = wasmtime::component::Linker::new(&self.engine);

        // Add WASI imports — guests compiled for wasm32-wasip2 need these
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(|e| {
            fraiseql_error::FraiseQLError::Validation {
                message: format!("Failed to link WASI imports: {e}"),
                path:    None,
            }
        })?;

        bindings::FraiseqlFunction::add_to_linker::<
            StoreData,
            wasmtime::component::HasSelf<StoreData>,
        >(&mut linker, |data| data)
        .map_err(|e| fraiseql_error::FraiseQLError::Validation {
            message: format!("Failed to link host imports: {e}"),
            path:    None,
        })?;

        // Instantiate the component using the low-level API so we can call
        // the export asynchronously (the generated `call_handle` is sync-only,
        // but async host imports require async dispatch).
        let instance = linker.instantiate_async(&mut store, &component).await.map_err(|e| {
            fraiseql_error::FraiseQLError::Internal {
                message: format!("Failed to instantiate WASM component: {e}"),
                source:  None,
            }
        })?;

        // Serialize event to JSON for the guest
        let event_json = serde_json::to_string(store.data().event_payload_ref())
            .unwrap_or_else(|_| "{}".to_string());

        // Get the handle export as a typed async-callable function.
        let handle_func = instance
            .get_typed_func::<(&str,), (std::result::Result<String, String>,)>(&mut store, "handle")
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("Failed to get handle export: {e}"),
                source:  None,
            })?;

        let call_result = handle_func.call_async(&mut store, (&event_json,)).await;

        let duration = start.elapsed();
        let collected_logs = store.data().logs.clone();
        let peak_memory = store.data().memory_peak_bytes;

        let outcome = match call_result {
            Ok((Ok(result_json),)) => GuestOutcome::Ok(result_json),
            Ok((Err(error_msg),)) => GuestOutcome::GuestError(error_msg),
            Err(trap) => {
                let msg = trap.to_string();
                if msg.contains("epoch deadline") || duration > timeout {
                    GuestOutcome::Timeout
                } else {
                    GuestOutcome::Trap(msg)
                }
            },
        };

        let result_value = map_guest_outcome(outcome)?;

        Ok(FunctionResult {
            value: result_value,
            logs: collected_logs,
            duration,
            memory_peak_bytes: peak_memory,
        })
    }
}

/// The outcome of calling the guest's `handle` export, normalised away from the
/// wasmtime-specific call-result types so the failure contract can be unit-tested.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GuestOutcome {
    /// The guest returned `Ok(json)` — the JSON string it produced as data.
    Ok(String),
    /// The guest returned `Err(message)` — a guest-reported failure.
    GuestError(String),
    /// The guest execution timed out (epoch deadline / duration exceeded).
    Timeout,
    /// The guest trapped (panic, OOM, illegal instruction); carries the trap message.
    Trap(String),
}

/// Map a guest outcome onto the shared runtime failure contract.
///
/// A successful guest return (`Ok`) yields `Ok(Some(value))`. Every failure mode
/// — a guest-returned `Err`, a timeout, or a trap — yields
/// `Err(FraiseQLError::Unsupported)`, matching what the Deno runtime returns for an
/// equivalent guest runtime error (M-fn-failure-contract). A guest error must not
/// be silently treated as successful data.
fn map_guest_outcome(outcome: GuestOutcome) -> Result<Option<serde_json::Value>> {
    match outcome {
        GuestOutcome::Ok(result_json) => {
            let value = serde_json::from_str(&result_json).unwrap_or_else(
                |e| serde_json::json!({ "error": format!("invalid JSON from guest: {e}") }),
            );
            Ok(Some(value))
        },
        GuestOutcome::GuestError(message) => {
            Err(fraiseql_error::FraiseQLError::Unsupported { message })
        },
        GuestOutcome::Timeout => Err(fraiseql_error::FraiseQLError::Unsupported {
            message: "function execution timed out".to_string(),
        }),
        GuestOutcome::Trap(msg) => Err(fraiseql_error::FraiseQLError::Unsupported {
            message: format!("WASM trap: {msg}"),
        }),
    }
}

impl FunctionRuntime for WasmRuntime {
    /// Execute a WASM component module with the given event and host context.
    ///
    /// For async IO operations, use `invoke_with_context()` directly with an
    /// `Arc<dyn DynHostContext>` that delegates to the real backends.
    ///
    /// # Errors
    ///
    /// Returns `Err` if component loading, instantiation, or guest execution fails.
    #[allow(clippy::manual_async_fn)] // Reason: impl Future syntax for trait compatibility
    fn invoke<H>(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        host: &H,
        limits: ResourceLimits,
    ) -> impl std::future::Future<Output = Result<FunctionResult>> + Send
    where
        H: HostContext + ?Sized,
    {
        // Snapshot captures sync state (auth_context, env_var, event_payload).
        // Async IO ops will return Unsupported — use invoke_with_context for full IO.
        let host_context: Arc<dyn DynHostContext> = Arc::from(HostContextSnapshot::capture(host));
        let runtime = self.clone();
        let module = module.clone();

        async move { runtime.invoke_with_context(&module, event, host_context, limits).await }
    }

    fn supported_extensions(&self) -> &[&str] {
        &[".wasm"]
    }

    fn supports_hot_reload(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "wasm"
    }
}

/// A snapshot of a `HostContext` that owns all its data, allowing `'static` lifetime.
///
/// This is necessary because `StoreData` needs `Arc<dyn DynHostContext>` which is `'static`,
/// but the `invoke()` method receives `&H` with a borrowed lifetime.
///
/// # Limitations
///
/// - **Env vars**: Always returns `Ok(None)`. The `HostContext::env_var` method reads from the
///   process environment filtered by an allowlist at call time, and we cannot capture that behavior
///   without knowing which names the guest will request. Use `invoke_with_context()` for full env
///   var support.
/// - **Async IO**: `query`, `sql_query`, `http_request`, `storage_get`, `storage_put` return
///   `Unsupported`. Use `invoke_with_context()` for full IO support.
struct HostContextSnapshot {
    event_payload: EventPayload,
    /// Pre-captured auth context (Ok value or error message).
    auth_context:  std::result::Result<serde_json::Value, String>,
}

impl HostContextSnapshot {
    fn capture<H: HostContext + ?Sized>(host: &H) -> Self {
        let event_payload = host.event_payload().clone();
        let auth_context = host.auth_context().map_err(|e| e.to_string());

        Self {
            event_payload,
            auth_context,
        }
    }
}

impl DynHostContext for HostContextSnapshot {
    fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + '_,
        >,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "query not available in snapshot context".to_string(),
            })
        })
    }

    fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<Vec<serde_json::Value>>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "sql_query not available in snapshot context".to_string(),
            })
        })
    }

    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<crate::host::HttpResponse>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "http_request not available in snapshot context".to_string(),
            })
        })
    }

    fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<u8>>> + Send + '_>,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "storage_get not available in snapshot context".to_string(),
            })
        })
    }

    fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<()>> + Send + '_>>
    {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "storage_put not available in snapshot context".to_string(),
            })
        })
    }

    fn send_email<'a>(
        &'a self,
        _request: &'a crate::outbound::SendEmailRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<crate::outbound::SendEmailResponse>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "send_email not available in snapshot context".to_string(),
            })
        })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        self.auth_context
            .clone()
            .map_err(|msg| fraiseql_error::FraiseQLError::Unsupported { message: msg })
    }

    fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
        tracing::debug!(
            var = name,
            "env_var called on HostContextSnapshot — returning None; use invoke_with_context for env var support"
        );
        Ok(None)
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event_payload
    }

    fn log(&self, level: crate::types::LogLevel, message: &str) {
        match level {
            crate::types::LogLevel::Debug => tracing::debug!("{}", message),
            crate::types::LogLevel::Info => tracing::info!("{}", message),
            crate::types::LogLevel::Warn => tracing::warn!("{}", message),
            crate::types::LogLevel::Error => tracing::error!("{}", message),
        }
    }
}

#[cfg(test)]
mod tests;
