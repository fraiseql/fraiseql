//! Function observer for executing functions in response to events.

#[cfg(test)]
mod tests;

use crate::runtime::FunctionRuntime;
use crate::types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits, RuntimeType};
use crate::HostContext;
use fraiseql_error::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Executes functions in response to trigger events.
///
/// This observer integrates with the fraiseql-observers action execution pipeline.
/// It receives trigger events, looks up the corresponding function module,
/// selects the appropriate runtime, and executes the function.
pub struct FunctionObserver {
    runtimes: HashMap<RuntimeType, Arc<dyn std::any::Any + Send + Sync>>,
}

impl FunctionObserver {
    /// Create a new function observer.
    pub fn new() -> Self {
        Self {
            runtimes: HashMap::new(),
        }
    }

    /// Register a runtime for a specific runtime type.
    ///
    /// # Errors
    ///
    /// Returns `Err` if runtime registration fails.
    pub fn register_runtime<R: FunctionRuntime + 'static>(
        &mut self,
        runtime_type: RuntimeType,
        runtime: R,
    ) {
        self.runtimes
            .insert(runtime_type, Arc::new(runtime));
    }

    /// Execute a function module in response to an event.
    ///
    /// Dispatches to the appropriate runtime based on the module's `runtime` field.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - No runtime is registered for the module's runtime type
    /// - The runtime fails to execute the module
    pub async fn invoke<H>(
        &self,
        module: &FunctionModule,
        _event: EventPayload,
        _host: &H,
        _limits: ResourceLimits,
    ) -> Result<FunctionResult>
    where
        H: HostContext + ?Sized,
    {
        let _runtime_box = self
            .runtimes
            .get(&module.runtime)
            .ok_or_else(|| fraiseql_error::FraiseQLError::Unsupported {
                message: format!(
                    "No runtime registered for {:?}",
                    module.runtime
                ),
            })?;

        // Dispatch based on runtime type
        #[allow(unreachable_patterns)]  // Reason: pattern reachability depends on features
        match module.runtime {
            RuntimeType::Wasm => {
                #[cfg(feature = "runtime-wasm")]
                {
                    let runtime = runtime_box
                        .downcast_ref::<crate::runtime::wasm::WasmRuntime>()
                        .ok_or_else(|| fraiseql_error::FraiseQLError::Unsupported {
                            message: "Invalid WASM runtime".to_string(),
                        })?;
                    runtime.invoke(module, event, host, limits).await
                }
                #[cfg(not(feature = "runtime-wasm"))]
                {
                    Err(fraiseql_error::FraiseQLError::Unsupported {
                        message: "WASM runtime not enabled".to_string(),
                    })
                }
            }
            RuntimeType::Deno => {
                #[cfg(feature = "runtime-deno")]
                {
                    let runtime = runtime_box
                        .downcast_ref::<crate::runtime::deno::DenoRuntime>()
                        .ok_or_else(|| fraiseql_error::FraiseQLError::Unsupported {
                            message: "Invalid Deno runtime".to_string(),
                        })?;
                    runtime.invoke(module, event, host, limits).await
                }
                #[cfg(not(feature = "runtime-deno"))]
                {
                    Err(fraiseql_error::FraiseQLError::Unsupported {
                        message: "Deno runtime not enabled".to_string(),
                    })
                }
            }
        }
    }
}

impl Default for FunctionObserver {
    fn default() -> Self {
        Self::new()
    }
}
