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

#[cfg(test)]
mod tests;
