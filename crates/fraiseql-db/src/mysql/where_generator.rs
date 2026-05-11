//! MySQL WHERE clause SQL generation.
//!
//! `MySqlWhereGenerator` is a type alias for
//! `GenericWhereGenerator<MySqlDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{dialect::MySqlDialect, where_generator::GenericWhereGenerator};

/// MySQL WHERE clause generator.
///
/// Type alias for `GenericWhereGenerator<MySqlDialect>`.
/// Refer to [`GenericWhereGenerator`] for full documentation.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::mysql::MySqlWhereGenerator;
/// use fraiseql_db::{WhereClause, WhereOperator, MySqlDialect};
/// use serde_json::json;
///
/// let generator = MySqlWhereGenerator::new(MySqlDialect);
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "JSON_UNQUOTE(JSON_EXTRACT(data, '$.email')) LIKE CONCAT('%', ?, '%')"
/// ```
pub type MySqlWhereGenerator = GenericWhereGenerator<MySqlDialect>;

/// Constructor compatibility shim for `MySqlWhereGenerator`.
impl MySqlWhereGenerator {
    /// Create a new MySQL WHERE generator.
    #[must_use]
    pub const fn mysql_new() -> Self {
        Self::new(MySqlDialect)
    }
}

#[cfg(test)]
mod tests;
