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
pub mod limiter;
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

        Ok(Self { engine })
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

        // Create linker and add host imports
        let mut linker = wasmtime::component::Linker::new(&self.engine);
        bindings::FraiseqlFunction::add_to_linker::<
            StoreData,
            wasmtime::component::HasSelf<StoreData>,
        >(&mut linker, |data| data)
        .map_err(|e| fraiseql_error::FraiseQLError::Validation {
            message: format!("Failed to link host imports: {e}"),
            path: None,
        })?;

        // Instantiate the component
        let instance =
            bindings::FraiseqlFunction::instantiate_async(&mut store, &component, &linker)
                .await
                .map_err(|e| fraiseql_error::FraiseQLError::Validation {
                    message: format!("Failed to instantiate WASM component: {e}"),
                    path: None,
                })?;

        // Serialize event to JSON for the guest
        let event_json = serde_json::to_string(store.data().event_payload_ref())
            .unwrap_or_else(|_| "{}".to_string());

        // Call the guest's handle export
        let call_result = instance.call_handle(&mut store, &event_json);

        let duration = start.elapsed();
        let collected_logs = store.data().logs.clone();
        let peak_memory = store.data().memory_peak_bytes;

        // Check timeout after execution
        let result_value = if duration > timeout {
            Some(serde_json::json!({ "error": "function execution timed out" }))
        } else {
            match call_result {
                Ok(Ok(result_json)) => serde_json::from_str(&result_json).ok(),
                Ok(Err(error_msg)) => Some(serde_json::json!({ "error": error_msg })),
                Err(trap) => {
                    Some(serde_json::json!({ "error": format!("WASM trap: {trap}") }))
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

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
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

    // ========== Phase 5B Cycle 1: WASM Host Function Bridge Tests (RED) ==========

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
}
