//! Parameterized WHERE and HAVING clause SQL generation.

use super::{
    AggregationSqlGenerator, DatabaseType, FactTableMetadata, FraiseQLError, Result,
    ValidatedHavingCondition, WhereClause, WhereOperator, to_snake_case,
};

impl AggregationSqlGenerator {
    /// Convert a [`WhereClause`] AST to parameterized SQL, appending bind values to `params`.
    pub(super) fn where_clause_to_sql_parameterized(
        &self,
        clause: &WhereClause,
        metadata: &FactTableMetadata,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        match clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => {
                let field_name = &path[0];
                let is_denormalized =
                    metadata.denormalized_filters.iter().any(|f| f.name == *field_name);
                if is_denormalized {
                    self.generate_direct_column_where_parameterized(
                        field_name, operator, value, params,
                    )
                } else {
                    let jsonb_column = &metadata.dimensions.name;
                    self.generate_jsonb_where_parameterized(
                        jsonb_column,
                        path,
                        operator,
                        value,
                        params,
                    )
                }
            },
            WhereClause::And(clauses) => {
                let conditions: Vec<String> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql_parameterized(c, metadata, params))
                    .collect::<Result<Vec<_>>>()?;
                Ok(format!("({})", conditions.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                let conditions: Vec<String> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql_parameterized(c, metadata, params))
                    .collect::<Result<Vec<_>>>()?;
                Ok(format!("({})", conditions.join(" OR ")))
            },
            WhereClause::Not(inner) => {
                let s = self.where_clause_to_sql_parameterized(inner, metadata, params)?;
                Ok(format!("NOT ({s})"))
            },
            WhereClause::NativeField {
                column,
                pg_cast,
                operator,
                value,
            } => {
                // Direct column reference (no JSONB extraction) for native SQL columns.
                // Use quote_identifier for dialect-correct quoting (MySQL backticks, SQL
                // Server brackets, PG/SQLite double-quotes).
                let col_ref = self.quote_identifier(column);
                // An ltree operator casts its own operand (`::ltree`, `::lquery`, …); the
                // column's cast suffix below would double it.
                if let Some(sql) = self.ltree_where(&col_ref, operator, value, params)? {
                    return Ok(sql);
                }
                let pre_len = params.len();
                let sql = self.generate_direct_column_where_parameterized(
                    &col_ref, operator, value, params,
                )?;

                // For PostgreSQL with a non-empty type cast, append ::type to the single
                // scalar placeholder that was just added.  IN / NOT IN push multiple params;
                // IsNull pushes none — neither needs a cast suffix here.
                if self.database_type == DatabaseType::PostgreSQL
                    && !pg_cast.is_empty()
                    && params.len() == pre_len + 1
                {
                    let ph = self.placeholder(pre_len);
                    Ok(sql.replace(&ph, &format!("{ph}::{pg_cast}")))
                } else {
                    Ok(sql)
                }
            },
            // Declared field types steer the JSONB-extraction casts in the
            // generic WHERE generator; this path builds its own SQL from the
            // fact-table metadata, so the annotation is transparent.
            WhereClause::Typed { inner, .. } => {
                self.where_clause_to_sql_parameterized(inner, metadata, params)
            },
            // Reason: non_exhaustive requires catch-all for cross-crate matches
            _ => Err(crate::FraiseQLError::Validation {
                message: "Unknown WhereClause variant".to_string(),
                path:    None,
            }),
        }
    }

    /// Parameterized WHERE for a denormalized (direct column) filter.
    pub(super) fn generate_direct_column_where_parameterized(
        &self,
        field: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        if let Some(test) = null_test(operator, value)? {
            return Ok(format!("{field} {test}"));
        }
        if let Some(sql) = self.ltree_where(field, operator, value, params)? {
            return Ok(sql);
        }

        let op_sql = self.operator_to_sql(operator)?;

        if matches!(operator, WhereOperator::In | WhereOperator::Nin) {
            let arr = value.as_array().ok_or_else(|| {
                FraiseQLError::validation("IN/NOT IN operators require array values")
            })?;
            let phs: Vec<String> = arr.iter().map(|v| self.emit_value_param(v, params)).collect();
            return Ok(format!("{field} {op_sql} ({})", phs.join(", ")));
        }

        if matches!(
            operator,
            WhereOperator::Contains
                | WhereOperator::Startswith
                | WhereOperator::Endswith
                | WhereOperator::Like
        ) {
            let s = value
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("LIKE operators require string values"))?;
            let (ph, needs_escape) = self.emit_like_pattern_param(operator, s, params);
            return if needs_escape {
                Ok(format!("{field} {op_sql} {ph} ESCAPE '!'"))
            } else {
                Ok(format!("{field} {op_sql} {ph}"))
            };
        }

        if operator.is_case_insensitive() {
            let s = value.as_str().ok_or_else(|| {
                FraiseQLError::validation("Case-insensitive operators require string values")
            })?;
            return self.generate_case_insensitive_where_parameterized(field, operator, s, params);
        }

        let ph = self.emit_value_param(value, params);
        Ok(format!("{field} {op_sql} {ph}"))
    }

    /// Parameterized WHERE for a JSONB dimension field.
    pub(super) fn generate_jsonb_where_parameterized(
        &self,
        jsonb_column: &str,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let field_path = &path[0];
        let db_field_path = to_snake_case(field_path);
        let jsonb_extract = self.jsonb_extract_sql(jsonb_column, &db_field_path);

        if let Some(test) = null_test(operator, value)? {
            return Ok(format!("{jsonb_extract} {test}"));
        }
        if let Some(sql) = self.ltree_where(&jsonb_extract, operator, value, params)? {
            return Ok(sql);
        }

        let op_sql = self.operator_to_sql(operator)?;

        if operator.is_case_insensitive() {
            let s = value.as_str().ok_or_else(|| {
                FraiseQLError::validation("Case-insensitive operators require string values")
            })?;
            return self.generate_case_insensitive_where_parameterized(
                &jsonb_extract,
                operator,
                s,
                params,
            );
        }

        if matches!(operator, WhereOperator::In | WhereOperator::Nin) {
            let arr = value.as_array().ok_or_else(|| {
                FraiseQLError::validation("IN/NOT IN operators require array values")
            })?;
            let phs: Vec<String> = arr.iter().map(|v| self.emit_value_param(v, params)).collect();
            return Ok(format!("{jsonb_extract} {op_sql} ({})", phs.join(", ")));
        }

        if matches!(
            operator,
            WhereOperator::Contains | WhereOperator::Startswith | WhereOperator::Endswith
        ) {
            let s = value
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("LIKE operators require string values"))?;
            // needs_escape is always true for semantic LIKE operators (Contains etc.)
            let (ph, _) = self.emit_like_pattern_param(operator, s, params);
            return Ok(format!("{jsonb_extract} {op_sql} {ph} ESCAPE '!'"));
        }

        let ph = self.emit_value_param(value, params);
        Ok(format!("{jsonb_extract} {op_sql} {ph}"))
    }

    /// SQL for an ltree operator on `lhs` (a column or a JSONB extraction), or `None` for
    /// any other operator.
    ///
    /// The SQL is the PostgreSQL dialect's, as the main WHERE generator emits it. This
    /// generator implemented no ltree operator, so `descendantOf` became `=` and later
    /// was refused (#1460).
    fn ltree_where(
        &self,
        lhs: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<Option<String>> {
        use fraiseql_db::{PostgresDialect, SqlDialect};

        let dialect = match self.database_type {
            DatabaseType::PostgreSQL => PostgresDialect,
        };
        let binary = |pg_op: &str, rhs_type: &str, params: &mut Vec<serde_json::Value>| {
            let ph = self.emit_value_param(value, params);
            dialect.ltree_binary_sql(pg_op, lhs, &ph, rhs_type)
        };
        let depth = |op: &str, params: &mut Vec<serde_json::Value>| {
            let ph = self.emit_value_param(value, params);
            dialect.ltree_depth_sql(op, lhs, &ph)
        };
        let list = |cast: &str, params: &mut Vec<serde_json::Value>| -> Result<Vec<String>> {
            let items = value.as_array().filter(|a| !a.is_empty()).ok_or_else(|| {
                FraiseQLError::validation(format!("{operator:?} requires a non-empty array"))
            })?;
            Ok(items
                .iter()
                .map(|v| format!("{}::{cast}", self.emit_value_param(v, params)))
                .collect())
        };
        let sql = match operator {
            WhereOperator::AncestorOf => binary("@>", "ltree", params),
            WhereOperator::DescendantOf => binary("<@", "ltree", params),
            WhereOperator::MatchesLquery => binary("~", "lquery", params),
            WhereOperator::MatchesLtxtquery => binary("@", "ltxtquery", params),
            WhereOperator::MatchesAnyLquery => {
                dialect.ltree_any_lquery_sql(lhs, &list("lquery", params)?)
            },
            WhereOperator::Lca => dialect.ltree_lca_sql(lhs, &list("ltree", params)?),
            WhereOperator::DepthEq => depth("=", params),
            WhereOperator::DepthNeq => depth("!=", params),
            WhereOperator::DepthGt => depth(">", params),
            WhereOperator::DepthGte => depth(">=", params),
            WhereOperator::DepthLt => depth("<", params),
            WhereOperator::DepthLte => depth("<=", params),
            WhereOperator::DescendantOfId | WhereOperator::AncestorOfId => {
                return Err(FraiseQLError::validation(format!(
                    "{operator:?} resolves a node id through a declared hierarchy, which a \
                     fact-table aggregate filter does not support; filter on the path with \
                     descendant_of / ancestor_of"
                )));
            },
            _ => return Ok(None),
        };
        sql.map(Some).map_err(|e| FraiseQLError::validation(e.to_string()))
    }

    /// Parameterized case-insensitive WHERE (ILIKE for PostgreSQL, `UPPER()` for others).
    pub(super) fn generate_case_insensitive_where_parameterized(
        &self,
        column: &str,
        operator: &WhereOperator,
        value_str: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let op = self.operator_to_sql(operator)?;
        if self.database_type == DatabaseType::PostgreSQL {
            let (ph, needs_escape) = self.emit_like_pattern_param(operator, value_str, params);
            Ok(if needs_escape {
                format!("{column} {op} {ph} ESCAPE '!'")
            } else {
                format!("{column} {op} {ph}")
            })
        } else {
            let upper = value_str.to_uppercase();
            let (ph, needs_escape) = self.emit_like_pattern_param(operator, &upper, params);
            Ok(if needs_escape {
                format!("UPPER({column}) LIKE {ph} ESCAPE '!'")
            } else {
                format!("UPPER({column}) LIKE {ph}")
            })
        }
    }

    /// Build a parameterized `WHERE …` clause, or an empty string if the clause is empty.
    ///
    /// # Errors
    ///
    /// Returns an error if WHERE clause generation fails.
    pub fn build_where_clause_parameterized(
        &self,
        where_clause: &WhereClause,
        metadata: &FactTableMetadata,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        if where_clause.is_empty() {
            return Ok(String::new());
        }
        let cond = self.where_clause_to_sql_parameterized(where_clause, metadata, params)?;
        Ok(format!("WHERE {cond}"))
    }

    /// Build a parameterized `HAVING …` clause.
    ///
    /// # Errors
    ///
    /// Returns an error if HAVING clause generation fails.
    pub(super) fn build_having_clause_parameterized(
        &self,
        having_conditions: &[ValidatedHavingCondition],
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        if having_conditions.is_empty() {
            return Ok(String::new());
        }
        let mut conditions = Vec::new();
        for condition in having_conditions {
            let aggregate_sql = self.aggregate_expression_to_sql(&condition.aggregate)?;
            let operator_sql = condition.operator.sql_operator();
            let value_sql = self.emit_value_param(&condition.value, params);
            conditions.push(format!("{aggregate_sql} {operator_sql} {value_sql}"));
        }
        Ok(format!("HAVING {}", conditions.join(" AND ")))
    }
}

/// `IS NULL` / `IS NOT NULL` for a null-test operator, following its operand; `None` for any
/// other operator.
///
/// The operand decides, as in the main WHERE generator (#828): `isnull: false` asks for the
/// rows that are NOT null. This path used to emit `IS NULL` whatever the operand said, and sent
/// `IsNotNull` through the operator table, where it became `=` (#1460).
fn null_test(operator: &WhereOperator, value: &serde_json::Value) -> Result<Option<&'static str>> {
    let negated = match operator {
        WhereOperator::IsNull => false,
        WhereOperator::IsNotNull => true,
        _ => return Ok(None),
    };
    let asserted = match value {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Null => true,
        other => {
            return Err(FraiseQLError::validation(format!(
                "{operator:?} takes a boolean operand, got {other}"
            )));
        },
    };
    Ok(Some(if asserted == negated {
        "IS NOT NULL"
    } else {
        "IS NULL"
    }))
}
