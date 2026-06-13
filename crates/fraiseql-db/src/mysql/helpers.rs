//! Helper functions for MySQL relay pagination SQL generation.
//!
//! MySQL error-number → `SQLSTATE` mapping lives in
//! [`super::adapter::map_mysql_error_code`] (the single live copy used by the
//! adapter's `mysql_sql_state` seam); this module no longer carries a drifted
//! duplicate of it.

use crate::types::sql_hints::{OrderByClause, OrderDirection};

/// Build the `ORDER BY` clause for a relay page query.
///
/// Custom `order_by` columns come first (using MySQL JSON path syntax), then the
/// cursor column is appended as a stable tiebreaker.  The sort direction is
/// flipped for backward queries (inner subquery) and restored by the outer
/// `ORDER BY _relay_cursor ASC` wrapper.
pub(super) fn build_mysql_relay_order_sql(
    quoted_col: &str,
    order_by: Option<&[OrderByClause]>,
    forward: bool,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(clauses) = order_by {
        for c in clauses {
            let dir = match (c.direction, forward) {
                (OrderDirection::Asc, true) | (OrderDirection::Desc, false) => "ASC",
                (OrderDirection::Desc, true) | (OrderDirection::Asc, false) => "DESC",
            };
            // JSON_UNQUOTE(JSON_EXTRACT(data, '$.field')) — field names are validated
            // GraphQL identifiers, which cannot contain ' or other SQL-special chars.
            let escaped = c.field.replace('\'', "''");
            parts.push(format!("JSON_UNQUOTE(JSON_EXTRACT(data, '$.{escaped}')) {dir}"));
        }
    }

    let cursor_dir = if forward { "ASC" } else { "DESC" };
    parts.push(format!("{quoted_col} {cursor_dir}"));
    format!(" ORDER BY {}", parts.join(", "))
}

/// Combine cursor and user WHERE conditions into a single `WHERE` clause fragment.
pub(super) fn build_mysql_relay_where(cursor_sql: Option<&str>, user_sql: Option<&str>) -> String {
    match (cursor_sql, user_sql) {
        (None, None) => String::new(),
        (Some(c), None) => format!(" WHERE {c}"),
        (None, Some(u)) => format!(" WHERE ({u})"),
        (Some(c), Some(u)) => format!(" WHERE {c} AND ({u})"),
    }
}
