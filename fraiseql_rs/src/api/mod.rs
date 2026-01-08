//! Public API for FraiseQL GraphQL Engine
//!
//! This module provides the official public interface for Python to interact
//! with the FraiseQL Rust engine. All other modules are internal and should
//! not be accessed directly from Python.
//!
//! # Usage
//!
//! ```python
//! from fraiseql_rs import GraphQLEngine
//!
//! engine = GraphQLEngine('{"db": "postgres://..."}')
//! result = engine.execute_query("{ users { id name } }", {})
//! ```

pub mod engine;
pub mod error;
pub mod types;
pub mod py_bindings;
pub mod parser;
pub mod planner;
pub mod executor;
pub mod storage;
pub mod cache;

#[cfg(test)]
mod integration_tests;

// Re-export public types for convenience
pub use engine::GraphQLEngine;
pub use error::ApiError;
pub use types::{
    QueryRequest, MutationRequest, GraphQLResponse,
    GraphQLError, SourceLocation, PathElement,
};
pub use py_bindings::PyGraphQLEngine;
pub use parser::{ParsedQuery, OperationType, FieldSelection, parse_graphql_query, parse_graphql_mutation};
pub use planner::{Planner, ExecutionPlan, SqlQuery, ResultMapping, ResponseMetadata};
pub use executor::{Executor, ExecutionResult, ExecutionError};
pub use storage::{StorageBackend, PostgresBackend, StorageError};
pub use cache::{CacheBackend, MemoryCache, CacheError};

/// Check if module is available
pub fn is_available() -> bool {
    true
}

/// Get module version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_available() {
        assert!(is_available());
    }

    #[test]
    fn test_api_version() {
        assert!(!version().is_empty());
    }
}
