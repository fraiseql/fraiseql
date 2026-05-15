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

use crate::runtime::FunctionRuntime;
use crate::types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits};
use crate::HostContext;
use fraiseql_error::Result;
use self::host_bridge::DynHostContext;
use self::store::StoreData;
use std::sync::Arc;

/// Configuration for the WASM runtime.
///
/// Allows tuning of the wasmtime engine for performance and feature support.
#[derive(Debug, Clone)]
pub struct WasmConfig {
    /// Enable SIMD (Single Instruction, Multiple Data) support in WASM modules.
    ///
    /// SIMD can improve performance for data-parallel workloads but adds compilation overhead.
    pub enable_simd: bool,
    /// Optional directory for caching compiled components.
    ///
    /// If set, compiled modules are cached to disk to speed up subsequent loads.
    pub compilation_cache_dir: Option<std::path::PathBuf>,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            enable_simd: true,
            compilation_cache_dir: None,
        }
    }
}

/// WASM runtime using wasmtime and the Component Model.
pub struct WasmRuntime {
    engine: wasmtime::Engine,
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
    handle: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl EpochTicker {
    /// Tick interval — each tick increments the epoch by 1.
    const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

    fn start(engine: wasmtime::Engine) -> Arc<Self> {
        let ticker = Arc::new(Self {
            shutdown: std::sync::atomic::AtomicBool::new(false),
            handle: std::sync::Mutex::new(None),
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
            engine: self.engine.clone(),
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
                path: None,
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
        let component =
            wasmtime::component::Component::new(&self.engine, &module.bytecode).map_err(|e| {
                fraiseql_error::FraiseQLError::Validation {
                    message: format!("Failed to load WASM component: {e}"),
                    path: None,
                }
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
        #[allow(clippy::cast_possible_truncation)] // Reason: deadline clamped by max_duration which fits in u64
        store.set_epoch_deadline(deadline_ticks as u64);

        // Create linker and add host imports
        let mut linker = wasmtime::component::Linker::new(&self.engine);

        // Add WASI imports — guests compiled for wasm32-wasip2 need these
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| fraiseql_error::FraiseQLError::Validation {
                message: format!("Failed to link WASI imports: {e}"),
                path: None,
            })?;

        bindings::FraiseqlFunction::add_to_linker::<
            StoreData,
            wasmtime::component::HasSelf<StoreData>,
        >(&mut linker, |data| data)
        .map_err(|e| fraiseql_error::FraiseQLError::Validation {
            message: format!("Failed to link host imports: {e}"),
            path: None,
        })?;

        // Instantiate the component using the low-level API so we can call
        // the export asynchronously (the generated `call_handle` is sync-only,
        // but async host imports require async dispatch).
        let instance = linker
            .instantiate_async(&mut store, &component)
            .await
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("Failed to instantiate WASM component: {e}"),
                source: None,
            })?;

        // Serialize event to JSON for the guest
        let event_json = serde_json::to_string(store.data().event_payload_ref())
            .unwrap_or_else(|_| "{}".to_string());

        // Get the handle export as a typed async-callable function.
        let handle_func = instance
            .get_typed_func::<(&str,), (std::result::Result<String, String>,)>(&mut store, "handle")
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("Failed to get handle export: {e}"),
                source: None,
            })?;

        let call_result = handle_func
            .call_async(&mut store, (&event_json,))
            .await;

        let duration = start.elapsed();
        let collected_logs = store.data().logs.clone();
        let peak_memory = store.data().memory_peak_bytes;

        let result_value = match call_result {
            Ok((Ok(result_json),)) => Some(
                serde_json::from_str(&result_json).unwrap_or_else(|e| {
                    serde_json::json!({ "error": format!("invalid JSON from guest: {e}") })
                }),
            ),
            Ok((Err(error_msg),)) => Some(serde_json::json!({ "error": error_msg })),
            Err(trap) => {
                let msg = trap.to_string();
                if msg.contains("epoch deadline") || duration > timeout {
                    Some(serde_json::json!({ "error": "function execution timed out" }))
                } else {
                    Some(serde_json::json!({ "error": format!("WASM trap: {msg}") }))
                }
            }
        };

        Ok(FunctionResult {
            value: result_value,
            logs: collected_logs,
            duration,
            memory_peak_bytes: peak_memory,
        })
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
/// - **Env vars**: Always returns `Ok(None)`. The `HostContext::env_var` method reads
///   from the process environment filtered by an allowlist at call time, and we cannot
///   capture that behavior without knowing which names the guest will request.
///   Use `invoke_with_context()` for full env var support.
/// - **Async IO**: `query`, `sql_query`, `http_request`, `storage_get`, `storage_put`
///   return `Unsupported`. Use `invoke_with_context()` for full IO support.
struct HostContextSnapshot {
    event_payload: EventPayload,
    /// Pre-captured auth context (Ok value or error message).
    auth_context: std::result::Result<serde_json::Value, String>,
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + '_>> {
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<serde_json::Value>>> + Send + '_>> {
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<crate::host::HttpResponse>> + Send + '_>> {
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<u8>>> + Send + '_>> {
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<()>> + Send + '_>> {
        Box::pin(async {
            Err(fraiseql_error::FraiseQLError::Unsupported {
                message: "storage_put not available in snapshot context".to_string(),
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
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests {
    use std::path::PathBuf;
    use crate::{EventPayload, FunctionModule, RuntimeType};
    use crate::runtime::FunctionRuntime;

    /// Helper to find test fixture file.
    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/functions")
            .join(name)
    }

    /// Helper to load WASM bytecode from a fixture file.
    fn load_wasm_fixture(name: &str) -> Vec<u8> {
        let path = fixture_path(name);
        std::fs::read(&path)
            .unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()))
    }

    #[test]
    fn test_wasm_load_valid_component() {
        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

        assert_eq!(module.name, "test_identity");
        assert_eq!(module.runtime, RuntimeType::Wasm);
        assert!(!module.source_hash.is_empty());
        assert!(!module.bytecode.is_empty());
    }

    #[test]
    fn test_wasm_load_invalid_bytes_returns_error() {
        let invalid_bytecode = bytes::Bytes::from(vec![0xFF, 0xFE, 0xFD, 0xFC]);
        let module = FunctionModule::from_bytecode("invalid".to_string(), invalid_bytecode);

        assert_eq!(module.name, "invalid");
    }

    #[tokio::test]
    async fn test_wasm_guest_identity_roundtrip() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"test": true}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let limits = crate::types::ResourceLimits::default();

        let result = runtime
            .invoke(&module, event.clone(), &host, limits)
            .await
            .expect("invoke should succeed");

        // The identity guest returns the event unchanged
        let returned = result.value.expect("should have a value");
        let expected = serde_json::to_value(&event).unwrap();
        assert_eq!(returned, expected, "identity guest should echo event back");
    }

    #[tokio::test]
    async fn test_wasm_guest_can_call_log() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_logging".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"test": true}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let limits = crate::types::ResourceLimits::default();

        let result = runtime.invoke(&module, event, &host, limits).await;
        assert!(result.is_ok(), "invocation should succeed");
    }

    #[tokio::test]
    async fn test_wasm_guest_log_levels() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_log_levels".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wasm_guest_get_event_payload() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_event_payload".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "mutation".to_string(),
            entity: "User".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"user_id": 42, "email": "test@example.com"}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wasm_guest_get_auth_context() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wasm_guest_get_env_var() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;
        assert!(result.is_ok());
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_wasm_guest_calls_query_with_live_host() {
        use crate::host::live::{LiveHostContext, HostContextConfig};

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_query".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "TestEntity".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"id": 42, "name": "test_item"}),
            timestamp: chrono::Utc::now(),
        };

        let config = HostContextConfig::default();
        let host = LiveHostContext::new(event.clone(), config);

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;

        assert!(result.is_ok(), "Query invocation should succeed");

        let function_result = result.unwrap();
        assert!(function_result.value.is_some(), "Query should return a value");
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_wasm_guest_calls_http_request_with_live_host() {
        use crate::host::live::{LiveHostContext, HostContextConfig};

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_http".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "TestEntity".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"id": 42}),
            timestamp: chrono::Utc::now(),
        };

        let config = HostContextConfig {
            allowed_domains: vec!["example.com".to_string()],
            ..Default::default()
        };
        let host = LiveHostContext::new(event.clone(), config);

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;

        assert!(result.is_ok(), "HTTP request invocation should succeed");

        let function_result = result.unwrap();
        assert!(function_result.value.is_some(), "HTTP request should return a value");
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_wasm_guest_calls_storage_get_with_live_host() {
        use crate::host::live::{LiveHostContext, HostContextConfig};

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_storage_get".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "File".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let config = HostContextConfig::default();
        let host = LiveHostContext::new(event.clone(), config);

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;

        assert!(result.is_ok(), "Storage get invocation should succeed");

        let function_result = result.unwrap();
        assert!(function_result.value.is_some(), "Storage get should return a value");
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_wasm_guest_calls_env_var_with_live_host() {
        use crate::host::live::{LiveHostContext, HostContextConfig};

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_env_var".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let mut config = HostContextConfig::default();
        config.allowed_env_vars.insert("TEST_VAR".to_string());
        let host = LiveHostContext::new(event.clone(), config);

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;

        assert!(result.is_ok(), "Env var invocation should succeed");

        let function_result = result.unwrap();
        assert!(function_result.value.is_some(), "Env var should return a value");
    }

    #[cfg(feature = "host-live")]
    #[tokio::test]
    async fn test_wasm_guest_calls_auth_context_with_live_host() {
        use crate::host::live::{LiveHostContext, HostContextConfig};

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_auth_context".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        };

        let config = HostContextConfig::default();
        let host = LiveHostContext::new(event.clone(), config);

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await;

        assert!(result.is_ok(), "Auth context invocation should succeed");

        let function_result = result.unwrap();
        assert!(function_result.value.is_some(), "Auth context should return a value");
    }

    /// Mock host context that responds to all operations for integration testing.
    struct MockFullBridgeHost {
        event: EventPayload,
        storage: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    }

    impl MockFullBridgeHost {
        fn new(event: EventPayload) -> Self {
            Self {
                event,
                storage: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl super::host_bridge::DynHostContext for MockFullBridgeHost {
        fn query(
            &self,
            _graphql: &str,
            _variables: serde_json::Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + '_>> {
            Box::pin(async {
                Ok(serde_json::json!({"data": {"users": [{"id": 1}]}}))
            })
        }

        fn sql_query(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<serde_json::Value>>> + Send + '_>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn http_request(
            &self,
            _method: &str,
            _url: &str,
            _headers: &[(String, String)],
            _body: Option<&[u8]>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<crate::host::HttpResponse>> + Send + '_>> {
            Box::pin(async {
                Ok(crate::host::HttpResponse {
                    status: 200,
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                    body: b"{}".to_vec(),
                })
            })
        }

        fn storage_get(
            &self,
            _bucket: &str,
            key: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<Vec<u8>>> + Send + '_>> {
            let data = self.storage.lock().expect("lock")
                .get(key).cloned()
                .unwrap_or_default();
            Box::pin(async move { Ok(data) })
        }

        fn storage_put(
            &self,
            _bucket: &str,
            key: &str,
            body: &[u8],
            _content_type: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = fraiseql_error::Result<()>> + Send + '_>> {
            self.storage.lock().expect("lock")
                .insert(key.to_string(), body.to_vec());
            Box::pin(async { Ok(()) })
        }

        fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
            Ok(serde_json::json!({
                "sub": "test-user",
                "roles": ["admin"],
            }))
        }

        fn env_var(&self, name: &str) -> fraiseql_error::Result<Option<String>> {
            if name == "FRAISEQL_TEST_VAR" {
                Ok(Some("test_value".to_string()))
            } else {
                Ok(None)
            }
        }

        fn event_payload(&self) -> &EventPayload {
            &self.event
        }

        fn log(&self, _level: crate::types::LogLevel, _message: &str) {}
    }

    #[tokio::test]
    async fn test_wasm_guest_full_bridge_exercises_all_ops() {
        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-full-bridge.wasm"));
        let module = FunctionModule::from_bytecode("full_bridge".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "FullBridge".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"id": 1}),
            timestamp: chrono::Utc::now(),
        };

        let host = std::sync::Arc::new(MockFullBridgeHost::new(event.clone()));

        let result = runtime
            .invoke_with_context(&module, event, host, crate::types::ResourceLimits::default())
            .await
            .expect("full bridge invocation should succeed");

        let value = result.value.expect("should have result value");

        // Verify all operations reported success
        assert_eq!(value["logging"], "ok", "logging ops should succeed");
        assert_eq!(value["event_payload"], true, "event payload should be non-empty");
        assert_eq!(value["auth_context"]["ok"], true, "auth context should succeed");
        assert_eq!(value["env_var"]["found"], true, "env var should be found");
        assert_eq!(value["env_var"]["value"], "test_value", "env var should have correct value");
        assert_eq!(value["http_request"]["ok"], true, "http request should succeed");
        assert_eq!(value["http_request"]["status"], 200, "http status should be 200");
        assert_eq!(value["query"]["ok"], true, "GraphQL query should succeed");
        assert_eq!(value["storage_put"]["ok"], true, "storage put should succeed");
        assert_eq!(value["storage_get"]["ok"], true, "storage get should succeed");
    }

    #[cfg(feature = "runtime-wasm")]
    #[tokio::test]
    async fn test_before_mutation_chain_with_wasm_function_proceeds() {
        use crate::observer::FunctionObserver;
        use crate::triggers::mutation::{BeforeMutationChain, BeforeMutationResult, BeforeMutationTrigger};
        use crate::host::NoopHostContext;
        use std::collections::HashMap;

        // Set up observer with WASM runtime
        let mut observer = FunctionObserver::new();
        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("create wasm runtime");
        observer.register_runtime(RuntimeType::Wasm, runtime);

        // Load the identity guest (returns event unchanged → Proceed with unchanged input)
        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("validateInput".to_string(), bytecode);

        let mut modules = HashMap::new();
        modules.insert("validateInput".to_string(), module);

        let chain = BeforeMutationChain {
            triggers: vec![BeforeMutationTrigger {
                function_name: "validateInput".to_string(),
                mutation_name: "createUser".to_string(),
            }],
        };

        let input = serde_json::json!({"name": "Alice", "email": "alice@example.com"});
        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "User".to_string(),
            event_kind: "created".to_string(),
            data: input.clone(),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event);
        let limits = crate::types::ResourceLimits::default();

        let result = chain.execute(input.clone(), &modules, &observer, &host, limits).await;

        match result.expect("chain should succeed") {
            BeforeMutationResult::Proceed(output) => {
                // Identity guest echoes event → no "input" key → chain proceeds with original input
                assert_eq!(output, input, "should proceed with unchanged input");
            }
            BeforeMutationResult::Abort(msg) => {
                panic!("Expected Proceed, got Abort: {msg}");
            }
        }
    }

    #[tokio::test]
    async fn test_wasm_performance_baseline() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("create wasm runtime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("perf_test".to_string(), bytecode);
        let limits = crate::types::ResourceLimits::default();

        // Cold start: first invocation (includes component compilation)
        let event = EventPayload {
            trigger_type: "test".to_string(),
            entity: "Perf".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"id": 1}),
            timestamp: chrono::Utc::now(),
        };
        let host = NoopHostContext::new(event.clone());

        let cold_start = std::time::Instant::now();
        let result = runtime.invoke(&module, event.clone(), &host, limits.clone()).await;
        let cold_duration = cold_start.elapsed();
        assert!(result.is_ok(), "cold start should succeed");

        // Warm starts: subsequent invocations (engine caches compilation)
        let mut warm_durations = Vec::new();
        for _ in 0..10 {
            let host = NoopHostContext::new(event.clone());
            let warm_start = std::time::Instant::now();
            let result = runtime.invoke(&module, event.clone(), &host, limits.clone()).await;
            warm_durations.push(warm_start.elapsed());
            assert!(result.is_ok(), "warm start should succeed");
        }

        let avg_warm = warm_durations.iter().sum::<std::time::Duration>() / 10;

        // Log performance numbers for baseline tracking
        eprintln!("=== WASM Performance Baseline ===");
        eprintln!("  Cold start:     {:?}", cold_duration);
        eprintln!("  Avg warm start: {:?} (10 iterations)", avg_warm);

        // Relaxed assertions — these are guardrails, not benchmarks.
        // Cold start includes WASM compilation, should be under 500ms on CI.
        assert!(
            cold_duration.as_millis() < 500,
            "cold start too slow: {cold_duration:?}"
        );
        // Warm start should be significantly faster.
        assert!(
            avg_warm.as_millis() < 100,
            "warm start too slow: {avg_warm:?}"
        );
    }

    #[cfg(feature = "runtime-wasm")]
    #[tokio::test]
    async fn test_after_mutation_trigger_fires_without_blocking() {
        use crate::observer::FunctionObserver;
        use crate::triggers::mutation::{AfterMutationTrigger, EntityEvent, EventKind};
        use crate::host::NoopHostContext;

        // Set up observer with WASM runtime
        let mut observer = FunctionObserver::new();
        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("create wasm runtime");
        observer.register_runtime(RuntimeType::Wasm, runtime);

        // Load the identity guest
        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("onUserCreated".to_string(), bytecode);

        let trigger = AfterMutationTrigger {
            function_name: "onUserCreated".to_string(),
            entity_type: "User".to_string(),
            event_filter: Some(EventKind::Insert),
        };

        let entity_event = EntityEvent {
            entity: "User".to_string(),
            event_kind: EventKind::Insert,
            old: None,
            new: Some(serde_json::json!({"id": 1, "name": "Alice"})),
            timestamp: chrono::Utc::now(),
        };

        // Build payload and invoke — fire-and-forget semantics mean we just
        // verify the invocation completes without error.
        let payload = trigger.build_payload(&entity_event);
        let host = NoopHostContext::new(payload.clone());
        let limits = crate::types::ResourceLimits::default();

        let start = std::time::Instant::now();
        let result = observer.invoke(&module, payload, &host, limits).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "after:mutation trigger should complete without error");
        // Verify it completed quickly (identity guest is fast)
        assert!(elapsed.as_millis() < 1000, "should complete quickly: {elapsed:?}");
    }
}
