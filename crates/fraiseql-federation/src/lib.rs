#![warn(missing_docs)]

//! Federation support for Apollo Federation v2.
//!
//! This crate implements the Apollo Federation v2 specification, enabling
//! multi-subgraph GraphQL composition with:
//! - Entity resolution via `_entities` query
//! - Service SDL via `_service` query
//! - Multiple resolution strategies (local, direct DB, HTTP)
//!
//! Saga orchestration lives in `fraiseql-saga`, which sits *above* `fraiseql-core`
//! so a saga step dispatches through the mutation chokepoint rather than building
//! its own SQL (#1354).
//!
//! # Production-ready vs unstable APIs
//!
//! | Component | Status | Notes |
//! |-----------|--------|-------|
//! | Subgraph mode — `HttpEntityResolver` (`_entities` HTTP resolution) | ✅ Production | SSRF-protected, retry, tracing |
//! | Composition validation — `CompositionValidator` | ✅ Production | compile-time only |
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
pub mod observability;
pub mod query_builder;
pub mod query_plan_cache;
pub mod representation;
pub mod requires_provides_validator;
pub mod selection_parser;
pub mod service_sdl;
pub mod sql_utils;
pub mod subscription_forwarder;
pub mod tls;
pub mod tracing;
pub mod types;

/// The `chrono` this crate's public API is built against (#1198).
pub use chrono;
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
pub use observability::{EntityResolutionMetrics, SubgraphLatencyEntry, SubgraphLatencyTracker};
pub use query_builder::*;
pub use query_plan_cache::{QueryPlan, QueryPlanCache, SubgraphFetch};
pub use representation::*;
pub use requires_provides_validator::{
    DirectiveValidationError, RequiresProvidesRuntimeValidator, RequiresProvidesValidator,
};
/// The `reqwest` this crate's public API is built against (#1198).
pub use reqwest;
pub use selection_parser::*;
/// The `serde_json` this crate's public API is built against (#1198).
pub use serde_json;
use serde_json::{Value, json};
pub use service_sdl::*;
pub use subscription_forwarder::{
    ForwardError, ForwardedEvent, SubscriptionForwarder, extract_subscription_field_name,
    lookup_remote_subscription,
};
pub use types::*;
/// The `uuid` this crate's public API is built against (#1198).
pub use uuid;
/// The `zeroize` this crate's public API is built against (#1198).
pub use zeroize;

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
