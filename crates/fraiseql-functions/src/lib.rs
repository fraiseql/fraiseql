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
pub mod outbound;
pub mod runtime;
pub mod store;
pub mod triggers;
pub mod types;

/// The `bytes` this crate's public API is built against (#1198).
pub use bytes;
/// The `chrono` this crate's public API is built against (#1198).
pub use chrono;
pub use host::{HostContext, NoopHostContext};
pub use observer::FunctionObserver;
pub use outbound::{
    EmailTransport, LoginEmailSender, SendContext, SendEmailRequest, SendEmailResponse,
    SendPolicyError, SenderIdentity, SenderIdentityResolver, resolve_sender_identity,
};
/// The `reqwest` this crate's public API is built against (#1198).
#[cfg(feature = "host-live")]
pub use reqwest;
pub use runtime::{FunctionRuntime, SendFunctionRuntime};
/// The `serde_json` this crate's public API is built against (#1198).
pub use serde_json;
pub use store::{FunctionRecord, FunctionStatus, FunctionStore, memory::InMemoryFunctionStore};
pub use triggers::{
    cron::{CronScheduler, CronSchedulerHandle, CronTrigger},
    ingest::{
        Attachment, Classification, InboundMessage, InboundRouting, IngestError, IngestSelector,
        IngestSource, IngestTrigger, PullBatch, PullContext, PullSource, PushSource, RawDelivery,
        Recipient, RoutingRule, Source, StorageRef, Transport,
        email::{ParsedEmail, PendingAttachment, classify, derive_thread_key, normalize_email},
        parse_recipient, resolve_routing,
    },
    mutation::{
        AfterMutationTrigger, BeforeMutationChain, BeforeMutationResult, BeforeMutationTrigger,
        EntityEvent, EventKind, TriggerMatcher,
    },
    registry::TriggerRegistry,
    source::{IngestSink, SourceOutcome, run_source_once},
};
pub use types::{
    EventPayload, FunctionDefinition, FunctionModule, FunctionResult, LogEntry, LogLevel,
    ResourceLimits, RunAs, RuntimeType,
};
/// The `wasmtime` this crate's public API is built against (#1198).
#[cfg(feature = "runtime-wasm")]
pub use wasmtime;

#[cfg(test)]
mod tests;
