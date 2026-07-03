//! Function runtime trait and implementations.

#[cfg(feature = "runtime-wasm")]
pub mod wasm;

#[cfg(feature = "runtime-deno")]
pub mod deno;

#[cfg(all(test, feature = "runtime-wasm", feature = "runtime-deno"))]
mod parity_tests;

use std::future::Future;

use async_trait::async_trait;
use fraiseql_error::Result;

use crate::{
    HostContext,
    types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits},
};

/// Trait for function execution backends (WASM, Deno, etc.).
///
/// Implementors provide the ability to load and execute function modules
/// with resource limits enforced. This trait uses native async for zero-cost
/// abstraction on hot paths.
pub trait FunctionRuntime: Send + Sync {
    /// Execute a function module with the given event and host context.
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - The module cannot be loaded or parsed
    /// - Execution raises an error (runtime error, timeout, memory limit exceeded)
    /// - The host context raises an error
    fn invoke<H>(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        host: &H,
        limits: ResourceLimits,
    ) -> impl Future<Output = Result<FunctionResult>> + Send
    where
        H: HostContext + ?Sized;

    /// Get the list of file extensions this runtime supports.
    fn supported_extensions(&self) -> &[&str];

    /// Check if this runtime supports hot-reloading modules without restart.
    fn supports_hot_reload(&self) -> bool;

    /// Get the name of this runtime (e.g., "wasm", "deno").
    fn name(&self) -> &str;
}

/// Type alias for a boxable function runtime (with static lifetime bounds).
/// Used for dynamic dispatch where concrete types aren't known at compile time.
pub type BoxedFunctionRuntime = Box<dyn FunctionRuntime + Send + Sync>;

/// Object-safe variant of `FunctionRuntime` for dynamic dispatch.
///
/// This trait has the same semantic methods but without generic parameters,
/// making it suitable for `Arc<dyn SendFunctionRuntime>`. The `invoke_raw`
/// method uses a [`NoopHostContext`](crate::NoopHostContext) internally.
#[async_trait]
pub trait SendFunctionRuntime: Send + Sync {
    /// Execute a function module with the given event and resource limits.
    ///
    /// Uses [`NoopHostContext`](crate::NoopHostContext) — callers that need
    /// host-bridge functionality should use [`FunctionRuntime::invoke`] with
    /// a concrete host context instead.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the module cannot be loaded, or execution fails.
    async fn invoke_raw(
        &self,
        module: &FunctionModule,
        event: EventPayload,
        limits: ResourceLimits,
    ) -> Result<FunctionResult>;

    /// Get the list of file extensions this runtime supports.
    fn supported_extensions(&self) -> &[&str];

    /// Check if this runtime supports hot-reloading modules without restart.
    fn supports_hot_reload(&self) -> bool;

    /// Get the name of this runtime (e.g., "wasm", "deno").
    fn name(&self) -> &str;
}
