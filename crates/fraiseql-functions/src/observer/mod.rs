//! Function observer for executing functions in response to events.

#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Arc};

use fraiseql_error::Result;

use crate::{
    HostContext,
    runtime::FunctionRuntime,
    types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits, RuntimeType},
};

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
    #[must_use]
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
        self.runtimes.insert(runtime_type, Arc::new(runtime));
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
        #[allow(unused_variables)] // Reason: used only when runtime features are enabled
        event: EventPayload,
        #[allow(unused_variables)] // Reason: used only when runtime features are enabled
        host: &H,
        #[allow(unused_variables)] // Reason: used only when runtime features are enabled
        limits: ResourceLimits,
    ) -> Result<FunctionResult>
    where
        H: HostContext + ?Sized,
    {
        #[allow(unused_variables)] // Reason: used only when runtime features are enabled
        let runtime_box = self.runtimes.get(&module.runtime).ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: format!("No runtime registered for {:?}", module.runtime),
            }
        })?;

        // Dispatch based on runtime type
        #[allow(unreachable_patterns)] // Reason: pattern reachability depends on features
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
            },
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
            },
        }
    }

    /// Find all `after:mutation` triggers that match a given entity event.
    ///
    /// Returns the list of matching [`AfterMutationTrigger`]s from `registry`.
    /// Used by after-mutation dispatchers to know which functions to invoke.
    /// This method is allocation-free when no triggers match.
    ///
    /// [`AfterMutationTrigger`]: crate::triggers::mutation::AfterMutationTrigger
    #[must_use]
    pub fn find_after_mutation_triggers(
        &self,
        registry: &crate::triggers::registry::TriggerRegistry,
        event: &crate::triggers::mutation::EntityEvent,
    ) -> Vec<crate::triggers::mutation::AfterMutationTrigger> {
        registry.after_mutation_triggers.find(&event.entity, event.event_kind)
    }

    /// Execute a WASM module with a full I/O-capable host context.
    ///
    /// Unlike [`invoke`](Self::invoke) — which snapshots the host into a
    /// sync-only `HostContextSnapshot` and so returns `Unsupported` for async
    /// I/O — this routes to
    /// [`WasmRuntime::invoke_with_context`](crate::runtime::wasm::WasmRuntime::invoke_with_context),
    /// giving the guest live HTTP / query / storage access through `host`. The
    /// after:mutation dispatcher uses this so side-effecting functions (webhooks,
    /// external provisioning) can reach the network.
    ///
    /// Only the WASM runtime is supported on this path; an event whose module
    /// targets another runtime returns `Unsupported`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if no WASM runtime is registered, the registered runtime is
    /// not a [`WasmRuntime`](crate::runtime::wasm::WasmRuntime), or guest
    /// execution fails.
    #[cfg(feature = "runtime-wasm")]
    pub async fn invoke_with_context(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        host: Arc<dyn crate::runtime::wasm::host_bridge::DynHostContext>,
        limits: ResourceLimits,
    ) -> Result<FunctionResult> {
        let runtime_box = self.runtimes.get(&RuntimeType::Wasm).ok_or_else(|| {
            fraiseql_error::FraiseQLError::Unsupported {
                message: "No WASM runtime registered for after:mutation dispatch".to_string(),
            }
        })?;
        let runtime =
            runtime_box.downcast_ref::<crate::runtime::wasm::WasmRuntime>().ok_or_else(|| {
                fraiseql_error::FraiseQLError::Unsupported {
                    message: "Invalid WASM runtime".to_string(),
                }
            })?;
        runtime.invoke_with_context(module, event, host, limits).await
    }
}

impl Default for FunctionObserver {
    fn default() -> Self {
        Self::new()
    }
}
