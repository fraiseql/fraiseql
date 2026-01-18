//! SQLite WHERE clause SQL generation.

use crate::{
    db::where_clause::{WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
};

/// SQLite WHERE clause generator.
///
/// Converts `WhereClause` AST to SQLite SQL with parameterized queries.
/// SQLite uses `?` for placeholders and has limited JSON support compared to PostgreSQL/MySQL.
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::db::sqlite::SqliteWhereGenerator;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = SqliteWhereGenerator::new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "json_extract(data, '$.email') LIKE '%' || ? || '%'"
/// // params: ["example.com"]
/// ```
pub struct SqliteWhereGenerator;

impl SqliteWhereGenerator {
    /// Create new SQLite WHERE generator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Generate SQL WHERE clause and parameters.
    ///
    /// # Arguments
    ///
    /// * `clause` - WHERE clause AST
    ///
    /// # Returns
    ///
    /// Returns tuple of (SQL string, parameter values).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if clause is invalid.
    pub fn generate(&self, clause: &WhereClause) -> Result<(String, Vec<serde_json::Value>)> {
        let mut params = Vec::new();
        let sql = self.generate_clause(clause, &mut params)?;
        Ok((sql, params))
    }

    fn generate_clause(
        &self,
        clause: &WhereClause,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        match clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => self.generate_field(path, operator, value, params),
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok("1=1".to_string()); // SQLite TRUE equivalent
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("1=0".to_string()); // SQLite FALSE equivalent
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(clause) => {
                let inner = self.generate_clause(clause, params)?;
                Ok(format!("NOT ({inner})"))
            },
        }
    }

    fn generate_field(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Build JSON path accessor for SQLite
        let field_path = self.build_json_path(path);

        // Generate operator-specific SQL
        match operator {
            // Comparison operators
            WhereOperator::Eq => self.generate_comparison(&field_path, "=", value, params),
            WhereOperator::Neq => self.generate_comparison(&field_path, "!=", value, params),
            WhereOperator::Gt => self.generate_comparison(&field_path, ">", value, params),
            WhereOperator::Gte => self.generate_comparison(&field_path, ">=", value, params),
            WhereOperator::Lt => self.generate_comparison(&field_path, "<", value, params),
            WhereOperator::Lte => self.generate_comparison(&field_path, "<=", value, params),

            // Containment operators
            WhereOperator::In => self.generate_in(&field_path, value, params),
            WhereOperator::Nin => {
                let in_clause = self.generate_in(&field_path, value, params)?;
                Ok(format!("NOT ({in_clause})"))
            },

            // String operators - SQLite uses LIKE and GLOB
            WhereOperator::Contains => {
                self.generate_like(&field_path, false, value, params, true, true)
            },
            WhereOperator::Icontains => {
                self.generate_like(&field_path, true, value, params, true, true)
            },
            WhereOperator::Startswith => {
                self.generate_like(&field_path, false, value, params, false, true)
            },
            WhereOperator::Istartswith => {
                self.generate_like(&field_path, true, value, params, false, true)
            },
            WhereOperator::Endswith => {
                self.generate_like(&field_path, false, value, params, true, false)
            },
            WhereOperator::Iendswith => {
                self.generate_like(&field_path, true, value, params, true, false)
            },
            WhereOperator::Like => self.generate_comparison(&field_path, "LIKE", value, params),
            WhereOperator::Ilike => {
                // SQLite LIKE is case-insensitive for ASCII by default
                self.generate_comparison(&field_path, "LIKE", value, params)
            },

            // Null checks
            WhereOperator::IsNull => {
                let is_null = if value.as_bool().unwrap_or(true) {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
                Ok(format!("{field_path} {is_null}"))
            },

            // Array operators - SQLite has limited JSON array support
            WhereOperator::ArrayContains => {
                self.generate_json_contains(&field_path, path, value, params)
            },
            WhereOperator::ArrayContainedBy | WhereOperator::ArrayOverlaps => {
                Err(FraiseQLError::validation(
                    "ArrayContainedBy and ArrayOverlaps operators not supported in SQLite"
                        .to_string(),
                ))
            },

            // Array length operators
            WhereOperator::LenEq => self.generate_array_length(&field_path, "=", value, params),
            WhereOperator::LenGt => self.generate_array_length(&field_path, ">", value, params),
            WhereOperator::LenLt => self.generate_array_length(&field_path, "<", value, params),
            WhereOperator::LenGte => self.generate_array_length(&field_path, ">=", value, params),
            WhereOperator::LenLte => self.generate_array_length(&field_path, "<=", value, params),
            WhereOperator::LenNeq => self.generate_array_length(&field_path, "!=", value, params),

            // Unsupported operators
            WhereOperator::CosineDistance
            | WhereOperator::L2Distance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance
            | WhereOperator::InnerProduct
            | WhereOperator::JaccardDistance => Err(FraiseQLError::validation(
                "Vector distance operators not supported in SQLite".to_string(),
            )),

            // Full-text search - SQLite uses FTS5
            WhereOperator::Matches
            | WhereOperator::PlainQuery
            | WhereOperator::PhraseQuery
            | WhereOperator::WebsearchQuery => Err(FraiseQLError::validation(
                "Full-text search operators require FTS5 extension in SQLite".to_string(),
            )),

            // Network operators - not supported in SQLite
            WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsPublic
            | WhereOperator::IsLoopback
            | WhereOperator::InSubnet
            | WhereOperator::ContainsSubnet
            | WhereOperator::ContainsIP
            | WhereOperator::Overlaps => Err(FraiseQLError::validation(
                "Network operators not supported in SQLite".to_string(),
            )),

            // JSONB operators
            WhereOperator::StrictlyContains => {
                self.generate_json_contains(&field_path, path, value, params)
            },

            // LTree operators - not supported in SQLite (PostgreSQL-specific)
            WhereOperator::AncestorOf
            | WhereOperator::DescendantOf
            | WhereOperator::MatchesLquery
            | WhereOperator::MatchesLtxtquery
            | WhereOperator::MatchesAnyLquery
            | WhereOperator::DepthEq
            | WhereOperator::DepthNeq
            | WhereOperator::DepthGt
            | WhereOperator::DepthGte
            | WhereOperator::DepthLt
            | WhereOperator::DepthLte
            | WhereOperator::Lca => Err(FraiseQLError::validation(
                "LTree operators not supported in SQLite".to_string(),
            )),
        }
    }

    /// Build SQLite JSON path expression.
    /// SQLite uses json_extract(data, '$.field')
    fn build_json_path(&self, path: &[String]) -> String {
        let json_path = path.join(".");
        format!("json_extract(data, '$.{json_path}')")
    }

    fn generate_comparison(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        params.push(value.clone());

        // For numeric comparisons, cast to appropriate type
        if value.is_number()
            && (op == ">" || op == ">=" || op == "<" || op == "<=" || op == "=" || op == "!=")
        {
            Ok(format!("CAST({field_path} AS REAL) {op} ?"))
        } else {
            Ok(format!("{field_path} {op} ?"))
        }
    }

    fn generate_in(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let array = value.as_array().ok_or_else(|| {
            FraiseQLError::validation("IN operator requires array value".to_string())
        })?;

        if array.is_empty() {
            return Ok("1=0".to_string()); // FALSE
        }

        let placeholders: Vec<&str> = array
            .iter()
            .map(|v| {
                params.push(v.clone());
                "?"
            })
            .collect();

        Ok(format!("{field_path} IN ({})", placeholders.join(", ")))
    }

    fn generate_like(
        &self,
        field_path: &str,
        case_insensitive: bool,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        prefix: bool,
        suffix: bool,
    ) -> Result<String> {
        let val_str = value.as_str().ok_or_else(|| {
            FraiseQLError::validation("LIKE operator requires string value".to_string())
        })?;

        params.push(serde_json::Value::String(val_str.to_string()));

        let pattern = match (prefix, suffix) {
            (true, true) => "'%' || ? || '%'".to_string(),
            (true, false) => "'%' || ?".to_string(),
            (false, true) => "? || '%'".to_string(),
            (false, false) => "?".to_string(),
        };

        if case_insensitive {
            // Use LOWER() for case-insensitive comparison
            Ok(format!("LOWER({field_path}) LIKE LOWER({pattern})"))
        } else {
            Ok(format!("{field_path} LIKE {pattern}"))
        }
    }

    fn generate_json_contains(
        &self,
        _field_path: &str,
        path: &[String],
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // SQLite doesn't have native JSON_CONTAINS
        // Use a workaround with json_each
        let json_path = path.join(".");
        params.push(value.clone());

        // Check if the JSON array contains the value
        Ok(format!(
            "EXISTS (SELECT 1 FROM json_each(json_extract(data, '$.{json_path}')) WHERE value = json(?))"
        ))
    }

    fn generate_array_length(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        params.push(value.clone());
        Ok(format!("json_array_length({field_path}) {op} ?"))
    }
}

impl Default for SqliteWhereGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "json_extract(data, '$.email') = ?");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "LOWER(json_extract(data, '$.email')) LIKE LOWER('%' || ? || '%')");
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "json_extract(data, '$.address.city') = ?");
        assert_eq!(params, vec![json!("Paris")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(18),
            },
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(
            sql,
            "(CAST(json_extract(data, '$.age') AS REAL) >= ? AND json_extract(data, '$.active') = ?)"
        );
        assert_eq!(params, vec![json!(18), json!(true)]);
    }

    #[test]
    fn test_in_operator() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "json_extract(data, '$.status') IN (?, ?)");
        assert_eq!(params, vec![json!("active"), json!("pending")]);
    }

    #[test]
    fn test_is_null() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "json_extract(data, '$.deleted_at') IS NULL");
    }

    #[test]
    fn test_array_length() {
        let gen = SqliteWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["tags".to_string()],
            operator: WhereOperator::LenGt,
            value:    json!(0),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "json_array_length(json_extract(data, '$.tags')) > ?");
        assert_eq!(params, vec![json!(0)]);
    }
}
