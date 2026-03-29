//! Generic WHERE clause generator parameterised over a SQL dialect.

use std::{collections::HashSet, sync::Arc};

use fraiseql_error::{FraiseQLError, Result};

use super::counter::ParamCounter;
use crate::{
    dialect::SqlDialect,
    where_clause::{WhereClause, WhereOperator},
};

/// Escape LIKE metacharacters (`%`, `_`, `\`) in a user-supplied string so
/// that it is treated as a literal substring inside a LIKE/ILIKE pattern.
///
/// Order matters: `\` is escaped first to avoid double-escaping.
pub(crate) fn escape_like_literal(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Maximum allowed length for user-supplied regex patterns.
///
/// PostgreSQL has no built-in regex timeout, so excessively long patterns
/// or patterns with nested quantifiers can cause CPU exhaustion (ReDoS).
const MAX_REGEX_PATTERN_LEN: usize = 1_000;

/// Validate a user-supplied regex pattern for obvious ReDoS risks.
///
/// Rejects:
/// - Patterns exceeding `MAX_REGEX_PATTERN_LEN` bytes
/// - Patterns containing nested quantifiers (e.g., `(a+)+`, `(a*)*`, `(a+)*`)
///
/// This is not a full ReDoS detector but catches the most common attack vectors.
fn validate_regex_pattern(pattern: &str) -> Result<()> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Regex pattern exceeds maximum length of {MAX_REGEX_PATTERN_LEN} bytes"
            ),
            path: None,
        });
    }

    // Detect nested quantifiers: a quantifier (+, *, ?, {n}) immediately after
    // a closing paren that itself follows a quantifier. Simplified heuristic:
    // look for `)` followed by a quantifier, where the group contains a quantifier.
    let bytes = pattern.as_bytes();
    let mut depth: i32 = 0;
    let mut group_has_quantifier = Vec::new(); // stack: does current group have a quantifier?

    for (i, &b) in bytes.iter().enumerate() {
        // Skip escaped characters
        if i > 0 && bytes[i - 1] == b'\\' {
            continue;
        }
        match b {
            b'(' => {
                depth += 1;
                group_has_quantifier.push(false);
            },
            b')' => {
                let had_quantifier = group_has_quantifier.pop().unwrap_or(false);
                depth -= 1;
                // Check if a quantifier follows this closing paren
                if had_quantifier {
                    let next = bytes.get(i + 1).copied();
                    if matches!(next, Some(b'+' | b'*' | b'?' | b'{')) {
                        return Err(FraiseQLError::Validation {
                            message: "Regex pattern contains nested quantifiers (potential \
                                      ReDoS). Simplify the pattern to avoid `(…+)+`, \
                                      `(…*)*`, or similar constructs."
                                .to_string(),
                            path: None,
                        });
                    }
                }
            },
            b'+' | b'*' | b'?' => {
                if let Some(flag) = group_has_quantifier.last_mut() {
                    *flag = true;
                }
            },
            b'{' if depth > 0 => {
                if let Some(flag) = group_has_quantifier.last_mut() {
                    *flag = true;
                }
            },
            _ => {},
        }
    }

    Ok(())
}

/// Generic WHERE clause SQL generator.
///
/// Replaces `PostgresWhereGenerator`, `MySqlWhereGenerator`,
/// `SqliteWhereGenerator`, and `SqlServerWhereGenerator` — all dialect-specific
/// primitives are delegated to `D: SqlDialect`.
///
/// # Interior mutability
///
/// The parameter counter uses `Cell<usize>` (via `ParamCounter`).  This is
/// safe because:
/// - `GenericWhereGenerator` is not `Sync` — no concurrent access is possible.
/// - `generate()` resets the counter before every call.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::dialect::PostgresDialect;
/// use fraiseql_db::where_generator::GenericWhereGenerator;
/// use fraiseql_db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let gen = GenericWhereGenerator::new(PostgresDialect);
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Eq,
///     value: json!("alice@example.com"),
/// };
/// let (sql, params) = gen.generate(&clause).unwrap();
/// assert_eq!(sql, "data->>'email' = $1");
/// ```
pub struct GenericWhereGenerator<D: SqlDialect> {
    dialect:         D,
    counter:         ParamCounter,
    /// Optional indexed-column set (PostgreSQL optimisation: short-circuits JSONB
    /// extraction when a generated column covers the path).
    indexed_columns: Option<Arc<HashSet<String>>>,
}

impl<D: SqlDialect> GenericWhereGenerator<D> {
    /// Create a new generator for the given dialect.
    pub const fn new(dialect: D) -> Self {
        Self {
            dialect,
            counter: ParamCounter::new(),
            indexed_columns: None,
        }
    }

    /// Attach an indexed-columns set (PostgreSQL optimisation).
    ///
    /// When a WHERE path matches a column name in this set, the generator
    /// emits `"col_name" = $N` instead of `data->>'col_name' = $N`.
    #[must_use]
    pub fn with_indexed_columns(mut self, cols: Arc<HashSet<String>>) -> Self {
        self.indexed_columns = Some(cols);
        self
    }

    /// Generate SQL WHERE clause starting parameter numbering at 1.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the clause uses an operator
    /// not supported by the dialect.
    pub fn generate(&self, clause: &WhereClause) -> Result<(String, Vec<serde_json::Value>)> {
        self.generate_with_param_offset(clause, 0)
    }

    /// Generate SQL WHERE clause with parameter numbering starting after `offset`.
    ///
    /// Use when the WHERE clause is appended to a query that already has bound
    /// parameters (e.g. cursor values in relay pagination).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the clause uses an unsupported
    /// operator.
    pub fn generate_with_param_offset(
        &self,
        clause: &WhereClause,
        offset: usize,
    ) -> Result<(String, Vec<serde_json::Value>)> {
        self.counter.reset_to(offset);
        let mut params = Vec::new();
        let sql = self.visit(clause, &mut params)?;
        Ok((sql, params))
    }

    // ── Visitor ───────────────────────────────────────────────────────────────

    fn visit(&self, clause: &WhereClause, params: &mut Vec<serde_json::Value>) -> Result<String> {
        match clause {
            WhereClause::And(clauses) => self.visit_and(clauses, params),
            WhereClause::Or(clauses) => self.visit_or(clauses, params),
            WhereClause::Not(inner) => Ok(format!("NOT ({})", self.visit(inner, params)?)),
            WhereClause::Field {
                path,
                operator,
                value,
            } => self.visit_field(path, operator, value, params),
        }
    }

    fn visit_and(
        &self,
        clauses: &[WhereClause],
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        if clauses.is_empty() {
            return Ok(self.dialect.always_true().to_string());
        }
        let parts: Result<Vec<_>> = clauses.iter().map(|c| self.visit(c, params)).collect();
        Ok(format!("({})", parts?.join(" AND ")))
    }

    fn visit_or(
        &self,
        clauses: &[WhereClause],
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        if clauses.is_empty() {
            return Ok(self.dialect.always_false().to_string());
        }
        let parts: Result<Vec<_>> = clauses.iter().map(|c| self.visit(c, params)).collect();
        Ok(format!("({})", parts?.join(" OR ")))
    }

    // ── Field expression resolution ───────────────────────────────────────────

    fn resolve_field_expr(&self, path: &[String]) -> String {
        // PostgreSQL indexed-column optimisation.
        if let Some(indexed) = &self.indexed_columns {
            let col_name = path.join("__");
            if indexed.contains(&col_name) {
                return self.dialect.quote_identifier(&col_name);
            }
        }
        self.dialect.json_extract_scalar("data", path)
    }

    // ── Push a parameter and return its placeholder ───────────────────────────

    fn push_param(&self, params: &mut Vec<serde_json::Value>, v: serde_json::Value) -> String {
        params.push(v);
        self.dialect.placeholder(self.counter.next())
    }

    // ── Field visitor ─────────────────────────────────────────────────────────

    fn visit_field(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let field_expr = self.resolve_field_expr(path);

        match operator {
            // ── Comparison ────────────────────────────────────────────────────
            WhereOperator::Eq => {
                let p = self.push_param(params, value.clone());
                if value.is_number() {
                    let cast = self.dialect.cast_to_numeric(&field_expr);
                    // Dialect-specific RHS cast: PostgreSQL uses (p::text)::numeric to
                    // avoid wire-protocol type mismatch; other dialects pass p unchanged.
                    let rhs = self.dialect.cast_param_numeric(&p);
                    Ok(format!("{cast} = {rhs}"))
                } else if value.is_boolean() {
                    let cast = self.dialect.cast_to_boolean(&field_expr);
                    Ok(format!("{cast} = {p}"))
                } else {
                    Ok(format!("{field_expr} = {p}"))
                }
            },
            WhereOperator::Neq => {
                let p = self.push_param(params, value.clone());
                let neq = self.dialect.neq_operator();
                if value.is_number() {
                    let cast = self.dialect.cast_to_numeric(&field_expr);
                    let rhs = self.dialect.cast_param_numeric(&p);
                    Ok(format!("{cast} {neq} {rhs}"))
                } else if value.is_boolean() {
                    let cast = self.dialect.cast_to_boolean(&field_expr);
                    Ok(format!("{cast} {neq} {p}"))
                } else {
                    Ok(format!("{field_expr} {neq} {p}"))
                }
            },
            WhereOperator::Gt | WhereOperator::Gte | WhereOperator::Lt | WhereOperator::Lte => {
                let op = match operator {
                    WhereOperator::Gt => ">",
                    WhereOperator::Gte => ">=",
                    WhereOperator::Lt => "<",
                    _ => "<=",
                };
                let cast = self.dialect.cast_to_numeric(&field_expr);
                let p = self.push_param(params, value.clone());
                let rhs = self.dialect.cast_param_numeric(&p);
                Ok(format!("{cast} {op} {rhs}"))
            },

            // ── Containment ───────────────────────────────────────────────────
            WhereOperator::In | WhereOperator::Nin => {
                let arr = value.as_array().ok_or_else(|| {
                    FraiseQLError::validation("IN operator requires an array value".to_string())
                })?;
                if arr.is_empty() {
                    return Ok(if matches!(operator, WhereOperator::In) {
                        self.dialect.always_false().to_string()
                    } else {
                        self.dialect.always_true().to_string()
                    });
                }
                let placeholders: Vec<_> =
                    arr.iter().map(|v| self.push_param(params, v.clone())).collect();
                let in_list = placeholders.join(", ");
                let sql = format!("{field_expr} IN ({in_list})");
                Ok(if matches!(operator, WhereOperator::Nin) {
                    format!("NOT ({sql})")
                } else {
                    sql
                })
            },

            // ── NULL ──────────────────────────────────────────────────────────
            WhereOperator::IsNull => {
                let is_null = value.as_bool().unwrap_or(true);
                let null_op = if is_null { "IS NULL" } else { "IS NOT NULL" };
                Ok(format!("{field_expr} {null_op}"))
            },

            // ── String: LIKE family ───────────────────────────────────────────
            WhereOperator::Contains => {
                let val_str = self.require_str(value, "Contains")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&["'%'", &p, "'%'"]);
                Ok(self.dialect.like_sql(&field_expr, &pattern))
            },
            WhereOperator::Icontains => {
                let val_str = self.require_str(value, "Icontains")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&["'%'", &p, "'%'"]);
                Ok(self.dialect.ilike_sql(&field_expr, &pattern))
            },
            WhereOperator::Startswith => {
                let val_str = self.require_str(value, "Startswith")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&[&p, "'%'"]);
                Ok(self.dialect.like_sql(&field_expr, &pattern))
            },
            WhereOperator::Istartswith => {
                let val_str = self.require_str(value, "Istartswith")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&[&p, "'%'"]);
                Ok(self.dialect.ilike_sql(&field_expr, &pattern))
            },
            WhereOperator::Endswith => {
                let val_str = self.require_str(value, "Endswith")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&["'%'", &p]);
                Ok(self.dialect.like_sql(&field_expr, &pattern))
            },
            WhereOperator::Iendswith => {
                let val_str = self.require_str(value, "Iendswith")?;
                let escaped = escape_like_literal(val_str);
                let p = self.push_param(params, serde_json::Value::String(escaped));
                let pattern = self.dialect.concat_sql(&["'%'", &p]);
                Ok(self.dialect.ilike_sql(&field_expr, &pattern))
            },
            WhereOperator::Like => {
                let p = self.push_param(params, value.clone());
                Ok(self.dialect.like_sql(&field_expr, &p))
            },
            WhereOperator::Ilike => {
                let p = self.push_param(params, value.clone());
                Ok(self.dialect.ilike_sql(&field_expr, &p))
            },
            WhereOperator::Nlike => {
                let p = self.push_param(params, value.clone());
                Ok(format!("NOT ({})", self.dialect.like_sql(&field_expr, &p)))
            },
            WhereOperator::Nilike => {
                let p = self.push_param(params, value.clone());
                Ok(format!("NOT ({})", self.dialect.ilike_sql(&field_expr, &p)))
            },

            // ── String: Regex ─────────────────────────────────────────────────
            WhereOperator::Regex => {
                if let Some(s) = value.as_str() {
                    validate_regex_pattern(s)?;
                }
                let p = self.push_param(params, value.clone());
                self.dialect
                    .regex_sql(&field_expr, &p, false, false)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::Iregex => {
                if let Some(s) = value.as_str() {
                    validate_regex_pattern(s)?;
                }
                let p = self.push_param(params, value.clone());
                self.dialect
                    .regex_sql(&field_expr, &p, true, false)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::Nregex => {
                if let Some(s) = value.as_str() {
                    validate_regex_pattern(s)?;
                }
                let p = self.push_param(params, value.clone());
                self.dialect
                    .regex_sql(&field_expr, &p, false, true)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::Niregex => {
                if let Some(s) = value.as_str() {
                    validate_regex_pattern(s)?;
                }
                let p = self.push_param(params, value.clone());
                self.dialect
                    .regex_sql(&field_expr, &p, true, true)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── Array: length ─────────────────────────────────────────────────
            WhereOperator::LenEq
            | WhereOperator::LenNeq
            | WhereOperator::LenGt
            | WhereOperator::LenGte
            | WhereOperator::LenLt
            | WhereOperator::LenLte => {
                let op = match operator {
                    WhereOperator::LenEq => "=",
                    WhereOperator::LenNeq => self.dialect.neq_operator(),
                    WhereOperator::LenGt => ">",
                    WhereOperator::LenGte => ">=",
                    WhereOperator::LenLt => "<",
                    _ => "<=",
                };
                let len_expr = self.dialect.json_array_length(&field_expr);
                let p = self.push_param(params, value.clone());
                Ok(format!("{len_expr} {op} {p}"))
            },

            // ── Array: containment ────────────────────────────────────────────
            WhereOperator::ArrayContains | WhereOperator::StrictlyContains => {
                // Both @> (ArrayContains) and @> (StrictlyContains, a JSONB-level
                // strict containment) are routed to array_contains_sql.
                let p = self.push_param(params, value.clone());
                self.dialect
                    .array_contains_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::ArrayContainedBy => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .array_contained_by_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::ArrayOverlaps => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .array_overlaps_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── Full-text search ──────────────────────────────────────────────
            WhereOperator::Matches => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .fts_matches_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::PlainQuery => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .fts_plain_query_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::PhraseQuery => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .fts_phrase_query_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::WebsearchQuery => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .fts_websearch_query_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── Vector (pgvector) ─────────────────────────────────────────────
            WhereOperator::CosineDistance => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .vector_distance_sql("<=>", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::L2Distance => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .vector_distance_sql("<->", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::L1Distance => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .vector_distance_sql("<+>", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::HammingDistance => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .vector_distance_sql("<~>", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::InnerProduct => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .vector_distance_sql("<#>", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::JaccardDistance => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .jaccard_distance_sql(&field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── Network (INET/CIDR) ───────────────────────────────────────────
            WhereOperator::IsIPv4 => self
                .dialect
                .inet_check_sql(&field_expr, "IsIPv4")
                .map_err(|e| FraiseQLError::validation(e.to_string())),
            WhereOperator::IsIPv6 => self
                .dialect
                .inet_check_sql(&field_expr, "IsIPv6")
                .map_err(|e| FraiseQLError::validation(e.to_string())),
            WhereOperator::IsPrivate => self
                .dialect
                .inet_check_sql(&field_expr, "IsPrivate")
                .map_err(|e| FraiseQLError::validation(e.to_string())),
            WhereOperator::IsPublic => self
                .dialect
                .inet_check_sql(&field_expr, "IsPublic")
                .map_err(|e| FraiseQLError::validation(e.to_string())),
            WhereOperator::IsLoopback => self
                .dialect
                .inet_check_sql(&field_expr, "IsLoopback")
                .map_err(|e| FraiseQLError::validation(e.to_string())),
            WhereOperator::InSubnet => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .inet_binary_sql("<<", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::ContainsSubnet | WhereOperator::ContainsIP => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .inet_binary_sql(">>", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::Overlaps => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .inet_binary_sql("&&", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── LTree ─────────────────────────────────────────────────────────
            WhereOperator::AncestorOf => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_binary_sql("@>", &field_expr, &p, "ltree")
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DescendantOf => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_binary_sql("<@", &field_expr, &p, "ltree")
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::MatchesLquery => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_binary_sql("~", &field_expr, &p, "lquery")
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::MatchesLtxtquery => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_binary_sql("@", &field_expr, &p, "ltxtquery")
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::MatchesAnyLquery => {
                let arr = value.as_array().ok_or_else(|| {
                    FraiseQLError::validation(
                        "matches_any_lquery operator requires an array value".to_string(),
                    )
                })?;
                if arr.is_empty() {
                    return Err(FraiseQLError::validation(
                        "matches_any_lquery requires at least one lquery".to_string(),
                    ));
                }
                let placeholders: Vec<_> = arr
                    .iter()
                    .map(|v| format!("{}::lquery", self.push_param(params, v.clone())))
                    .collect();
                self.dialect
                    .ltree_any_lquery_sql(&field_expr, &placeholders)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthEq => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql("=", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthNeq => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql("!=", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthGt => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql(">", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthGte => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql(">=", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthLt => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql("<", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::DepthLte => {
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_depth_sql("<=", &field_expr, &p)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::Lca => {
                let arr = value.as_array().ok_or_else(|| {
                    FraiseQLError::validation("lca operator requires an array value".to_string())
                })?;
                if arr.is_empty() {
                    return Err(FraiseQLError::validation(
                        "lca operator requires at least one path".to_string(),
                    ));
                }
                let placeholders: Vec<_> = arr
                    .iter()
                    .map(|v| format!("{}::ltree", self.push_param(params, v.clone())))
                    .collect();
                self.dialect
                    .ltree_lca_sql(&field_expr, &placeholders)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },

            // ── Extended operators ────────────────────────────────────────────
            WhereOperator::Extended(op) => {
                self.dialect.generate_extended_sql(op, &field_expr, params)
            },

            // ── Unknown / future operators ────────────────────────────────────
            // This arm is only reachable if WhereOperator gains new variants
            // (it is #[non_exhaustive]).  Suppress the lint that fires when all
            // current variants are already matched above.
            #[allow(unreachable_patterns)]  // Reason: defensive catch-all for future non_exhaustive variants
            _ => Err(FraiseQLError::Validation {
                message: format!(
                    "Operator {operator:?} is not supported by the {} dialect",
                    self.dialect.name()
                ),
                path:    None,
            }),
        }
    }

    fn require_str<'a>(&self, value: &'a serde_json::Value, op: &'static str) -> Result<&'a str> {
        value.as_str().ok_or_else(|| {
            FraiseQLError::validation(format!("{op} operator requires a string value"))
        })
    }
}

// ── Default impl ──────────────────────────────────────────────────────────────

impl<D: SqlDialect + Default> Default for GenericWhereGenerator<D> {
    fn default() -> Self {
        Self::new(D::default())
    }
}

// ── ExtendedOperatorHandler — single blanket impl ─────────────────────────────
// Delegates to `D::generate_extended_sql`, which each dialect implements.

impl<D: SqlDialect> crate::filters::ExtendedOperatorHandler for GenericWhereGenerator<D> {
    fn generate_extended_sql(
        &self,
        operator: &crate::filters::ExtendedOperator,
        field_sql: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        self.dialect.generate_extended_sql(operator, field_sql, params)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;

    use super::GenericWhereGenerator;
    use crate::{
        dialect::PostgresDialect,
        where_clause::{WhereClause, WhereOperator},
    };

    fn field(path: &str, op: WhereOperator, val: serde_json::Value) -> WhereClause {
        WhereClause::Field {
            path:     vec![path.to_string()],
            operator: op,
            value:    val,
        }
    }

    // ── Core comparison / logical operators ──────────────────────────

    #[test]
    fn generic_eq_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("email", WhereOperator::Eq, json!("alice@example.com"));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' = $1");
        assert_eq!(params, vec![json!("alice@example.com")]);
    }

    #[test]
    fn generic_and_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::And(vec![
            field("status", WhereOperator::Eq, json!("active")),
            field("age", WhereOperator::Gte, json!(18)),
        ]);
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.starts_with("(data->>'status' = $1 AND"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn generic_empty_and_returns_true() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::And(vec![]);
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "TRUE");
        assert!(params.is_empty());
    }

    #[test]
    fn generic_empty_or_returns_false() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Or(vec![]);
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "FALSE");
        assert!(params.is_empty());
    }

    #[test]
    fn generic_not_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Not(Box::new(field("deleted", WhereOperator::Eq, json!(true))));
        let (sql, _) = gen.generate(&clause).unwrap();
        assert!(sql.starts_with("NOT ("));
    }

    #[test]
    fn generate_resets_counter() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("x", WhereOperator::Eq, json!(1));
        let (sql1, _) = gen.generate(&clause).unwrap();
        let (sql2, _) = gen.generate(&clause).unwrap();
        assert_eq!(sql1, sql2);
        // Both must reference $1, not $1 then $2
        assert!(sql1.contains("$1"));
        assert!(!sql1.contains("$2"));
    }

    #[test]
    fn generate_with_param_offset() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("email", WhereOperator::Eq, json!("a@b.com"));
        let (sql, _) = gen.generate_with_param_offset(&clause, 2).unwrap();
        assert!(sql.contains("$3"), "Expected $3 (offset 2 + 1), got: {sql}");
    }

    // ── String operators ─────────────────────────────────────────────

    #[test]
    fn generic_icontains_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("email", WhereOperator::Icontains, json!("example.com"));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' ILIKE '%' || $1 || '%'");
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn generic_startswith_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Startswith, json!("Al"));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'name' LIKE $1 || '%'");
        assert_eq!(params, vec![json!("Al")]);
    }

    #[test]
    fn generic_endswith_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Endswith, json!("son"));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'name' LIKE '%' || $1");
        assert_eq!(params, vec![json!("son")]);
    }

    // ── Array / IN operators ────────────────────────────────────────

    #[test]
    fn generic_in_postgres() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("status", WhereOperator::In, json!(["active", "pending"]));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'status' IN ($1, $2)");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn generic_in_empty_returns_false() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("status", WhereOperator::In, json!([]));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "FALSE");
        assert!(params.is_empty());
    }

    #[test]
    fn generic_nin_empty_returns_true() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("status", WhereOperator::Nin, json!([]));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "TRUE");
        assert!(params.is_empty());
    }

    // ── Security: no value interpolation ─────────────────────────────────────

    #[test]
    fn no_value_in_sql_string() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let injection = "'; DROP TABLE users; --";
        let clause = field("email", WhereOperator::Eq, json!(injection));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(!sql.contains(injection), "Value must not appear in SQL: {sql}");
        assert_eq!(params[0], json!(injection));
    }

    // ── PG-only: Vector operators ─────────────────────────────────────────────

    #[test]
    fn generic_pg_cosine_distance() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("embedding", WhereOperator::CosineDistance, json!([0.1, 0.2]));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("<=>"), "Expected <=> operator, got: {sql}");
        assert!(sql.contains("::vector"), "Expected ::vector cast, got: {sql}");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn generic_pg_network_ipv4() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("ip", WhereOperator::IsIPv4, json!(true));
        let (sql, _) = gen.generate(&clause).unwrap();
        assert!(sql.contains("family("), "Expected family() call, got: {sql}");
        assert!(sql.contains("= 4"), "Expected = 4, got: {sql}");
    }

    #[test]
    fn generic_pg_ltree_ancestor_of() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("path", WhereOperator::AncestorOf, json!("europe.france"));
        let (sql, params) = gen.generate(&clause).unwrap();
        assert!(sql.contains("@>") && sql.contains("ltree"), "Got: {sql}");
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn non_pg_vector_op_returns_error() {
        use crate::dialect::MySqlDialect;
        let gen = GenericWhereGenerator::new(MySqlDialect);
        let clause = field("embedding", WhereOperator::CosineDistance, json!([0.1]));
        let err = gen.generate(&clause).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("VectorDistance") || msg.contains("not supported"), "Got: {msg}");
    }

    #[test]
    fn non_pg_network_op_returns_error() {
        use crate::dialect::SqliteDialect;
        let gen = GenericWhereGenerator::new(SqliteDialect);
        let clause = field("ip", WhereOperator::IsIPv4, json!(true));
        let err = gen.generate(&clause).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Inet") || msg.contains("not supported"), "Got: {msg}");
    }

    // ── LIKE metacharacter escaping (C3 fix verification) ──────────────

    #[test]
    fn escape_like_literal_escapes_percent_and_underscore() {
        assert_eq!(super::escape_like_literal("50%"), "50\\%");
        assert_eq!(super::escape_like_literal("user_name"), "user\\_name");
        assert_eq!(super::escape_like_literal("a%b_c\\d"), "a\\%b\\_c\\\\d");
        assert_eq!(super::escape_like_literal("plain"), "plain");
    }

    #[test]
    fn contains_escapes_like_metacharacters() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Contains, json!("50%off"));
        let (_sql, params) = gen.generate(&clause).unwrap();
        // The param value must have % escaped so it's treated as a literal.
        assert_eq!(params[0], json!("50\\%off"));
    }

    #[test]
    fn startswith_escapes_like_metacharacters() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Startswith, json!("user_"));
        let (_sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(params[0], json!("user\\_"));
    }

    #[test]
    fn endswith_escapes_like_metacharacters() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Endswith, json!("100%"));
        let (_sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(params[0], json!("100\\%"));
    }

    // ── Regex complexity guard (C5 fix verification) ──────────────────

    #[test]
    fn regex_rejects_nested_quantifiers() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Regex, json!("(a+)+$"));
        let err = gen.generate(&clause).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nested quantifiers"), "Got: {msg}");
    }

    #[test]
    fn regex_rejects_star_star_pattern() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Regex, json!("(x*)*"));
        let err = gen.generate(&clause).unwrap_err();
        assert!(err.to_string().contains("nested quantifiers"));
    }

    #[test]
    fn regex_rejects_too_long_pattern() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let long_pattern = "a".repeat(1_001);
        let clause = field("name", WhereOperator::Regex, json!(long_pattern));
        let err = gen.generate(&clause).unwrap_err();
        assert!(err.to_string().contains("maximum length"));
    }

    #[test]
    fn regex_allows_safe_patterns() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Regex, json!("^[a-z]+$"));
        assert!(gen.generate(&clause).is_ok());
    }

    #[test]
    fn iregex_also_validates_pattern() {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let clause = field("name", WhereOperator::Iregex, json!("(a+)+"));
        assert!(gen.generate(&clause).is_err());
    }
}
