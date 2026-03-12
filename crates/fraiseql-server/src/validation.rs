//! GraphQL request validation — re-exported from `fraiseql-core`.
//!
//! The canonical AST-based implementation lives in
//! [`fraiseql_core::graphql::complexity`]. This module re-exports the public
//! types for use within `fraiseql-server` without duplicating logic.

pub use fraiseql_core::graphql::complexity::{
    ComplexityConfig, QueryMetrics, RequestValidator, ValidationError,
};
