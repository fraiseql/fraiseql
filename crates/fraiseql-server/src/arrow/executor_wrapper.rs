//! Executor wrapper that implements `QueryExecutor` trait for Arrow Flight.
//!
//! This module provides a wrapper around the generic `Executor<A>` type
//! that implements the `QueryExecutor` trait for type erasure, allowing
//! the executor to be used with `FraiseQLFlightService`.

use std::sync::Arc;

use async_trait::async_trait;
use fraiseql_arrow::QueryExecutor;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, security::SecurityContext};

/// Wrapper that adapts `Executor<A>` to the `QueryExecutor` trait.
///
/// This enables the Arrow Flight service to execute GraphQL queries
/// with RLS filtering without knowing the specific database adapter type.
pub struct ExecutorQueryAdapter<A: DatabaseAdapter> {
    /// The underlying executor
    executor: Arc<Executor<A>>,
}

impl<A: DatabaseAdapter> ExecutorQueryAdapter<A> {
    /// Create a new executor adapter.
    ///
    /// # Arguments
    /// * `executor` - The executor instance to wrap
    #[must_use]
    pub const fn new(executor: Arc<Executor<A>>) -> Self {
        Self { executor }
    }
}

// Reason: QueryExecutor is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl<A: DatabaseAdapter + 'static> QueryExecutor for ExecutorQueryAdapter<A> {
    /// # Errors
    ///
    /// Returns the executor's error unchanged.
    ///
    /// #1201: this used to be `.map_err(|e| e.to_string())`, and that call was
    /// where the Flight transport's error classification died. Everything below
    /// it saw a bare `String`, so a client's parse or validation error and a
    /// database outage were indistinguishable and both became gRPC `INTERNAL`.
    /// Passing the typed error through is the fix; the mapping to a status code
    /// happens once, at the transport edge.
    async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<serde_json::Value, fraiseql_core::error::FraiseQLError> {
        self.executor.execute_with_security(query, variables, security_context).await
    }
}
