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
    let Some(clauses) = order_by.filter(|c| !c.is_empty()) else {
        return Ok(false);
    };
    sql.push_str(" ORDER BY ");
    for (i, clause) in clauses.iter().enumerate() {
        OrderByClause::validate_field_name(&clause.field)?;
        if i > 0 {
            sql.push_str(", ");
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
        write!(sql, "{expr} {}", clause.direction.as_sql()).expect("write to String is infallible");
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
