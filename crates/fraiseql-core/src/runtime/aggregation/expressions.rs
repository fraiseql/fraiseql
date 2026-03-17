//! SELECT, GROUP BY, and aggregate expression SQL generation.

use super::{
    AggregateExpression, AggregateFunction, AggregationSqlGenerator, DatabaseType,
    GroupByExpression, OrderByClause, OrderDirection, Result, TemporalBucket,
};

impl AggregationSqlGenerator {
    /// Build SELECT clause.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if an unrecognised aggregate function
    /// or GROUP BY expression cannot be converted to SQL.
    pub(super) fn build_select_clause(
        &self,
        group_by_expressions: &[GroupByExpression],
        aggregate_expressions: &[AggregateExpression],
    ) -> Result<String> {
        let mut columns = Vec::new();

        // Add GROUP BY columns to SELECT
        for expr in group_by_expressions {
            let column = self.group_by_expression_to_sql(expr)?;
            let alias = match expr {
                GroupByExpression::JsonbPath { alias, .. }
                | GroupByExpression::TemporalBucket { alias, .. }
                | GroupByExpression::CalendarPath { alias, .. } => alias,
            };
            columns.push(format!("{} AS {}", column, alias));
        }

        // Add aggregate columns to SELECT
        for expr in aggregate_expressions {
            let column = self.aggregate_expression_to_sql(expr)?;
            let alias = match expr {
                AggregateExpression::Count { alias }
                | AggregateExpression::CountDistinct { alias, .. }
                | AggregateExpression::MeasureAggregate { alias, .. }
                | AggregateExpression::AdvancedAggregate { alias, .. }
                | AggregateExpression::BoolAggregate { alias, .. } => alias,
            };
            columns.push(format!("{} AS {}", column, alias));
        }

        Ok(format!("SELECT\n  {}", columns.join(",\n  ")))
    }

    /// Convert GROUP BY expression to SQL.
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for future expression types that may
    /// require validation (e.g., computed GROUP BY expressions).
    pub(super) fn group_by_expression_to_sql(&self, expr: &GroupByExpression) -> Result<String> {
        match expr {
            GroupByExpression::JsonbPath {
                jsonb_column, path, ..
            } => Ok(self.jsonb_extract_sql(jsonb_column, path)),
            GroupByExpression::TemporalBucket { column, bucket, .. } => {
                Ok(self.temporal_bucket_sql(column, *bucket))
            },
            GroupByExpression::CalendarPath {
                calendar_column,
                json_key,
                ..
            } => {
                // Calendar dimension: reuse JSONB extraction for all 4 databases
                Ok(self.jsonb_extract_sql(calendar_column, json_key))
            },
        }
    }

    /// Generate temporal bucket SQL
    pub(super) fn temporal_bucket_sql(&self, column: &str, bucket: TemporalBucket) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                format!("DATE_TRUNC('{}', {})", bucket.postgres_arg(), column)
            },
            DatabaseType::MySQL => {
                let format = match bucket {
                    TemporalBucket::Second => "%Y-%m-%d %H:%i:%s",
                    TemporalBucket::Minute => "%Y-%m-%d %H:%i:00",
                    TemporalBucket::Hour => "%Y-%m-%d %H:00:00",
                    TemporalBucket::Day => "%Y-%m-%d",
                    TemporalBucket::Week => "%Y-%u",
                    TemporalBucket::Month => "%Y-%m",
                    TemporalBucket::Quarter => "%Y-Q%q",
                    TemporalBucket::Year => "%Y",
                };
                format!("DATE_FORMAT({}, '{}')", column, format)
            },
            DatabaseType::SQLite => {
                let format = match bucket {
                    TemporalBucket::Second => "%Y-%m-%d %H:%M:%S",
                    TemporalBucket::Minute => "%Y-%m-%d %H:%M:00",
                    TemporalBucket::Hour => "%Y-%m-%d %H:00:00",
                    TemporalBucket::Day => "%Y-%m-%d",
                    TemporalBucket::Week => "%Y-W%W",
                    TemporalBucket::Month => "%Y-%m",
                    TemporalBucket::Quarter => "%Y-Q",
                    TemporalBucket::Year => "%Y",
                };
                format!("strftime('{}', {})", format, column)
            },
            DatabaseType::SQLServer => {
                let datepart = match bucket {
                    TemporalBucket::Second => "second",
                    TemporalBucket::Minute => "minute",
                    TemporalBucket::Hour => "hour",
                    TemporalBucket::Day => "day",
                    TemporalBucket::Week => "week",
                    TemporalBucket::Month => "month",
                    TemporalBucket::Quarter => "quarter",
                    TemporalBucket::Year => "year",
                };
                // SQL Server doesn't have DATE_TRUNC, use DATEADD/DATEDIFF pattern
                match bucket {
                    TemporalBucket::Day => format!("CAST({} AS DATE)", column),
                    TemporalBucket::Month => {
                        format!("DATEADD(month, DATEDIFF(month, 0, {}), 0)", column)
                    },
                    TemporalBucket::Year => {
                        format!("DATEADD(year, DATEDIFF(year, 0, {}), 0)", column)
                    },
                    _ => format!("DATEPART({}, {})", datepart, column),
                }
            },
        }
    }

    /// Convert aggregate expression to SQL.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` when an advanced aggregate function
    /// (e.g., `STRING_AGG`) is not supported by the target database dialect.
    pub(super) fn aggregate_expression_to_sql(&self, expr: &AggregateExpression) -> Result<String> {
        match expr {
            AggregateExpression::Count { .. } => Ok("COUNT(*)".to_string()),
            AggregateExpression::CountDistinct { column, .. } => {
                Ok(format!("COUNT(DISTINCT {})", column))
            },
            AggregateExpression::MeasureAggregate {
                column, function, ..
            } => {
                // Handle statistical functions with database-specific SQL
                #[allow(clippy::enum_glob_use)]  // Reason: enum glob use in test/match context for readability
                // Reason: glob import reduces noise in exhaustive match arms
                use AggregateFunction::*;
                match function {
                    Stddev => Ok(self.generate_stddev_sql(column)),
                    Variance => Ok(self.generate_variance_sql(column)),
                    _ => Ok(format!("{}({})", function.sql_name(), column)),
                }
            },
            AggregateExpression::AdvancedAggregate {
                column,
                function,
                delimiter,
                order_by,
                ..
            } => self.advanced_aggregate_to_sql(
                column,
                *function,
                delimiter.as_deref(),
                order_by.as_ref(),
            ),
            AggregateExpression::BoolAggregate {
                column, function, ..
            } => Ok(self.generate_bool_agg_sql(column, *function)),
        }
    }

    /// Generate SQL for advanced aggregates
    pub(super) fn advanced_aggregate_to_sql(
        &self,
        column: &str,
        function: AggregateFunction,
        delimiter: Option<&str>,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> Result<String> {
        #[allow(clippy::enum_glob_use)]  // Reason: enum glob use in test/match context for readability
        // Reason: glob import reduces noise in exhaustive match arms
        use AggregateFunction::*;

        match function {
            ArrayAgg => Ok(self.generate_array_agg_sql(column, order_by)),
            JsonAgg => Ok(self.generate_json_agg_sql(column, order_by)),
            JsonbAgg => Ok(self.generate_jsonb_agg_sql(column, order_by)),
            StringAgg => {
                Ok(self.generate_string_agg_sql(column, delimiter.unwrap_or(","), order_by))
            },
            _ => Ok(format!("{}({})", function.sql_name(), column)),
        }
    }

    /// Generate ARRAY_AGG SQL
    pub(super) fn generate_array_agg_sql(
        &self,
        column: &str,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("ARRAY_AGG({} ORDER BY {})", column, self.order_by_to_sql(order))
                } else {
                    format!("ARRAY_AGG({})", column)
                }
            },
            DatabaseType::MySQL => {
                // MySQL doesn't have ARRAY_AGG, use JSON_ARRAYAGG
                format!("JSON_ARRAYAGG({})", column)
            },
            DatabaseType::SQLite => {
                // SQLite: emulate with GROUP_CONCAT, wrap in JSON array syntax
                format!("'[' || GROUP_CONCAT('\"' || {} || '\"', ',') || ']'", column)
            },
            DatabaseType::SQLServer => {
                // SQL Server: use STRING_AGG and wrap in JSON array
                format!(
                    "'[' + STRING_AGG('\"' + CAST({} AS NVARCHAR(MAX)) + '\"', ',') + ']'",
                    column
                )
            },
        }
    }

    /// Generate JSON_AGG SQL
    pub(super) fn generate_json_agg_sql(
        &self,
        column: &str,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("JSON_AGG({} ORDER BY {})", column, self.order_by_to_sql(order))
                } else {
                    format!("JSON_AGG({})", column)
                }
            },
            DatabaseType::MySQL => {
                // MySQL: JSON_ARRAYAGG for arrays
                format!("JSON_ARRAYAGG({})", column)
            },
            DatabaseType::SQLite => {
                // SQLite: limited JSON support
                format!("JSON_ARRAY({})", column)
            },
            DatabaseType::SQLServer => {
                // SQL Server: FOR JSON PATH
                format!("(SELECT {} FOR JSON PATH)", column)
            },
        }
    }

    /// Generate JSONB_AGG SQL (PostgreSQL-specific)
    pub(super) fn generate_jsonb_agg_sql(
        &self,
        column: &str,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("JSONB_AGG({} ORDER BY {})", column, self.order_by_to_sql(order))
                } else {
                    format!("JSONB_AGG({})", column)
                }
            },
            // Fall back to JSON_AGG for other databases
            _ => self.generate_json_agg_sql(column, order_by),
        }
    }

    /// Generate STRING_AGG SQL
    pub(super) fn generate_string_agg_sql(
        &self,
        column: &str,
        delimiter: &str,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> String {
        // Escape the delimiter to prevent SQL injection via single-quote sequences.
        // A delimiter like "O'Reilly" would otherwise break out of the string literal.
        let safe_delimiter = self.escape_sql_string(delimiter);
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!(
                        "STRING_AGG({}, '{}' ORDER BY {})",
                        column,
                        safe_delimiter,
                        self.order_by_to_sql(order)
                    )
                } else {
                    format!("STRING_AGG({}, '{}')", column, safe_delimiter)
                }
            },
            DatabaseType::MySQL => {
                let mut sql = format!("GROUP_CONCAT({}", column);
                if let Some(order) = order_by {
                    sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
                }
                sql.push_str(&format!(" SEPARATOR '{}')", safe_delimiter));
                sql
            },
            DatabaseType::SQLite => {
                // SQLite GROUP_CONCAT doesn't support ORDER BY in older versions
                format!("GROUP_CONCAT({}, '{}')", column, safe_delimiter)
            },
            DatabaseType::SQLServer => {
                let mut sql =
                    format!("STRING_AGG(CAST({} AS NVARCHAR(MAX)), '{}')", column, safe_delimiter);
                if let Some(order) = order_by {
                    sql.push_str(&format!(
                        " WITHIN GROUP (ORDER BY {})",
                        self.order_by_to_sql(order)
                    ));
                }
                sql
            },
        }
    }

    /// Convert ORDER BY clauses to SQL
    pub(super) fn order_by_to_sql(&self, order_by: &[OrderByClause]) -> String {
        order_by
            .iter()
            .map(|clause| {
                let direction = match clause.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                    // Reason: non_exhaustive requires catch-all for cross-crate matches
                    _ => "ASC",
                };
                format!("{} {}", self.quote_identifier(&clause.field), direction)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Generate STDDEV SQL (database-specific)
    ///
    /// Database support:
    /// - PostgreSQL: STDDEV_SAMP() (default), STDDEV_POP() also available
    /// - MySQL: STDDEV_SAMP() or STD()
    /// - SQLite: Not natively supported (returns NULL or use custom function)
    /// - SQL Server: STDEV()
    pub(super) fn generate_stddev_sql(&self, column: &str) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => format!("STDDEV_SAMP({})", column),
            DatabaseType::MySQL => format!("STDDEV_SAMP({})", column),
            DatabaseType::SQLite => {
                // SQLite doesn't have built-in STDDEV
                // Return NULL to indicate unavailable
                "NULL /* STDDEV not supported in SQLite */".to_string()
            },
            DatabaseType::SQLServer => format!("STDEV({})", column),
        }
    }

    /// Generate VARIANCE SQL (database-specific)
    ///
    /// Database support:
    /// - PostgreSQL: VAR_SAMP() (default), VAR_POP() also available
    /// - MySQL: VAR_SAMP() or VARIANCE()
    /// - SQLite: Not natively supported (returns NULL or use custom function)
    /// - SQL Server: VAR()
    pub(super) fn generate_variance_sql(&self, column: &str) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => format!("VAR_SAMP({})", column),
            DatabaseType::MySQL => format!("VAR_SAMP({})", column),
            DatabaseType::SQLite => {
                // SQLite doesn't have built-in VARIANCE
                // Return NULL to indicate unavailable
                "NULL /* VARIANCE not supported in SQLite */".to_string()
            },
            DatabaseType::SQLServer => format!("VAR({})", column),
        }
    }

    /// Generate BOOL_AND/BOOL_OR SQL
    pub(super) fn generate_bool_agg_sql(
        &self,
        column: &str,
        function: crate::compiler::aggregate_types::BoolAggregateFunction,
    ) -> String {
        use crate::compiler::aggregate_types::BoolAggregateFunction;

        match self.database_type {
            DatabaseType::PostgreSQL => {
                // PostgreSQL has native BOOL_AND/BOOL_OR
                format!("{}({})", function.sql_name(), column)
            },
            DatabaseType::MySQL | DatabaseType::SQLite => {
                // MySQL/SQLite: emulate with MIN/MAX on boolean as integer (0/1)
                match function {
                    BoolAggregateFunction::And => format!("MIN({}) = 1", column),
                    BoolAggregateFunction::Or => format!("MAX({}) = 1", column),
                }
            },
            DatabaseType::SQLServer => {
                // SQL Server: emulate with MIN/MAX on CAST to BIT
                match function {
                    BoolAggregateFunction::And => format!("MIN(CAST({} AS BIT)) = 1", column),
                    BoolAggregateFunction::Or => format!("MAX(CAST({} AS BIT)) = 1", column),
                }
            },
        }
    }

    /// Build GROUP BY clause.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if any GROUP BY expression cannot
    /// be converted to SQL (e.g., unsupported temporal bucket on the current dialect).
    pub(super) fn build_group_by_clause(
        &self,
        group_by_expressions: &[GroupByExpression],
    ) -> Result<String> {
        let mut columns = Vec::new();

        for expr in group_by_expressions {
            let column = self.group_by_expression_to_sql(expr)?;
            columns.push(column);
        }

        Ok(format!("GROUP BY {}", columns.join(", ")))
    }

    /// Build ORDER BY clause.
    ///
    /// # Errors
    ///
    /// Currently infallible; reserved for future validation of column references
    /// against the aggregation plan.
    pub(super) fn build_order_by_clause(&self, order_by: &[OrderByClause]) -> Result<String> {
        let clauses: Vec<String> = order_by
            .iter()
            .map(|clause| {
                let direction = match clause.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                    // Reason: non_exhaustive requires catch-all for cross-crate matches
                    _ => "ASC",
                };
                format!("{} {}", self.quote_identifier(&clause.field), direction)
            })
            .collect();

        Ok(format!("ORDER BY {}", clauses.join(", ")))
    }
}
