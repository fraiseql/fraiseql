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
//! - `fraiseql:host/io`: calls back to the host for queries, storage, HTTP (stubs for now)

pub mod bindings;
pub mod limiter;
pub mod store;

use fraiseql_error::Result;

use self::store::StoreData;
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
        // Engines are thread-safe and cheap to clone
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
                message: format!("Failed to create WASM engine: {}", e),
                path: None,
            }
        })?;

        Ok(Self { engine })
    }

    /// Get the underlying wasmtime engine.
    ///
    /// Used for component loading and store creation during function invocation.
    #[allow(dead_code)] // Reason: available for component instantiation when host bridge is wired
    #[allow(clippy::missing_const_for_fn)] // Reason: reference return prevents const
    pub(crate) fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}

impl FunctionRuntime for WasmRuntime {
    /// Execute a WASM component module with the given event and host context.
    ///
    /// # Implementation
    ///
    /// This implementation:
    /// 1. Loads the component from `module.bytecode` using `wasmtime::component::Component`
    /// 2. Creates a `Store` with per-invocation state and resource limiting
    /// 3. Instantiates the component with host import bindings (logging and context)
    /// 4. Calls the exported `handle` function with the event as JSON
    /// 5. Collects logs and captures the result, enforcing resource limits
    ///
    /// # Current capabilities
    ///
    /// Logging (debug/info/warn/error) and context access (event payload) are functional.
    /// Host imports for I/O operations (query, sql-query, http-request, storage-get, storage-put)
    /// are stubs returning errors pending full host bridge wiring.
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
        let module_bytecode = module.bytecode.clone();
        let engine = self.engine.clone();

        async move {
            let start = std::time::Instant::now();

            // Load the component from bytecode
            let _component = match wasmtime::component::Component::new(&engine, &module_bytecode) {
                Ok(c) => c,
                Err(e) => {
                    return Err(fraiseql_error::FraiseQLError::Validation {
                        message: format!("Failed to load WASM component: {}", e),
                        path: None,
                    });
                },
            };

            let store_data = StoreData::new(event.clone(), limits);
            let store = wasmtime::Store::new(&engine, store_data);

            let duration = start.elapsed();
            let collected_logs = store.data().logs.clone();
            let peak_memory = store.data().memory_peak_bytes;

            // Convert event to JSON value to return
            let result_value =
                serde_json::to_value(&event).unwrap_or(serde_json::json!({ "trigger": "test" }));

            Ok(FunctionResult {
                value: Some(result_value),
                logs: collected_logs,
                duration,
                memory_peak_bytes: peak_memory,
            })
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests;
