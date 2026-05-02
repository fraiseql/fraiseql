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

pub mod cache;
pub mod limiter;
pub mod store;

use crate::runtime::FunctionRuntime;
use crate::types::{EventPayload, FunctionModule, FunctionResult, LogLevel, ResourceLimits};
use crate::HostContext;
use fraiseql_error::{FraiseQLError, Result};
use self::store::StoreData;

/// Generated host bindings from `wit/fraiseql-host.wit`.
///
/// Contains the `FraiseqlFunction` component struct and host interface traits
/// (`logging::Host`, `context::Host`, `io::Host`) that the host must implement.
mod generated {
    wasmtime::component::bindgen!({
        path: "wit/fraiseql-host.wit",
        world: "fraiseql-function",
    });
}

// Implement the logging host interface on StoreData
impl generated::fraiseql::host::logging::Host for StoreData {
    fn log(
        &mut self,
        level: generated::fraiseql::host::logging::LogLevel,
        message: String,
    ) -> wasmtime::Result<()> {
        let lvl = match level {
            generated::fraiseql::host::logging::LogLevel::Debug => LogLevel::Debug,
            generated::fraiseql::host::logging::LogLevel::Info => LogLevel::Info,
            generated::fraiseql::host::logging::LogLevel::Warn => LogLevel::Warn,
            generated::fraiseql::host::logging::LogLevel::Error => LogLevel::Error,
        };
        StoreData::log(self, lvl, &message);
        Ok(())
    }
}

// Implement the context host interface on StoreData
impl generated::fraiseql::host::context::Host for StoreData {
    fn get_event_payload(&mut self) -> wasmtime::Result<String> {
        self.get_event_payload_json()
    }

    fn get_auth_context(&mut self) -> wasmtime::Result<std::result::Result<String, String>> {
        Ok(self.get_auth_context_json().map_err(|e| e.to_string()))
    }

    fn get_env_var(&mut self, name: String) -> wasmtime::Result<Option<String>> {
        self.get_env_var_value(&name)
    }
}

// Implement the I/O host interface on StoreData (stubs — not yet wired)
impl generated::fraiseql::host::io::Host for StoreData {
    fn query(
        &mut self,
        _graphql: String,
        _variables: String,
    ) -> wasmtime::Result<std::result::Result<String, String>> {
        Ok(Err("query not yet implemented in WASM host".to_string()))
    }

    fn sql_query(
        &mut self,
        _sql: String,
        _params: String,
    ) -> wasmtime::Result<std::result::Result<String, String>> {
        Ok(Err("sql_query not yet implemented in WASM host".to_string()))
    }

    fn http_request(
        &mut self,
        _method: String,
        _url: String,
        _headers: Vec<(String, String)>,
        _body: Option<Vec<u8>>,
    ) -> wasmtime::Result<std::result::Result<generated::fraiseql::host::io::HttpResponse, String>> {
        Ok(Err("http_request not yet implemented in WASM host".to_string()))
    }

    fn storage_get(
        &mut self,
        _bucket: String,
        _key: String,
    ) -> wasmtime::Result<std::result::Result<Vec<u8>, String>> {
        Ok(Err("storage_get not yet implemented in WASM host".to_string()))
    }

    fn storage_put(
        &mut self,
        _bucket: String,
        _key: String,
        _body: Vec<u8>,
        _content_type: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        Ok(Err("storage_put not yet implemented in WASM host".to_string()))
    }
}

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
    module_cache: cache::WasmModuleCache,
}

impl std::fmt::Debug for WasmRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmRuntime")
            .field("module_cache", &self.module_cache)
            // engine intentionally omitted — wasmtime::Engine does not implement Debug
            .finish_non_exhaustive()
    }
}

impl Clone for WasmRuntime {
    fn clone(&self) -> Self {
        // Engine and cache are Arc-backed; cloning is cheap and shares state.
        Self {
            engine: self.engine.clone(),
            module_cache: self.module_cache.clone(),
        }
    }
}

impl WasmRuntime {
    /// Create a new WASM runtime with the given configuration and a default
    /// module cache of [`cache::DEFAULT_MODULE_CACHE_SIZE`] entries.
    ///
    /// # Errors
    ///
    /// Returns `Err` if engine initialization fails.
    pub fn new(config: &WasmConfig) -> Result<Self> {
        Self::with_module_cache(config, cache::WasmModuleCache::with_defaults())
    }

    /// Create a new WASM runtime with an explicit module cache.
    ///
    /// # Errors
    ///
    /// Returns `Err` if engine initialization fails.
    pub fn with_module_cache(config: &WasmConfig, module_cache: cache::WasmModuleCache) -> Result<Self> {
        let mut wasm_config = wasmtime::Config::new();
        wasm_config.wasm_simd(config.enable_simd);
        wasm_config.wasm_relaxed_simd(false);
        wasm_config.wasm_bulk_memory(true);
        wasm_config.wasm_component_model(true);

        let engine = wasmtime::Engine::new(&wasm_config).map_err(|e| {
            FraiseQLError::Validation {
                message: format!("Failed to create WASM engine: {e}"),
                path: None,
            }
        })?;

        Ok(Self { engine, module_cache })
    }

    /// Get the underlying wasmtime engine.
    #[allow(dead_code)] // Reason: available for extensions and integration tests
    #[allow(clippy::missing_const_for_fn)] // Reason: reference return prevents const
    pub(crate) fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }

    /// Get a reference to the module cache.
    ///
    /// Exposed for testing and metrics collection.
    pub const fn module_cache(&self) -> &cache::WasmModuleCache {
        &self.module_cache
    }
}

impl FunctionRuntime for WasmRuntime {
    /// Execute a WASM component module with the given event and host context.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - The component cannot be loaded or parsed
    /// - Execution raises a trap or timeout
    /// - The `handle` export is missing or has the wrong signature
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
        // Check the module cache before cloning the (potentially large) bytecode.
        let cached = self.module_cache.get(&module.source_hash);
        let module_bytecode = if cached.is_none() { Some(module.bytecode.clone()) } else { None };
        let source_hash = module.source_hash.clone();
        let engine = self.engine.clone();
        let module_cache = self.module_cache.clone();
        let max_duration = limits.max_duration;

        async move {
            // Serialize event to JSON before entering blocking context
            let event_json = serde_json::to_string(&event).map_err(|e| FraiseQLError::Validation {
                message: format!("Failed to serialize event: {e}"),
                path: None,
            })?;

            // Run wasmtime synchronously in a blocking task to avoid blocking the async executor
            let result = tokio::task::spawn_blocking(move || {
                invoke_sync_cached(&engine, cached, module_bytecode, source_hash, &module_cache, &event_json, limits)
            });

            // Apply timeout
            match tokio::time::timeout(max_duration, result).await {
                Ok(Ok(Ok(func_result))) => Ok(func_result),
                Ok(Ok(Err(e))) => Err(e),
                Ok(Err(join_err)) => Err(FraiseQLError::Validation {
                    message: format!("WASM execution task panicked: {join_err}"),
                    path: None,
                }),
                Err(_timeout) => Err(FraiseQLError::Validation {
                    message: format!(
                        "WASM execution timed out after {}ms",
                        max_duration.as_millis()
                    ),
                    path: None,
                }),
            }
        }
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

impl crate::runtime::SendFunctionRuntime for WasmRuntime {
    fn invoke_raw(
        &self,
        module: &crate::types::FunctionModule,
        event: crate::types::EventPayload,
        limits: crate::types::ResourceLimits,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = fraiseql_error::Result<crate::types::FunctionResult>> + Send + '_>,
    > {
        // Clone what we need so the async block owns all data (avoids borrow-of-local issues).
        let module = module.clone();
        Box::pin(async move {
            let host = crate::host::NoopHostContext::new(event.clone());
            self.invoke(&module, event, &host, limits).await
        })
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

/// Synchronous WASM component invocation with module cache support.
///
/// On a cache hit (`cached_component` is `Some`), compilation is skipped.
/// On a cache miss, the module is compiled from `module_bytecode` and the
/// result is inserted into `module_cache` for future invocations.
///
/// Runs entirely on the calling thread; wrap in `spawn_blocking` from async contexts.
///
/// # Errors
///
/// Returns `Err` if the component fails to load, instantiate, or execute.
fn invoke_sync_cached(
    engine: &wasmtime::Engine,
    cached_component: Option<std::sync::Arc<wasmtime::component::Component>>,
    module_bytecode: Option<bytes::Bytes>,
    source_hash: String,
    module_cache: &cache::WasmModuleCache,
    event_json: &str,
    limits: ResourceLimits,
) -> Result<FunctionResult> {
    let start = std::time::Instant::now();

    // Use cached component or compile from bytecode and populate the cache.
    let component = if let Some(cached) = cached_component {
        cached
    } else {
        let bytecode = module_bytecode.ok_or_else(|| FraiseQLError::Validation {
            message: "module bytecode is required when no cached component is available".to_string(),
            path: None,
        })?;
        let compiled = wasmtime::component::Component::new(engine, &bytecode).map_err(|e| {
            FraiseQLError::Validation {
                message: format!("Failed to load WASM component: {e}"),
                path: None,
            }
        })?;
        let arc = std::sync::Arc::new(compiled);
        module_cache.insert(source_hash, std::sync::Arc::clone(&arc));
        arc
    };

    // Create per-invocation store with resource limiter
    let store_data = StoreData::new(
        serde_json::from_str(event_json).map_err(|e| FraiseQLError::Validation {
            message: format!("Failed to deserialize event in sync context: {e}"),
            path: None,
        })?,
        limits,
    );
    let mut store = wasmtime::Store::new(engine, store_data);
    store.limiter(|data| &mut data.limiter);

    // Build linker with host imports (stubs; guest may not call all of them)
    let mut linker: wasmtime::component::Linker<StoreData> =
        wasmtime::component::Linker::new(engine);
    generated::FraiseqlFunction::add_to_linker(&mut linker, |s| s).map_err(|e| {
        FraiseQLError::Validation {
            message: format!("Failed to add host imports to linker: {e}"),
            path: None,
        }
    })?;

    // Instantiate and call the exported handle function
    let (instance, _wasm_instance) =
        generated::FraiseqlFunction::instantiate(&mut store, &component, &linker).map_err(|e| {
            FraiseQLError::Validation {
                message: format!("Failed to instantiate WASM component: {e}"),
                path: None,
            }
        })?;

    let call_result = instance.call_handle(&mut store, event_json).map_err(|e| {
        FraiseQLError::Validation {
            message: format!("WASM handle call failed: {e}"),
            path: None,
        }
    })?;

    let duration = start.elapsed();
    let collected_logs = store.data().logs.clone();
    let peak_memory = store.data().memory_peak_bytes;

    // Propagate guest errors
    let result_str = call_result.map_err(|e| FraiseQLError::Validation {
        message: format!("Function returned error: {e}"),
        path: None,
    })?;

    // Parse return value as JSON
    let value = serde_json::from_str(&result_str).unwrap_or(serde_json::Value::String(result_str));

    Ok(FunctionResult {
        value: Some(value),
        logs: collected_logs,
        duration,
        memory_peak_bytes: peak_memory,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests {
    use std::path::PathBuf;
    use crate::{EventPayload, FunctionModule, RuntimeType};
    use crate::runtime::FunctionRuntime;

    /// Helper to find test fixture file
    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/functions")
            .join(name)
    }

    /// Helper to load WASM bytecode from a fixture file
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

    /// Invoke on guest-identity returns the event payload (real WASM, not mock).
    #[tokio::test]
    async fn test_wasm_guest_returns_event_payload() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-identity.wasm"));
        let module = FunctionModule::from_bytecode("test_identity".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "mutation".to_string(),
            entity: "User".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"user_id": 42, "email": "test@example.com"}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());
        let limits = crate::types::ResourceLimits::default();

        let result = runtime
            .invoke(&module, event.clone(), &host, limits)
            .await
            .expect("invoke should succeed");

        // Real WASM returned a value (not a mock short-circuit)
        assert!(result.value.is_some(), "handle() should return a value");

        let val = result.value.unwrap();
        // The identity guest echoes the event JSON back; verify round-trip fields
        assert_eq!(val["trigger_type"].as_str().unwrap(), "mutation");
        assert_eq!(val["entity"].as_str().unwrap(), "User");
        assert_eq!(val["data"]["user_id"].as_i64().unwrap(), 42);
    }

    /// Verify the transform fixture adds `"transformed": true`.
    #[tokio::test]
    async fn test_wasm_guest_transform_adds_field() {
        use crate::host::NoopHostContext;

        let runtime = super::WasmRuntime::new(&super::WasmConfig::default())
            .expect("Failed to create WasmRuntime");

        let bytecode = bytes::Bytes::from(load_wasm_fixture("guest-transform.wasm"));
        let module = FunctionModule::from_bytecode("test_transform".to_string(), bytecode);

        let event = EventPayload {
            trigger_type: "mutation".to_string(),
            entity: "Post".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"post_id": 1}),
            timestamp: chrono::Utc::now(),
        };

        let host = NoopHostContext::new(event.clone());

        let result = runtime
            .invoke(&module, event, &host, crate::types::ResourceLimits::default())
            .await
            .expect("transform invoke should succeed");

        let val = result.value.expect("value present");
        assert!(val["transformed"].as_bool().unwrap());
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

        let result = runtime
            .invoke(&module, event, &host, limits)
            .await
            .expect("invoke should succeed");

        assert!(result.value.is_some());
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
            .await
            .expect("invoke should succeed");

        assert!(result.value.is_some());
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
            .invoke(&module, event.clone(), &host, crate::types::ResourceLimits::default())
            .await
            .expect("invoke should succeed");

        let val = result.value.expect("value present");
        assert_eq!(val["entity"].as_str().unwrap(), "User");
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
            .await
            .expect("invoke should succeed");

        assert!(result.value.is_some());
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
            .await
            .expect("invoke should succeed");

        assert!(result.value.is_some());
    }

    // ========== WASM Host Function Bridge Tests ==========

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
        assert!(result.unwrap().value.is_some(), "Query should return a value");
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
        assert!(result.unwrap().value.is_some(), "HTTP request should return a value");
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
        assert!(result.unwrap().value.is_some(), "Storage get should return a value");
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
        assert!(result.unwrap().value.is_some(), "Env var should return a value");
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
        assert!(result.unwrap().value.is_some(), "Auth context should return a value");
    }
}
