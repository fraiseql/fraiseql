//! Keyset (cursor) paging over an ordering (#1521).
//!
//! A relay page after a cursor resumes **after the cursor's row in the requested order**:
//! past the row's sort-key values, lexicographically, then past its position (the
//! connection's cursor column), which makes the order total. Resuming on the position alone,
//! as before #1521, is correct only when the ordering *is* the position: under any other
//! ordering it skipped and repeated rows.
//!
//! The predicate is expanded per key, so keys may sort in different directions and hold
//! NULLs. PostgreSQL places NULLs last for `ASC` and first for `DESC`, and the expansion
//! places them the same way:
//!
//! ```text
//! (k1 after v1)
//! OR (k1 = v1 AND k2 after v2)
//! OR (k1 = v1 AND k2 = v2 AND position > p)
//! ```
//!
//! A key is rendered by the same rules the `ORDER BY` uses
//! ([`sort_expr`](crate::order_by::sort_expr)): its native column or typed JSON
//! extraction, its collation (#1512), a localized key's label (#1513). Values
//! travel as text (each key is selected `::text` beside its row) and are cast back to the
//! key's own type when bound, so a number compares as a number and a timestamp as a
//! timestamp.

use std::fmt::Write as _;

use crate::{
    dialect::{PostgresDialect, SqlDialect},
    types::{
        DatabaseType,
        sql_hints::{OrderByClause, OrderDirection, ScalarFieldType},
    },
};

/// The key a keyset page's document carries its row's sort-key values under.
///
/// They are a JSON array of text (or `null`) in ordering order: what the next page's cursor is
/// built from. Present only on a page with an ordering; never selected by a projection.
pub const SORT_KEYS_KEY: &str = "__fraiseql_sort_keys";

/// `document` with its row's sort-key values under [`SORT_KEYS_KEY`], for a page ordered by
/// `keys`; `document` unchanged when there are none.
#[must_use]
pub fn with_sort_keys(document: &str, keys: &[KeysetKey]) -> String {
    if keys.is_empty() {
        return document.to_string();
    }
    let values: Vec<String> = keys.iter().map(|k| format!("({})::text", k.expr)).collect();
    format!(
        "({document} || jsonb_build_object('{SORT_KEYS_KEY}', jsonb_build_array({})))",
        values.join(", ")
    )
}

/// One sort key of a keyset: its SQL expression, its direction, and how a value bound for it
/// is cast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetKey {
    /// The key as the `ORDER BY` renders it, collation included.
    pub expr:      String,
    /// The requested direction.
    pub direction: OrderDirection,
    /// The cast a bound value takes, applied to a text placeholder.
    cast:          Cast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cast {
    /// Compared as text.
    Text,
    /// A typed JSON extraction or a scalar: the dialect's scalar cast.
    Scalar(ScalarFieldType),
    /// A native column of this PostgreSQL type.
    Native(String),
    /// A vector distance.
    Float,
}

impl KeysetKey {
    /// `placeholder` as a value of this key's type.
    ///
    /// Every value is bound as text, so the placeholder is resolved as `text` first and cast
    /// from there, booleans included (the filter path binds those natively, and its casts
    /// leave them bare).
    fn param(&self, placeholder: &str) -> String {
        let dialect = PostgresDialect;
        match &self.cast {
            Cast::Text => format!("{placeholder}::text"),
            Cast::Scalar(ty) => match dialect.cast_type_name(*ty) {
                Some(name) => format!("({placeholder}::text)::{name}"),
                None => format!("{placeholder}::text"),
            },
            Cast::Native(native) => match native.to_lowercase().as_str() {
                "boolean" | "bool" => format!("({placeholder}::text)::boolean"),
                "text" | "varchar" | "character varying" | "char" | "bpchar" | "name" => {
                    format!("{placeholder}::text")
                },
                _ => dialect.cast_native_param(placeholder, native),
            },
            Cast::Float => format!("({placeholder}::text)::float8"),
        }
    }
}

/// The keyset keys of `order_by`, in order: one per clause, as the `ORDER BY` sorts them.
///
/// # Errors
///
/// A relevance ordering (it cannot be paged by cursor, #1284); a key the renderer refuses.
pub fn keyset_keys(order_by: Option<&[OrderByClause]>) -> crate::Result<Vec<KeysetKey>> {
    crate::order_by::refuse_relevance_under_cursor_pagination(order_by)?;
    order_by
        .unwrap_or_default()
        .iter()
        .map(|clause| {
            let expr = crate::order_by::sort_expr(clause, DatabaseType::PostgreSQL)?;
            let cast = if clause.vector.is_some() {
                Cast::Float
            } else if clause.localized.is_some() {
                Cast::Text
            } else if let (Some(_), Some(native)) = (&clause.native_column, &clause.native_type) {
                Cast::Native(native.clone())
            } else {
                Cast::Scalar(clause.field_type)
            };
            Ok(KeysetKey {
                expr,
                direction: clause.direction,
                cast,
            })
        })
        .collect()
}

/// The ordering `keys` sort by, as text: each key's SQL expression and direction.
///
/// Two orderings that page alike have the same signature, and a cursor records the signature
/// of the ordering it was issued under, so it is never resumed under another: another field or
/// direction, another locale's label or collation, another vector operand.
#[must_use]
pub fn keyset_signature(keys: &[KeysetKey]) -> String {
    keys.iter()
        .map(|key| {
            let direction = match key.direction {
                OrderDirection::Asc => "ASC",
                OrderDirection::Desc => "DESC",
            };
            format!("{} {direction}", key.expr)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The keyset predicate: rows strictly after (`forward`) or before the cursor's row.
///
/// The cursor row's key values are `values` (text, `None` for NULL); its position is the
/// parameter at the placeholder this returns.
///
/// `position` is the position column's expression. Key values are bound from `first_param`
/// on, in key order, skipping NULLs (which bind nothing); the position is bound right after
/// them. Returns the SQL, the key parameters to bind, and the position's placeholder index.
///
/// # Errors
///
/// `Validation` when `values` does not have one entry per key.
pub fn keyset_predicate(
    keys: &[KeysetKey],
    values: &[Option<String>],
    position: &str,
    position_param: impl Fn(&str) -> String,
    forward: bool,
    first_param: usize,
) -> crate::Result<(String, Vec<String>, usize)> {
    if keys.len() != values.len() {
        return Err(fraiseql_error::FraiseQLError::validation(format!(
            "the cursor carries {} sort key(s), and the ordering has {}",
            values.len(),
            keys.len()
        )));
    }
    let dialect = PostgresDialect;
    let mut params = Vec::new();
    let mut disjuncts = Vec::new();
    let mut equal_so_far: Vec<String> = Vec::new();
    for (key, value) in keys.iter().zip(values) {
        let expr = &key.expr;
        let bound = value.as_ref().map(|v| {
            params.push(v.clone());
            key.param(&dialect.placeholder(first_param + params.len() - 1))
        });
        // Past this key in the direction the page reads it. The ORDER BY puts NULLs last for
        // ASC and first for DESC, so a page reading the key ascending (ASC forward, DESC
        // backward) meets every value, then the NULLs; one reading it descending meets the
        // NULLs first, so they are behind any value.
        let ascending_page = matches!(key.direction, OrderDirection::Asc) == forward;
        let beyond = match (&bound, ascending_page) {
            (Some(v), true) => Some(format!("({expr} > {v} OR {expr} IS NULL)")),
            (Some(v), false) => Some(format!("{expr} < {v}")),
            // From a NULL: read ascending, nothing is beyond the NULLs; read descending,
            // every value is.
            (None, true) => None,
            (None, false) => Some(format!("{expr} IS NOT NULL")),
        };
        if let Some(beyond) = beyond {
            disjuncts.push(conjunction(&equal_so_far, beyond));
        }
        equal_so_far.push(match &bound {
            Some(v) => format!("{expr} = {v}"),
            None => format!("{expr} IS NULL"),
        });
    }
    let position_index = first_param + params.len();
    let comparison = if forward { ">" } else { "<" };
    let position_value = position_param(&dialect.placeholder(position_index));
    disjuncts.push(conjunction(&equal_so_far, format!("{position} {comparison} {position_value}")));
    Ok((format!("({})", disjuncts.join(" OR ")), params, position_index))
}

/// `prefix AND last`, parenthesized when there is a prefix.
fn conjunction(prefix: &[String], last: String) -> String {
    if prefix.is_empty() {
        return last;
    }
    let mut terms = prefix.to_vec();
    terms.push(last);
    format!("({})", terms.join(" AND "))
}

/// The `ORDER BY` terms of a keyset page.
///
/// Forward: every key in its requested direction, then the position ascending. Backward: every
/// key and the position reversed (the inner query of a backward page, which the outer query
/// re-sorts with [`carried_keys`]).
#[must_use]
pub fn keyset_order(keys: &[KeysetKey], position: &str, forward: bool) -> String {
    let mut terms: Vec<String> = keys
        .iter()
        .map(|key| {
            let direction = match (key.direction, forward) {
                (OrderDirection::Asc, true) | (OrderDirection::Desc, false) => "ASC",
                _ => "DESC",
            };
            format!("{} {direction}", key.expr)
        })
        .collect();
    terms.push(format!("{position} {}", if forward { "ASC" } else { "DESC" }));
    terms.join(", ")
}

/// The keys a backward page's inner query carries out beside its rows, so its outer query can
/// restore the requested order.
///
/// Returns the select-list suffix (`, <key> AS <prefix>0, …`) and the keys re-read from those
/// columns of the subquery aliased `subquery` (`<subquery>.<prefix>0`, …), each with its
/// direction; the columns keep their expression's collation.
#[must_use]
pub fn carried_keys(keys: &[KeysetKey], prefix: &str, subquery: &str) -> (String, Vec<KeysetKey>) {
    let mut select = String::new();
    let carried = keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let _ = write!(select, ", {} AS {prefix}{i}", key.expr);
            KeysetKey {
                expr: format!("{subquery}.{prefix}{i}"),
                ..key.clone()
            }
        })
        .collect();
    (select, carried)
}

/// The cursor row's position as a parameter, and its placeholder's SQL.
///
/// A `uuid` position is bound as text and resolved as `text` before its cast: a bare
/// `$n::uuid` makes PostgreSQL expect a uuid's binary encoding.
#[cfg(feature = "postgres")]
#[must_use]
pub fn position_param(
    position: &crate::traits::CursorValue,
) -> (crate::types::QueryParam, fn(&str) -> String) {
    use crate::{traits::CursorValue, types::QueryParam};
    match position {
        CursorValue::Int64(pk) => (QueryParam::BigInt(*pk), str::to_string),
        CursorValue::Uuid(uuid) => (QueryParam::Text(uuid.clone()), |p| format!("{p}::text::uuid")),
    }
}

#[cfg(test)]
mod tests;
