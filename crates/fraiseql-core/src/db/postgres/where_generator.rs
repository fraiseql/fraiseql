//! PostgreSQL WHERE clause SQL generation.

use crate::error::{FraiseQLError, Result};
use crate::db::where_clause::{WhereClause, WhereOperator};

/// PostgreSQL WHERE clause generator.
///
/// Converts `WhereClause` AST to PostgreSQL SQL with parameterized queries.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::db::postgres::PostgresWhereGenerator;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = PostgresWhereGenerator::new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "data->>'email' ILIKE '%' || $1 || '%'"
/// // params: ["example.com"]
/// ```
pub struct PostgresWhereGenerator {
    param_counter: std::cell::Cell<usize>,
}

impl PostgresWhereGenerator {
    /// Create new PostgreSQL WHERE generator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            param_counter: std::cell::Cell::new(0),
        }
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
        self.param_counter.set(0);
        let mut params = Vec::new();
        let sql = self.generate_clause(clause, &mut params)?;
        Ok((sql, params))
    }

    fn generate_clause(&self, clause: &WhereClause, params: &mut Vec<serde_json::Value>) -> Result<String> {
        match clause {
            WhereClause::Field { path, operator, value } => {
                self.generate_field(path, operator, value, params)
            }
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok("TRUE".to_string());
                }
                let parts: Result<Vec<String>> = clauses
                    .iter()
                    .map(|c| self.generate_clause(c, params))
                    .collect();
                Ok(format!("({})", parts?.join(" AND ")))
            }
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
                }
                let parts: Result<Vec<String>> = clauses
                    .iter()
                    .map(|c| self.generate_clause(c, params))
                    .collect();
                Ok(format!("({})", parts?.join(" OR ")))
            }
            WhereClause::Not(clause) => {
                let inner = self.generate_clause(clause, params)?;
                Ok(format!("NOT ({inner})"))
            }
        }
    }

    fn generate_field(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Build JSONB path accessor
        let field_path = self.build_jsonb_path(path);

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
            }

            // String operators
            WhereOperator::Contains => self.generate_like(&field_path, "LIKE", value, params, true, true),
            WhereOperator::Icontains => self.generate_like(&field_path, "ILIKE", value, params, true, true),
            WhereOperator::Startswith => self.generate_like(&field_path, "LIKE", value, params, false, true),
            WhereOperator::Istartswith => self.generate_like(&field_path, "ILIKE", value, params, false, true),
            WhereOperator::Endswith => self.generate_like(&field_path, "LIKE", value, params, true, false),
            WhereOperator::Iendswith => self.generate_like(&field_path, "ILIKE", value, params, true, false),
            WhereOperator::Like => self.generate_comparison(&field_path, "LIKE", value, params),
            WhereOperator::Ilike => self.generate_comparison(&field_path, "ILIKE", value, params),

            // Null checks
            WhereOperator::IsNull => {
                let is_null = if value.as_bool().unwrap_or(true) {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
                Ok(format!("{field_path} {is_null}"))
            }

            // Array operators
            WhereOperator::ArrayContains => self.generate_jsonb_op(&field_path, "@>", value, params),
            WhereOperator::ArrayContainedBy => self.generate_jsonb_op(&field_path, "<@", value, params),
            WhereOperator::ArrayOverlaps => self.generate_jsonb_op(&field_path, "&&", value, params),
            WhereOperator::LenEq => self.generate_array_length(&field_path, "=", value, params),
            WhereOperator::LenGt => self.generate_array_length(&field_path, ">", value, params),
            WhereOperator::LenLt => self.generate_array_length(&field_path, "<", value, params),
            WhereOperator::LenGte => self.generate_array_length(&field_path, ">=", value, params),
            WhereOperator::LenLte => self.generate_array_length(&field_path, "<=", value, params),
            WhereOperator::LenNeq => self.generate_array_length(&field_path, "!=", value, params),

            // Vector operators (pgvector)
            WhereOperator::CosineDistance => self.generate_vector_distance(&field_path, "<=>", value, params),
            WhereOperator::L2Distance => self.generate_vector_distance(&field_path, "<->", value, params),
            WhereOperator::L1Distance => self.generate_vector_distance(&field_path, "<+>", value, params),
            WhereOperator::HammingDistance => self.generate_vector_distance(&field_path, "<~>", value, params),
            WhereOperator::InnerProduct => self.generate_vector_distance(&field_path, "<#>", value, params),
            WhereOperator::JaccardDistance => self.generate_jaccard_distance(&field_path, value, params),

            // Full-text search
            WhereOperator::Matches => self.generate_fts(&field_path, "@@", value, params),
            WhereOperator::PlainQuery => self.generate_fts_func(&field_path, "plainto_tsquery", value, params),
            WhereOperator::PhraseQuery => self.generate_fts_func(&field_path, "phraseto_tsquery", value, params),
            WhereOperator::WebsearchQuery => self.generate_fts_func(&field_path, "websearch_to_tsquery", value, params),

            // Network operators
            WhereOperator::IsIPv4 => Ok(format!("family({field_path}::inet) = 4")),
            WhereOperator::IsIPv6 => Ok(format!("family({field_path}::inet) = 6")),
            WhereOperator::IsPrivate => {
                Ok(format!(
                    "({field_path}::inet << '10.0.0.0/8'::inet OR {field_path}::inet << '172.16.0.0/12'::inet OR {field_path}::inet << '192.168.0.0/16'::inet OR {field_path}::inet << '169.254.0.0/16'::inet)"
                ))
            }
            WhereOperator::IsPublic => {
                Ok(format!(
                    "NOT ({field_path}::inet << '10.0.0.0/8'::inet OR {field_path}::inet << '172.16.0.0/12'::inet OR {field_path}::inet << '192.168.0.0/16'::inet OR {field_path}::inet << '169.254.0.0/16'::inet)"
                ))
            }
            WhereOperator::IsLoopback => {
                Ok(format!(
                    "(family({field_path}::inet) = 4 AND {field_path}::inet << '127.0.0.0/8'::inet) OR (family({field_path}::inet) = 6 AND {field_path}::inet << '::1/128'::inet)"
                ))
            }
            WhereOperator::InSubnet => self.generate_inet_op(&field_path, "<<", value, params),
            WhereOperator::ContainsSubnet => self.generate_inet_op(&field_path, ">>", value, params),
            WhereOperator::ContainsIP => self.generate_inet_op(&field_path, ">>", value, params),
            WhereOperator::Overlaps => self.generate_inet_op(&field_path, "&&", value, params),

            // JSONB operators
            WhereOperator::StrictlyContains => self.generate_jsonb_op(&field_path, "@>", value, params),

            // LTree operators
            WhereOperator::AncestorOf => self.generate_comparison(&field_path, "@>", value, params),
            WhereOperator::DescendantOf => self.generate_comparison(&field_path, "<@", value, params),
            WhereOperator::MatchesLquery => self.generate_comparison(&field_path, "~", value, params),
        }
    }

    fn build_jsonb_path(&self, path: &[String]) -> String {
        if path.len() == 1 {
            format!("data->>'{}'{}", path[0], "")
        } else {
            let mut result = "data".to_string();
            for (i, segment) in path.iter().enumerate() {
                if i < path.len() - 1 {
                    result.push_str(&format!("->'{segment}'"));
                } else {
                    result.push_str(&format!("->>'{segment}'"));
                }
            }
            result
        }
    }

    fn next_param(&self) -> String {
        let current = self.param_counter.get();
        self.param_counter.set(current + 1);
        format!("${}", current + 1)
    }

    fn generate_comparison(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());

        // For numeric comparisons, cast both sides to numeric type
        // Use text format for parameter to avoid wire protocol issues
        if value.is_number() && (op == ">" || op == ">=" || op == "<" || op == "<=" || op == "=" || op == "!=") {
            Ok(format!("({field_path})::numeric {op} ({param}::text)::numeric"))
        } else if value.is_boolean() && (op == "=" || op == "!=") {
            // For boolean comparisons, cast the JSONB text field to boolean
            Ok(format!("({field_path})::boolean {op} {param}"))
        } else {
            Ok(format!("{field_path} {op} {param}"))
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

        let placeholders: Vec<String> = array
            .iter()
            .map(|v| {
                let param = self.next_param();
                params.push(v.clone());
                param
            })
            .collect();

        Ok(format!("{field_path} IN ({})", placeholders.join(", ")))
    }

    fn generate_like(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        prefix: bool,
        suffix: bool,
    ) -> Result<String> {
        let param = self.next_param();
        let val_str = value.as_str().ok_or_else(|| {
            FraiseQLError::validation("LIKE operator requires string value".to_string())
        })?;

        let pattern = if prefix && suffix {
            format!("'%' || {param} || '%'")
        } else if prefix {
            format!("'%' || {param}")
        } else if suffix {
            format!("{param} || '%'")
        } else {
            param.clone()
        };

        params.push(serde_json::Value::String(val_str.to_string()));
        Ok(format!("{field_path} {op} {pattern}"))
    }

    fn generate_jsonb_op(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::jsonb {op} {param}::jsonb"))
    }

    fn generate_array_length(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("jsonb_array_length({field_path}::jsonb) {op} {param}"))
    }

    fn generate_vector_distance(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::vector {op} {param}::vector"))
    }

    fn generate_fts(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("to_tsvector({field_path}) {op} to_tsquery({param})"))
    }

    fn generate_fts_func(
        &self,
        field_path: &str,
        func: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("to_tsvector({field_path}) @@ {func}({param})"))
    }

    fn generate_jaccard_distance(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        // Jaccard distance uses text arrays
        Ok(format!("({field_path})::text[] <%> ({param})::text[]"))
    }

    fn generate_inet_op(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::inet {op} {param}::inet"))
    }
}

impl Default for PostgresWhereGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_equality() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' = $1");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value: json!("example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' ILIKE '%' || $1 || '%'");
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value: json!("Paris"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->'address'->>'city' = $1");
        assert_eq!(params, vec![json!("Paris")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value: json!(18),
            },
            WhereClause::Field {
                path: vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value: json!(true),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        // Numeric comparisons cast to ::numeric, boolean comparisons cast to ::boolean
        assert_eq!(sql, "((data->>'age')::numeric >= ($1::text)::numeric AND (data->>'active')::boolean = $2)");
        assert_eq!(params, vec![json!(18), json!(true)]);
    }

    #[test]
    fn test_or_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path: vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value: json!("admin"),
            },
            WhereClause::Field {
                path: vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value: json!("moderator"),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "(data->>'role' = $1 OR data->>'role' = $2)");
        assert_eq!(params, vec![json!("admin"), json!("moderator")]);
    }

    #[test]
    fn test_not_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Not(Box::new(WhereClause::Field {
            path: vec!["deleted".to_string()],
            operator: WhereOperator::Eq,
            value: json!(true),
        }));

        let (sql, params) = gen.generate(&clause).unwrap();
        // Boolean comparisons now cast to ::boolean
        assert_eq!(sql, "NOT ((data->>'deleted')::boolean = $1)");
        assert_eq!(params, vec![json!(true)]);
    }

    #[test]
    fn test_in_operator() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["status".to_string()],
            operator: WhereOperator::In,
            value: json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'status' IN ($1, $2)");
        assert_eq!(params, vec![json!("active"), json!("pending")]);
    }

    #[test]
    fn test_is_null() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value: json!(true),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'deleted_at' IS NULL");
    }

    #[test]
    fn test_array_contains() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path: vec!["tags".to_string()],
            operator: WhereOperator::ArrayContains,
            value: json!(["rust"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'tags'::jsonb @> $1::jsonb");
        assert_eq!(params, vec![json!(["rust"])]);
    }
}
