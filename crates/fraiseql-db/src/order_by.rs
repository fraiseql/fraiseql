//! Shared ORDER BY clause builder for all database adapters.
//!
//! Generates dialect-specific `ORDER BY` SQL from [`OrderByClause`] slices,
//! validating field names and converting camelCase GraphQL names to snake_case
//! JSONB storage keys.

use std::fmt::Write;

use crate::{
    projection_generator::ComputedExpr,
    types::{
        DatabaseType,
        sql_hints::{OrderByClause, RelevanceOrder, VectorOperandKind},
    },
};

/// A rendered `ORDER BY`: its column expressions and the parameters they bind.
///
/// Ordering used to be pure SQL text, which is why full-text relevance had
/// nowhere to put its search term and shipped as an unparseable magic key
/// instead (#1284). An expression that needs a *value* now has a channel for
/// one, and that value is bound rather than escaped into the string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedOrderBy {
    /// The column expressions, comma-separated, without the `ORDER BY` keyword.
    pub columns: String,
    /// Text values the expressions bound, in placeholder order, starting at the
    /// `next_param` index the caller supplied. Empty for every ordering that is
    /// a plain field or a vector distance.
    ///
    /// Typed as text rather than as `QueryParam` because this module is not
    /// gated on the `postgres` feature and `QueryParam` is; the only ordering
    /// that binds anything is full-text relevance, whose operand is a search
    /// string. A caller pushes these as text parameters.
    pub params:  Vec<String>,
}

/// Append an `ORDER BY` clause to the SQL buffer.
///
/// Each field name is validated via `OrderByClause::validate_field_name` (the SQL
/// injection boundary) and converted to its snake_case storage key before being
/// interpolated into a dialect-specific JSON field expression.
///
/// `next_param` is the 1-based index of the next free placeholder in `sql`;
/// returns the parameters the ordering bound, which the caller must push onto
/// its parameter list **before** appending `LIMIT`/`OFFSET`. Empty for every
/// ordering that is a plain field or a vector distance, so a caller with no
/// relevance ordering sees no change in behaviour.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if any field name fails validation.
///
/// # Examples
///
/// ```
/// use fraiseql_db::order_by::append_order_by;
/// use fraiseql_db::{DatabaseType, OrderByClause, OrderDirection};
///
/// let mut sql = "SELECT data FROM v_user WHERE true".to_string();
/// let clauses = [
///     OrderByClause::new("createdAt".into(), OrderDirection::Desc),
/// ];
/// let bound = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL, 1).unwrap();
/// assert!(bound.is_empty());
/// assert!(sql.contains("ORDER BY data->>'created_at' DESC"));
/// ```
pub fn append_order_by(
    sql: &mut String,
    order_by: Option<&[OrderByClause]>,
    db_type: DatabaseType,
    next_param: usize,
) -> crate::Result<Vec<String>> {
    match render_order_by_columns(order_by, db_type, next_param)? {
        Some(rendered) => {
            sql.push_str(" ORDER BY ");
            sql.push_str(&rendered.columns);
            Ok(rendered.params)
        },
        None => Ok(Vec::new()),
    }
}

/// Render the `ORDER BY` column expressions **without** a leading `ORDER BY` keyword.
///
/// For backends whose query builder supplies the `ORDER BY` keyword itself (e.g. the
/// fraiseql-wire [`QueryBuilder`](https://docs.rs/fraiseql-wire) which emits
/// `… ORDER BY {expr}`). Returns `None` when `order_by` is `None` or empty — the caller
/// then emits no ordering. Each field name is validated via
/// [`OrderByClause::validate_field_name`] (the SQL injection boundary) and converted to
/// its snake_case storage key, identical to [`append_order_by`].
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if any field name fails validation.
///
/// # Examples
///
/// ```
/// use fraiseql_db::order_by::render_order_by_columns;
/// use fraiseql_db::{DatabaseType, OrderByClause, OrderDirection};
///
/// let clauses = [OrderByClause::new("createdAt".into(), OrderDirection::Desc)];
/// let rendered = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL, 1)
///     .unwrap()
///     .unwrap();
/// assert_eq!(rendered.columns, "data->>'created_at' DESC");
/// assert!(rendered.params.is_empty());
/// ```
pub fn render_order_by_columns(
    order_by: Option<&[OrderByClause]>,
    db_type: DatabaseType,
    next_param: usize,
) -> crate::Result<Option<RenderedOrderBy>> {
    let Some(clauses) = order_by.filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let mut columns = String::new();
    let mut params: Vec<String> = Vec::new();
    for (i, clause) in clauses.iter().enumerate() {
        if i > 0 {
            columns.push_str(", ");
        }
        // Full-text relevance (#1284) is not an ordering *by a field*, so it is
        // resolved before `field` is validated: the constructor leaves that
        // string empty precisely because nothing may read it.
        if let Some(ref relevance) = clause.relevance {
            let placeholder = db_type.dialect().placeholder(next_param + params.len());
            let rank = relevance_rank_expr(relevance, db_type, &placeholder)?;
            params.push(relevance.query.clone());
            // Reason: fmt::Write for String is infallible
            write!(columns, "{rank} {}", clause.direction.as_sql())
                .expect("write to String is infallible");
            continue;
        }
        OrderByClause::validate_field_name(&clause.field)?;
        // Vector-distance ordering (#386): `{col} {op} '{vec}'::vector` — the
        // pgvector ANN shape. Only valid against a native column (a JSONB
        // extraction would re-parse text per row and defeat every index).
        if let Some(distance) = vector_distance_expr(clause, db_type)? {
            // Reason: fmt::Write for String is infallible
            write!(columns, "{} {}", distance.as_sql(), clause.direction.as_sql())
                .expect("write to String is infallible");
            continue;
        }
        // When a native typed column is available, use it directly — this
        // enables index support and avoids JSON extraction + cast overhead.
        let expr = if let Some(ref col) = clause.native_column {
            col.clone()
        } else {
            let key = clause.storage_key();
            db_type.typed_json_field_expr(&key, clause.field_type)
        };
        // Reason: fmt::Write for String is infallible
        write!(columns, "{expr} {}", clause.direction.as_sql())
            .expect("write to String is infallible");
    }
    Ok(Some(RenderedOrderBy { columns, params }))
}

/// Refuse a relevance ordering on a cursor-paginated read (#1284).
///
/// A keyset cursor resumes from the last row's *sort key*. `ts_rank` is a score
/// computed per query — not stored, not unique, and not derivable from a row
/// alone — so there is nothing for the next page to resume from, and every page
/// after the first would re-read or skip rows.
///
/// It is refused rather than rendered, because the relay builder assembles its
/// `ORDER BY` into a `format!` string with hand-managed parameter indices: a
/// relevance clause rendered there would emit a placeholder that collides with
/// the cursor's, which is a wrong result set under a `200` rather than an error.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when any clause binds a parameter.
pub fn refuse_relevance_under_cursor_pagination(
    order_by: Option<&[OrderByClause]>,
) -> crate::Result<()> {
    if order_by.is_some_and(|cs| cs.iter().any(OrderByClause::binds_parameter)) {
        return Err(fraiseql_error::FraiseQLError::validation(
            "full-text relevance ordering cannot be combined with cursor pagination: a relevance \
             rank is computed per query and is not a resumable sort key. Use offset pagination, \
             or name an explicit sort.",
        ));
    }
    Ok(())
}

/// The `ts_rank(...)` expression a relevance-ordered clause sorts by (#1284).
///
/// The document is the searchable fields concatenated, each wrapped in
/// `coalesce(…, '')`. The coalesce is load-bearing rather than defensive:
/// `'a' || NULL` is `NULL` in SQL, so a single absent JSON key would null the
/// whole document and flatten every row's rank to `NULL` — the ordering would
/// silently disappear under a 200, which is the failure mode this issue is
/// about. The matching predicate needs no coalesce, because a `NULL @@ query`
/// is simply not a match.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the clause names no field, when a
/// field name is not a plain identifier (the same SQL-injection boundary
/// `validate_field_name` is), or when the dialect has no full-text ranking.
fn relevance_rank_expr(
    relevance: &RelevanceOrder,
    db_type: DatabaseType,
    placeholder: &str,
) -> crate::Result<String> {
    if relevance.fields.is_empty() {
        return Err(fraiseql_error::FraiseQLError::validation(
            "relevance ordering needs at least one searchable field to rank over",
        ));
    }
    let mut document = String::new();
    for (i, key) in relevance.fields.iter().enumerate() {
        // The key is interpolated into `data->>'key'`, so it passes the same
        // boundary an ORDER BY field does.
        OrderByClause::validate_field_name(key)?;
        if i > 0 {
            document.push_str(" || ' ' || ");
        }
        // Reason: fmt::Write for String is infallible
        write!(document, "coalesce({}, '')", db_type.json_field_expr(key))
            .expect("write to String is infallible");
    }
    db_type
        .dialect()
        .fts_rank_sql(&document, placeholder)
        .map_err(|e| fraiseql_error::FraiseQLError::validation(e.to_string()))
}

/// The distance expression a vector-ordered clause sorts by, e.g.
/// `"embedding" <=> '[1,0,0]'::vector` (#386, #959).
///
/// `Ok(None)` when the clause carries no vector operand — an ordinary ORDER BY.
///
/// This is the **one** construction site for a pgvector distance expression, so
/// the number a query reports and the order it comes back in cannot be computed
/// two different ways: projecting the distance (#959) renders exactly the
/// expression the ORDER BY sorts by, because it is the same string.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the view exposes no native column
/// for the vector, when the dialect is not PostgreSQL, or when the operator or
/// the literal does not belong to the clause's operand kind.
pub fn vector_distance_expr(
    clause: &OrderByClause,
    db_type: DatabaseType,
) -> crate::Result<Option<ComputedExpr>> {
    let Some(ref vector) = clause.vector else {
        return Ok(None);
    };
    let Some(ref col) = clause.native_column else {
        return Err(fraiseql_error::FraiseQLError::validation(
            "vector-distance ordering requires the view to expose the vector as a native column",
        ));
    };
    if db_type != DatabaseType::PostgreSQL {
        return Err(fraiseql_error::FraiseQLError::validation(
            "vector-distance ordering is PostgreSQL-only (pgvector)",
        ));
    }
    validate_vector_operator(&vector.operator, vector.kind)?;
    validate_vector_literal(&vector.query_vector, vector.kind)?;
    Ok(Some(ComputedExpr::from_validated_parts(format!(
        "{col} {} '{}'::{}",
        vector.operator,
        vector.query_vector,
        vector.kind.cast()
    ))))
}

/// The pgvector distance operators reachable through `nearest`, per operand
/// kind (#386, #959).
///
/// The pairing is checked, not just membership: `<~>` against a `vector` cast
/// (or `<=>` against a `varbit` one) is an operator PostgreSQL has no candidate
/// for, and emitting it turns a metric mix-up into a runtime SQL error instead
/// of a named refusal.
fn validate_vector_operator(op: &str, kind: VectorOperandKind) -> crate::Result<()> {
    let ok = match kind {
        VectorOperandKind::Bit => matches!(op, "<~>" | "<%>"),
        _ => matches!(op, "<=>" | "<->" | "<#>"),
    };
    if ok {
        Ok(())
    } else {
        Err(fraiseql_error::FraiseQLError::validation(format!(
            "unsupported vector distance operator '{op}' for a {kind:?} operand"
        )))
    }
}

/// Defence in depth for the interpolated vector literal: the builder constructs
/// it exclusively from formatted `f64` values (or from `0`/`1` characters for a
/// bit vector), so anything outside that character set is a bug — refuse rather
/// than emit.
fn validate_vector_literal(literal: &str, kind: VectorOperandKind) -> crate::Result<()> {
    let ok = match kind {
        VectorOperandKind::Bit => {
            !literal.is_empty() && literal.chars().all(|c| matches!(c, '0' | '1'))
        },
        // `{1:0.5,7:0.25}/1000` — index:value pairs and the dimension count.
        VectorOperandKind::Sparse => {
            literal.starts_with('{')
                && literal.contains("}/")
                && literal.chars().all(|c| {
                    c.is_ascii_digit()
                        || matches!(c, '{' | '}' | '/' | ':' | ',' | '.' | '-' | '+' | 'e' | 'E')
                })
        },
        _ => {
            literal.starts_with('[')
                && literal.ends_with(']')
                && literal.chars().all(|c| {
                    c.is_ascii_digit() || matches!(c, '[' | ']' | ',' | '.' | '-' | '+' | 'e' | 'E')
                })
        },
    };
    if ok {
        Ok(())
    } else {
        Err(fraiseql_error::FraiseQLError::validation(
            "malformed vector literal in ORDER BY (expected a pgvector text literal built \
             from numeric values, a non-empty run of 0/1 bits, or a sparse \
             '{index:value,…}/dimensions' literal)",
        ))
    }
}

#[cfg(test)]
mod tests;
