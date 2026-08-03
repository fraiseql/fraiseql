//! Shared ORDER BY clause builder for all database adapters.
//!
//! Generates dialect-specific `ORDER BY` SQL from [`OrderByClause`] slices,
//! validating field names and converting camelCase GraphQL names to snake_case
//! JSONB storage keys.

use std::fmt::Write;

use crate::types::{DatabaseType, sql_hints::OrderByClause};

/// Append an `ORDER BY` clause to the SQL buffer.
///
/// Each field name is validated via `OrderByClause::validate_field_name` (the SQL
/// injection boundary) and converted to its snake_case storage key before being
/// interpolated into a dialect-specific JSON field expression.
///
/// Returns `true` if an ORDER BY clause was appended, `false` if `order_by` was
/// `None` or empty.
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
/// let appended = append_order_by(&mut sql, Some(&clauses), DatabaseType::PostgreSQL).unwrap();
/// assert!(appended);
/// assert!(sql.contains("ORDER BY data->>'created_at' DESC"));
/// ```
pub fn append_order_by(
    sql: &mut String,
    order_by: Option<&[OrderByClause]>,
    db_type: DatabaseType,
) -> crate::Result<bool> {
    match render_order_by_columns(order_by, db_type)? {
        Some(columns) => {
            sql.push_str(" ORDER BY ");
            sql.push_str(&columns);
            Ok(true)
        },
        None => Ok(false),
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
/// let columns = render_order_by_columns(Some(&clauses), DatabaseType::PostgreSQL)
///     .unwrap()
///     .unwrap();
/// assert_eq!(columns, "data->>'created_at' DESC");
/// ```
pub fn render_order_by_columns(
    order_by: Option<&[OrderByClause]>,
    db_type: DatabaseType,
) -> crate::Result<Option<String>> {
    let Some(clauses) = order_by.filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let mut columns = String::new();
    for (i, clause) in clauses.iter().enumerate() {
        OrderByClause::validate_field_name(&clause.field)?;
        if i > 0 {
            columns.push_str(", ");
        }
        // Vector-distance ordering (#386): `{col} {op} '{vec}'::vector` — the
        // pgvector ANN shape. Only valid against a native column (a JSONB
        // extraction would re-parse text per row and defeat every index).
        if let Some(ref vector) = clause.vector {
            let Some(ref col) = clause.native_column else {
                return Err(fraiseql_error::FraiseQLError::validation(
                    "vector-distance ordering requires the view to expose the vector as a \
                     native column",
                ));
            };
            if db_type != DatabaseType::PostgreSQL {
                return Err(fraiseql_error::FraiseQLError::validation(
                    "vector-distance ordering is PostgreSQL-only (pgvector)",
                ));
            }
            validate_vector_operator(&vector.operator)?;
            validate_vector_literal(&vector.query_vector)?;
            // Reason: fmt::Write for String is infallible
            write!(
                columns,
                "{col} {} '{}'::vector {}",
                vector.operator,
                vector.query_vector,
                clause.direction.as_sql()
            )
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
    Ok(Some(columns))
}

/// The three pgvector distance operators reachable through `nearest` (#386).
fn validate_vector_operator(op: &str) -> crate::Result<()> {
    match op {
        "<=>" | "<->" | "<#>" => Ok(()),
        other => Err(fraiseql_error::FraiseQLError::validation(format!(
            "unsupported vector distance operator '{other}'"
        ))),
    }
}

/// Defence in depth for the interpolated vector literal: the builder constructs
/// it exclusively from formatted `f64` values, so anything outside a numeric
/// literal's character set is a bug — refuse rather than emit.
fn validate_vector_literal(literal: &str) -> crate::Result<()> {
    let ok = literal.starts_with('[')
        && literal.ends_with(']')
        && literal.chars().all(|c| {
            c.is_ascii_digit() || matches!(c, '[' | ']' | ',' | '.' | '-' | '+' | 'e' | 'E')
        });
    if ok {
        Ok(())
    } else {
        Err(fraiseql_error::FraiseQLError::validation(
            "malformed vector literal in ORDER BY (expected a pgvector text literal built \
             from numeric values)",
        ))
    }
}

#[cfg(test)]
mod tests;
