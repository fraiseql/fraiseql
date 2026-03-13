//! SQLite WHERE clause SQL generation.
//!
//! `SqliteWhereGenerator` is a type alias for
//! `GenericWhereGenerator<SqliteDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{
    dialect::SqliteDialect,
    where_generator::GenericWhereGenerator,
};

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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;

    use crate::{
        dialect::SqliteDialect,
        where_clause::{WhereClause, WhereOperator},
    };

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = SqliteWhereGenerator::new(SqliteDialect);
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("json_extract"), "Expected json_extract: {sql}");
        assert!(sql.contains("= ?"), "Expected = ?: {sql}");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_placeholders_are_question_marks() {
        let gen = SqliteWhereGenerator::new(SqliteDialect);
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["a".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("x"),
            },
            WhereClause::Field {
                path:     vec!["b".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("y"),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(!sql.contains("$1"), "SQLite must not use $N placeholders: {sql}");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_always_false_sentinel() {
        let gen = SqliteWhereGenerator::new(SqliteDialect);
        let clause = WhereClause::Or(vec![]);
        let (sql, _) = gen.generate(&clause).unwrap();
        // SQLite dialect uses "1=0" for always-false
        assert_eq!(sql, "1=0");
    }
}
