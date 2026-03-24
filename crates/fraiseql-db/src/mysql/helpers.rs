//! Helper functions for MySQL error mapping and relay pagination SQL generation.

use crate::types::sql_hints::{OrderByClause, OrderDirection};

/// Map MySQL error numbers to SQLSTATE strings for uniform error reporting.
///
/// MySQL error numbers are numeric codes from the MySQL error reference.
/// This mapping covers the most common integrity and transaction errors.
///
/// # Arguments
///
/// * `code` - MySQL error number (e.g., 1062 for duplicate entry)
///
/// # Returns
///
/// Returns a SQLSTATE string if a mapping exists, or `None` for unmapped codes.
pub(super) fn map_mysql_error_code(code: u16) -> Option<String> {
    let sqlstate = match code {
        // 1062: Duplicate entry for key (unique constraint violation)
        // 1169: Unique constraint violation (alternate code)
        1062 | 1169 => "23505",
        // 1048: Column cannot be null (NOT NULL violation)
        1048 => "23502",
        // 1451: Cannot delete or update a parent row (FK parent violation)
        // 1452: Cannot add or update a child row (FK child violation)
        1451 | 1452 => "23503",
        // 1205: Lock wait timeout exceeded — treat as serialization failure
        1205 => "40001",
        // 1213: Deadlock found when trying to get lock
        1213 => "40001",
        _ => return None,
    };
    Some(sqlstate.to_string())
}

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
