//! Generic WHERE clause generator parameterised over a SQL dialect.

use std::{collections::HashSet, sync::Arc};

use fraiseql_error::{FraiseQLError, Result};

use super::counter::ParamCounter;
use crate::{
    dialect::SqlDialect,
    where_clause::{FieldTypeMap, WhereClause, WhereOperator, field_types::operand_type},
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
            path:    None,
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
                            path:    None,
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
        let sql = self.visit_impl(clause, &mut params, Some(hierarchy_ctx), None)?;
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
        self.visit_impl(clause, params, None, None)
    }

    fn visit_impl(
        &self,
        clause: &WhereClause,
        params: &mut Vec<serde_json::Value>,
        hierarchy_ctx: Option<&super::HierarchyContext>,
        types: Option<&FieldTypeMap>,
    ) -> Result<String> {
        match clause {
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok(self.dialect.always_true().to_string());
                }
                let parts: Result<Vec<_>> = clauses
                    .iter()
                    .map(|c| self.visit_impl(c, params, hierarchy_ctx, types))
                    .collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok(self.dialect.always_false().to_string());
                }
                let parts: Result<Vec<_>> = clauses
                    .iter()
                    .map(|c| self.visit_impl(c, params, hierarchy_ctx, types))
                    .collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(inner) => {
                Ok(format!("NOT ({})", self.visit_impl(inner, params, hierarchy_ctx, types)?))
            },
            WhereClause::Typed {
                types: subtree_types,
                inner,
            } => self.visit_impl(inner, params, hierarchy_ctx, Some(subtree_types)),
            WhereClause::Field {
                path,
                operator,
                value,
            } => self.visit_field(path, operator, value, params, hierarchy_ctx, types),
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

    /// Vector threshold predicate (#386): `((expr)::vector <op> $v::vector) <= $t`.
    ///
    /// Operand shape: `{vector: [Float, …], threshold: Float}`.
    ///
    /// - For the distance operators (`<=>`, `<->`, `<+>`), `threshold` is the **maximum distance**:
    ///   rows within it (inclusive) match.
    /// - For inner product, `threshold` is the **minimum (raw) inner product**: rows at least that
    ///   similar match. pgvector's `<#>` returns the *negated* inner product, so the comparison
    ///   stays `<=` and the bound threshold value is negated instead (`ip >= t  ⇔  -ip <= -t`).
    ///
    /// The query vector binds as a **text** parameter (`$n::vector` — jsonb has
    /// no cast to vector), built from the operand's numbers, so its dimension
    /// and syntax are checked by pgvector itself. The field expression is
    /// parenthesised before the cast: `data->>'f'::vector` would parse as
    /// `data->>('f'::vector)` because `::` binds tighter than `->>`.
    fn vector_threshold_sql(
        &self,
        field_expr: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        negate_threshold: bool,
    ) -> Result<String> {
        let shape_err = || {
            FraiseQLError::validation(
                "vector operators take {vector: [Float, ...], threshold: Float} — e.g. \
                 {cosine_distance: {vector: [0.1, 0.2], threshold: 0.5}}",
            )
        };
        let obj = value.as_object().ok_or_else(shape_err)?;
        let operand = obj.get("vector").ok_or_else(shape_err)?;
        let threshold =
            obj.get("threshold").and_then(serde_json::Value::as_f64).ok_or_else(shape_err)?;
        if !threshold.is_finite() {
            return Err(FraiseQLError::validation("vector threshold must be a finite number"));
        }
        // A sparse operand names itself: pgvector's `{1:0.5,7:0.25}/1000` text
        // form is a string where a dense vector is an array, and casting it to
        // `::vector` is an input-syntax error rather than a wrong answer. The
        // shape of the operand picks the cast, exactly as it does for the
        // binary operators (#959).
        if let Some(sparse) = operand.as_str() {
            return self.sparse_threshold_sql(
                field_expr,
                op,
                sparse,
                threshold,
                params,
                negate_threshold,
            );
        }
        let vector = operand.as_array().ok_or_else(shape_err)?;
        if vector.is_empty() {
            return Err(FraiseQLError::validation("vector operand must not be empty"));
        }
        let mut literal = String::with_capacity(vector.len() * 8 + 2);
        literal.push('[');
        for (i, component) in vector.iter().enumerate() {
            let n = component.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                FraiseQLError::validation("vector operand components must all be finite numbers")
            })?;
            if i > 0 {
                literal.push(',');
            }
            // Reason: fmt::Write for String is infallible
            std::fmt::Write::write_fmt(&mut literal, format_args!("{n}"))
                .expect("write to String is infallible");
        }
        literal.push(']');

        let vector_param = self.push_param(params, serde_json::Value::String(literal));
        let bound = if negate_threshold {
            -threshold
        } else {
            threshold
        };
        let threshold_param = self.push_param(params, serde_json::json!(bound));
        // `$n::text::vector`, not `$n::vector`: a bare `::vector` cast makes the
        // prepared-statement protocol infer the *parameter type* as vector, and
        // the driver binds text — a wire-format mismatch. The double cast pins
        // the parameter as text and converts server-side.
        // The threshold likewise goes through text (`::text::float8`): this
        // crate's parameter convention sends JSON numbers as text (see
        // `QueryParam::from`), so the placeholder must be inferred as text.
        Ok(format!(
            "(({field_expr})::vector {op} {vector_param}::text::vector) <= \
             {threshold_param}::text::float8"
        ))
    }

    /// Sparse-vector threshold predicate (#959):
    /// `((expr)::sparsevec <=> $v::text::sparsevec) <= $t`.
    ///
    /// The operand is pgvector's own text form, `{index:value,…}/dimensions`
    /// with 1-based indices. Its character set is re-validated here before it
    /// binds, as it is for every other vector literal — it reaches the server as
    /// a parameter, and the cast is what gives it a type.
    ///
    /// A dimension disagreement is `different sparsevec dimensions 5 and 4` from
    /// the server, which is a refusal rather than a plausible number, so no
    /// length check is needed on this path.
    ///
    /// `negate_threshold` carries the inner-product convention through
    /// unchanged: pgvector's `<#>` returns the negated inner product on a sparse
    /// vector exactly as it does on a dense one, so the bound value is negated
    /// here too. Dropping it would make `inner_product` mean its own opposite on
    /// this one operand shape.
    #[allow(clippy::too_many_arguments)] // Reason: mirrors `vector_threshold_sql`'s operands
    fn sparse_threshold_sql(
        &self,
        field_expr: &str,
        op: &str,
        sparse: &str,
        threshold: f64,
        params: &mut Vec<serde_json::Value>,
        negate_threshold: bool,
    ) -> Result<String> {
        let valid = sparse.starts_with('{')
            && sparse.contains("}/")
            && sparse.chars().all(|c| {
                c.is_ascii_digit()
                    || matches!(c, '{' | '}' | '/' | ':' | ',' | '.' | '-' | '+' | 'e' | 'E')
            });
        if !valid {
            return Err(FraiseQLError::validation(
                "a sparse vector operand is pgvector's `{index:value,...}/dimensions` form — \
                 e.g. {cosine_distance: {vector: \"{1:0.5,7:0.25}/1000\", threshold: 0.25}}",
            ));
        }
        let vector_param = self.push_param(params, serde_json::Value::String(sparse.to_string()));
        let bound = if negate_threshold {
            -threshold
        } else {
            threshold
        };
        let threshold_param = self.push_param(params, serde_json::json!(bound));
        Ok(format!(
            "(({field_expr})::sparsevec {op} {vector_param}::text::sparsevec) <= \
             {threshold_param}::text::float8"
        ))
    }

    /// Binary-vector threshold predicate (#959):
    /// `((expr)::varbit <~> $v::text::varbit) <= $t`.
    ///
    /// Operand shape: `{vector: "1011…", threshold: Float}` — the query vector
    /// is the text form of a `bit(N)` value, which is what a `BitVector` field
    /// carries in the JSONB payload and what `binary_quantize(…)::bit(N)`
    /// produces. `threshold` is the maximum distance, inclusive, as for the
    /// float metrics: hamming counts differing bits, jaccard is `1 -
    /// |intersection| / |union|` over set bits.
    ///
    /// Both sides are cast to `varbit`, never to `bit`: `'1011'::bit` is
    /// `bit(1)`, so the length-less cast would compare the first bit of the
    /// column against the first bit of the operand and answer *0* for every
    /// row that happens to start the same way. `varbit` keeps each side's own
    /// width, and pgvector then refuses a width disagreement with
    /// `different bit lengths 8 and 4` instead of returning a plausible number.
    fn bit_threshold_sql(
        &self,
        field_expr: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let shape_err = || {
            FraiseQLError::validation(
                "binary vector operators take {vector: \"1011…\", threshold: Float} — e.g. \
                 {hamming_distance: {vector: \"11110000\", threshold: 2}}",
            )
        };
        let obj = value.as_object().ok_or_else(shape_err)?;
        let bits = obj.get("vector").and_then(serde_json::Value::as_str).ok_or_else(shape_err)?;
        let threshold =
            obj.get("threshold").and_then(serde_json::Value::as_f64).ok_or_else(shape_err)?;
        if !threshold.is_finite() {
            return Err(FraiseQLError::validation("vector threshold must be a finite number"));
        }
        if bits.is_empty() || !bits.chars().all(|c| matches!(c, '0' | '1')) {
            return Err(FraiseQLError::validation(
                "a binary vector operand must be a non-empty run of '0' and '1' characters",
            ));
        }

        let vector_param = self.push_param(params, serde_json::Value::String(bits.to_string()));
        let threshold_param = self.push_param(params, serde_json::json!(threshold));
        // `$n::text::varbit` for the same reason `::text::vector` is used on the
        // float path: a bare `::varbit` cast makes the prepared-statement
        // protocol infer the parameter type as varbit while the driver binds
        // text.
        Ok(format!(
            "(({field_expr})::varbit {op} {vector_param}::text::varbit) <= \
             {threshold_param}::text::float8"
        ))
    }

    // ── Field visitor ─────────────────────────────────────────────────────────

    fn visit_field(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        hierarchy_ctx: Option<&super::HierarchyContext>,
        types: Option<&FieldTypeMap>,
    ) -> Result<String> {
        let field_expr = self.resolve_field_expr(path);

        match operator {
            // ── Comparison ────────────────────────────────────────────────────
            // One cast decision for every scalar comparison. Splitting it per
            // operator is what left `Gt`/`Gte`/`Lt`/`Lte` casting unconditionally
            // to numeric (#798) while `In`/`Nin` cast not at all (#800).
            WhereOperator::Eq
            | WhereOperator::Neq
            | WhereOperator::Gt
            | WhereOperator::Gte
            | WhereOperator::Lt
            | WhereOperator::Lte => {
                let op = match operator {
                    WhereOperator::Eq => "=",
                    WhereOperator::Neq => self.dialect.neq_operator(),
                    WhereOperator::Gt => ">",
                    WhereOperator::Gte => ">=",
                    WhereOperator::Lt => "<",
                    _ => "<=",
                };
                let ty = operand_type(types, path, value);
                let lhs = self.dialect.cast_expr_as(&field_expr, ty);
                let p = self.push_param(params, value.clone());
                let rhs = self.dialect.cast_param_as(&p, ty);
                Ok(format!("{lhs} {op} {rhs}"))
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
                let ty = operand_type(types, path, value);
                let lhs = self.dialect.cast_expr_as(&field_expr, ty);
                let placeholders: Vec<_> = arr
                    .iter()
                    .map(|v| {
                        let p = self.push_param(params, v.clone());
                        self.dialect.cast_param_as(&p, ty).into_owned()
                    })
                    .collect();
                let in_list = placeholders.join(", ");
                let sql = format!("{lhs} IN ({in_list})");
                Ok(if matches!(operator, WhereOperator::Nin) {
                    format!("NOT ({sql})")
                } else {
                    sql
                })
            },

            // ── NULL ──────────────────────────────────────────────────────────
            WhereOperator::IsNull | WhereOperator::IsNotNull => {
                let negated = matches!(operator, WhereOperator::IsNotNull);
                // A non-boolean operand is a client error, not a licence to
                // pick a default: `?deletedAt[is_null]=false` coerced to the
                // field's declared type used to silently mean IS NULL (#828).
                let asserted = match value {
                    serde_json::Value::Bool(b) => *b,
                    serde_json::Value::Null => true,
                    other => {
                        return Err(FraiseQLError::validation(format!(
                            "{operator:?} requires a boolean operand, got {other}"
                        )));
                    },
                };
                let null_op = if asserted != negated {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
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

            // ── Vector (pgvector) threshold predicates (#386) ─────────────────
            // Operand: `{vector: [Float,…], threshold: Float}`; semantics and
            // the three repairs over the original never-executable emission are
            // documented on `vector_threshold_sql`.
            WhereOperator::CosineDistance => {
                self.vector_threshold_sql(&field_expr, "<=>", value, params, false)
            },
            WhereOperator::L2Distance => {
                self.vector_threshold_sql(&field_expr, "<->", value, params, false)
            },
            WhereOperator::L1Distance => {
                self.vector_threshold_sql(&field_expr, "<+>", value, params, false)
            },
            WhereOperator::InnerProduct => {
                self.vector_threshold_sql(&field_expr, "<#>", value, params, true)
            },
            // Hamming and Jaccard operate on pgvector's *binary* (`bit`)
            // vectors, which a `BitVector` field declares (#959).
            WhereOperator::HammingDistance => {
                self.bit_threshold_sql(&field_expr, "<~>", value, params)
            },
            WhereOperator::JaccardDistance => {
                self.bit_threshold_sql(&field_expr, "<%>", value, params)
            },

            // ── Network (INET/CIDR) ───────────────────────────────────────────
            WhereOperator::IsIPv4 => {
                // Value controls negation, like every sibling INET operator:
                // `is_ipv4: false` asks to EXCLUDE IPv4 (#870.2).
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate { "IsNotIPv4" } else { "IsIPv4" };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
            WhereOperator::IsIPv6 => {
                let negate = value.as_bool().is_some_and(|v| !v);
                let check_name = if negate { "IsNotIPv6" } else { "IsIPv6" };
                self.dialect
                    .inet_check_sql(&field_expr, check_name)
                    .map_err(|e| FraiseQLError::validation(e.to_string()))
            },
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

#[cfg(test)]
mod tests;
