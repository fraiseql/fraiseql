//! `RelayDatabaseAdapter` impl for `CachedDatabaseAdapter`.
//!
//! Relay pagination results are not cached — all calls are forwarded
//! directly to the underlying adapter.

use super::adapter::CachedDatabaseAdapter;
use crate::{
    db::{DatabaseAdapter, RelayDatabaseAdapter},
    error::Result,
};

impl<A: RelayDatabaseAdapter + DatabaseAdapter> RelayDatabaseAdapter for CachedDatabaseAdapter<A> {
    async fn execute_relay_page(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<crate::db::traits::CursorValue>,
        before: Option<crate::db::traits::CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&crate::db::where_clause::WhereClause>,
        order_by: Option<&[crate::compiler::aggregation::OrderByClause]>,
        include_total_count: bool,
        session_vars: &[(&str, &str)],
    ) -> Result<crate::db::traits::RelayPageResult> {
        // Relay pagination results are not cached — always delegate to the underlying adapter
        self.adapter
            .execute_relay_page(
                view,
                cursor_column,
                after,
                before,
                limit,
                forward,
                where_clause,
                order_by,
                include_total_count,
                session_vars,
            )
            .await
    }
}
