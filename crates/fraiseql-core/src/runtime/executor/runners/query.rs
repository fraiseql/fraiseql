//! Regular query execution runner.
//!
//! [`QueryRunner`] executes regular GraphQL queries (non-aggregate, non-mutation)
//! against the database. It is the execution engine for the most common path.
//!
//! Implementation is split across sub-modules:
//! - [`query_projection`](super::query_projection): projection field builders and ORDER BY
//!   enrichment
//! - [`query_params`](super::query_params): WHERE clause helpers and cache key computation
//! - [`query_regular`](super::query_regular): regular (non-relay) query execution
//! - [`query_relay`](super::query_relay): Relay connection and node query execution

use std::sync::Arc;

use super::super::context::ExecutorContext;
// Re-export sub-module items so that `use super::*` in tests
// continues to find them without import path changes.
#[cfg(test)]
pub use super::query_params::{
    combine_explicit_arg_where, compute_projection_reduction, pg_type_to_cast,
};
#[cfg(test)]
pub use super::query_projection::selections_contain_field;
#[cfg(test)]
pub use crate::backend::{WhereClause, WhereOperator};

/// Executes regular GraphQL queries and relay/node lookups.
pub(in super::super) struct QueryRunner {
    pub(super) ctx: Arc<ExecutorContext>,
}

impl QueryRunner {
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext>) -> Self {
        Self { ctx }
    }
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod query_runner_tests;
