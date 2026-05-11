//! SQLite WHERE clause SQL generation.
//!
//! `SqliteWhereGenerator` is a type alias for
//! `GenericWhereGenerator<SqliteDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{dialect::SqliteDialect, where_generator::GenericWhereGenerator};

/// SQLite WHERE clause generator.
///
/// Type alias for `GenericWhereGenerator<SqliteDialect>`.
/// Refer to [`GenericWhereGenerator`] for full documentation.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::sqlite::SqliteWhereGenerator;
/// use fraiseql_db::{WhereClause, WhereOperator, SqliteDialect};
/// use serde_json::json;
///
/// let generator = SqliteWhereGenerator::new(SqliteDialect);
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "LOWER(json_extract(data, '$.email')) LIKE LOWER('%' || ? || '%')"
/// ```
pub type SqliteWhereGenerator = GenericWhereGenerator<SqliteDialect>;

/// Constructor compatibility shim for `SqliteWhereGenerator`.
impl SqliteWhereGenerator {
    /// Create a new SQLite WHERE generator.
    #[must_use]
    pub const fn sqlite_new() -> Self {
        Self::new(SqliteDialect)
    }
}

#[cfg(test)]
mod tests;
