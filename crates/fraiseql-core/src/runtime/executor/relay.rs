//! Type-erased relay cursor dispatch.
//!
//! `RelayDispatch` is a private type-erased relay executor stored as
//! `Option<Arc<dyn RelayDispatch>>` in `Executor<A>`.  It is populated at
//! construction time only when `A: RelayDatabaseAdapter`, giving us:
//!
//!  - No `unreachable!()` in non-relay adapters.
//!  - No capability flag to keep in sync.
//!  - `execute_relay_page` exists *only* on relay-capable adapters.
//!  - One clean runtime `Option::is_some()` check in the dispatcher.
//!
//! This design works in stable Rust because specialisation is not required —
//! the selection happens in two differently-named constructors.

use std::sync::Arc;

use futures::future::BoxFuture;

use crate::{
    compiler::aggregation::OrderByClause,
    db::{CursorValue, RelayDatabaseAdapter, WhereClause, traits::RelayPageResult},
    error::Result,
};

pub(super) trait RelayDispatch: Send + Sync {
    // Reason: relay pagination requires all cursor/filter/sort/count arguments; no natural grouping.
    #[allow(clippy::too_many_arguments)]
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> BoxFuture<'a, Result<RelayPageResult>>;
}

pub(super) struct RelayDispatchImpl<A: RelayDatabaseAdapter>(pub(super) Arc<A>);

impl<A: RelayDatabaseAdapter + Send + Sync + 'static> RelayDispatch for RelayDispatchImpl<A> {
    // Reason: relay pagination requires all cursor/filter/sort/count arguments; no natural grouping.
    #[allow(clippy::too_many_arguments)]
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> BoxFuture<'a, Result<RelayPageResult>> {
        Box::pin(self.0.execute_relay_page(
            view,
            cursor_column,
            after,
            before,
            limit,
            forward,
            where_clause,
            order_by,
            include_total_count,
        ))
    }
}
