//! MySQL WHERE clause SQL generation.

use crate::{
    db::where_clause::{WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
};

/// MySQL WHERE clause generator.
///
/// Converts `WhereClause` AST to MySQL SQL with parameterized queries.
/// MySQL uses `?` for placeholders instead of PostgreSQL's `$1, $2, ...`
///
/// # Example
///
/// ```rust,ignore
/// use fraiseql_core::db::mysql::MySqlWhereGenerator;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = MySqlWhereGenerator::new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "JSON_UNQUOTE(JSON_EXTRACT(data, '$.email')) LIKE CONCAT('%', ?, '%')"
/// // params: ["example.com"]
/// ```
pub struct MySqlWhereGenerator;

impl MySqlWhereGenerator {
    /// Create new MySQL WHERE generator.
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
                    return Ok("TRUE".to_string());
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
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
        // Build JSON path accessor for MySQL
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

            // String operators - MySQL uses LIKE (case-sensitive) or LOWER()+LIKE for
            // case-insensitive
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
                // MySQL LIKE is case-insensitive by default with utf8mb4_unicode_ci
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

            // Array operators - MySQL uses JSON_CONTAINS
            WhereOperator::ArrayContains => self.generate_json_contains(&field_path, value, params),
            WhereOperator::ArrayContainedBy => {
                // Reverse containment: check if array is contained by value
                self.generate_json_contained_by(&field_path, value, params)
            },
            WhereOperator::ArrayOverlaps => self.generate_json_overlaps(&field_path, value, params),

            // Array length operators
            WhereOperator::LenEq => self.generate_array_length(&field_path, "=", value, params),
            WhereOperator::LenGt => self.generate_array_length(&field_path, ">", value, params),
            WhereOperator::LenLt => self.generate_array_length(&field_path, "<", value, params),
            WhereOperator::LenGte => self.generate_array_length(&field_path, ">=", value, params),
            WhereOperator::LenLte => self.generate_array_length(&field_path, "<=", value, params),
            WhereOperator::LenNeq => self.generate_array_length(&field_path, "!=", value, params),

            // Unsupported operators in MySQL
            WhereOperator::CosineDistance
            | WhereOperator::L2Distance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance
            | WhereOperator::InnerProduct
            | WhereOperator::JaccardDistance => Err(FraiseQLError::validation(
                "Vector distance operators not supported in MySQL".to_string(),
            )),

            // Full-text search - MySQL uses MATCH ... AGAINST
            WhereOperator::Matches => self.generate_fts(&field_path, value, params),
            WhereOperator::PlainQuery
            | WhereOperator::PhraseQuery
            | WhereOperator::WebsearchQuery => {
                // MySQL FTS uses different syntax
                self.generate_fts(&field_path, value, params)
            },

            // Network operators - not natively supported in MySQL
            WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsPublic
            | WhereOperator::IsLoopback
            | WhereOperator::InSubnet
            | WhereOperator::ContainsSubnet
            | WhereOperator::ContainsIP
            | WhereOperator::Overlaps => Err(FraiseQLError::validation(
                "Network operators not supported in MySQL".to_string(),
            )),

            // JSONB operators
            WhereOperator::StrictlyContains => {
                self.generate_json_contains(&field_path, value, params)
            },

            // LTree operators - not supported in MySQL (PostgreSQL-specific)
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
            | WhereOperator::Lca => {
                Err(FraiseQLError::validation("LTree operators not supported in MySQL".to_string()))
            },

            // Extended operators for rich scalar types
            WhereOperator::Extended(op) => {
                use crate::filters::ExtendedOperatorHandler;
                self.generate_extended_sql(op, &field_path, params)
            }
        }
    }

    /// Build MySQL JSON path expression.
    /// MySQL uses JSON_EXTRACT(data, '$.field') or data->>'$.field' (MySQL 8.0+)
    fn build_json_path(&self, path: &[String]) -> String {
        let escaped_path = crate::db::path_escape::escape_mysql_json_path(path);
        // Use JSON_UNQUOTE(JSON_EXTRACT(...)) to get text value
        format!("JSON_UNQUOTE(JSON_EXTRACT(data, '{}'))", escaped_path)
    }

    /// Build raw JSON path for JSON functions (without UNQUOTE).
    /// Used for JSON array/object operations where unquoting is not desired.
    #[allow(dead_code)] // Reserved for future JSON array/object operations
    fn build_raw_json_path(&self, path: &[String]) -> String {
        let json_path = path.join(".");
        format!("JSON_EXTRACT(data, '$.{json_path}')")
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
            Ok(format!("CAST({field_path} AS DECIMAL) {op} ?"))
        } else {
            // Boolean and other comparisons use direct comparison
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
            return Ok("FALSE".to_string());
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
            (true, true) => "CONCAT('%', ?, '%')".to_string(),
            (true, false) => "CONCAT('%', ?)".to_string(),
            (false, true) => "CONCAT(?, '%')".to_string(),
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
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Get raw path (without UNQUOTE) for JSON_CONTAINS
        let raw_path = Self::strip_json_unquote(field_path);
        params.push(value.clone());
        Ok(format!("JSON_CONTAINS({raw_path}, ?)"))
    }

    fn generate_json_contained_by(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Get raw path for JSON_CONTAINS
        let raw_path = Self::strip_json_unquote(field_path);
        params.push(value.clone());
        // Reverse the arguments: check if value contains field
        Ok(format!("JSON_CONTAINS(?, {raw_path})"))
    }

    fn generate_json_overlaps(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Get raw path for JSON_OVERLAPS (MySQL 8.0.17+)
        let raw_path = Self::strip_json_unquote(field_path);
        params.push(value.clone());
        Ok(format!("JSON_OVERLAPS({raw_path}, ?)"))
    }

    fn generate_array_length(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Get raw path for JSON_LENGTH
        let raw_path = Self::strip_json_unquote(field_path);
        params.push(value.clone());
        Ok(format!("JSON_LENGTH({raw_path}) {op} ?"))
    }

    /// Strip the outer JSON_UNQUOTE wrapper from a field path.
    /// Converts `JSON_UNQUOTE(JSON_EXTRACT(data, '$.field'))` to `JSON_EXTRACT(data, '$.field')`
    fn strip_json_unquote(field_path: &str) -> &str {
        field_path
            .strip_prefix("JSON_UNQUOTE(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(field_path)
    }

    fn generate_fts(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        params.push(value.clone());
        // MySQL full-text search uses MATCH ... AGAINST
        // Note: Requires FULLTEXT index on the column
        Ok(format!("MATCH({field_path}) AGAINST(? IN NATURAL LANGUAGE MODE)"))
    }
}

impl Default for MySqlWhereGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::filters::ExtendedOperatorHandler for MySqlWhereGenerator {
    fn generate_extended_sql(
        &self,
        operator: &crate::filters::ExtendedOperator,
        _field_sql: &str,
        _params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // TODO: Week 3 implementation
        // For now, return a stub error
        Err(FraiseQLError::validation(
            format!("Extended operator not yet implemented: {}", operator),
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_UNQUOTE(JSON_EXTRACT(data, '$.email')) = ?");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(
            sql,
            "LOWER(JSON_UNQUOTE(JSON_EXTRACT(data, '$.email'))) LIKE LOWER(CONCAT('%', ?, '%'))"
        );
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_UNQUOTE(JSON_EXTRACT(data, '$.address.city')) = ?");
        assert_eq!(params, vec![json!("Paris")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = MySqlWhereGenerator::new();
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
            "(CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.age')) AS DECIMAL) >= ? AND JSON_UNQUOTE(JSON_EXTRACT(data, '$.active')) = ?)"
        );
        assert_eq!(params, vec![json!(18), json!(true)]);
    }

    #[test]
    fn test_in_operator() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_UNQUOTE(JSON_EXTRACT(data, '$.status')) IN (?, ?)");
        assert_eq!(params, vec![json!("active"), json!("pending")]);
    }

    #[test]
    fn test_is_null() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_UNQUOTE(JSON_EXTRACT(data, '$.deleted_at')) IS NULL");
    }

    #[test]
    fn test_array_contains() {
        let gen = MySqlWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["tags".to_string()],
            operator: WhereOperator::ArrayContains,
            value:    json!(["rust"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_CONTAINS(JSON_EXTRACT(data, '$.tags'), ?)");
        assert_eq!(params, vec![json!(["rust"])]);
    }
}
