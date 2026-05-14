//! WHERE clause to SQL string generator for fraiseql-wire.
//!
//! Converts FraiseQL's WHERE clause AST to SQL predicates that can be used
//! with fraiseql-wire's `where_sql()` method.

use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{WhereClause, WhereOperator};

/// Maximum allowed byte length for a string value embedded in a raw SQL query.
///
/// Applies to SQL fragments assembled via string escaping (e.g. LIKE patterns,
/// JSON path keys). Regular parameterized query paths are unaffected.
/// 64 KiB is generous for any realistic filter value while blocking DoS inputs.
const MAX_SQL_VALUE_BYTES: usize = 65_536;

/// Generates SQL WHERE clause strings from AST.
///
/// # Note on continued existence
///
/// This generator embeds values as escaped string literals rather than using
/// bind parameters.  It is intentionally retained for the **FraiseQL Wire
/// Adapter** (`fraiseql_wire_adapter`), which constructs raw SQL strings for
/// the wire protocol — a context where parameterized queries are not available.
///
/// **Do not use this in new production code.**  All other query paths must use
/// [`GenericWhereGenerator`](crate::GenericWhereGenerator) which produces
/// parameterized SQL (`$1`, `?`, etc.) and is safe by design.
#[doc(hidden)]
pub struct WhereSqlGenerator;

impl WhereSqlGenerator {
    /// Convert WHERE clause AST to SQL string.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// // fraiseql-db can be used directly or via `fraiseql_core::db` (re-export).
    /// use fraiseql_db::{WhereClause, WhereOperator, where_sql_generator::WhereSqlGenerator};
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
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the clause contains an unsupported
    /// operator or an invalid value for the given operator.
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
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
                }
                let parts: Result<Vec<_>> = clauses.iter().map(Self::to_sql).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(clause) => {
                let inner = Self::to_sql(clause)?;
                Ok(format!("NOT ({})", inner))
            },
            WhereClause::NativeField {
                column,
                operator,
                value,
                ..
            } => {
                // Wire adapter: use native column name directly with escaped literal value.
                // Cast suffix is omitted — wire protocol assembles raw SQL without bind params.
                let escaped_col = Self::escape_sql_string(column)?;
                let col_expr = format!("\"{escaped_col}\"");
                let sql_op = Self::operator_to_sql(operator)?;
                let val_sql = Self::value_to_sql(value, operator)?;
                Ok(format!("{col_expr} {sql_op} {val_sql}"))
            },
        }
    }

    fn generate_field_predicate(
        path: &[String],
        operator: &WhereOperator,
        value: &Value,
    ) -> Result<String> {
        let json_path = Self::build_json_path(path)?;
        let sql = if operator == &WhereOperator::IsNull {
            let is_null = value.as_bool().unwrap_or(true);
            if is_null {
                format!("{json_path} IS NULL")
            } else {
                format!("{json_path} IS NOT NULL")
            }
        } else {
            let sql_op = Self::operator_to_sql(operator)?;
            let sql_value = Self::value_to_sql(value, operator)?;
            format!("{json_path} {sql_op} {sql_value}")
        };
        Ok(sql)
    }

    fn build_json_path(path: &[String]) -> Result<String> {
        if path.is_empty() {
            return Ok("data".to_string());
        }

        if path.len() == 1 {
            // Simple path: data->>'field'
            // SECURITY: Escape field name to prevent SQL injection
            let escaped = Self::escape_sql_string(&path[0])?;
            Ok(format!("data->>'{}'", escaped))
        } else {
            // Nested path: data#>'{a,b,c}'->>'d'
            // SECURITY: Escape all field names to prevent SQL injection
            let nested = &path[..path.len() - 1];
            let last = &path[path.len() - 1];

            // Escape all nested components
            let escaped_nested: Vec<String> =
                nested.iter().map(|n| Self::escape_sql_string(n)).collect::<Result<Vec<_>>>()?;
            let nested_path = escaped_nested.join(",");
            let escaped_last = Self::escape_sql_string(last)?;
            Ok(format!("data#>'{{{}}}'->>'{}'", nested_path, escaped_last))
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
            WhereOperator::Nlike => "NOT LIKE",
            WhereOperator::Nilike => "NOT ILIKE",
            WhereOperator::Regex => "~",
            WhereOperator::Iregex => "~*",
            WhereOperator::Nregex => "!~",
            WhereOperator::Niregex => "!~*",

            // Array operations
            WhereOperator::ArrayContains => "@>",
            WhereOperator::ArrayContainedBy => "<@",
            WhereOperator::ArrayOverlaps => "&&",

            // These operators require special handling
            WhereOperator::IsNull => {
                return Err(FraiseQLError::Internal {
                    message: "IsNull should be handled separately".to_string(),
                    source:  None,
                });
            },
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
                    source:  None,
                });
            },

            // Vector operations not supported
            WhereOperator::L2Distance
            | WhereOperator::CosineDistance
            | WhereOperator::L1Distance
            | WhereOperator::HammingDistance
            | WhereOperator::InnerProduct
            | WhereOperator::JaccardDistance => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Vector operations not supported in fraiseql-wire: {operator:?}"
                    ),
                    source:  None,
                });
            },

            // Full-text search operators not supported yet
            WhereOperator::Matches
            | WhereOperator::PlainQuery
            | WhereOperator::PhraseQuery
            | WhereOperator::WebsearchQuery => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Full-text search operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source:  None,
                });
            },

            // Network operators not supported yet
            WhereOperator::IsIPv4
            | WhereOperator::IsIPv6
            | WhereOperator::IsPrivate
            | WhereOperator::IsLoopback
            | WhereOperator::IsMulticast
            | WhereOperator::IsLinkLocal
            | WhereOperator::IsDocumentation
            | WhereOperator::IsCarrierGrade
            | WhereOperator::InSubnet
            | WhereOperator::ContainsSubnet
            | WhereOperator::ContainsIP
            | WhereOperator::Overlaps
            | WhereOperator::StrictlyContains
            | WhereOperator::AncestorOf
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
            | WhereOperator::Lca
            | WhereOperator::Extended(_) => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "Advanced operators not yet supported in fraiseql-wire: {operator:?}"
                    ),
                    source:  None,
                });
            },
        })
    }

    fn value_to_sql(value: &Value, operator: &WhereOperator) -> Result<String> {
        match (value, operator) {
            (Value::Null, _) => Ok("NULL".to_string()),
            (Value::Bool(b), _) => Ok(b.to_string()),
            (Value::Number(n), _) => Ok(n.to_string()),

            // String operators with wildcards
            (Value::String(s), WhereOperator::Contains | WhereOperator::Icontains) => {
                Ok(format!("'%{}%'", Self::escape_sql_string(s)?))
            },
            (Value::String(s), WhereOperator::Startswith | WhereOperator::Istartswith) => {
                Ok(format!("'{}%'", Self::escape_sql_string(s)?))
            },
            (Value::String(s), WhereOperator::Endswith | WhereOperator::Iendswith) => {
                Ok(format!("'%{}'", Self::escape_sql_string(s)?))
            },

            // Regular strings
            (Value::String(s), _) => Ok(format!("'{}'", Self::escape_sql_string(s)?)),

            // Arrays (for IN operator)
            (Value::Array(arr), WhereOperator::In | WhereOperator::Nin) => {
                let values: Result<Vec<_>> =
                    arr.iter().map(|v| Self::value_to_sql(v, &WhereOperator::Eq)).collect();
                Ok(format!("ARRAY[{}]", values?.join(", ")))
            },

            // Array operations
            (
                Value::Array(_),
                WhereOperator::ArrayContains
                | WhereOperator::ArrayContainedBy
                | WhereOperator::ArrayOverlaps,
            ) => {
                // SECURITY: Serialize to JSON string and escape single quotes to prevent
                // SQL injection. The serde_json serializer handles internal escaping, and
                // we escape single quotes for the SQL string literal context.
                let json_str =
                    serde_json::to_string(value).map_err(|e| FraiseQLError::Internal {
                        message: format!("Failed to serialize JSON for array operator: {e}"),
                        source:  None,
                    })?;
                if json_str.len() > MAX_SQL_VALUE_BYTES {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "JSONB value exceeds maximum allowed size for SQL embedding \
                             ({} bytes, limit is {} bytes)",
                            json_str.len(),
                            MAX_SQL_VALUE_BYTES
                        ),
                        path:    None,
                    });
                }
                let escaped = json_str.replace('\'', "''");
                Ok(format!("'{}'::jsonb", escaped))
            },

            _ => Err(FraiseQLError::Internal {
                message: format!(
                    "Unsupported value type for operator: {value:?} with {operator:?}"
                ),
                source:  None,
            }),
        }
    }

    fn escape_sql_string(s: &str) -> Result<String> {
        if s.len() > MAX_SQL_VALUE_BYTES {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "String value exceeds maximum allowed size for SQL embedding \
                     ({} bytes, limit is {} bytes)",
                    s.len(),
                    MAX_SQL_VALUE_BYTES
                ),
                path:    None,
            });
        }
        Ok(s.replace('\'', "''"))
    }
}

#[cfg(test)]
mod tests;
