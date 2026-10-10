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
    /// Whether the key is a native column PostgreSQL proves `NOT NULL` (#1533): the only
    /// kind a row comparison may read, since a NULL in one drops the row.
    seekable:      bool,
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
        match self.value_type() {
            Some(ty) => format!("({placeholder}::text)::{ty}"),
            None => format!("{placeholder}::text"),
        }
    }

    /// The PostgreSQL type a value bound for this key is cast to from text, `None` when it
    /// is compared as text. The one answer [`param`](Self::param) casts to and
    /// [`cursor_values_probe`] checks a cursor's values against.
    fn value_type(&self) -> Option<String> {
        match &self.cast {
            Cast::Text => None,
            Cast::Scalar(ty) => PostgresDialect.cast_type_name(*ty).map(str::to_string),
            Cast::Native(native) => match native.to_lowercase().as_str() {
                "boolean" | "bool" => Some("boolean".to_string()),
                "text" | "varchar" | "character varying" | "char" | "bpchar" | "name" => None,
                _ => Some(native.clone()),
            },
            Cast::Float => Some("float8".to_string()),
        }
    }
}

/// A statement asking PostgreSQL whether a cursor's values are values of their types, for a
/// page that failed on a data exception while resuming from that cursor (#1521).
///
/// A cursor is client data: a forged one carries a sort-key value that is not of its key's
/// type, or a position that is not a UUID, and PostgreSQL refuses the cast with a data
/// exception. This asks the same parser (`pg_input_is_valid`) about each typed value, so the
/// failure can be told apart from one the data itself raised. Each value and its type are
/// bound parameters; the statement returns one boolean per check, in order.
///
/// `uuid_position` is the cursor's position on a UUID connection. `None` when nothing is cast.
#[must_use]
pub fn cursor_values_probe(
    keys: &[KeysetKey],
    values: &[Option<String>],
    uuid_position: Option<&str>,
) -> Option<(String, Vec<String>)> {
    let typed = keys
        .iter()
        .zip(values)
        .filter_map(|(key, value)| Some((value.clone()?, key.value_type()?)))
        .chain(uuid_position.map(|p| (p.to_string(), "uuid".to_string())));
    let mut checks = Vec::new();
    let mut params = Vec::new();
    for (value, ty) in typed {
        params.push(value);
        params.push(ty);
        checks.push(format!(
            "pg_input_is_valid(${}::text, ${}::text)",
            params.len() - 1,
            params.len()
        ));
    }
    if checks.is_empty() {
        return None;
    }
    Some((format!("SELECT ARRAY[{}] AS valid", checks.join(", ")), params))
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
            let seekable = matches!(cast, Cast::Native(_)) && clause.native_not_null;
            Ok(KeysetKey {
                expr,
                direction: clause.direction,
                cast,
                seekable,
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
    if let Some(seek) =
        row_comparison(keys, values, position, &position_param, forward, first_param)
    {
        return Ok(seek);
    }
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

/// The keyset predicate as one row comparison, `(k1, …, kn, position) > ($1, …, $n, $p)`
/// (`<` for a backward page), which PostgreSQL seeks an index with (#1533); `None` when the
/// expanded form is required.
///
/// It is the expanded form's meaning only when no key can be NULL (a NULL in a row comparison
/// is NULL, so the row would be dropped from every page after the cursor) and every key reads
/// in the position's direction, which is ascending: so every key is a native column proven
/// `NOT NULL`, every key is `ASC`, and the cursor carries a value for each.
fn row_comparison(
    keys: &[KeysetKey],
    values: &[Option<String>],
    position: &str,
    position_param: &impl Fn(&str) -> String,
    forward: bool,
    first_param: usize,
) -> Option<(String, Vec<String>, usize)> {
    let seekable = !keys.is_empty()
        && keys
            .iter()
            .all(|key| key.seekable && matches!(key.direction, OrderDirection::Asc));
    if !seekable {
        return None;
    }
    let values: Vec<&String> = values.iter().map(Option::as_ref).collect::<Option<_>>()?;
    let dialect = PostgresDialect;
    let mut left: Vec<&str> = keys.iter().map(|key| key.expr.as_str()).collect();
    left.push(position);
    let mut right: Vec<String> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| key.param(&dialect.placeholder(first_param + i)))
        .collect();
    let position_index = first_param + keys.len();
    right.push(position_param(&dialect.placeholder(position_index)));
    let comparison = if forward { ">" } else { "<" };
    let params = values.into_iter().cloned().collect();
    Some((
        format!("(({}) {comparison} ({}))", left.join(", "), right.join(", ")),
        params,
        position_index,
    ))
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
