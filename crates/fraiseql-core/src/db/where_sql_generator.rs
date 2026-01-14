//! WHERE clause to SQL string generator for fraiseql-wire.
//!
//! Converts FraiseQL's WHERE clause AST to SQL predicates that can be used
//! with fraiseql-wire's `where_sql()` method.

use crate::db::{WhereClause, WhereOperator};
use crate::error::{FraiseQLError, Result};
use serde_json::Value;

/// Generates SQL WHERE clause strings from AST.
pub struct WhereSqlGenerator;

impl WhereSqlGenerator {
    /// Convert WHERE clause AST to SQL string.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::db::{WhereClause, WhereOperator, where_sql_generator::WhereSqlGenerator};
    /// use serde_json::json;
    ///
    /// let clause = WhereClause::Field {
    ///     path: vec!["status".to_string()],
    ///     operator: WhereOperator::Eq,
    ///     value: json!("active"),
    /// };
    ///
    /// let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
    /// assert_eq!(sql, "data->>'status' = 'active'");
    /// ```
    pub fn to_sql(clause: &WhereClause) -> Result<String> {
        match clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => Self::generate_field_predicate(path, operator, value),
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok("TRUE".to_string());
                }
                let parts: Result<Vec<_>> = clauses.iter().map(Self::to_sql).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            }
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
                }
                let parts: Result<Vec<_>> = clauses.iter().map(Self::to_sql).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            }
            WhereClause::Not(clause) => {
                let inner = Self::to_sql(clause)?;
                Ok(format!("NOT ({})", inner))
            }
        }
    }

    fn generate_field_predicate(
        path: &[String],
        operator: &WhereOperator,
        value: &Value,
    ) -> Result<String> {
        let json_path = Self::build_json_path(path);
        let sql = match operator {
            // Null checks
            WhereOperator::IsNull => {
                let is_null = value.as_bool().unwrap_or(true);
                if is_null {
                    format!("{json_path} IS NULL")
                } else {
                    format!("{json_path} IS NOT NULL")
                }
            }
            // All other operators
            _ => {
                let sql_op = Self::operator_to_sql(operator)?;
                let sql_value = Self::value_to_sql(value, operator)?;
                format!("{json_path} {sql_op} {sql_value}")
            }
        };
        Ok(sql)
    }

    fn build_json_path(path: &[String]) -> String {
        if path.is_empty() {
            return "data".to_string();
        }

        if path.len() == 1 {
            // Simple path: data->>'field'
            format!("data->>'{}'", path[0])
        } else {
            // Nested path: data#>'{a,b,c}'->>'d'
            let nested = &path[..path.len() - 1];
            let last = &path[path.len() - 1];
            let nested_path = nested.join(",");
            format!("data#>'{{{}}}'->>'{}'", nested_path, last)
        }
    }

    fn operator_to_sql(operator: &WhereOperator) -> Result<&'static str> {
        Ok(match operator {
            // Comparison
            WhereOperator::Eq => "=",
            WhereOperator::Neq => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",

            // Containment
            WhereOperator::In => "= ANY",
            WhereOperator::Nin => "!= ALL",

            // String operations
            WhereOperator::Contains => "LIKE",
            WhereOperator::Icontains => "ILIKE",
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Istartswith => "ILIKE",
            WhereOperator::Endswith => "LIKE",
            WhereOperator::Iendswith => "ILIKE",
            WhereOperator::Like => "LIKE",
            WhereOperator::Ilike => "ILIKE",

            // Array operations
            WhereOperator::ArrayContains => "@>",
            WhereOperator::ArrayContainedBy => "<@",
            WhereOperator::ArrayOverlaps => "&&",

            // These operators require special handling
            WhereOperator::IsNull => {
                return Err(FraiseQLError::Internal {
                    message: "IsNull should be handled separately".to_string(),
                    source: None,
                })
            }
            WhereOperator::LenEq
            | WhereOperator::LenGt
            | WhereOperator::LenLt
            | WhereOperator::LenGte
            | WhereOperator::LenLte
            | WhereOperator::LenNeq => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Array length operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source: None,
                })
            }

            // Vector operations not supported
            WhereOperator::L2Distance
            | WhereOperator::CosineDistance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Vector operations not supported in fraiseql-wire: {operator:?}"
                    ),
                    source: None,
                })
            }

            // Full-text search operators not supported yet
            WhereOperator::Matches
            | WhereOperator::PlainQuery
            | WhereOperator::PhraseQuery
            | WhereOperator::WebsearchQuery => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Full-text search operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source: None,
                })
            }

            // Network operators not supported yet
            WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsPublic
            | WhereOperator::InSubnet
            | WhereOperator::Overlaps
            | WhereOperator::StrictlyContains
            | WhereOperator::AncestorOf
            | WhereOperator::DescendantOf
            | WhereOperator::MatchesLquery => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Advanced operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source: None,
                })
            }
        })
    }

    fn value_to_sql(value: &Value, operator: &WhereOperator) -> Result<String> {
        match (value, operator) {
            (Value::Null, _) => Ok("NULL".to_string()),
            (Value::Bool(b), _) => Ok(b.to_string()),
            (Value::Number(n), _) => Ok(n.to_string()),

            // String operators with wildcards
            (Value::String(s), WhereOperator::Contains | WhereOperator::Icontains) => {
                Ok(format!("'%{}%'", Self::escape_sql_string(s)))
            }
            (Value::String(s), WhereOperator::Startswith | WhereOperator::Istartswith) => {
                Ok(format!("'{}%'", Self::escape_sql_string(s)))
            }
            (Value::String(s), WhereOperator::Endswith | WhereOperator::Iendswith) => {
                Ok(format!("'%{}'", Self::escape_sql_string(s)))
            }

            // Regular strings
            (Value::String(s), _) => Ok(format!("'{}'", Self::escape_sql_string(s))),

            // Arrays (for IN operator)
            (Value::Array(arr), WhereOperator::In | WhereOperator::Nin) => {
                let values: Result<Vec<_>> = arr
                    .iter()
                    .map(|v| Self::value_to_sql(v, &WhereOperator::Eq))
                    .collect();
                Ok(format!("ARRAY[{}]", values?.join(", ")))
            }

            // Array operations
            (Value::Array(_), WhereOperator::ArrayContains)
            | (Value::Array(_), WhereOperator::ArrayContainedBy)
            | (Value::Array(_), WhereOperator::ArrayOverlaps) => {
                // For array operators, use JSONB representation
                Ok(format!("'{}'::jsonb", value))
            }

            _ => Err(FraiseQLError::Internal {
                message: format!("Unsupported value type for operator: {value:?} with {operator:?}"),
                source: None,
            }),
        }
    }

    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_equality() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'status' = 'active'");
    }

    #[test]
    fn test_nested_path() {
        let clause = WhereClause::Field {
            path: vec!["user".to_string(), "email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("test@example.com"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data#>'{user}'->>'email' = 'test@example.com'");
    }

    #[test]
    fn test_icontains() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Icontains,
            value: json!("john"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' ILIKE '%john%'");
    }

    #[test]
    fn test_startswith() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Startswith,
            value: json!("admin"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'email' LIKE 'admin%'");
    }

    #[test]
    fn test_and_clause() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value: json!("active"),
            },
            WhereClause::Field {
                path: vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value: json!(18),
            },
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "(data->>'status' = 'active' AND data->>'age' >= 18)");
    }

    #[test]
    fn test_or_clause() {
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path: vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value: json!("admin"),
            },
            WhereClause::Field {
                path: vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value: json!("moderator"),
            },
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(
            sql,
            "(data->>'type' = 'admin' OR data->>'type' = 'moderator')"
        );
    }

    #[test]
    fn test_not_clause() {
        let clause = WhereClause::Not(Box::new(WhereClause::Field {
            path: vec!["deleted".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        }));

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "NOT (data->>'deleted' = true)");
    }

    #[test]
    fn test_is_null() {
        let clause = WhereClause::Field {
            path: vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(true),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'deleted_at' IS NULL");
    }

    #[test]
    fn test_is_not_null() {
        let clause = WhereClause::Field {
            path: vec!["updated_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(false),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'updated_at' IS NOT NULL");
    }

    #[test]
    fn test_in_operator() {
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::In,
            value: json!(["active", "pending", "approved"]),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(
            sql,
            "data->>'status' = ANY ARRAY['active', 'pending', 'approved']"
        );
    }

    #[test]
    fn test_sql_injection_prevention() {
        let clause = WhereClause::Field {
            path: vec!["name".to_string()],
            operator: WhereOperator::Eq,
            value: json!("'; DROP TABLE users; --"),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'name' = '''; DROP TABLE users; --'");
        // Single quotes are escaped to ''
    }

    #[test]
    fn test_numeric_comparison() {
        let clause = WhereClause::Field {
            path: vec!["price".to_string()],
            operator: WhereOperator::Gt,
            value: json!(99.99),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'price' > 99.99");
    }

    #[test]
    fn test_boolean_value() {
        let clause = WhereClause::Field {
            path: vec!["published".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        };

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "data->>'published' = true");
    }

    #[test]
    fn test_empty_and_clause() {
        let clause = WhereClause::And(vec![]);
        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "TRUE");
    }

    #[test]
    fn test_empty_or_clause() {
        let clause = WhereClause::Or(vec![]);
        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(sql, "FALSE");
    }

    #[test]
    fn test_complex_nested_condition() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["type".to_string()],
                operator: WhereOperator::Eq,
                value: json!("article"),
            },
            WhereClause::Or(vec![
                WhereClause::Field {
                    path: vec!["status".to_string()],
                    operator: WhereOperator::Eq,
                    value: json!("published"),
                },
                WhereClause::And(vec![
                    WhereClause::Field {
                        path: vec!["status".to_string()],
                        operator: WhereOperator::Eq,
                        value: json!("draft"),
                    },
                    WhereClause::Field {
                        path: vec!["author".to_string(), "role".to_string()],
                        operator: WhereOperator::Eq,
                        value: json!("admin"),
                    },
                ]),
            ]),
        ]);

        let sql = WhereSqlGenerator::to_sql(&clause).unwrap();
        assert_eq!(
            sql,
            "(data->>'type' = 'article' AND (data->>'status' = 'published' OR (data->>'status' = 'draft' AND data#>'{author}'->>'role' = 'admin')))"
        );
    }
}
