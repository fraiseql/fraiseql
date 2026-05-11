//! FraiseQL serverless functions runtime.
//!
//! This crate provides the core infrastructure for executing serverless functions
//! in FraiseQL, with support for multiple runtimes (WASM, Deno, etc.).
//!
//! # Architecture
//!
//! - `FunctionRuntime`: Trait for implementing function execution backends
//! - `WasmRuntime`: WASM component model executor (feature: `runtime-wasm`)
//! - `DenoRuntime`: JavaScript/TypeScript executor via V8 (feature: `runtime-deno`)
//! - `FunctionObserver`: Integrates with fraiseql-observers for trigger execution

pub mod host;
pub mod migrations;
pub mod observer;
pub mod runtime;
pub mod secrets;
pub mod store;
pub mod triggers;
pub mod types;

pub use host::{HostContext, NoopHostContext};
#[cfg(feature = "host-live")]
pub use host::live::{HostContextConfig, LiveHostContext, QueryExecutor, SqlExecutor};
pub use migrations::{cron_migration_sql, functions_migration_sql};
pub use observer::FunctionObserver;
pub use runtime::{FunctionRuntime, SendFunctionRuntime};
pub use runtime::sandbox::{
    ConcurrencyLimiter, ConcurrencyLimiterRegistry, DEFAULT_MAX_CONCURRENT,
};
#[cfg(feature = "runtime-wasm")]
pub use runtime::wasm::cache::{WasmModuleCache, DEFAULT_MODULE_CACHE_SIZE};
pub use secrets::{FunctionSecretsStore, InMemorySecretsStore};
pub use store::{FunctionRecord, FunctionStatus, FunctionStore};
pub use store::memory::InMemoryFunctionStore;
pub use triggers::mutation::{
    AfterMutationTrigger, BeforeMutationChain, BeforeMutationResult, BeforeMutationTrigger,
    EntityEvent, EventKind, TriggerMatcher,
};
pub use triggers::registry::TriggerRegistry;
pub use triggers::cron::{CronScheduler, CronSchedulerHandle, CronTrigger};
pub use types::{
    EventPayload, FunctionDefinition, FunctionModule, FunctionResult, LogEntry, LogLevel,
    ResourceLimits, RuntimeType,
};

#[cfg(test)]
mod tests;
