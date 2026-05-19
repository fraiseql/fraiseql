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
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
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
    dialect: D,
    counter: ParamCounter,
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

    /// Generate SQL WHERE clause with hierarchy context for ID-based ltree operators.
    ///
    /// The `hierarchy_ctx` provides metadata (`table`, `path_column`, `fk_column`)
    /// needed by `DescendantOfId` / `AncestorOfId` operators to generate the
    /// correct subquery SQL.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the clause uses an unsupported
    /// operator or the hierarchy context is missing for an ID-based operator.
    pub fn generate_with_hierarchy(
        &self,
        clause: &WhereClause,
        hierarchy_ctx: &super::HierarchyContext,
    ) -> Result<(String, Vec<serde_json::Value>)> {
        self.counter.reset_to(0);
        let mut params = Vec::new();
        let sql = self.visit_impl(clause, &mut params, Some(hierarchy_ctx))?;
        Ok((sql, params))
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
        self.visit_impl(clause, params, None)
    }

    fn visit_impl(
        &self,
        clause: &WhereClause,
        params: &mut Vec<serde_json::Value>,
        hierarchy_ctx: Option<&super::HierarchyContext>,
    ) -> Result<String> {
        match clause {
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok(self.dialect.always_true().to_string());
                }
                let parts: Result<Vec<_>> =
                    clauses.iter().map(|c| self.visit_impl(c, params, hierarchy_ctx)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok(self.dialect.always_false().to_string());
                }
                let parts: Result<Vec<_>> =
                    clauses.iter().map(|c| self.visit_impl(c, params, hierarchy_ctx)).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(inner) => {
                Ok(format!("NOT ({})", self.visit_impl(inner, params, hierarchy_ctx)?))
            },
            WhereClause::Field {
                path,
                operator,
                value,
            } => self.visit_field(path, operator, value, params, hierarchy_ctx),
            WhereClause::NativeField {
                column,
                pg_cast,
                operator,
                value,
            } => self.visit_native_field(column, pg_cast, operator, value, params),
        }
    }

    /// Generate SQL for a native-column condition.
    ///
    /// Emits `"column" = <cast>` where `<cast>` is a dialect-appropriate
    /// expression (e.g. `$1::text::uuid` for PostgreSQL, `CAST(? AS CHAR)` for
    /// MySQL) instead of the JSONB extraction path.
    fn visit_native_field(
        &self,
        column: &str,
        pg_cast: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let col_expr = self.dialect.quote_identifier(column);
        let p = self.push_param(params, value.clone());
        let rhs = if pg_cast.is_empty() {
            p
        } else {
            self.dialect.cast_native_param(&p, pg_cast)
        };
        match operator {
            WhereOperator::Eq => Ok(format!("{col_expr} = {rhs}")),
            WhereOperator::Neq => {
                let neq = self.dialect.neq_operator();
                Ok(format!("{col_expr} {neq} {rhs}"))
            },
            _ => Err(FraiseQLError::validation(format!(
                "Operator {operator:?} is not supported for native column conditions"
            ))),
        }
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
        hierarchy_ctx: Option<&super::HierarchyContext>,
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
            WhereOperator::IsPrivate => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate { "IsPublic" } else { "IsPrivate" };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsLoopback => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate {
                    "IsNotLoopback"
                } else {
                    "IsLoopback"
                };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsMulticast => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate {
                    "IsNotMulticast"
                } else {
                    "IsMulticast"
                };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsLinkLocal => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate {
                    "IsNotLinkLocal"
                } else {
                    "IsLinkLocal"
                };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsDocumentation => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate {
                    "IsNotDocumentation"
                } else {
                    "IsDocumentation"
                };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsCarrierGrade => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate {
                    "IsNotCarrierGrade"
                } else {
                    "IsCarrierGrade"
                };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
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

            // ── LTree ID-based operators ──────────────────────────────────────
            WhereOperator::DescendantOfId | WhereOperator::AncestorOfId => {
                let ctx = hierarchy_ctx.ok_or_else(|| {
                    FraiseQLError::validation(
                        "descendantOfId/ancestorOfId requires HierarchyContext — \
                         configure [hierarchies] in fraiseql.toml"
                            .to_string(),
                    )
                })?;
                let pg_op = if matches!(operator, WhereOperator::DescendantOfId) {
                    "<@"
                } else {
                    "@>"
                };
                let p = self.push_param(params, value.clone());
                self.dialect
                    .ltree_id_subquery_sql(
                        pg_op,
                        &field_expr,
                        &ctx.table,
                        &ctx.path_column,
                        ctx.fk_column.as_deref(),
                        &p,
                    )
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
            #[allow(unreachable_patterns)]
            // Reason: defensive catch-all for future non_exhaustive variants
            _ => Err(FraiseQLError::Validation {
                message: format!(
                    "Operator {operator:?} is not supported by the {} dialect",
                    self.dialect.name()
                ),
                path: None,
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
mod tests;
