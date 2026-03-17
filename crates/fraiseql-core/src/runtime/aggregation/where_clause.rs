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
            // Reason: non_exhaustive requires catch-all for cross-crate matches
            _ => Err(crate::FraiseQLError::Validation {
                message: "Unknown WhereClause variant".to_string(),
                path: None,
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
        if matches!(operator, WhereOperator::IsNull) {
            return Ok(format!("{field} IS NULL"));
        }

        let op_sql = self.operator_to_sql(operator);

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
        let op_sql = self.operator_to_sql(operator);

        if matches!(operator, WhereOperator::IsNull) {
            return Ok(format!("{jsonb_extract} IS NULL"));
        }

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

    /// Parameterized case-insensitive WHERE (ILIKE for PostgreSQL, UPPER() for others).
    pub(super) fn generate_case_insensitive_where_parameterized(
        &self,
        column: &str,
        operator: &WhereOperator,
        value_str: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let op = self.operator_to_sql(operator);
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
