//! MySQL WHERE clause SQL generation.
//!
//! `MySqlWhereGenerator` is a type alias for
//! `GenericWhereGenerator<MySqlDialect>`.  All logic lives in
//! [`crate::where_generator::GenericWhereGenerator`].

use crate::{
    dialect::MySqlDialect,
    where_generator::GenericWhereGenerator,
};

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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;

    use crate::{
        dialect::MySqlDialect,
        where_clause::{WhereClause, WhereOperator},
    };

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = MySqlWhereGenerator::new(MySqlDialect);
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("JSON_UNQUOTE"), "Expected JSON_UNQUOTE: {sql}");
        assert!(sql.contains("= ?"), "Expected = ?: {sql}");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = MySqlWhereGenerator::new(MySqlDialect);
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("alice"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("CONCAT"), "Expected CONCAT: {sql}");
        assert_eq!(params, vec![json!("alice")]);
    }

    #[test]
    fn test_placeholders_are_question_marks() {
        let gen = MySqlWhereGenerator::new(MySqlDialect);
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
        assert!(!sql.contains("$1"), "MySQL must not use $N placeholders: {sql}");
        assert_eq!(params.len(), 2);
    }
}
