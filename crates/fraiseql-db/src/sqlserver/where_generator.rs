//! SQL Server WHERE clause SQL generation.
//!
//! `SqlServerWhereGenerator` is a type alias for
//! `GenericWhereGenerator<SqlServerDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{dialect::SqlServerDialect, where_generator::GenericWhereGenerator};

/// SQL Server WHERE clause generator.
///
/// Type alias for `GenericWhereGenerator<SqlServerDialect>`.
/// Refer to [`GenericWhereGenerator`] for full documentation.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::sqlserver::SqlServerWhereGenerator;
/// use fraiseql_db::{WhereClause, WhereOperator, SqlServerDialect};
/// use serde_json::json;
///
/// let generator = SqlServerWhereGenerator::new(SqlServerDialect);
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "JSON_VALUE(data, '$.email') LIKE '%' + @p1 + '%'"
/// ```
pub type SqlServerWhereGenerator = GenericWhereGenerator<SqlServerDialect>;

/// Constructor compatibility shim for `SqlServerWhereGenerator`.
impl SqlServerWhereGenerator {
    /// Create a new SQL Server WHERE generator.
    #[must_use]
    pub const fn sqlserver_new() -> Self {
        Self::new(SqlServerDialect)
    }
}

#[cfg(test)]
mod tests;
