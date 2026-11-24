//! Where a sort key puts its NULLs: the placement a `SortDirection` value names.
//!
//! `ASC` and `DESC` sort a key's NULLs where PostgreSQL does for the direction: last
//! ascending, first descending. The other four name the placement, the two that spell out
//! the direction's default included. Every reader works from the **effective** placement
//! ([`nulls_last`]), so an omitted placement and the direction's default spelled out are
//! one ordering: the same SQL, the same cursor fingerprint, the same cached page.

use serde::{Deserialize, Serialize};

use super::sql_hints::OrderDirection;

/// Where a sort key puts its NULLs, when its `SortDirection` names a placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NullsOrder {
    /// NULLs before every value.
    First,
    /// NULLs after every value.
    Last,
}

/// Whether a key sorted `direction` puts its NULLs last, given its requested placement.
#[must_use]
pub const fn nulls_last(direction: OrderDirection, nulls: Option<NullsOrder>) -> bool {
    match nulls {
        Some(NullsOrder::Last) => true,
        Some(NullsOrder::First) => false,
        None => matches!(direction, OrderDirection::Asc),
    }
}

/// The `NULLS` clause a key sorted `direction` with its NULLs `last` needs.
///
/// Empty where that is PostgreSQL's default for the direction, so an ordering that asks for
/// nothing else keeps its SQL, and an index declared with the same placement still serves it.
#[must_use]
pub const fn nulls_sql(direction: OrderDirection, last: bool) -> &'static str {
    match (matches!(direction, OrderDirection::Asc), last) {
        (true, true) | (false, false) => "",
        (_, true) => " NULLS LAST",
        (_, false) => " NULLS FIRST",
    }
}

/// The published `SortDirection` values: the two directions, then each with a placement.
pub const SORT_DIRECTIONS: [&str; 6] = [
    "ASC",
    "DESC",
    "ASC_NULLS_FIRST",
    "ASC_NULLS_LAST",
    "DESC_NULLS_FIRST",
    "DESC_NULLS_LAST",
];

/// The direction and placement a [`SORT_DIRECTIONS`] value names, or `None` for any other
/// string. Exact: a caller that reads a direction in any case upper-cases it first.
#[must_use]
pub fn parse_sort_direction(name: &str) -> Option<(OrderDirection, Option<NullsOrder>)> {
    let (direction, placement) = match name.strip_prefix("ASC") {
        Some(rest) => (OrderDirection::Asc, rest),
        None => (OrderDirection::Desc, name.strip_prefix("DESC")?),
    };
    let nulls = match placement {
        "" => None,
        "_NULLS_FIRST" => Some(NullsOrder::First),
        "_NULLS_LAST" => Some(NullsOrder::Last),
        _ => return None,
    };
    Some((direction, nulls))
}

/// [`SORT_DIRECTIONS`] as a refusal lists them: `` `ASC`, `DESC`, … ``.
#[must_use]
pub fn sort_directions_listed() -> String {
    SORT_DIRECTIONS.map(|d| format!("`{d}`")).join(", ")
}

#[cfg(test)]
#[path = "nulls_order_tests.rs"]
mod tests;
