//! Helper functions for SQL Server parameter binding and Relay SQL generation.

use fraiseql_error::{FraiseQLError, Result};

use crate::types::sql_hints::{OrderByClause, OrderDirection};

// ============================================================================
// Parameter binding helpers
// ============================================================================

/// Pre-serialise `Array` and `Object` values so their string forms live long enough
/// to be referenced by `bind_json_params`.
pub(super) fn serialise_complex_params(params: &[serde_json::Value]) -> Vec<String> {
    params
        .iter()
        .filter(|v| matches!(v, serde_json::Value::Array(_) | serde_json::Value::Object(_)))
        .map(|v| v.to_string())
        .collect()
}

/// Bind `serde_json::Value` parameters to a tiberius `Query`.
///
/// `string_params` must be the output of `serialise_complex_params` for the same
/// `params` slice.  The caller allocates this before creating the `Query` so the
/// strings live long enough for the lifetime `'a`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if a `Number` value cannot be represented
/// as either `i64` or `f64` (this is extremely rare and indicates an out-of-range
/// numeric literal in the GraphQL input).
pub(super) fn bind_json_params<'a>(
    query: &mut tiberius::Query<'a>,
    params: &'a [serde_json::Value],
    string_params: &'a [String],
) -> Result<()> {
    let mut string_idx = 0usize;
    for param in params {
        match param {
            serde_json::Value::String(s) => query.bind(s.as_str()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    query.bind(i);
                } else if let Some(f) = n.as_f64() {
                    query.bind(f);
                } else {
                    return Err(FraiseQLError::Validation {
                        message: format!("Cannot bind numeric value {n}: out of i64 and f64 range"),
                        path:    None,
                    });
                }
            },
            serde_json::Value::Bool(b) => query.bind(*b),
            serde_json::Value::Null => query.bind(Option::<&str>::None),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                query.bind(string_params[string_idx].as_str());
                string_idx += 1;
            },
        }
    }
    Ok(())
}

// ============================================================================
// Relay cursor pagination
// ============================================================================

/// Build the ORDER BY clause for a relay page query.
///
/// Custom sort columns come first, then the cursor column as tiebreaker.
/// For backward pagination every direction is flipped so the inner `FETCH NEXT` subquery
/// retrieves the correct `N` rows before the cursor; the outer re-sort in
/// `build_relay_backward_outer_order_sql` then restores the original order.
pub(super) fn build_relay_order_sql(
    quoted_col: &str,
    order_by: Option<&[OrderByClause]>,
    forward: bool,
) -> String {
    if let Some(clauses) = order_by {
        let mut parts: Vec<String> = clauses
            .iter()
            .map(|c| {
                // Flip every custom sort direction for backward pages so the inner
                // DESC subquery fetches the correct N rows before the cursor.
                let dir = match (c.direction, forward) {
                    (OrderDirection::Asc, true) => "ASC",
                    (OrderDirection::Asc, false) => "DESC",
                    (OrderDirection::Desc, true) => "DESC",
                    (OrderDirection::Desc, false) => "ASC",
                };
                // Field names are validated as GraphQL identifiers (no single quotes).
                format!("JSON_VALUE(data, '$.{}') {dir}", c.field)
            })
            .collect();
        let primary_dir = if forward { "ASC" } else { "DESC" };
        parts.push(format!("{quoted_col} {primary_dir}"));
        format!(" ORDER BY {}", parts.join(", "))
    } else {
        let dir = if forward { "ASC" } else { "DESC" };
        format!(" ORDER BY {quoted_col} {dir}")
    }
}

/// Build the outer ORDER BY for the backward-page re-sort wrapper.
///
/// Uses the *original* (non-flipped) sort directions and references the aliased
/// sort columns (`_relay_sort_0`, `_relay_sort_1`, …) projected by the inner query.
/// The cursor column is always the final tiebreaker, re-sorted ASC to present rows
/// in ascending cursor order as required by the Relay spec.
pub(super) fn build_relay_backward_outer_order_sql(order_by: Option<&[OrderByClause]>) -> String {
    if let Some(clauses) = order_by {
        let mut parts: Vec<String> = clauses
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let dir = match c.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };
                format!("_relay_sort_{i} {dir}")
            })
            .collect();
        parts.push("_relay_cursor ASC".to_string());
        format!(" ORDER BY {}", parts.join(", "))
    } else {
        " ORDER BY _relay_cursor ASC".to_string()
    }
}

/// Validate that a string looks like a UUID without pulling in regex.
///
/// Accepts the canonical `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` form (36 chars).
/// Returns `false` for any other format so callers can emit a clean
/// `FraiseQLError::Validation` instead of letting SQL Server produce an opaque
/// type-conversion error (MSSQL 8169).
pub(super) fn is_valid_uuid_format(uuid: &str) -> bool {
    let parts: Vec<&str> = uuid.split('-').collect();
    matches!(
        parts.as_slice(),
        [p0, p1, p2, p3, p4]
            if p0.len() == 8
            && p1.len() == 4
            && p2.len() == 4
            && p3.len() == 4
            && p4.len() == 12
            && uuid.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
    )
}

/// Build the WHERE portion of a relay page query.
pub(super) fn build_relay_where_sql(cursor_part: Option<&str>, user_part: Option<&str>) -> String {
    match (cursor_part, user_part) {
        (None, None) => String::new(),
        (Some(c), None) => format!(" WHERE {c}"),
        (None, Some(u)) => format!(" WHERE ({u})"),
        (Some(c), Some(u)) => format!(" WHERE {c} AND ({u})"),
    }
}
