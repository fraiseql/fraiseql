#![warn(missing_docs)]

//! Federation support for Apollo Federation v2.
//!
//! This crate implements the Apollo Federation v2 specification, enabling
//! multi-subgraph GraphQL composition with:
//! - Entity resolution via `_entities` query
//! - Service SDL via `_service` query
//! - Multiple resolution strategies (local, direct DB, HTTP)
//!
//! # Production-ready vs unstable APIs
//!
//! | Component | Status | Notes |
//! |-----------|--------|-------|
//! | Subgraph mode — `HttpEntityResolver` (`_entities` HTTP resolution) | ✅ Production | SSRF-protected, retry, tracing |
//! | Composition validation — `CompositionValidator` | ✅ Production | compile-time only |
//! | Saga forward execution — `SagaExecutor::{execute_step_local, execute_saga_local, execution_state}` | 🚧 Unstable | requires `unstable-saga` feature; dispatches real mutations (local SQL, or over HTTPS to a registered peer subgraph) and persists real step/saga state (see [#429](https://github.com/fraiseql/fraiseql/issues/429)). `@requires` pre-fetch is not yet wired. |
//! | Saga compensation — `SagaCompensator::{compensate_step_local, compensate_saga_local}` | 🚧 Unstable | requires `unstable-saga` feature; rolls back completed steps in reverse execution order — each on the same transport its forward step used (local SQL adapter, or over HTTPS to a registered peer subgraph) — and persists real `Compensated` state (see [#429](https://github.com/fraiseql/fraiseql/issues/429)). |
//! | Saga recovery — `SagaRecoveryManager::{run_iteration_local, start_background_loop_local}` | 🚧 Unstable | requires `unstable-saga` feature; re-drives crash-interrupted (`Pending`/`Executing`) sagas to a terminal state by replaying `execute_saga_local`, records recovery attempts, and cleans up stale sagas (see [#429](https://github.com/fraiseql/fraiseql/issues/429)). Stuck sagas are claimed under a lease via `FOR UPDATE SKIP LOCKED`, so concurrent recovery workers never double-drive one. |
//! | Saga coordination — `WiredSagaCoordinator::{create_saga, execute_saga, get_saga_status, cancel_saga, get_saga_result, list_in_flight_sagas}` | 🚧 Unstable | requires `unstable-saga` feature; ties forward execution + compensation into one handle — persists a saga with compensation metadata, runs the forward phase, and on failure (under `Automatic`) rolls back the completed steps via the local SQL adapter; `cancel_saga` compensates then marks the saga `Cancelled` (see [#429](https://github.com/fraiseql/fraiseql/issues/429)). `with_http_client` + `with_subgraph` route a step to a registered peer subgraph over HTTPS, for both forward execution and compensation (rollback). |
//! | Saga placeholders — `SagaExecutor::{execute_step, execute_saga, get_execution_state}` + `SagaCompensator::{compensate_step, compensate_saga}` + `SagaRecoveryManager::{run_iteration, start_background_loop}` + `SagaCoordinator::{create_saga, execute_saga, get_saga_status, cancel_saga, get_saga_result, list_in_flight_sagas}` | 🚧 Not implemented | return `SagaStoreError::NotImplemented` in every build (see [#429](https://github.com/fraiseql/fraiseql/issues/429)). `PostgresSagaStore` persistence is real. |
//! | HTTP mutation propagation — `HttpMutationClient` | ✅ Production | SSRF-protected |
//! | Gateway mode — `ConnectionManager::get_or_create_connection` | 🚧 Unstable | requires `unstable` feature |
//! | Direct-DB federation — `DirectDbResolver` | 🚧 Unstable | stub only; not yet implemented |
//!
//! To enable unstable APIs:
//! ```toml
//! [dependencies]
//! fraiseql-federation = { version = "…", features = ["unstable"] }
//! ```
//!
//! # Architecture
//!
//! The federation system works in phases:
//! 1. **Parsing**: Transform `_Any` scalar input to `EntityRepresentation`
//! 2. **Strategy Selection**: Determine how to resolve entity (local/DB/HTTP)
//! 3. **Batching**: Group entities by typename and strategy
//! 4. **Resolution**: Execute queries/requests to get entities
//! 5. **Projection**: Filter results to requested fields
//!
//! # Example
//!
//! ```text
//! // Requires: live database adapter and federation metadata.
//! // See: tests/integration/ for runnable examples.
//! let executor = FederationExecutor::new(adapter, metadata);
//! let response = executor.handle_entities_query(input).await?;
//! ```

pub mod composition_validator;
pub mod connection_manager;
pub mod database_resolver;
pub mod dependency_graph;
pub mod direct_db_resolver;
pub mod entity_resolver;
pub mod health;
pub mod http_resolver;
pub mod logging;
pub mod metadata_helpers;
pub mod mutation_detector;
pub mod mutation_executor;
pub mod mutation_http_client;
pub mod mutation_query_builder;
pub mod observability;
pub mod query_builder;
pub mod query_plan_cache;
pub mod representation;
pub mod requires_provides_validator;
pub mod saga_compensator;
pub mod saga_coordinator;
pub mod saga_executor;
pub mod saga_recovery_manager;
pub mod saga_store;
pub mod selection_parser;
pub mod service_sdl;
pub mod sql_utils;
pub mod subscription_forwarder;
pub mod tls;
pub mod tracing;
pub mod types;

pub use composition_validator::{
    ComposedSchema, ComposedType, CompositionError, CompositionValidator, CrossSubgraphValidator,
};
pub use connection_manager::*;
pub use database_resolver::*;
pub use dependency_graph::DependencyGraph;
pub use direct_db_resolver::*;
pub use entity_resolver::*;
use fraiseql_error::FraiseQLError;
pub use fraiseql_error::Result;
pub use health::{FederationHealthReport, SubgraphHealthAggregator, SubgraphHealthStatus};
pub use http_resolver::*;
pub use logging::{
    FederationLogContext, FederationOperationType, LogTimer, OperationStatus, ResolutionStrategy,
};
pub use mutation_detector::*;
pub use mutation_executor::*;
pub use mutation_http_client::*;
pub use mutation_query_builder::*;
pub use observability::{EntityResolutionMetrics, SubgraphLatencyEntry, SubgraphLatencyTracker};
pub use query_builder::*;
pub use query_plan_cache::{QueryPlan, QueryPlanCache, SubgraphFetch};
pub use representation::*;
pub use requires_provides_validator::{
    DirectiveValidationError, RequiresProvidesRuntimeValidator, RequiresProvidesValidator,
};
pub use saga_compensator::{
    CompensationResult, CompensationStatus, CompensationStepResult, SagaCompensator,
};
#[cfg(feature = "unstable-saga")]
pub use saga_coordinator::WiredSagaCoordinator;
pub use saga_coordinator::{
    CompensationStrategy, SagaCoordinator, SagaResult, SagaStatus, SagaStep as SagaCoordinatorStep,
};
pub use saga_executor::{ExecutionState, SagaExecutor, StepExecutionResult};
pub use saga_recovery_manager::{RecoveryConfig, RecoveryStats, SagaRecoveryManager};
pub use saga_store::{
    MutationType, PostgresSagaStore, Saga, SagaRecovery, SagaState, SagaStep, SagaStoreError,
    StepState,
};
pub use selection_parser::*;
use serde_json::{Value, json};
pub use service_sdl::*;
pub use subscription_forwarder::{
    ForwardError, ForwardedEvent, SubscriptionForwarder, extract_subscription_field_name,
    lookup_remote_subscription,
};
pub use types::*;

pub use crate::tracing::{FederationSpan, FederationTraceContext};

/// Handle federation queries (federation introspection)
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the query name is unknown or requires
/// executor context (e.g., `_entities`).
pub async fn handle_federation_query(
    query_name: &str,
    _args: &std::collections::BTreeMap<String, Value>,
) -> Result<Value> {
    match query_name {
        "_service" => handle_service_query().await,
        "_entities" => {
            // Will be handled at executor level with proper context
            Err(FraiseQLError::Validation {
                message: "_entities query requires executor context".to_string(),
                path:    None,
            })
        },
        _ => Err(FraiseQLError::Validation {
            message: format!("Unknown federation query: {}", query_name),
            path:    None,
        }),
    }
}

/// Handle _service query returning SDL
async fn handle_service_query() -> Result<Value> {
    // This will be populated by the executor with actual schema
    Ok(json!({
        "_service": {
            "sdl": ""
        }
    }))
}

/// Check if a query is a federation query
#[must_use]
pub fn is_federation_query(query_name: &str) -> bool {
    matches!(query_name, "_service" | "_entities")
}
