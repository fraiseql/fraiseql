//! Regular query execution runner.
//!
//! [`QueryRunner`] executes regular GraphQL queries (non-aggregate, non-mutation)
//! against the database. It is the execution engine for the most common path.
//!
//! Implementation is split across sub-modules:
//! - [`query_projection`](super::query_projection): projection field builders and ORDER BY enrichment
//! - [`query_params`](super::query_params): WHERE clause helpers and cache key computation
//! - [`query_regular`](super::query_regular): regular (non-relay) query execution
//! - [`query_relay`](super::query_relay): Relay connection and node query execution

use std::sync::Arc;

use super::super::context::ExecutorContext;
use crate::db::traits::DatabaseAdapter;

// Re-export sub-module items so that `use super::*` in tests
// continues to find them without import path changes.
#[cfg(test)]
pub use super::query_params::{
    combine_explicit_arg_where, compute_projection_reduction, pg_type_to_cast,
};
#[cfg(test)]
pub use super::query_projection::selections_contain_field;
#[cfg(test)]
pub use crate::db::{WhereClause, WhereOperator};

/// Executes regular GraphQL queries and relay/node lookups.
pub(in super::super) struct QueryRunner<A: DatabaseAdapter> {
    pub(super) ctx: Arc<ExecutorContext<A>>,
}

impl<A: DatabaseAdapter> QueryRunner<A> {
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext<A>>) -> Self {
        Self { ctx }
    }

    /// Extract an inline node ID literal from a `node(id: "...")` query string.
    ///
    /// Used as a fallback when the ID is not provided via variables.
    /// Returns `None` if no inline string literal can be found.
    pub(super) fn extract_inline_node_id(query: &str) -> Option<String> {
        let after_node = query.find("node(")?;
        let args_region = &query[after_node..];
        let after_id = args_region.find("id:")?;
        let after_colon = args_region[after_id + 3..].trim_start();
        let quote_char = after_colon.chars().next()?;
        if quote_char != '"' && quote_char != '\'' {
            return None;
        }
        let inner = &after_colon[1..];
        let end = inner.find(quote_char)?;
        Some(inner[..end].to_string())
    }
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod query_runner_tests;
