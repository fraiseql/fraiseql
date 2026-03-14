//! SQL Server WHERE clause SQL generation.

use fraiseql_error::{FraiseQLError, Result};

use crate::where_clause::{WhereClause, WhereOperator};

/// SQL Server WHERE clause generator.
///
/// Converts `WhereClause` AST to SQL Server T-SQL with parameterized queries.
/// SQL Server uses `@p1, @p2, ...` for named parameters.
///
/// # Example
///
/// ```no_run
/// use fraiseql_core::db::sqlserver::SqlServerWhereGenerator;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = SqlServerWhereGenerator::new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "JSON_VALUE(data, '$.email') LIKE '%' + @p1 + '%'"
/// // params: ["example.com"]
/// ```
pub struct SqlServerWhereGenerator {
    param_counter: std::cell::Cell<usize>,
}

impl SqlServerWhereGenerator {
    /// Create new SQL Server WHERE generator.
    #[must_use]
    pub const fn new() -> Self {
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

    /// Generate SQL WHERE clause with a parameter index offset.
    ///
    /// Used when WHERE clause parameters must not start at `@p1` because
    /// earlier parameters (e.g. a cursor value) already occupy lower indices.
    ///
    /// # Arguments
    ///
    /// * `clause` - WHERE clause AST
    /// * `offset` - number of parameters already bound before this clause
    ///
    /// # Returns
    ///
    /// Returns tuple of (SQL string, parameter values). Parameter placeholders
    /// start at `@p{offset+1}`.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if clause is invalid.
    ///
    /// # Pitfalls
    ///
    /// After this call, `param_counter` is left at `offset + params.len()`.
    /// Calling `generate()` on the **same** instance afterwards will reset the counter
    /// to 0, silently discarding that state.  Always create a fresh
    /// `SqlServerWhereGenerator::new()` per query rather than reusing instances across
    /// calls.
    pub fn generate_with_param_offset(
        &self,
        clause: &WhereClause,
        offset: usize,
    ) -> Result<(String, Vec<serde_json::Value>)> {
        self.param_counter.set(offset);
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
                    return Ok("1=1".to_string());
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("1=0".to_string());
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
        // Build JSON path accessor for SQL Server
        let field_path = self.build_json_path(path);

        // Generate operator-specific SQL
        match operator {
            // Comparison operators
            WhereOperator::Eq => self.generate_comparison(&field_path, "=", value, params),
            WhereOperator::Neq => self.generate_comparison(&field_path, "<>", value, params), // SQL Server uses <>
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

            // String operators - SQL Server uses LIKE with COLLATE for case sensitivity
            WhereOperator::Contains => {
                self.generate_like(&field_path, false, value, params, true, true)
            }
            WhereOperator::Icontains => {
                self.generate_like(&field_path, true, value, params, true, true)
            }
            WhereOperator::Startswith => {
                self.generate_like(&field_path, false, value, params, false, true)
            }
            WhereOperator::Istartswith => {
                self.generate_like(&field_path, true, value, params, false, true)
            }
            WhereOperator::Endswith => {
                self.generate_like(&field_path, false, value, params, true, false)
            }
            WhereOperator::Iendswith => {
                self.generate_like(&field_path, true, value, params, true, false)
            }
            WhereOperator::Like => self.generate_comparison(&field_path, "LIKE", value, params),
            WhereOperator::Ilike => {
                // SQL Server LIKE with case-insensitive collation
                let param = self.next_param();
                params.push(value.clone());
                Ok(format!(
                    "{field_path} COLLATE Latin1_General_CI_AI LIKE {param}"
                ))
            }
            WhereOperator::Nlike => {
                self.generate_comparison(&field_path, "NOT LIKE", value, params)
            }
            WhereOperator::Nilike => Err(FraiseQLError::Unsupported {
                message: "NILIKE operator not supported in SQL Server (no native ILIKE)".to_string(),
            }),
            WhereOperator::Regex
            | WhereOperator::Iregex
            | WhereOperator::Nregex
            | WhereOperator::Niregex => Err(FraiseQLError::Unsupported {
                message: "POSIX regex operators not supported in SQL Server".to_string(),
            }),

            // Null checks
            WhereOperator::IsNull => {
                let is_null = if value.as_bool().unwrap_or(true) {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
                Ok(format!("{field_path} {is_null}"))
            }

            // Array operators - SQL Server has limited JSON array support
            WhereOperator::ArrayContains => self.generate_json_contains(&field_path, path, value, params),
            WhereOperator::ArrayContainedBy | WhereOperator::ArrayOverlaps => {
                Err(FraiseQLError::validation(
                    "ArrayContainedBy and ArrayOverlaps operators require custom functions in SQL Server".to_string(),
                ))
            }

            // Array length operators
            WhereOperator::LenEq => self.generate_array_length(&field_path, path, "=", value, params),
            WhereOperator::LenGt => self.generate_array_length(&field_path, path, ">", value, params),
            WhereOperator::LenLt => self.generate_array_length(&field_path, path, "<", value, params),
            WhereOperator::LenGte => self.generate_array_length(&field_path, path, ">=", value, params),
            WhereOperator::LenLte => self.generate_array_length(&field_path, path, "<=", value, params),
            WhereOperator::LenNeq => self.generate_array_length(&field_path, path, "<>", value, params),

            // Unsupported operators
            WhereOperator::CosineDistance
            | WhereOperator::L2Distance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance
            | WhereOperator::InnerProduct
            | WhereOperator::JaccardDistance => Err(FraiseQLError::validation(
                "Vector distance operators not supported in SQL Server".to_string(),
            )),

            // Full-text search - SQL Server uses CONTAINS and FREETEXT
            WhereOperator::Matches => self.generate_fts(&field_path, "CONTAINS", value, params),
            WhereOperator::PlainQuery | WhereOperator::PhraseQuery => {
                self.generate_fts(&field_path, "FREETEXT", value, params)
            }
            WhereOperator::WebsearchQuery => {
                Err(FraiseQLError::validation(
                    "WebsearchQuery not directly supported in SQL Server".to_string(),
                ))
            }

            // Network operators - not natively supported
            WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsPublic
            | WhereOperator::IsLoopback
            | WhereOperator::InSubnet
            | WhereOperator::ContainsSubnet
            | WhereOperator::ContainsIP
            | WhereOperator::Overlaps => Err(FraiseQLError::validation(
                "Network operators not natively supported in SQL Server".to_string(),
            )),

            // JSONB operators
            WhereOperator::StrictlyContains => self.generate_json_contains(&field_path, path, value, params),

            // LTree operators - not supported in SQL Server (PostgreSQL-specific)
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
                Err(FraiseQLError::validation(
                    "LTree operators not supported in SQL Server".to_string(),
                ))
            }

            // Extended operators for rich scalar types
            WhereOperator::Extended(op) => {
                use crate::filters::ExtendedOperatorHandler;
                self.generate_extended_sql(op, &field_path, params)
            }
        }
    }

    /// Build SQL Server JSON path expression.
    /// SQL Server uses JSON_VALUE(data, '$.field') for scalar values
    fn build_json_path(&self, path: &[String]) -> String {
        let escaped_path = crate::path_escape::escape_sqlserver_json_path(path);
        format!("JSON_VALUE(data, '{}')", escaped_path)
    }

    fn next_param(&self) -> String {
        let current = self.param_counter.get();
        self.param_counter.set(current + 1);
        format!("@p{}", current + 1)
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

        // For numeric comparisons, cast to appropriate type
        if value.is_number()
            && (op == ">" || op == ">=" || op == "<" || op == "<=" || op == "=" || op == "<>")
        {
            Ok(format!("CAST({field_path} AS FLOAT) {op} {param}"))
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
            return Ok("1=0".to_string());
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
        case_insensitive: bool,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        prefix: bool,
        suffix: bool,
    ) -> Result<String> {
        let val_str = value.as_str().ok_or_else(|| {
            FraiseQLError::validation("LIKE operator requires string value".to_string())
        })?;

        let param = self.next_param();
        params.push(serde_json::Value::String(val_str.to_string()));

        let pattern = match (prefix, suffix) {
            (true, true) => format!("'%' + {param} + '%'"),
            (true, false) => format!("'%' + {param}"),
            (false, true) => format!("{param} + '%'"),
            (false, false) => param,
        };

        if case_insensitive {
            // Use case-insensitive collation
            Ok(format!("{field_path} COLLATE Latin1_General_CI_AI LIKE {pattern}"))
        } else {
            // Use case-sensitive collation
            Ok(format!("{field_path} COLLATE Latin1_General_CS_AS LIKE {pattern}"))
        }
    }

    fn generate_json_contains(
        &self,
        _field_path: &str,
        path: &[String],
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // SQL Server uses OPENJSON to check array containment
        let json_path = path.join(".");
        let param = self.next_param();
        params.push(value.clone());

        Ok(format!(
            "EXISTS (SELECT 1 FROM OPENJSON(data, '$.{json_path}') WHERE value = {param})"
        ))
    }

    fn generate_array_length(
        &self,
        _field_path: &str,
        path: &[String],
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // SQL Server counts array elements using OPENJSON
        let json_path = path.join(".");
        let param = self.next_param();
        params.push(value.clone());

        Ok(format!("(SELECT COUNT(*) FROM OPENJSON(data, '$.{json_path}')) {op} {param}"))
    }

    fn generate_fts(
        &self,
        field_path: &str,
        func: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        // SQL Server full-text search requires a full-text index
        Ok(format!("{func}({field_path}, {param})"))
    }
}

impl Default for SqlServerWhereGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::filters::ExtendedOperatorHandler for SqlServerWhereGenerator {
    fn generate_extended_sql(
        &self,
        operator: &crate::filters::ExtendedOperator,
        field_sql: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        match operator {
            // Email domain extraction: extract part after @
            crate::filters::ExtendedOperator::EmailDomainEq(domain) => {
                params.push(serde_json::Value::String(domain.clone()));
                let param = self.next_param();
                // SQL Server: SUBSTRING(field, CHARINDEX('@', field) + 1, LEN(field)) = @pN
                Ok(format!(
                    "SUBSTRING({field_sql}, CHARINDEX('@', {field_sql}) + 1, LEN({field_sql})) = {param}"
                ))
            },

            crate::filters::ExtendedOperator::EmailDomainIn(domains) => {
                let mut placeholders = Vec::new();
                for d in domains {
                    params.push(serde_json::Value::String(d.clone()));
                    placeholders.push(self.next_param());
                }
                Ok(format!(
                    "SUBSTRING({field_sql}, CHARINDEX('@', {field_sql}) + 1, LEN({field_sql})) IN ({})",
                    placeholders.join(", ")
                ))
            },

            crate::filters::ExtendedOperator::EmailDomainEndswith(suffix) => {
                params.push(serde_json::Value::String(suffix.clone()));
                let param = self.next_param();
                // SQL Server: SUBSTRING(field, CHARINDEX('@', field) + 1, LEN(field)) LIKE '%' +
                // @pN
                Ok(format!(
                    "SUBSTRING({field_sql}, CHARINDEX('@', {field_sql}) + 1, LEN({field_sql})) LIKE '%' + {param}"
                ))
            },

            crate::filters::ExtendedOperator::EmailLocalPartStartswith(prefix) => {
                params.push(serde_json::Value::String(prefix.clone()));
                let param = self.next_param();
                // SQL Server: SUBSTRING(field, 1, CHARINDEX('@', field) - 1) LIKE @pN + '%'
                Ok(format!(
                    "SUBSTRING({field_sql}, 1, CHARINDEX('@', {field_sql}) - 1) LIKE {param} + '%'"
                ))
            },

            // VIN operations
            crate::filters::ExtendedOperator::VinWmiEq(wmi) => {
                params.push(serde_json::Value::String(wmi.clone()));
                let param = self.next_param();
                // SQL Server: SUBSTRING(field, 1, 3) = @pN
                Ok(format!("SUBSTRING({field_sql}, 1, 3) = {param}"))
            },

            // IBAN operations
            crate::filters::ExtendedOperator::IbanCountryEq(country) => {
                params.push(serde_json::Value::String(country.clone()));
                let param = self.next_param();
                // SQL Server: SUBSTRING(field, 1, 2) = @pN
                Ok(format!("SUBSTRING({field_sql}, 1, 2) = {param}"))
            },

            // Fallback: not implemented
            _ => Err(FraiseQLError::validation(format!(
                "Extended operator not yet implemented: {}",
                operator
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code — panics are acceptable failures

    use serde_json::json;

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.email') = @p1");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(
            sql,
            "JSON_VALUE(data, '$.email') COLLATE Latin1_General_CI_AI LIKE '%' + @p1 + '%'"
        );
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.address.city') = @p1");
        assert_eq!(params, vec![json!("Paris")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = SqlServerWhereGenerator::new();
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
            "(CAST(JSON_VALUE(data, '$.age') AS FLOAT) >= @p1 AND JSON_VALUE(data, '$.active') = @p2)"
        );
        assert_eq!(params, vec![json!(18), json!(true)]);
    }

    #[test]
    fn test_in_operator() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.status') IN (@p1, @p2)");
        assert_eq!(params, vec![json!("active"), json!("pending")]);
    }

    #[test]
    fn test_is_null() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.deleted_at') IS NULL");
    }

    #[test]
    fn test_not_equal() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Neq,
            value:    json!("deleted"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.status') <> @p1");
        assert_eq!(params, vec![json!("deleted")]);
    }

    #[test]
    fn test_gt_number_casts_to_float() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["score".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(50),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "CAST(JSON_VALUE(data, '$.score') AS FLOAT) > @p1");
        assert_eq!(params, vec![json!(50)]);
    }

    #[test]
    fn test_lt_number_casts_to_float() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Lt,
            value:    json!(18),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "CAST(JSON_VALUE(data, '$.age') AS FLOAT) < @p1");
        assert_eq!(params, vec![json!(18)]);
    }

    #[test]
    fn test_startswith_uses_collated_like() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["code".to_string()],
            operator: WhereOperator::Startswith,
            value:    json!("US-"),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.code') COLLATE Latin1_General_CS_AS LIKE @p1 + '%'");
        assert_eq!(params, vec![json!("US-")]);
    }

    #[test]
    fn test_endswith_uses_collated_like() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["filename".to_string()],
            operator: WhereOperator::Endswith,
            value:    json!(".pdf"),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(
            sql,
            "JSON_VALUE(data, '$.filename') COLLATE Latin1_General_CS_AS LIKE '%' + @p1"
        );
        assert_eq!(params, vec![json!(".pdf")]);
    }

    #[test]
    fn test_contains_uses_collated_like() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: WhereOperator::Contains,
            value:    json!("alice"),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(
            sql,
            "JSON_VALUE(data, '$.name') COLLATE Latin1_General_CS_AS LIKE '%' + @p1 + '%'"
        );
        assert_eq!(params, vec![json!("alice")]);
    }

    #[test]
    fn test_ilike_uses_ci_collation() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["tag".to_string()],
            operator: WhereOperator::Ilike,
            value:    json!("rust%"),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("Latin1_General_CI_AI"), "Ilike should use CI_AI collation: {sql}");
        assert_eq!(params, vec![json!("rust%")]);
    }

    #[test]
    fn test_is_not_null() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["published_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(false),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "JSON_VALUE(data, '$.published_at') IS NOT NULL");
        assert!(params.is_empty());
    }

    #[test]
    fn test_nin_not_in() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::Nin,
            value:    json!(["deleted", "archived"]),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("NOT"), "Nin should produce NOT: {sql}");
        assert!(sql.contains("IN"), "Nin should contain IN: {sql}");
        assert_eq!(params.len(), 2);
        // SQL Server uses numbered params @p1, @p2
        assert!(sql.contains("@p1"), "SQL Server param @p1: {sql}");
        assert!(sql.contains("@p2"), "SQL Server param @p2: {sql}");
    }

    #[test]
    fn test_param_counter_increments_across_and_clause() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gt,
                value:    json!(18),
            },
        ]);
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("@p1"), "@p1 present: {sql}");
        assert!(sql.contains("@p2"), "@p2 present: {sql}");
        assert!(sql.contains("@p3"), "@p3 present: {sql}");
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_or_combinator() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("pending"),
            },
            WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("processing"),
            },
        ]);
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("OR"), "SQL Server OR combinator: {sql}");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_not_combinator() {
        let gen = SqlServerWhereGenerator::new();
        let clause = WhereClause::Not(Box::new(WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(false),
        }));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.starts_with("NOT ("), "NOT combinator: {sql}");
        assert_eq!(params.len(), 1);
    }
}
