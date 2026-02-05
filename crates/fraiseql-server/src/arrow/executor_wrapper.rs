//! Executor wrapper that implements QueryExecutor trait for Arrow Flight.
//!
//! This module provides a wrapper around the generic `Executor<A>` type
//! that implements the `QueryExecutor` trait for type erasure, allowing
//! the executor to be used with FraiseQLFlightService.

use async_trait::async_trait;
use fraiseql_arrow::QueryExecutor;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::Executor,
    security::SecurityContext,
};
use std::sync::Arc;

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
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<A: DatabaseAdapter + 'static> QueryExecutor for ExecutorQueryAdapter<A> {
    async fn execute_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String, String> {
        self.executor
            .execute_with_security(query, variables, security_context)
            .await
            .map_err(|e| e.to_string())
    }
}
