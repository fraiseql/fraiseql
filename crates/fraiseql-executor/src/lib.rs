//! # fraiseql-executor
//!
//! Query execution engine for FraiseQL v2.
//!
//! This crate re-exports [`Executor`] from `fraiseql-core` and provides the
//! [`ExecutorAdapter`] trait for type-erased execution.
//!
//! # Usage
//!
//! ```ignore
//! use fraiseql_executor::{Executor, ExecutorAdapter};
//! use fraiseql_core::db::PostgresAdapter;
//! use fraiseql_core::schema::CompiledSchema;
//!
//! let schema = CompiledSchema::from_json(json_str)?;
//! let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
//! let executor = Executor::new(schema, std::sync::Arc::new(adapter));
//! ```

#![forbid(unsafe_code)]
#![allow(missing_docs)]

// Re-export Executor from fraiseql-core for convenience
pub use fraiseql_core::runtime::Executor;

use fraiseql_core::{error::Result, runtime::ExecutionContext};
use std::pin::Pin;

/// Type-erased executor interface.
///
/// Allows code that drives query execution (fraiseql-server, tests) to hold
/// `Arc<dyn ExecutorAdapter>` without being generic over `DatabaseAdapter`.
///
/// # CS-2: ExecutorAdapter Trait
///
/// This trait enables type erasure for the executor, removing the need for
/// generic `A` type parameters in `Server<A>` and similar types.
///
/// # Example
///
/// ```ignore
/// use fraiseql_executor::ExecutorAdapter;
/// use std::sync::Arc;
///
/// async fn run_query(exec: Arc<dyn ExecutorAdapter>, query: &str) -> String {
///     let ctx = ExecutionContext::new("query-1".to_string());
///     exec.execute_with_context(query, None, &ctx).await.unwrap()
/// }
/// ```
pub trait ExecutorAdapter: Send + Sync {
    /// Execute a GraphQL query with an execution context.
    fn execute_with_context<'a>(
        &'a self,
        query: &'a str,
        variables: Option<&'a serde_json::Value>,
        ctx: &'a ExecutionContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>>;
}
