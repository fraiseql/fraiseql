//! Server subsystem assembly and lifecycle management.
//!
//! [`ServerSubsystems`] bundles the three optional platform extensions —
//! object storage, serverless functions, and realtime entity streams — into a
//! single coherent struct that the server can query, route-mount, and shut down
//! in a controlled order.
//!
//! # Assembly
//!
//! Use [`builder::ServerSubsystemsBuilder`] to assemble subsystems from their
//! pre-built parts. The builder validates cross-subsystem dependencies before
//! returning the final [`ServerSubsystems`]:
//!
//! ```rust,ignore
//! let subsystems = ServerSubsystemsBuilder::new()
//!     .with_storage(storage_subsystem)
//!     .with_functions(functions_subsystem)
//!     .with_realtime(realtime_subsystem)
//!     .build()?;
//! ```
//!
//! # Shutdown order
//!
//! Shutdown proceeds in reverse initialization order:
//! 1. Stop the cron scheduler (functions)
//! 2. Drain the realtime event channel
//! 3. Drop the realtime server (closes all `WebSocket` connections)
//! 4. Drop the functions observer (stops dispatching events)
//! 5. Drop the storage backend (flushes any pending writes)

pub mod builder;
pub mod validator;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use fraiseql_functions::{FunctionObserver, triggers::TriggerRegistry};

use crate::{
    realtime::{
        observer::RealtimeBroadcastObserver,
        routes::RealtimeSchemaConfig,
        server::RealtimeServer,
    },
    schema::loader::{FunctionsConfig, SchemaStorageConfig},
};

pub use builder::{ServerSubsystemsBuilder, SubsystemBuildError};
pub use validator::{SubsystemConfigWarning, validate_subsystems_config};

// ── Subsystem structs ─────────────────────────────────────────────────────────

/// Storage subsystem: backend, metadata repository, RLS evaluator, and bucket config.
///
/// Assembled at server startup from the `[storage]` section of the compiled schema
/// and the server's `PgPool`. Use the [`fraiseql_storage::storage_router`] with the
/// contained `state` to mount the `/storage/v1` route tree.
pub struct StorageSubsystem {
    /// Runtime storage state (backend + metadata repo + RLS + bucket config map).
    ///
    /// Pass this to [`fraiseql_storage::storage_router`] to mount the HTTP routes.
    pub state: fraiseql_storage::StorageState,

    /// Schema-level bucket definitions from the compiled schema.
    pub schema_config: SchemaStorageConfig,
}

/// Functions subsystem: observer and trigger registry.
///
/// Assembled at server startup from the `[functions]` section of the compiled schema.
/// The observer dispatches events to function runtimes; the registry maps triggers to
/// function definitions and provides HTTP route matchers.
pub struct FunctionsSubsystem {
    /// Observer that dispatches trigger events to the appropriate function runtime.
    pub observer: Arc<FunctionObserver>,

    /// Registry mapping trigger types to function definitions.
    pub trigger_registry: TriggerRegistry,

    /// Loaded function modules keyed by function name.
    ///
    /// Populated at server startup by reading source files from `config.module_dir`.
    /// Used by the before-mutation chain and the after-mutation dispatcher.
    pub module_registry: std::collections::HashMap<String, fraiseql_functions::FunctionModule>,

    /// Schema-level functions configuration (definitions + module directory).
    pub config: FunctionsConfig,
}

/// Realtime subsystem: `WebSocket` broadcast server and event observer.
///
/// Assembled at server startup from the `[realtime]` section of the compiled schema.
/// The server handles `WebSocket` connections; the observer receives mutation events
/// from the observer pipeline and forwards them to connected clients.
pub struct RealtimeSubsystem {
    /// The `WebSocket` broadcast server.
    ///
    /// Pass this to [`crate::realtime::routes::realtime_router`] to mount `/realtime/v1`.
    pub server: Arc<RealtimeServer>,

    /// Observer that forwards mutation events into the realtime delivery pipeline.
    pub observer: RealtimeBroadcastObserver,

    /// Schema-level realtime configuration (enabled flag, entity list, capacity overrides).
    pub schema_config: RealtimeSchemaConfig,
}

// ── Aggregated container ──────────────────────────────────────────────────────

/// All optional platform subsystems assembled from the compiled schema.
///
/// Each field is `None` when the corresponding section is absent from or disabled
/// in the compiled schema. Callers can use [`is_storage_enabled`][Self::is_storage_enabled]
/// etc. to check at a glance, or match directly on the `Option` fields.
#[allow(missing_debug_implementations)] // Reason: inner types (RealtimeBroadcastObserver) don't implement Debug
pub struct ServerSubsystems {
    /// Object storage subsystem, present when the schema's `"storage"` key is set.
    pub storage: Option<StorageSubsystem>,

    /// Serverless functions subsystem, present when the schema's `"functions"` key is set.
    pub functions: Option<FunctionsSubsystem>,

    /// Realtime broadcast subsystem, present when the schema's `"realtime"` key is set
    /// and `enabled` is `true`.
    pub realtime: Option<RealtimeSubsystem>,
}

impl ServerSubsystems {
    /// Create an empty `ServerSubsystems` with all subsystems disabled.
    ///
    /// Equivalent to `ServerSubsystemsBuilder::new().build().unwrap()`.
    #[must_use]
    pub const fn none() -> Self {
        Self { storage: None, functions: None, realtime: None }
    }

    /// Returns `true` if the storage subsystem is present.
    #[must_use]
    pub const fn is_storage_enabled(&self) -> bool {
        self.storage.is_some()
    }

    /// Returns `true` if the functions subsystem is present.
    #[must_use]
    pub const fn is_functions_enabled(&self) -> bool {
        self.functions.is_some()
    }

    /// Returns `true` if the realtime subsystem is present.
    #[must_use]
    pub const fn is_realtime_enabled(&self) -> bool {
        self.realtime.is_some()
    }
}

// ── Before-mutation hook bundle ───────────────────────────────────────────────

/// Shared bundle of before-mutation state passed into `AppState` for handler access.
///
/// This is a lightweight, cloneable snapshot of the parts of [`FunctionsSubsystem`]
/// that are needed on the hot path for before-mutation checks. It is extracted once
/// at server startup and stored in [`AppState`] via an `Arc`.
pub struct BeforeMutationHooks {
    /// Registry of all loaded triggers, keyed by trigger type and mutation name.
    pub trigger_registry: TriggerRegistry,
    /// Loaded function modules keyed by function name.
    pub module_registry: std::collections::HashMap<String, fraiseql_functions::FunctionModule>,
    /// Observer that dispatches events to the appropriate function runtime.
    pub observer: std::sync::Arc<FunctionObserver>,
}
