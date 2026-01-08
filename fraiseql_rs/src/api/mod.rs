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

pub mod cache;
pub mod engine;
pub mod error;
pub mod executor;
pub mod parser;
pub mod planner;
pub mod py_bindings;
pub mod storage;
pub mod types;

#[cfg(test)]
mod integration_tests;

// Re-export public types for convenience
pub use cache::{CacheBackend, CacheError, MemoryCache};
pub use engine::GraphQLEngine;
pub use error::ApiError;
pub use executor::{ExecutionError, ExecutionResult, Executor};
pub use parser::{
    parse_graphql_mutation, parse_graphql_query, FieldSelection, OperationType, ParsedQuery,
};
pub use planner::{ExecutionPlan, Planner, ResponseMetadata, ResultMapping, SqlQuery};
pub use py_bindings::PyGraphQLEngine;
pub use storage::{PostgresBackend, StorageBackend, StorageError};
pub use types::{
    GraphQLError, GraphQLResponse, MutationRequest, PathElement, QueryRequest, SourceLocation,
};

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
