//! Trigger system for serverless functions.
//!
//! Triggers enable functions to execute in response to specific events:
//! - `after:mutation`: Fire after mutation completes (async, non-blocking)
//! - `before:mutation`: Fire before mutation (sync, can abort)
//! - `after:storage`: Fire after storage operations
//! - `cron`: Fire on schedule
//! - `http`: Custom HTTP endpoints
//!
//! # Integration Overview
//!
//! The trigger system is designed for modular integration with the FraiseQL server:
//!
//! 1. **Registry Loading**: `TriggerRegistry::load_from_definitions()` parses function definitions
//!    from the compiled schema and initializes all trigger types.
//!
//! 2. **Trigger Types**:
//!    - `AfterMutationTrigger`: Receives mutation events via observer pipeline (async dispatch)
//!    - `BeforeMutationTrigger`: Intercepts mutations before DB write (sync, can abort)
//!    - `StorageTrigger`: Responds to storage operations (async dispatch)
//!    - `CronTrigger`: Scheduled execution with state persistence
//!    - `HttpTriggerRoute`: Custom HTTP endpoints
//!
//! 3. **Lifecycle**:
//!    - **Startup**: `TriggerRegistry` initializes cron scheduler, validates triggers
//!    - **Runtime**: Triggers dispatch to function observer based on events
//!    - **Shutdown**: Cron scheduler stops, pending tasks drain
//!
//! # Usage Example
//!
//! ```ignore
//! // Load all triggers from schema
//! let registry = TriggerRegistry::load_from_definitions(&functions)?;
//!
//! // Query triggers by type
//! let http_routes = registry.http_routes();
//! let before_hooks = registry.before_mutation_triggers_for("createUser");
//! ```
//!
//! # Error Handling
//!
//! `RegistryError` is returned for invalid trigger formats or misconfiguration.
//! All trigger types validate input and return clear error messages.

pub mod cron;
pub mod http;
pub mod mutation;
pub mod registry;
pub mod storage;
#[cfg(test)]
mod tests;

pub use cron::{CronTrigger, CronSchedule, CronExecutionState};
pub use http::{HttpTriggerRoute, HttpTriggerPayload, HttpTriggerResponse, HttpTriggerMatcher};
pub use mutation::{AfterMutationTrigger, BeforeMutationTrigger};
pub use registry::{TriggerRegistry, ParsedTrigger, RegistryError};
pub use storage::{StorageTrigger, StorageOperation, StorageEventPayload};
