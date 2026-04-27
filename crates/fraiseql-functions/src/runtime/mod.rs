//! Function runtime trait and implementations.

use crate::types::{EventPayload, FunctionModule, FunctionResult, ResourceLimits};
use crate::HostContext;
use fraiseql_error::Result;
use std::future::Future;

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
/// This trait has the same semantic methods but without generic parameters,
/// making it suitable for `Box<dyn SendFunctionRuntime>`.
pub trait SendFunctionRuntime: Send + Sync {
    /// Get the list of file extensions this runtime supports.
    fn supported_extensions(&self) -> &[&str];

    /// Check if this runtime supports hot-reloading modules without restart.
    fn supports_hot_reload(&self) -> bool;

    /// Get the name of this runtime (e.g., "wasm", "deno").
    fn name(&self) -> &str;
}
