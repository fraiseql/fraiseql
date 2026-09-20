#![warn(missing_docs)]

//! Distributed saga orchestration for FraiseQL (#429).
//!
//! A saga sequences a multi-step write across subgraphs and rolls the completed
//! steps back when a later one fails. This crate owns the forward phase, the
//! compensation phase, crash recovery, and the Postgres store that persists both.
//!
//! # Where this crate sits
//!
//! ```text
//! fraiseql-db ──► fraiseql-federation ──► fraiseql-core ──► fraiseql-saga
//!                 (entity resolution)      (chokepoint)      (orchestration)
//! ```
//!
//! A saga step is a **client of the mutation chokepoint**, not a second writer.
//! Its local arm calls [`fraiseql_core::runtime::Executor::execute_mutation_with_security`],
//! so `requires_role`, `requires_actor`, the operation `Authorizer`, argument
//! validation, the `before:mutation` chain, session-variable binding and the
//! change-log write all run — the same gates any other write faces.
//!
//! That is why this crate exists. The orchestrator used to live *below* the
//! engine, in `fraiseql-federation`, where the chokepoint was unreachable; it had
//! grown its own INSERT/UPDATE/DELETE string builder and dispatched it raw,
//! skipping every gate above (#1354). The layering was the defect.
//!
//! # Components
//!
//! | Component | Status | Notes |
//! |-----------|--------|-------|
//! | Forward execution — `SagaExecutor::{execute_step, execute_saga, execution_state}` | ✅ Stable | Dispatches real mutations (through the chokepoint locally, or over HTTPS to a registered peer subgraph) and persists real step/saga state. A `RetryPolicy` retries a transient step failure with exponential backoff (+ optional per-step timeout) before the saga gives up. A step's caller-supplied `@requires` fields (`RequiredField`) are pre-fetched from their owning subgraph's `_entities` endpoint and merged into the mutation variables before dispatch; an unresolved field fails the step before its mutation runs. |
//! | Compensation — `SagaCompensator::{compensate_step, compensate_saga}` | ✅ Stable | Rolls back completed steps in reverse execution order — each on the same transport its forward step used — and persists real `Compensated` state. |
//! | Recovery — `SagaRecoveryManager::{run_iteration, start_background_loop}` | ✅ Stable | Re-drives crash-interrupted (`Pending`/`Executing`) sagas to a terminal state by replaying `execute_saga`, records recovery attempts, and cleans up stale sagas. Stuck sagas are claimed under a lease via `FOR UPDATE SKIP LOCKED`, so concurrent recovery workers never double-drive one. |
//! | Coordination — `SagaCoordinator::{create_saga, execute_saga, get_saga_status, cancel_saga, get_saga_result, list_in_flight_sagas}` | ✅ Stable | Ties forward execution + compensation into one handle. `with_http_client` + `with_subgraph` route a step to a registered peer subgraph over HTTPS, for both forward execution and compensation; `with_http_client_mtls` adds mutual-TLS authentication; `with_entity_resolver` enables cross-subgraph `@requires` pre-fetch. |
//! | HTTP mutation propagation — `HttpMutationClient` | ✅ Production | SSRF-protected. |
//!
//! Enabling the crate *is* the opt-in: nothing in the server or core feature chain
//! depends on it, so a deployment that does not orchestrate cross-subgraph
//! transactions never compiles the Postgres saga store or its dependencies.

pub mod mutation_detector;
pub mod mutation_executor;
pub mod mutation_http_client;
pub mod mutation_query_builder;
pub mod saga_compensator;
pub mod saga_coordinator;
pub mod saga_executor;
pub mod saga_recovery_manager;
pub mod saga_store;

/// The `chrono` this crate's public API is built against (#1198).
pub use chrono;
pub use fraiseql_error::Result;
pub use mutation_detector::*;
pub use mutation_executor::*;
pub use mutation_http_client::*;
pub use mutation_query_builder::*;
/// The `reqwest` this crate's public API is built against (#1198).
pub use reqwest;
pub use saga_compensator::{
    CompensationResult, CompensationStatus, CompensationStepResult, SagaCompensator,
};
pub use saga_coordinator::{
    CompensationStrategy, SagaCoordinator, SagaResult, SagaStatus, SagaStep as SagaCoordinatorStep,
};
pub use saga_executor::{ExecutionState, RetryPolicy, SagaExecutor, StepExecutionResult};
pub use saga_recovery_manager::{
    RecoveryConfig, RecoveryRouting, RecoveryStats, SagaRecoveryManager,
};
pub use saga_store::{
    MutationType, PostgresSagaStore, RequiredField, Saga, SagaRecovery, SagaState, SagaStep,
    SagaStoreError, StepState,
};
/// The `serde_json` this crate's public API is built against (#1198).
pub use serde_json;
/// The `uuid` this crate's public API is built against (#1198).
pub use uuid;
