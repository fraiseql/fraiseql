//! Type-erased executor interface.
//!
//! This module provides [`ExecutorAdapter`], a trait that allows code driving
//! query execution (e.g., `fraiseql-server`, tests) to hold an
//! `Arc<dyn ExecutorAdapter>` without being generic over a concrete
//! `DatabaseAdapter` type parameter.
//!
//! # Design Rationale
//!
//! `Executor<A>` is generic over its database adapter. Without type erasure,
//! every struct that holds an executor — the HTTP server, middleware, test
//! harnesses — must carry that type parameter, which produces significant
//! generic noise and makes dynamic dispatch impossible.
//!
//! `ExecutorAdapter` solves this by providing a single object-safe trait that
//! concrete `Executor<A>` implementations can implement, enabling uniform
//! `Arc<dyn ExecutorAdapter>` storage.
//!
//! # Example
//!
//! ```no_run
//! // Requires: a concrete ExecutorAdapter implementation.
//! use fraiseql_core::runtime::{ExecutionContext, ExecutorAdapter};
//! use std::sync::Arc;
//!
//! async fn run_query(exec: Arc<dyn ExecutorAdapter>, query: &str) -> String {
//!     let ctx = ExecutionContext::new("query-1".to_string());
//!     exec.execute_with_context(query, None, &ctx).await.unwrap()
//! }
//! ```

use std::pin::Pin;

use crate::{error::Result, runtime::ExecutionContext};

/// Type-erased executor interface.
///
/// Allows code that drives query execution (`fraiseql-server`, tests) to hold
/// `Arc<dyn ExecutorAdapter>` without being generic over `DatabaseAdapter`.
///
/// Concrete implementations should implement this trait on their
/// `Executor<A>` type to participate in the type-erased execution path.
pub trait ExecutorAdapter: Send + Sync {
    /// Execute a GraphQL query string with an execution context.
    ///
    /// # Arguments
    ///
    /// * `query` — the raw GraphQL query document
    /// * `variables` — optional JSON object of query variables
    /// * `ctx` — execution context carrying the query ID and cancellation token
    ///
    /// # Returns
    ///
    /// A JSON-serialised GraphQL response string on success.
    ///
    /// # Errors
    ///
    /// Returns a [`crate::error::FraiseQLError`] if parsing, validation, or
    /// SQL execution fails, or if the context's cancellation token has already
    /// been triggered.
    fn execute_with_context<'a>(
        &'a self,
        query: &'a str,
        variables: Option<&'a serde_json::Value>,
        ctx: &'a ExecutionContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>>;
}
