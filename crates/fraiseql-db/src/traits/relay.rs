//! Relay cursor pagination trait.
//!
//! [`RelayDatabaseAdapter`] extends [`DatabaseAdapter`](super::DatabaseAdapter)
//! with keyset-based pagination for Relay-style GraphQL connections.

use std::future::Future;

use fraiseql_error::Result;

use super::{DatabaseAdapter, RelayCursor, RelayPageResult};
use crate::{
    types::{ReadRouting, sql_hints::OrderByClause},
    where_clause::WhereClause,
};

/// Database adapter supertrait for adapters that implement Relay cursor pagination.
///
/// Only adapters that genuinely support keyset pagination need to implement this trait.
/// Non-implementing adapters carry no relay code at all — no stubs, no flags.
///
/// # Implementors
///
/// - `PostgresAdapter` — full keyset pagination
/// - `CachedDatabaseAdapter<A>` — delegates to inner `A`
///
/// `MySqlAdapter` was listed here until #374 removed the non-PostgreSQL backends.
///
/// # Usage
///
/// Construct an `Executor` with `Executor::new_with_relay` to enable relay
/// query execution. The bound `A: RelayDatabaseAdapter` is enforced at that call site.
pub trait RelayDatabaseAdapter: DatabaseAdapter {
    /// Execute keyset (cursor-based) pagination against a JSONB view.
    ///
    /// # Arguments
    ///
    /// * `view`                — SQL view name (will be quoted before use)
    /// * `cursor_column`       — the connection's position column (e.g. `pk_user`, `id`), the
    ///   ordering's final tie-breaker, which makes it total
    /// * `after`               — forward cursor: rows after the cursor row in the ordering
    /// * `before`              — backward cursor: rows before the cursor row in the ordering
    /// * `limit`               — row fetch count (pass `page_size + 1` to detect `hasNextPage`)
    /// * `forward`             — `true`: the `limit` rows after `after`; `false`: the `limit` rows
    ///   nearest before `before`
    /// * `where_clause`        — optional user-supplied filter applied after the cursor condition
    /// * `order_by`            — optional custom sort; cursor column appended as tiebreaker
    /// * `include_total_count` — when `true`, compute the matching row count before LIMIT
    ///
    /// # Contract (#1521)
    ///
    /// The ordering is `order_by`'s keys, then `cursor_column` ascending. A cursor resumes past
    /// its row's sort-key values ([`RelayCursor::sort_keys`]), key by key in each key's
    /// direction with NULLs where the `ORDER BY` places them, then past its position. Rows come
    /// back **in that order whichever the direction**, so a backward page's extra row (the one
    /// past the page) is its first. On a page with an `order_by`, each row's document carries its
    /// sort-key values, as text or `null`, under [`crate::keyset::SORT_KEYS_KEY`]: the next
    /// cursor is built from them, and a row without them is refused.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on SQL execution failure.
    fn execute_relay_page<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<RelayCursor>,
        before: Option<RelayCursor>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
    ) -> impl Future<Output = Result<RelayPageResult>> + Send + 'a;

    /// Connection-affine variant of [`execute_relay_page`](Self::execute_relay_page).
    ///
    /// Applies `session_vars` transaction-locally on the same connection that
    /// runs the page (and total-count) queries, so RLS-protected relay
    /// pagination over views reading `current_setting()` returns the correct
    /// tenant's rows (fixes #329 for the cursor-pagination path).
    ///
    /// Adapters that do not support session variables inherit the default,
    /// which drops `session_vars` and delegates to `execute_relay_page`.
    ///
    /// # Errors
    ///
    /// Same errors as [`execute_relay_page`](Self::execute_relay_page);
    /// additionally returns `FraiseQLError::Database` if `set_config` fails.
    #[allow(clippy::too_many_arguments)] // Reason: relay pagination requires all cursor/filter/sort/count arguments plus session vars; no natural grouping
    fn execute_relay_page_with_session<'a>(
        &'a self,
        view: &'a str,
        cursor_column: &'a str,
        after: Option<RelayCursor>,
        before: Option<RelayCursor>,
        limit: u32,
        forward: bool,
        where_clause: Option<&'a WhereClause>,
        order_by: Option<&'a [OrderByClause]>,
        include_total_count: bool,
        _session_vars: &'a [(&'a str, &'a str)],
        _routing: ReadRouting,
    ) -> impl Future<Output = Result<RelayPageResult>> + Send + 'a {
        self.execute_relay_page(
            view,
            cursor_column,
            after,
            before,
            limit,
            forward,
            where_clause,
            order_by,
            include_total_count,
        )
    }
}
