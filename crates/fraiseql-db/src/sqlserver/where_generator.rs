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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        dialect::SqlServerDialect,
        where_clause::{WhereClause, WhereOperator},
    };

    #[test]
    fn test_simple_equality() {
        let gen = SqlServerWhereGenerator::new(SqlServerDialect);
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("JSON_VALUE"), "Expected JSON_VALUE: {sql}");
        assert!(sql.contains("= @p1"), "Expected @p1: {sql}");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_placeholders_are_named() {
        let gen = SqlServerWhereGenerator::new(SqlServerDialect);
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
        assert!(sql.contains("@p1"), "SQL Server must use @pN placeholders: {sql}");
        assert!(sql.contains("@p2"), "SQL Server must use @pN placeholders: {sql}");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_always_false_sentinel() {
        let gen = SqlServerWhereGenerator::new(SqlServerDialect);
        let clause = WhereClause::Or(vec![]);
        let (sql, _) = gen.generate(&clause).unwrap();
        // SQL Server dialect uses "1=0" for always-false
        assert_eq!(sql, "1=0");
    }
}
