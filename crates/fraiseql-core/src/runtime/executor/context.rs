//! Shared execution context — holds all state shared across executor sub-components.
//!
//! [`ExecutorContext`] is the single source of truth for the schema, adapter,
//! configuration, and caches used during query execution. It is always accessed
//! via `Arc<ExecutorContext<A>>` so sub-executors can be cheaply cloned.

use std::{collections::HashMap, sync::Arc};

use moka::sync::Cache as MokaCache;

use crate::{
    db::{traits::DatabaseAdapter, types::PoolMetrics},
    graphql::ParsedQuery,
    runtime::{QueryMatcher, QueryPlanner, RuntimeConfig},
    schema::{CompiledSchema, IntrospectionResponses},
};

use super::{QueryType, support::relay::RelayDispatch};

/// All shared state for an executor instance.
///
/// Constructed once at `Executor::new()` / `Executor::with_config()` and then
/// stored as `Arc<ExecutorContext<A>>`. Sub-executors (query runner, mutation
/// runner, etc.) each hold a clone of this `Arc`.
pub(super) struct ExecutorContext<A: DatabaseAdapter> {
    /// Compiled schema with optimized SQL templates.
    pub(super) schema: CompiledSchema,

    /// Shared database adapter for query execution.
    pub(super) adapter: Arc<A>,

    /// Type-erased relay capability slot.
    ///
    /// `Some` when constructed via `new_with_relay`. `None` returns a
    /// `FraiseQLError::Validation` for relay queries — no `unreachable!()`.
    pub(super) relay: Option<Arc<dyn RelayDispatch>>,

    /// Query matching engine (stateless).
    pub(super) matcher: QueryMatcher,

    /// Query execution planner (stateless).
    pub(super) planner: QueryPlanner,

    /// Runtime configuration (timeouts, complexity limits, etc.).
    pub(super) config: RuntimeConfig,

    /// Pre-built introspection responses cached for `__schema` and `__type` queries.
    pub(super) introspection: IntrospectionResponses,

    /// O(1) lookup index for Relay `node(id)` queries.
    pub(super) node_type_index: HashMap<String, Arc<str>>,

    /// Parsed GraphQL AST cache, keyed by xxHash64 of the query string.
    pub(super) parse_cache: MokaCache<u64, Arc<(QueryType, Option<ParsedQuery>)>>,

    /// Optional executor-level response cache.
    pub(super) response_cache: Option<Arc<crate::cache::ResponseCache>>,
}

impl<A: DatabaseAdapter> ExecutorContext<A> {
    /// Return current connection pool metrics.
    pub(super) fn pool_metrics(&self) -> PoolMetrics {
        self.adapter.pool_metrics()
    }
}
