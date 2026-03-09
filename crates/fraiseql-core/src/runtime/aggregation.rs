//! Runtime Aggregation SQL Generation Module
//!
//! This module generates database-specific SQL from aggregation execution plans.
//!
//! # Database-Specific SQL
//!
//! ## PostgreSQL
//! ```sql
//! SELECT
//!   data->>'category' AS category,
//!   DATE_TRUNC('day', occurred_at) AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = $1
//! GROUP BY data->>'category', DATE_TRUNC('day', occurred_at)
//! HAVING SUM(revenue) > $2
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```
//!
//! ## MySQL
//! ```sql
//! SELECT
//!   JSON_UNQUOTE(JSON_EXTRACT(data, '$.category')) AS category,
//!   DATE_FORMAT(occurred_at, '%Y-%m-%d') AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = ?
//! GROUP BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.category')), DATE_FORMAT(occurred_at, '%Y-%m-%d')
//! HAVING SUM(revenue) > ?
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```
//!
//! ## SQLite
//! ```sql
//! SELECT
//!   json_extract(data, '$.category') AS category,
//!   strftime('%Y-%m-%d', occurred_at) AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = ?
//! GROUP BY json_extract(data, '$.category'), strftime('%Y-%m-%d', occurred_at)
//! HAVING SUM(revenue) > ?
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```

use crate::{
    compiler::{
        aggregate_types::{AggregateFunction, TemporalBucket},
        aggregation::{
            AggregateExpression, AggregationPlan, GroupByExpression, OrderByClause, OrderDirection,
            ValidatedHavingCondition,
        },
        fact_table::FactTableMetadata,
    },
    db::{
        identifier::{
            quote_mysql_identifier, quote_postgres_identifier, quote_sqlserver_identifier,
        },
        path_escape::{
            escape_mysql_json_path, escape_postgres_jsonb_segment, escape_sqlite_json_path,
            escape_sqlserver_json_path,
        },
        types::DatabaseType,
        where_clause::{WhereClause, WhereOperator},
    },
    error::{FraiseQLError, Result},
    utils::casing::to_snake_case,
};

/// Aggregate query with bind parameters instead of escaped string literals.
///
/// Produced by [`AggregationSqlGenerator::generate_parameterized`].  Pass `sql`
/// and `params` directly to [`crate::db::DatabaseAdapter::execute_parameterized_aggregate`].
#[derive(Debug, Clone)]
pub struct ParameterizedAggregationSql {
    /// SQL with `$N` (PostgreSQL), `?` (MySQL / SQLite), or `@P1` (SQL Server) placeholders.
    pub sql:    String,
    /// Bind parameters in placeholder order.
    pub params: Vec<serde_json::Value>,
}

/// Aggregation SQL generator
pub struct AggregationSqlGenerator {
    database_type: DatabaseType,
}

impl AggregationSqlGenerator {
    /// Create new SQL generator for specific database
    #[must_use]
    pub const fn new(database_type: DatabaseType) -> Self {
        Self { database_type }
    }

    /// Build SELECT clause
    fn build_select_clause(
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

    /// Convert GROUP BY expression to SQL
    fn group_by_expression_to_sql(&self, expr: &GroupByExpression) -> Result<String> {
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

    /// Generate JSONB extraction SQL with per-database path escaping.
    ///
    /// Each database uses a different string literal syntax for JSON paths.
    /// Single quotes or other metacharacters in `path` could otherwise break
    /// out of the string literal and inject arbitrary SQL. The per-database
    /// escape functions from `fraiseql_db::path_escape` are applied here as
    /// a second line of defence after schema allowlist validation in the planner.
    fn jsonb_extract_sql(&self, jsonb_column: &str, path: &str) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                let escaped = escape_postgres_jsonb_segment(path);
                format!("{}->>'{}' ", jsonb_column, escaped)
            },
            DatabaseType::MySQL => {
                // escape_mysql_json_path returns "$.escaped_segment"
                let escaped = escape_mysql_json_path(&[path.to_owned()]);
                format!("JSON_UNQUOTE(JSON_EXTRACT({}, '{}'))", jsonb_column, escaped)
            },
            DatabaseType::SQLite => {
                // escape_sqlite_json_path returns "$.escaped_segment"
                let escaped = escape_sqlite_json_path(&[path.to_owned()]);
                format!("json_extract({}, '{}')", jsonb_column, escaped)
            },
            DatabaseType::SQLServer => {
                // escape_sqlserver_json_path returns "$.escaped_segment"
                let escaped = escape_sqlserver_json_path(&[path.to_owned()]);
                format!("JSON_VALUE({}, '{}')", jsonb_column, escaped)
            },
        }
    }

    /// Generate temporal bucket SQL
    fn temporal_bucket_sql(&self, column: &str, bucket: TemporalBucket) -> String {
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

    /// Convert aggregate expression to SQL
    fn aggregate_expression_to_sql(&self, expr: &AggregateExpression) -> Result<String> {
        match expr {
            AggregateExpression::Count { .. } => Ok("COUNT(*)".to_string()),
            AggregateExpression::CountDistinct { column, .. } => {
                Ok(format!("COUNT(DISTINCT {})", column))
            },
            AggregateExpression::MeasureAggregate {
                column, function, ..
            } => {
                // Handle statistical functions with database-specific SQL
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
    fn advanced_aggregate_to_sql(
        &self,
        column: &str,
        function: AggregateFunction,
        delimiter: Option<&str>,
        order_by: Option<&Vec<OrderByClause>>,
    ) -> Result<String> {
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
    fn generate_array_agg_sql(
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
    fn generate_json_agg_sql(&self, column: &str, order_by: Option<&Vec<OrderByClause>>) -> String {
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
    fn generate_jsonb_agg_sql(
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
    fn generate_string_agg_sql(
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
                let mut sql = format!(
                    "STRING_AGG(CAST({} AS NVARCHAR(MAX)), '{}')",
                    column, safe_delimiter
                );
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
    fn order_by_to_sql(&self, order_by: &[OrderByClause]) -> String {
        order_by
            .iter()
            .map(|clause| {
                let direction = match clause.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
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
    fn generate_stddev_sql(&self, column: &str) -> String {
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
    fn generate_variance_sql(&self, column: &str) -> String {
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
    fn generate_bool_agg_sql(
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

    /// Convert WhereOperator to SQL operator
    const fn operator_to_sql(&self, operator: &WhereOperator) -> &'static str {
        match operator {
            WhereOperator::Eq => "=",
            WhereOperator::Neq => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",
            WhereOperator::In => "IN",
            WhereOperator::Nin => "NOT IN",
            WhereOperator::Like | WhereOperator::Contains => "LIKE",
            WhereOperator::Ilike | WhereOperator::Icontains => {
                match self.database_type {
                    DatabaseType::PostgreSQL => "ILIKE",
                    _ => "LIKE", // Other databases use LIKE with UPPER/LOWER
                }
            },
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Istartswith => match self.database_type {
                DatabaseType::PostgreSQL => "ILIKE",
                _ => "LIKE",
            },
            WhereOperator::Endswith => "LIKE",
            WhereOperator::Iendswith => match self.database_type {
                DatabaseType::PostgreSQL => "ILIKE",
                _ => "LIKE",
            },
            _ => "=", // Safe default for other operators
        }
    }

    /// Quote a validated field alias/column name using the database-appropriate identifier syntax.
    ///
    /// Field names arrive here after `OrderByClause::validate_field_name` has verified they
    /// match `[_A-Za-z][_0-9A-Za-z]*`, so no delimiter-escaping is required — but quoting
    /// still protects against SQL reserved words (`order`, `count`, `group`, `select`, …)
    /// that would break unquoted ORDER BY clauses.
    fn quote_identifier(&self, name: &str) -> String {
        match self.database_type {
            DatabaseType::MySQL => quote_mysql_identifier(name),
            DatabaseType::SQLServer => quote_sqlserver_identifier(name),
            // PostgreSQL and SQLite both use double-quote syntax.
            DatabaseType::PostgreSQL | DatabaseType::SQLite => quote_postgres_identifier(name),
        }
    }

    /// Escape a string value for embedding inside a SQL string literal.
    ///
    /// MySQL treats backslash as an escape character in string literals by default
    /// (unless `NO_BACKSLASH_ESCAPES` sql_mode is set). Backslashes must be doubled
    /// before single-quote escaping to prevent injection via sequences like `\';`.
    ///
    /// Null bytes (`\x00`) are stripped before escaping. PostgreSQL rejects null
    /// bytes in string literals with "invalid byte sequence for encoding", which
    /// would surface as a confusing database error. Stripping them produces
    /// deterministic SQL regardless of the database's null-byte handling.
    fn escape_sql_string(&self, s: &str) -> String {
        // Strip null bytes — never valid in SQL string literals.
        let without_nulls: std::borrow::Cow<str> = if s.contains('\0') {
            s.replace('\0', "").into()
        } else {
            s.into()
        };
        if matches!(self.database_type, DatabaseType::MySQL) {
            // Escape backslashes first, then single quotes.
            without_nulls.replace('\\', "\\\\").replace('\'', "''")
        } else {
            // Standard SQL: only double single quotes.
            without_nulls.replace('\'', "''")
        }
    }

    /// Build GROUP BY clause
    fn build_group_by_clause(&self, group_by_expressions: &[GroupByExpression]) -> Result<String> {
        let mut columns = Vec::new();

        for expr in group_by_expressions {
            let column = self.group_by_expression_to_sql(expr)?;
            columns.push(column);
        }

        Ok(format!("GROUP BY {}", columns.join(", ")))
    }

    /// Build ORDER BY clause
    fn build_order_by_clause(&self, order_by: &[OrderByClause]) -> Result<String> {
        let clauses: Vec<String> = order_by
            .iter()
            .map(|clause| {
                let direction = match clause.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };
                format!("{} {}", self.quote_identifier(&clause.field), direction)
            })
            .collect();

        Ok(format!("ORDER BY {}", clauses.join(", ")))
    }

    // =========================================================================
    // Parameterized query generation (bind parameters, no string embedding)
    // =========================================================================

    /// Returns the bind-parameter placeholder for position `index` (0-based).
    fn placeholder(&self, index: usize) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => format!("${}", index + 1),
            DatabaseType::SQLServer  => format!("@P{}", index + 1),
            _                        => "?".to_string(),
        }
    }

    /// If `value` is non-NULL, appends it to `params` and returns the placeholder.
    ///
    /// `NULL` is emitted inline as the literal `NULL`; it cannot be reliably
    /// parameterized across all four database drivers in the same way.
    fn emit_value_param(
        &self,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> String {
        if matches!(value, serde_json::Value::Null) {
            return "NULL".to_string();
        }
        let idx = params.len();
        params.push(value.clone());
        self.placeholder(idx)
    }

    /// Build a LIKE pattern string, escape LIKE metacharacters with `!`, bind it as a
    /// parameter, and return the placeholder.  Returns `(placeholder, needs_escape_clause)`
    /// where `needs_escape_clause` indicates whether `ESCAPE '!'` should be appended to
    /// the SQL fragment.
    fn emit_like_pattern_param(
        &self,
        operator: &WhereOperator,
        value: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> (String, bool) {
        // Strip null bytes before binding (same invariant as escape_sql_string).
        let clean: String = if value.contains('\0') {
            value.replace('\0', "")
        } else {
            value.to_string()
        };

        let (pattern, needs_escape) = match operator {
            WhereOperator::Contains | WhereOperator::Icontains => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("%{esc}%"), true)
            },
            WhereOperator::Startswith | WhereOperator::Istartswith => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("{esc}%"), true)
            },
            WhereOperator::Endswith | WhereOperator::Iendswith => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("%{esc}"), true)
            },
            // Like / Ilike: caller controls wildcards — bind as-is.
            _ => (clean, false),
        };

        let ph = self.emit_value_param(&serde_json::Value::String(pattern), params);
        (ph, needs_escape)
    }

    /// Convert a [`WhereClause`] AST to parameterized SQL, appending bind values to `params`.
    fn where_clause_to_sql_parameterized(
        &self,
        clause: &WhereClause,
        metadata: &FactTableMetadata,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        match clause {
            WhereClause::Field { path, operator, value } => {
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
                        jsonb_column, path, operator, value, params,
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
        }
    }

    /// Parameterized WHERE for a denormalized (direct column) filter.
    fn generate_direct_column_where_parameterized(
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
            let phs: Vec<String> =
                arr.iter().map(|v| self.emit_value_param(v, params)).collect();
            return Ok(format!("{field} {op_sql} ({})", phs.join(", ")));
        }

        if matches!(
            operator,
            WhereOperator::Contains
                | WhereOperator::Startswith
                | WhereOperator::Endswith
                | WhereOperator::Like
        ) {
            let s = value.as_str().ok_or_else(|| {
                FraiseQLError::validation("LIKE operators require string values")
            })?;
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
            return self.generate_case_insensitive_where_parameterized(
                field, operator, s, params,
            );
        }

        let ph = self.emit_value_param(value, params);
        Ok(format!("{field} {op_sql} {ph}"))
    }

    /// Parameterized WHERE for a JSONB dimension field.
    fn generate_jsonb_where_parameterized(
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
                &jsonb_extract, operator, s, params,
            );
        }

        if matches!(operator, WhereOperator::In | WhereOperator::Nin) {
            let arr = value.as_array().ok_or_else(|| {
                FraiseQLError::validation("IN/NOT IN operators require array values")
            })?;
            let phs: Vec<String> =
                arr.iter().map(|v| self.emit_value_param(v, params)).collect();
            return Ok(format!("{jsonb_extract} {op_sql} ({})", phs.join(", ")));
        }

        if matches!(
            operator,
            WhereOperator::Contains | WhereOperator::Startswith | WhereOperator::Endswith
        ) {
            let s = value.as_str().ok_or_else(|| {
                FraiseQLError::validation("LIKE operators require string values")
            })?;
            // needs_escape is always true for semantic LIKE operators (Contains etc.)
            let (ph, _) = self.emit_like_pattern_param(operator, s, params);
            return Ok(format!("{jsonb_extract} {op_sql} {ph} ESCAPE '!'"));
        }

        let ph = self.emit_value_param(value, params);
        Ok(format!("{jsonb_extract} {op_sql} {ph}"))
    }

    /// Parameterized case-insensitive WHERE (ILIKE for PostgreSQL, UPPER() for others).
    fn generate_case_insensitive_where_parameterized(
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
    fn build_having_clause_parameterized(
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
            let operator_sql  = condition.operator.sql_operator();
            let value_sql     = self.emit_value_param(&condition.value, params);
            conditions.push(format!("{aggregate_sql} {operator_sql} {value_sql}"));
        }
        Ok(format!("HAVING {}", conditions.join(" AND ")))
    }

    /// Generate a parameterized aggregate SQL query.
    ///
    /// All user-supplied string values in `WHERE` and `HAVING` clauses are emitted as
    /// bind placeholders (`$N` / `?` / `@P1` depending on the database dialect) rather
    /// than being embedded as escaped string literals.  Numeric, boolean, and `NULL`
    /// values are still inlined since they carry no injection risk.
    ///
    /// # Errors
    ///
    /// Returns error if SQL generation fails (unknown aggregate function, etc.).
    pub fn generate_parameterized(
        &self,
        plan: &AggregationPlan,
    ) -> Result<ParameterizedAggregationSql> {
        let mut params: Vec<serde_json::Value> = Vec::new();

        let select_sql =
            self.build_select_clause(&plan.group_by_expressions, &plan.aggregate_expressions)?;
        let from_sql = format!("FROM {}", plan.request.table_name);

        let where_sql = if let Some(ref wc) = plan.request.where_clause {
            self.build_where_clause_parameterized(wc, &plan.metadata, &mut params)?
        } else {
            String::new()
        };

        let group_sql = if !plan.group_by_expressions.is_empty() {
            self.build_group_by_clause(&plan.group_by_expressions)?
        } else {
            String::new()
        };

        let having_sql =
            self.build_having_clause_parameterized(&plan.having_conditions, &mut params)?;

        let order_sql = if !plan.request.order_by.is_empty() {
            self.build_order_by_clause(&plan.request.order_by)?
        } else {
            String::new()
        };

        let mut parts: Vec<&str> =
            vec![&select_sql, &from_sql, &where_sql, &group_sql, &having_sql, &order_sql];
        parts.retain(|s| !s.is_empty());

        let mut sql = parts.join("\n");

        if let Some(limit) = plan.request.limit {
            sql.push_str(&format!("\nLIMIT {limit}"));
        }
        if let Some(offset) = plan.request.offset {
            sql.push_str(&format!("\nOFFSET {offset}"));
        }

        Ok(ParameterizedAggregationSql { sql, params })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::compiler::{
        aggregate_types::HavingOperator,
        aggregation::{AggregateSelection, AggregationRequest, GroupBySelection},
        fact_table::{DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType},
    };

    fn create_test_plan() -> AggregationPlan {
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        };

        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: None,
            group_by:     vec![
                GroupBySelection::Dimension {
                    path:  "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "occurred_at".to_string(),
                    bucket: TemporalBucket::Day,
                    alias:  "day".to_string(),
                },
            ],
            aggregates:   vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure:  "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias:    "revenue_sum".to_string(),
                },
            ],
            having:       vec![],
            order_by:     vec![],
            limit:        Some(10),
            offset:       None,
        };

        crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
    }

    #[test]
    fn test_postgres_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("dimensions->>'category'"));
        assert!(sql.sql.contains("DATE_TRUNC('day', occurred_at)"));
        assert!(sql.sql.contains("COUNT(*)"));
        assert!(sql.sql.contains("SUM(revenue)"));
        assert!(sql.sql.contains("GROUP BY"));
        assert!(sql.sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_mysql_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(
            sql.sql
                .contains("JSON_UNQUOTE(JSON_EXTRACT(dimensions, '$.category'))")
        );
        assert!(sql.sql.contains("DATE_FORMAT(occurred_at"));
        assert!(sql.sql.contains("COUNT(*)"));
        assert!(sql.sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_sqlite_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("json_extract(dimensions, '$.category')"));
        assert!(sql.sql.contains("strftime"));
        assert!(sql.sql.contains("COUNT(*)"));
        assert!(sql.sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_sqlserver_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("JSON_VALUE(dimensions, '$.category')"));
        assert!(sql.sql.contains("CAST(occurred_at AS DATE)"));
        assert!(sql.sql.contains("COUNT(*)"));
        assert!(sql.sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_having_clause() {
        let mut plan = create_test_plan();
        plan.having_conditions = vec![ValidatedHavingCondition {
            aggregate: AggregateExpression::MeasureAggregate {
                column:   "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "revenue_sum".to_string(),
            },
            operator:  HavingOperator::Gt,
            value:     serde_json::json!(1000),
        }];

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("HAVING SUM(revenue) > $1"));
        assert_eq!(sql.params, vec![serde_json::json!(1000)]);
    }

    #[test]
    fn test_order_by_clause() {
        use crate::compiler::aggregation::OrderByClause;

        let mut plan = create_test_plan();
        plan.request.order_by = vec![OrderByClause {
            field:     "revenue_sum".to_string(),
            direction: OrderDirection::Desc,
        }];

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("ORDER BY \"revenue_sum\" DESC"));
    }

    // ========================================
    // Advanced Aggregates Tests
    // ========================================

    #[test]
    fn test_array_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        // Test without ORDER BY
        let sql = generator.generate_array_agg_sql("product_id", None);
        assert_eq!(sql, "ARRAY_AGG(product_id)");

        // Test with ORDER BY
        let order_by = vec![OrderByClause {
            field:     "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_array_agg_sql("product_id", Some(&order_by));
        assert_eq!(sql, "ARRAY_AGG(product_id ORDER BY \"revenue\" DESC)");
    }

    #[test]
    fn test_array_agg_mysql() {
        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = generator.generate_array_agg_sql("product_id", None);
        assert_eq!(sql, "JSON_ARRAYAGG(product_id)");
    }

    #[test]
    fn test_array_agg_sqlite() {
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = generator.generate_array_agg_sql("product_id", None);
        assert!(sql.contains("GROUP_CONCAT"));
        assert!(sql.contains("'[' ||"));
        assert!(sql.contains("|| ']'"));
    }

    #[test]
    fn test_string_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        // Test without ORDER BY
        let sql = generator.generate_string_agg_sql("product_name", ", ", None);
        assert_eq!(sql, "STRING_AGG(product_name, ', ')");

        // Test with ORDER BY
        let order_by = vec![OrderByClause {
            field:     "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert_eq!(sql, "STRING_AGG(product_name, ', ' ORDER BY \"revenue\" DESC)");
    }

    #[test]
    fn test_string_agg_mysql() {
        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);

        let order_by = vec![OrderByClause {
            field:     "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert_eq!(sql, "GROUP_CONCAT(product_name ORDER BY `revenue` DESC SEPARATOR ', ')");
    }

    #[test]
    fn test_string_agg_sqlserver() {
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);

        let order_by = vec![OrderByClause {
            field:     "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert!(sql.contains("STRING_AGG(CAST(product_name AS NVARCHAR(MAX)), ', ')"));
        assert!(sql.contains("WITHIN GROUP (ORDER BY [revenue] DESC)"));
    }

    #[test]
    fn test_json_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_json_agg_sql("data", None);
        assert_eq!(sql, "JSON_AGG(data)");

        let order_by = vec![OrderByClause {
            field:     "created_at".to_string(),
            direction: OrderDirection::Asc,
        }];
        let sql = generator.generate_json_agg_sql("data", Some(&order_by));
        assert_eq!(sql, "JSON_AGG(data ORDER BY \"created_at\" ASC)");
    }

    #[test]
    fn test_jsonb_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_jsonb_agg_sql("data", None);
        assert_eq!(sql, "JSONB_AGG(data)");
    }

    #[test]
    fn test_bool_and_postgres() {
        use crate::compiler::aggregate_types::BoolAggregateFunction;

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
        assert_eq!(sql, "BOOL_AND(is_active)");

        let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
        assert_eq!(sql, "BOOL_OR(has_discount)");
    }

    #[test]
    fn test_bool_and_mysql() {
        use crate::compiler::aggregate_types::BoolAggregateFunction;

        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
        assert_eq!(sql, "MIN(is_active) = 1");

        let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
        assert_eq!(sql, "MAX(has_discount) = 1");
    }

    #[test]
    fn test_bool_and_sqlserver() {
        use crate::compiler::aggregate_types::BoolAggregateFunction;

        let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = generator.generate_bool_agg_sql("is_active", BoolAggregateFunction::And);
        assert_eq!(sql, "MIN(CAST(is_active AS BIT)) = 1");

        let sql = generator.generate_bool_agg_sql("has_discount", BoolAggregateFunction::Or);
        assert_eq!(sql, "MAX(CAST(has_discount AS BIT)) = 1");
    }

    #[test]
    fn test_advanced_aggregate_full_query() {
        // Create a plan with advanced aggregates
        let mut plan = create_test_plan();

        // Add an ARRAY_AGG aggregate
        plan.aggregate_expressions.push(AggregateExpression::AdvancedAggregate {
            column:    "product_id".to_string(),
            function:  AggregateFunction::ArrayAgg,
            alias:     "products".to_string(),
            delimiter: None,
            order_by:  Some(vec![OrderByClause {
                field:     "revenue".to_string(),
                direction: OrderDirection::Desc,
            }]),
        });

        // Add a STRING_AGG aggregate
        plan.aggregate_expressions.push(AggregateExpression::AdvancedAggregate {
            column:    "product_name".to_string(),
            function:  AggregateFunction::StringAgg,
            alias:     "product_names".to_string(),
            delimiter: Some(", ".to_string()),
            order_by:  None,
        });

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        assert!(sql.sql.contains("ARRAY_AGG(product_id ORDER BY \"revenue\" DESC)"));
        assert!(sql.sql.contains("STRING_AGG(product_name, ', ')"));
    }

    // ========================================
    // Security / Escaping Tests
    // ========================================

    #[test]
    fn test_having_string_value_is_bound_not_escaped() {
        use crate::compiler::aggregate_types::AggregateFunction;

        let mut plan = create_test_plan();
        plan.having_conditions = vec![ValidatedHavingCondition {
            aggregate: AggregateExpression::MeasureAggregate {
                column:   "label".to_string(),
                function: AggregateFunction::Max,
                alias:    "label_max".to_string(),
            },
            operator:  HavingOperator::Eq,
            value:     serde_json::json!("O'Reilly"),
        }];

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_parameterized(&plan).unwrap();

        // Value must be a bind parameter — raw string must never appear in SQL.
        assert!(sql.sql.contains("HAVING MAX(label) = $1"));
        assert!(!sql.sql.contains("O'Reilly"), "raw string must not appear in SQL: {}", sql.sql);
        assert_eq!(sql.params, vec![serde_json::json!("O'Reilly")]);
    }

    #[test]
    fn test_escape_sql_string_mysql_doubles_backslash() {
        // MySQL treats backslash as an escape character in string literals.
        // A bare backslash before the closing quote would consume it, breaking the SQL.
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        assert_eq!(gen.escape_sql_string("test\\"), "test\\\\");
        assert_eq!(gen.escape_sql_string("te'st"), "te''st");
        // Backslash followed by a quote: escape backslash first (→ \\), then double the
        // quote (→ '').  Result for te\'st is te\\''st.
        assert_eq!(gen.escape_sql_string("te\\'st"), "te\\\\''st");
    }

    #[test]
    fn test_escape_sql_string_postgres_only_doubles_quote() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        // Backslash is not special in standard SQL string literals.
        assert_eq!(gen.escape_sql_string("test\\"), "test\\");
        assert_eq!(gen.escape_sql_string("te'st"), "te''st");
    }

    #[test]
    fn test_escape_sql_string_strips_null_bytes() {
        // Null bytes are never valid in SQL string literals.
        // PostgreSQL rejects them with "invalid byte sequence"; stripping is safer than an error.
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        assert_eq!(gen.escape_sql_string("before\x00after"), "beforeafter");
        assert_eq!(gen.escape_sql_string("\x00"), "");
        assert_eq!(gen.escape_sql_string("no-null"), "no-null");

        // Same for MySQL — null stripping happens before backslash/quote escaping.
        let mysql = AggregationSqlGenerator::new(DatabaseType::MySQL);
        assert_eq!(mysql.escape_sql_string("te\x00st\\"), "test\\\\");
    }

    // ── jsonb_extract_sql injection tests ──────────────────────────────────────

    #[test]
    fn test_jsonb_postgres_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.jsonb_extract_sql("dimensions", "user'name");
        // Single quote must be doubled; must not break out of the string literal.
        assert!(sql.contains("user''name"), "Expected doubled quote, got: {sql}");
        assert!(!sql.contains("user'name'"), "Unescaped quote still present");
    }

    #[test]
    fn test_jsonb_postgres_pg_sleep_injection_neutralised() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.jsonb_extract_sql("dimensions", "a' || pg_sleep(10) --");
        // The injected payload must appear inside the string literal (quote doubled).
        assert!(sql.contains("a'' || pg_sleep(10) --"), "Escaping not applied: {sql}");
    }

    #[test]
    fn test_jsonb_postgres_clean_path_unchanged() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.jsonb_extract_sql("dimensions", "category");
        assert!(sql.contains("dimensions->>'category'"), "Clean path altered: {sql}");
    }

    #[test]
    fn test_jsonb_mysql_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = gen.jsonb_extract_sql("dimensions", "user'name");
        // MySQL JSON paths use doubled-quote escaping (''): backslash escaping is NOT used.
        assert!(sql.contains("user''name"), "Expected doubled-quote escape in MySQL: {sql}");
    }

    #[test]
    fn test_jsonb_mysql_path_prefix_not_doubled() {
        // escape_mysql_json_path already adds "$." — must not appear as "$.$.path"
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = gen.jsonb_extract_sql("dimensions", "category");
        assert!(sql.contains("$.category"), "Path prefix missing: {sql}");
        assert!(!sql.contains("$.$."), "Double prefix detected: {sql}");
    }

    #[test]
    fn test_jsonb_sqlite_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = gen.jsonb_extract_sql("dimensions", "it's");
        // SQLite JSON paths use doubled-quote escaping (''): backslash escaping is NOT used.
        assert!(sql.contains("it''s"), "Expected doubled-quote escape in SQLite: {sql}");
    }

    #[test]
    fn test_jsonb_sqlserver_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = gen.jsonb_extract_sql("dimensions", "user'name");
        assert!(sql.contains("user''name"), "Expected doubled quote in SQL Server: {sql}");
    }

    // ── STRING_AGG delimiter injection tests ───────────────────────────────────

    #[test]
    fn test_stringagg_delimiter_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_string_agg_sql("product_name", "O'Reilly", None);
        assert!(sql.contains("'O''Reilly'"), "single quote must be doubled: {sql}");
        assert!(!sql.contains("'O'Reilly'"), "unescaped quote must not appear");
    }

    #[test]
    fn test_stringagg_delimiter_injection_payload_neutralised() {
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let payload = "'; DROP TABLE users; --";
        let sql = gen.generate_string_agg_sql("product_name", payload, None);
        // After escaping, the payload single quote is doubled — no free semicolon outside a literal.
        assert!(sql.contains("''"), "single quotes must be doubled: {sql}");
        // Verify the SQL starts and ends as a valid STRING_AGG call (no injected statements).
        assert!(sql.starts_with("STRING_AGG("), "must remain a STRING_AGG call: {sql}");
    }

    #[test]
    fn test_stringagg_delimiter_mysql_backslash_and_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        // MySQL also escapes backslashes; a trailing backslash could consume the closing quote.
        let sql = gen.generate_string_agg_sql("col", r"a\b", None);
        assert!(sql.contains(r"a\\b"), "backslash must be doubled for MySQL: {sql}");
    }

    #[test]
    fn test_stringagg_delimiter_mysql_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = gen.generate_string_agg_sql("col", "O'Reilly", None);
        assert!(sql.contains("O''Reilly"), "single quote must be doubled for MySQL: {sql}");
    }

    #[test]
    fn test_stringagg_delimiter_sqlite_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = gen.generate_string_agg_sql("col", "it's", None);
        assert!(sql.contains("it''s"), "single quote must be doubled for SQLite: {sql}");
    }

    #[test]
    fn test_stringagg_delimiter_sqlserver_single_quote_escaped() {
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = gen.generate_string_agg_sql("col", "O'Reilly", None);
        assert!(sql.contains("O''Reilly"), "single quote must be doubled for SQL Server: {sql}");
    }

    #[test]
    fn test_stringagg_delimiter_clean_value_unchanged() {
        // A safe delimiter should pass through unchanged.
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = gen.generate_string_agg_sql("product_name", ", ", None);
        assert_eq!(sql, "STRING_AGG(product_name, ', ')");
    }

    // =========================================================================
    // Parameterized query generation tests
    // =========================================================================

    fn make_string_where_plan(_db: DatabaseType) -> AggregationPlan {
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name:     "status".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        };

        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: Some(WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::Eq,
                value:    serde_json::json!("test_value"),
            }),
            group_by:     vec![GroupBySelection::Dimension {
                path:  "category".to_string(),
                alias: "category".to_string(),
            }],
            aggregates:   vec![AggregateSelection::Count {
                alias: "count".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
    }

    #[test]
    fn test_generate_parameterized_where_string_becomes_placeholder() {
        // PostgreSQL: string value must become $1, not an escaped literal
        let plan = make_string_where_plan(DatabaseType::PostgreSQL);
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(
            result.sql.contains("$1"),
            "PostgreSQL placeholder must be $1: {}", result.sql
        );
        assert!(
            !result.sql.contains("'test_value'"),
            "String value must not appear as literal: {}", result.sql
        );
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], serde_json::json!("test_value"));
    }

    #[test]
    fn test_generate_parameterized_having_string_becomes_placeholder() {
        // MySQL: HAVING string value must become ? placeholder, not escaped inline
        let injection = "test\\' injection";
        // Build a base plan and then inject HAVING directly (like test_having_clause).
        let mut plan = create_test_plan();
        plan.having_conditions = vec![ValidatedHavingCondition {
            aggregate: AggregateExpression::MeasureAggregate {
                column:   "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "revenue_sum".to_string(),
            },
            operator:  HavingOperator::Eq,
            value:     serde_json::json!(injection),
        }];

        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(
            result.sql.contains("HAVING SUM(revenue) = ?"),
            "SQL must use ? placeholder: {}", result.sql
        );
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], serde_json::json!(injection));
        // injection string must NOT appear verbatim in the SQL
        assert!(
            !result.sql.contains("injection"),
            "Injection string must not appear in SQL: {}", result.sql
        );
    }

    #[test]
    fn test_parameterized_postgres_placeholder_numbering() {
        // WHERE uses $1, HAVING uses $2 (shared counter).
        // Build a plan with a WHERE clause on a denormalized filter field,
        // then inject a HAVING condition directly (like test_having_clause).
        let injection = "risky";
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name:     "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "channel".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed:  true,
                },
            ],
            calendar_dimensions:  vec![],
        };

        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: Some(WhereClause::Field {
                path:     vec!["channel".to_string()],
                operator: WhereOperator::Eq,
                value:    serde_json::json!(injection),
            }),
            group_by:     vec![GroupBySelection::TemporalBucket {
                column: "occurred_at".to_string(),
                bucket: TemporalBucket::Day,
                alias:  "day".to_string(),
            }],
            aggregates:   vec![AggregateSelection::MeasureAggregate {
                measure:  "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "total".to_string(),
            }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let mut plan =
            crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap();
        // Inject HAVING directly to avoid navigating the unvalidated HavingCondition type.
        plan.having_conditions = vec![ValidatedHavingCondition {
            aggregate: AggregateExpression::MeasureAggregate {
                column:   "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias:    "total".to_string(),
            },
            operator:  HavingOperator::Gt,
            value:     serde_json::json!("threshold"),
        }];

        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(result.sql.contains("WHERE channel = $1"), "SQL: {}", result.sql);
        assert!(result.sql.contains("HAVING SUM(revenue) > $2"), "SQL: {}", result.sql);
        assert_eq!(result.params.len(), 2);
        assert_eq!(result.params[0], serde_json::json!(injection));
        assert_eq!(result.params[1], serde_json::json!("threshold"));
    }

    #[test]
    fn test_parameterized_mysql_uses_question_mark() {
        let plan = make_string_where_plan(DatabaseType::MySQL);
        let gen = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(result.sql.contains("WHERE status = ?"), "SQL: {}", result.sql);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], serde_json::json!("test_value"));
    }

    #[test]
    fn test_parameterized_sqlserver_uses_at_p_placeholder() {
        let plan = make_string_where_plan(DatabaseType::SQLServer);
        let gen = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(result.sql.contains("WHERE status = @P1"), "SQL: {}", result.sql);
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], serde_json::json!("test_value"));
    }

    #[test]
    fn test_parameterized_in_array_expands_to_multiple_placeholders() {
        // WHERE status IN ("a","b","c") → WHERE status IN ($1,$2,$3) with 3 params
        let metadata = FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![],
            dimensions:           DimensionColumn { name: "data".to_string(), paths: vec![] },
            denormalized_filters: vec![FilterColumn {
                name:     "status".to_string(),
                sql_type: SqlType::Timestamp,
                indexed:  true,
            }],
            calendar_dimensions:  vec![],
        };
        let request = AggregationRequest {
            table_name:   "tf_sales".to_string(),
            where_clause: Some(WhereClause::Field {
                path:     vec!["status".to_string()],
                operator: WhereOperator::In,
                value:    serde_json::json!(["a", "b", "c"]),
            }),
            group_by:     vec![],
            aggregates:   vec![AggregateSelection::Count { alias: "count".to_string() }],
            having:       vec![],
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };
        let plan = crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap();
        let gen = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let result = gen.generate_parameterized(&plan).unwrap();

        assert!(
            result.sql.contains("status IN ($1, $2, $3)"),
            "IN clause must expand to 3 placeholders: {}", result.sql
        );
        assert_eq!(result.params.len(), 3);
        assert_eq!(result.params[0], serde_json::json!("a"));
        assert_eq!(result.params[1], serde_json::json!("b"));
        assert_eq!(result.params[2], serde_json::json!("c"));
    }
}
