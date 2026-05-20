//! REST request handler — direct execution without GraphQL parsing.
//!
//! Receives HTTP requests, resolves routes from [`RestRouteTable`], extracts
//! parameters via `RestParamExtractor`, builds a `QueryMatch` or mutation
//! call, and executes directly via the [`Executor`] APIs.

pub mod coercion;
pub mod headers;
pub mod mutation;
pub mod prefer;
pub mod query;
pub mod response;
pub mod routing;
pub mod search;

#[cfg(test)]
mod tests;

// Re-export public types from submodules for external use
use std::sync::Arc;

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::{CompiledSchema, RestConfig},
};
// Re-export header utilities for cross-module callers (streaming/, bulk/)
pub(crate) use headers::{set_preference_applied, set_request_id};
pub use prefer::{CountPreference, HandlingPreference, PreferHeader};
pub use response::{RestError, RestResponse};
pub use routing::{ResolvedGetQuery, ResolvedRoute};

use super::{idempotency::IdempotencyStore, resource::RestRouteTable};

// ---------------------------------------------------------------------------
// REST Handler
// ---------------------------------------------------------------------------

/// REST request handler — translates HTTP requests to direct executor calls.
///
/// This handler does NOT construct GraphQL strings. It builds typed
/// `QueryMatch` or mutation calls and executes them directly.
pub struct RestHandler<'a, A: DatabaseAdapter> {
    pub(super) executor: &'a Arc<Executor<A>>,
    pub(super) schema: &'a CompiledSchema,
    pub(super) config: &'a RestConfig,
    pub(super) route_table: &'a RestRouteTable,
    pub(super) idempotency_store: Option<&'a Arc<dyn IdempotencyStore>>,
}

impl<'a, A: DatabaseAdapter> RestHandler<'a, A> {
    /// Create a new REST handler.
    #[must_use]
    pub const fn new(
        executor: &'a Arc<Executor<A>>,
        schema: &'a CompiledSchema,
        config: &'a RestConfig,
        route_table: &'a RestRouteTable,
    ) -> Self {
        Self {
            executor,
            schema,
            config,
            route_table,
            idempotency_store: None,
        }
    }

    /// Access the underlying executor.
    #[must_use]
    pub const fn executor(&self) -> &Arc<Executor<A>> {
        self.executor
    }

    /// Access the REST configuration.
    #[must_use]
    pub const fn config(&self) -> &RestConfig {
        self.config
    }
}
