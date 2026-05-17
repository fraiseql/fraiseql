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
pub mod store;
pub mod triggers;
pub mod types;

pub use host::{HostContext, NoopHostContext};
pub use observer::FunctionObserver;
pub use runtime::{FunctionRuntime, SendFunctionRuntime};
pub use store::{FunctionRecord, FunctionStore, InMemoryFunctionStore};
pub use triggers::{
    cron::{CronScheduler, CronSchedulerHandle, CronTrigger},
    mutation::{
        AfterMutationTrigger, BeforeMutationChain, BeforeMutationResult, BeforeMutationTrigger,
        EntityEvent, EventKind, TriggerMatcher,
    },
    registry::TriggerRegistry,
};
pub use types::{
    EventPayload, FunctionDefinition, FunctionModule, FunctionResult, LogEntry, LogLevel,
    ResourceLimits, RuntimeType,
};

#[cfg(test)]
mod tests;
