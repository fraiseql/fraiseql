// Allow list mirrors fraiseql-core — same code, same suppressions.
// Reason: pedantic lints applied to existing federation module code.
#![allow(clippy::doc_markdown)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::format_push_string)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::implicit_saturating_sub)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::large_stack_arrays)]

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
//! | Saga orchestration — `SagaExecutor` / `SagaCompensator` | ✅ Production | requires `PostgresSagaStore` |
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
//! ```no_run
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
pub mod http_resolver;
pub mod logging;
pub mod metadata_helpers;
pub mod mutation_detector;
pub mod mutation_executor;
pub mod mutation_http_client;
pub mod mutation_query_builder;
pub mod query_builder;
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
use fraiseql_error::{FraiseQLError, Result};
pub use http_resolver::*;
pub use logging::{
    FederationLogContext, FederationOperationType, LogTimer, OperationStatus, ResolutionStrategy,
};
pub use mutation_detector::*;
pub use mutation_executor::*;
pub use mutation_http_client::*;
pub use mutation_query_builder::*;
pub use query_builder::*;
pub use representation::*;
pub use requires_provides_validator::{
    DirectiveValidationError, RequiresProvidesRuntimeValidator, RequiresProvidesValidator,
};
pub use saga_compensator::{
    CompensationResult, CompensationStatus, CompensationStepResult, SagaCompensator,
};
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
pub use types::*;

pub use crate::tracing::{FederationSpan, FederationTraceContext};

/// Handle federation queries (federation introspection).
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the query name is unknown
/// or `_entities` is called without executor context.
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
pub fn is_federation_query(query_name: &str) -> bool {
    matches!(query_name, "_service" | "_entities")
}
