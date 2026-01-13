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

use crate::compiler::aggregation::{
    AggregateExpression, AggregationPlan, GroupByExpression, OrderByClause, OrderDirection,
    ValidatedHavingCondition,
};
use crate::compiler::aggregate_types::{AggregateFunction, HavingOperator, TemporalBucket};
use crate::compiler::fact_table::FactTableMetadata;
use crate::db::types::DatabaseType;
use crate::db::where_clause::{WhereClause, WhereOperator};
use crate::error::{FraiseQLError, Result};

/// SQL query components
#[derive(Debug, Clone)]
pub struct AggregationSql {
    /// SELECT clause
    pub select: String,
    /// FROM clause
    pub from: String,
    /// WHERE clause (if present)
    pub where_clause: Option<String>,
    /// GROUP BY clause (if present)
    pub group_by: Option<String>,
    /// HAVING clause (if present)
    pub having: Option<String>,
    /// ORDER BY clause (if present)
    pub order_by: Option<String>,
    /// LIMIT clause (if present)
    pub limit: Option<u32>,
    /// OFFSET clause (if present)
    pub offset: Option<u32>,
    /// Complete SQL query
    pub complete_sql: String,
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

    /// Generate SQL from aggregation plan
    ///
    /// # Errors
    ///
    /// Returns error if SQL generation fails
    pub fn generate(&self, plan: &AggregationPlan) -> Result<AggregationSql> {
        // Build SELECT clause
        let select = self.build_select_clause(&plan.group_by_expressions, &plan.aggregate_expressions)?;

        // Build FROM clause
        let from = format!("FROM {}", plan.request.table_name);

        // Build WHERE clause (if present)
        let where_clause = if let Some(ref where_clause) = plan.request.where_clause {
            Some(self.build_where_clause(where_clause, &plan.metadata)?)
        } else {
            None
        };

        // Build GROUP BY clause (if present)
        let group_by = if !plan.group_by_expressions.is_empty() {
            Some(self.build_group_by_clause(&plan.group_by_expressions)?)
        } else {
            None
        };

        // Build HAVING clause (if present)
        let having = if !plan.having_conditions.is_empty() {
            Some(self.build_having_clause(&plan.having_conditions)?)
        } else {
            None
        };

        // Build ORDER BY clause (if present)
        let order_by = if !plan.request.order_by.is_empty() {
            Some(self.build_order_by_clause(&plan.request.order_by)?)
        } else {
            None
        };

        // Build complete SQL
        let complete_sql = self.assemble_sql(
            &select,
            &from,
            where_clause.as_deref(),
            group_by.as_deref(),
            having.as_deref(),
            order_by.as_deref(),
            plan.request.limit,
            plan.request.offset,
        );

        Ok(AggregationSql {
            select,
            from,
            where_clause,
            group_by,
            having,
            order_by,
            limit: plan.request.limit,
            offset: plan.request.offset,
            complete_sql,
        })
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
                GroupByExpression::JsonbPath { alias, .. } | GroupByExpression::TemporalBucket { alias, .. } => alias,
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
            GroupByExpression::JsonbPath { jsonb_column, path, .. } => {
                Ok(self.jsonb_extract_sql(jsonb_column, path))
            }
            GroupByExpression::TemporalBucket { column, bucket, .. } => {
                Ok(self.temporal_bucket_sql(column, *bucket))
            }
        }
    }

    /// Generate JSONB extraction SQL
    fn jsonb_extract_sql(&self, jsonb_column: &str, path: &str) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                format!("{}->>'{}' ", jsonb_column, path)
            }
            DatabaseType::MySQL => {
                format!("JSON_UNQUOTE(JSON_EXTRACT({}, '$.{}'))", jsonb_column, path)
            }
            DatabaseType::SQLite => {
                format!("json_extract({}, '$.{}')", jsonb_column, path)
            }
            DatabaseType::SQLServer => {
                format!("JSON_VALUE({}, '$.{}')", jsonb_column, path)
            }
        }
    }

    /// Generate temporal bucket SQL
    fn temporal_bucket_sql(&self, column: &str, bucket: TemporalBucket) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                format!("DATE_TRUNC('{}', {})", bucket.postgres_arg(), column)
            }
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
            }
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
            }
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
                    }
                    TemporalBucket::Year => {
                        format!("DATEADD(year, DATEDIFF(year, 0, {}), 0)", column)
                    }
                    _ => format!("DATEPART({}, {})", datepart, column),
                }
            }
        }
    }

    /// Convert aggregate expression to SQL
    fn aggregate_expression_to_sql(&self, expr: &AggregateExpression) -> Result<String> {
        match expr {
            AggregateExpression::Count { .. } => Ok("COUNT(*)".to_string()),
            AggregateExpression::CountDistinct { column, .. } => {
                Ok(format!("COUNT(DISTINCT {})", column))
            }
            AggregateExpression::MeasureAggregate { column, function, .. } => {
                Ok(format!("{}({})", function.sql_name(), column))
            }
            AggregateExpression::AdvancedAggregate { column, function, delimiter, order_by, .. } => {
                self.advanced_aggregate_to_sql(column, *function, delimiter.as_deref(), order_by.as_ref())
            }
            AggregateExpression::BoolAggregate { column, function, .. } => {
                Ok(self.generate_bool_agg_sql(column, *function))
            }
        }
    }

    /// Generate SQL for advanced aggregates (Phase 6)
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
            StringAgg => Ok(self.generate_string_agg_sql(column, delimiter.unwrap_or(","), order_by)),
            _ => Ok(format!("{}({})", function.sql_name(), column)),
        }
    }

    /// Generate ARRAY_AGG SQL
    fn generate_array_agg_sql(&self, column: &str, order_by: Option<&Vec<OrderByClause>>) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("ARRAY_AGG({} ORDER BY {})", column, self.order_by_to_sql(order))
                } else {
                    format!("ARRAY_AGG({})", column)
                }
            }
            DatabaseType::MySQL => {
                // MySQL doesn't have ARRAY_AGG, use JSON_ARRAYAGG
                format!("JSON_ARRAYAGG({})", column)
            }
            DatabaseType::SQLite => {
                // SQLite: emulate with GROUP_CONCAT, wrap in JSON array syntax
                format!("'[' || GROUP_CONCAT('\"' || {} || '\"', ',') || ']'", column)
            }
            DatabaseType::SQLServer => {
                // SQL Server: use STRING_AGG and wrap in JSON array
                format!("'[' + STRING_AGG('\"' + CAST({} AS NVARCHAR(MAX)) + '\"', ',') + ']'", column)
            }
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
            }
            DatabaseType::MySQL => {
                // MySQL: JSON_ARRAYAGG for arrays
                format!("JSON_ARRAYAGG({})", column)
            }
            DatabaseType::SQLite => {
                // SQLite: limited JSON support
                format!("JSON_ARRAY({})", column)
            }
            DatabaseType::SQLServer => {
                // SQL Server: FOR JSON PATH
                format!("(SELECT {} FOR JSON PATH)", column)
            }
        }
    }

    /// Generate JSONB_AGG SQL (PostgreSQL-specific)
    fn generate_jsonb_agg_sql(&self, column: &str, order_by: Option<&Vec<OrderByClause>>) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("JSONB_AGG({} ORDER BY {})", column, self.order_by_to_sql(order))
                } else {
                    format!("JSONB_AGG({})", column)
                }
            }
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
        match self.database_type {
            DatabaseType::PostgreSQL => {
                if let Some(order) = order_by {
                    format!("STRING_AGG({}, '{}' ORDER BY {})", column, delimiter, self.order_by_to_sql(order))
                } else {
                    format!("STRING_AGG({}, '{}')", column, delimiter)
                }
            }
            DatabaseType::MySQL => {
                let mut sql = format!("GROUP_CONCAT({}",  column);
                if let Some(order) = order_by {
                    sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
                }
                sql.push_str(&format!(" SEPARATOR '{}')", delimiter));
                sql
            }
            DatabaseType::SQLite => {
                // SQLite GROUP_CONCAT doesn't support ORDER BY in older versions
                format!("GROUP_CONCAT({}, '{}')", column, delimiter)
            }
            DatabaseType::SQLServer => {
                let mut sql = format!("STRING_AGG(CAST({} AS NVARCHAR(MAX)), '{}')", column, delimiter);
                if let Some(order) = order_by {
                    sql.push_str(&format!(" WITHIN GROUP (ORDER BY {})", self.order_by_to_sql(order)));
                }
                sql
            }
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
                format!("{} {}", clause.field, direction)
            })
            .collect::<Vec<_>>()
            .join(", ")
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
            }
            DatabaseType::MySQL | DatabaseType::SQLite => {
                // MySQL/SQLite: emulate with MIN/MAX on boolean as integer (0/1)
                match function {
                    BoolAggregateFunction::And => format!("MIN({}) = 1", column),
                    BoolAggregateFunction::Or => format!("MAX({}) = 1", column),
                }
            }
            DatabaseType::SQLServer => {
                // SQL Server: emulate with MIN/MAX on CAST to BIT
                match function {
                    BoolAggregateFunction::And => format!("MIN(CAST({} AS BIT)) = 1", column),
                    BoolAggregateFunction::Or => format!("MAX(CAST({} AS BIT)) = 1", column),
                }
            }
        }
    }

    /// Build WHERE clause SQL
    ///
    /// Handles two types of filterable fields:
    /// 1. Denormalized filters (direct columns): WHERE customer_id = $1
    /// 2. Dimensions (JSONB paths): WHERE data->>'category' = $1
    pub fn build_where_clause(&self, where_clause: &WhereClause, metadata: &FactTableMetadata) -> Result<String> {
        if where_clause.is_empty() {
            return Ok(String::new());
        }

        let condition_sql = self.where_clause_to_sql(where_clause, metadata)?;
        Ok(format!("WHERE {}", condition_sql))
    }

    /// Convert WhereClause AST to SQL
    fn where_clause_to_sql(&self, clause: &WhereClause, metadata: &FactTableMetadata) -> Result<String> {
        match clause {
            WhereClause::Field { path, operator, value } => {
                let field_name = &path[0];

                // Check if field is a denormalized filter (direct column)
                let is_denormalized = metadata.denormalized_filters
                    .iter()
                    .any(|f| f.name == *field_name);

                if is_denormalized {
                    // Direct column: WHERE customer_id = $1
                    self.generate_direct_column_where(field_name, operator, value)
                } else {
                    // JSONB dimension: WHERE data->>'category' = $1
                    let jsonb_column = &metadata.dimensions.name; // "data"
                    self.generate_jsonb_where(jsonb_column, path, operator, value)
                }
            }
            WhereClause::And(clauses) => {
                let conditions: Vec<String> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql(c, metadata))
                    .collect::<Result<Vec<_>>>()?;
                Ok(format!("({})", conditions.join(" AND ")))
            }
            WhereClause::Or(clauses) => {
                let conditions: Vec<String> = clauses
                    .iter()
                    .map(|c| self.where_clause_to_sql(c, metadata))
                    .collect::<Result<Vec<_>>>()?;
                Ok(format!("({})", conditions.join(" OR ")))
            }
            WhereClause::Not(clause) => {
                let inner = self.where_clause_to_sql(clause, metadata)?;
                Ok(format!("NOT ({})", inner))
            }
        }
    }

    /// Generate WHERE for direct column (denormalized filter)
    fn generate_direct_column_where(
        &self,
        field: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
    ) -> Result<String> {
        let op_sql = self.operator_to_sql(operator);

        // Handle NULL checks (no value needed)
        if matches!(operator, WhereOperator::IsNull) {
            return Ok(format!("{} IS NULL", field));
        }

        // Handle IN/NOT IN (array values)
        if matches!(operator, WhereOperator::In | WhereOperator::Nin) {
            let values = self.format_array_values(value)?;
            return Ok(format!("{} {} ({})", field, op_sql, values));
        }

        // Regular comparison
        let value_sql = self.format_sql_value(value);
        Ok(format!("{} {} {}", field, op_sql, value_sql))
    }

    /// Generate WHERE for JSONB dimension field
    fn generate_jsonb_where(
        &self,
        jsonb_column: &str,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
    ) -> Result<String> {
        let field_path = &path[0]; // Simple path for now (no nested paths)
        let jsonb_extract = self.jsonb_extract_sql(jsonb_column, field_path);
        let op_sql = self.operator_to_sql(operator);

        // Handle NULL checks
        if matches!(operator, WhereOperator::IsNull) {
            return Ok(format!("{} IS NULL", jsonb_extract));
        }

        // Handle case-insensitive operators
        if operator.is_case_insensitive() {
            return self.generate_case_insensitive_where(&jsonb_extract, operator, value);
        }

        // Handle IN/NOT IN for JSONB
        if matches!(operator, WhereOperator::In | WhereOperator::Nin) {
            let values = self.format_array_values(value)?;
            return Ok(format!("{} {} ({})", jsonb_extract, op_sql, values));
        }

        // Handle LIKE pattern operators (Contains, Startswith, Endswith)
        if matches!(operator, WhereOperator::Contains | WhereOperator::Startswith | WhereOperator::Endswith) {
            let value_str = value.as_str()
                .ok_or_else(|| FraiseQLError::validation("LIKE operators require string values"))?;
            let pattern = self.format_like_pattern(operator, value_str);
            return Ok(format!("{} {} {}", jsonb_extract, op_sql, pattern));
        }

        // Regular comparison
        let value_sql = self.format_sql_value(value);
        Ok(format!("{} {} {}", jsonb_extract, op_sql, value_sql))
    }

    /// Convert WhereOperator to SQL operator
    fn operator_to_sql(&self, operator: &WhereOperator) -> &'static str {
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
            }
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Istartswith => {
                match self.database_type {
                    DatabaseType::PostgreSQL => "ILIKE",
                    _ => "LIKE",
                }
            }
            WhereOperator::Endswith => "LIKE",
            WhereOperator::Iendswith => {
                match self.database_type {
                    DatabaseType::PostgreSQL => "ILIKE",
                    _ => "LIKE",
                }
            }
            _ => "=", // Safe default for other operators
        }
    }

    /// Generate case-insensitive WHERE clause
    fn generate_case_insensitive_where(
        &self,
        column: &str,
        operator: &WhereOperator,
        value: &serde_json::Value,
    ) -> Result<String> {
        let value_str = value.as_str()
            .ok_or_else(|| FraiseQLError::validation("Case-insensitive operators require string values"))?;

        match self.database_type {
            DatabaseType::PostgreSQL => {
                // PostgreSQL has ILIKE
                let op = self.operator_to_sql(operator);
                let pattern = self.format_like_pattern(operator, value_str);
                Ok(format!("{} {} {}", column, op, pattern))
            }
            _ => {
                // Other databases: use UPPER() for case-insensitive comparison
                let op = "LIKE";
                let pattern = self.format_like_pattern(operator, &value_str.to_uppercase());
                Ok(format!("UPPER({}) {} {}", column, op, pattern))
            }
        }
    }

    /// Format LIKE pattern based on operator
    fn format_like_pattern(&self, operator: &WhereOperator, value: &str) -> String {
        match operator {
            WhereOperator::Contains | WhereOperator::Icontains => {
                format!("'%{}%'", value.replace('\'', "''"))
            }
            WhereOperator::Startswith | WhereOperator::Istartswith => {
                format!("'{}%'", value.replace('\'', "''"))
            }
            WhereOperator::Endswith | WhereOperator::Iendswith => {
                format!("'%{}'", value.replace('\'', "''"))
            }
            _ => format!("'{}'", value.replace('\'', "''")),
        }
    }

    /// Format array values for IN/NOT IN clauses
    fn format_array_values(&self, value: &serde_json::Value) -> Result<String> {
        let array = value.as_array()
            .ok_or_else(|| FraiseQLError::validation("IN/NOT IN operators require array values"))?;

        let formatted: Vec<String> = array
            .iter()
            .map(|v| self.format_sql_value(v))
            .collect();

        Ok(formatted.join(", "))
    }

    /// Format a single SQL value
    fn format_sql_value(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "NULL".to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            _ => "NULL".to_string(), // Fallback for arrays/objects
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

    /// Build HAVING clause
    fn build_having_clause(&self, having_conditions: &[ValidatedHavingCondition]) -> Result<String> {
        let mut conditions = Vec::new();

        for condition in having_conditions {
            let aggregate_sql = self.aggregate_expression_to_sql(&condition.aggregate)?;
            let operator_sql = condition.operator.sql_operator();

            // Format value based on type
            let value_sql = match &condition.value {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => format!("'{}'", s),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => return Err(FraiseQLError::Validation {
                    message: "Invalid HAVING value type".to_string(),
                    path: None,
                }),
            };

            conditions.push(format!("{} {} {}", aggregate_sql, operator_sql, value_sql));
        }

        Ok(format!("HAVING {}", conditions.join(" AND ")))
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
                format!("{} {}", clause.field, direction)
            })
            .collect();

        Ok(format!("ORDER BY {}", clauses.join(", ")))
    }

    /// Assemble complete SQL query
    fn assemble_sql(
        &self,
        select: &str,
        from: &str,
        where_clause: Option<&str>,
        group_by: Option<&str>,
        having: Option<&str>,
        order_by: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> String {
        let mut sql = String::new();

        sql.push_str(select);
        sql.push('\n');
        sql.push_str(from);

        if let Some(where_sql) = where_clause {
            sql.push('\n');
            sql.push_str(where_sql);
        }

        if let Some(group_by_sql) = group_by {
            sql.push('\n');
            sql.push_str(group_by_sql);
        }

        if let Some(having_sql) = having {
            sql.push('\n');
            sql.push_str(having_sql);
        }

        if let Some(order_by_sql) = order_by {
            sql.push('\n');
            sql.push_str(order_by_sql);
        }

        if let Some(limit_val) = limit {
            sql.push('\n');
            sql.push_str(&format!("LIMIT {}", limit_val));
        }

        if let Some(offset_val) = offset {
            sql.push('\n');
            sql.push_str(&format!("OFFSET {}", offset_val));
        }

        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::aggregation::{AggregateSelection, AggregationRequest, GroupBySelection};
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, FactTableMetadata, MeasureColumn, SqlType};

    fn create_test_plan() -> AggregationPlan {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![MeasureColumn {
                name: "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![FilterColumn {
                name: "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed: true,
            }],
        };

        let request = AggregationRequest {
            table_name: "tf_sales".to_string(),
            where_clause: None,
            group_by: vec![
                GroupBySelection::Dimension {
                    path: "category".to_string(),
                    alias: "category".to_string(),
                },
                GroupBySelection::TemporalBucket {
                    column: "occurred_at".to_string(),
                    bucket: TemporalBucket::Day,
                    alias: "day".to_string(),
                },
            ],
            aggregates: vec![
                AggregateSelection::Count {
                    alias: "count".to_string(),
                },
                AggregateSelection::MeasureAggregate {
                    measure: "revenue".to_string(),
                    function: AggregateFunction::Sum,
                    alias: "revenue_sum".to_string(),
                },
            ],
            having: vec![],
            order_by: vec![],
            limit: Some(10),
            offset: None,
        };

        crate::compiler::aggregation::AggregationPlanner::plan(request, metadata).unwrap()
    }

    #[test]
    fn test_postgres_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("data->>'category'"));
        assert!(sql.complete_sql.contains("DATE_TRUNC('day', occurred_at)"));
        assert!(sql.complete_sql.contains("COUNT(*)"));
        assert!(sql.complete_sql.contains("SUM(revenue)"));
        assert!(sql.complete_sql.contains("GROUP BY"));
        assert!(sql.complete_sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_mysql_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("JSON_UNQUOTE(JSON_EXTRACT(data, '$.category'))"));
        assert!(sql.complete_sql.contains("DATE_FORMAT(occurred_at"));
        assert!(sql.complete_sql.contains("COUNT(*)"));
        assert!(sql.complete_sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_sqlite_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLite);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("json_extract(data, '$.category')"));
        assert!(sql.complete_sql.contains("strftime"));
        assert!(sql.complete_sql.contains("COUNT(*)"));
        assert!(sql.complete_sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_sqlserver_sql_generation() {
        let plan = create_test_plan();
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("JSON_VALUE(data, '$.category')"));
        assert!(sql.complete_sql.contains("CAST(occurred_at AS DATE)"));
        assert!(sql.complete_sql.contains("COUNT(*)"));
        assert!(sql.complete_sql.contains("SUM(revenue)"));
    }

    #[test]
    fn test_having_clause() {
        let mut plan = create_test_plan();
        plan.having_conditions = vec![ValidatedHavingCondition {
            aggregate: AggregateExpression::MeasureAggregate {
                column: "revenue".to_string(),
                function: AggregateFunction::Sum,
                alias: "revenue_sum".to_string(),
            },
            operator: HavingOperator::Gt,
            value: serde_json::json!(1000),
        }];

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.having.is_some());
        assert!(sql.having.as_ref().unwrap().contains("HAVING SUM(revenue) > 1000"));
    }

    #[test]
    fn test_order_by_clause() {
        use crate::compiler::aggregation::OrderByClause;

        let mut plan = create_test_plan();
        plan.request.order_by = vec![OrderByClause {
            field: "revenue_sum".to_string(),
            direction: OrderDirection::Desc,
        }];

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.order_by.is_some());
        assert!(sql.order_by.as_ref().unwrap().contains("ORDER BY revenue_sum DESC"));
    }

    // ========================================
    // Phase 6: Advanced Aggregates Tests
    // ========================================

    #[test]
    fn test_array_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);

        // Test without ORDER BY
        let sql = generator.generate_array_agg_sql("product_id", None);
        assert_eq!(sql, "ARRAY_AGG(product_id)");

        // Test with ORDER BY
        let order_by = vec![OrderByClause {
            field: "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_array_agg_sql("product_id", Some(&order_by));
        assert_eq!(sql, "ARRAY_AGG(product_id ORDER BY revenue DESC)");
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
            field: "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert_eq!(sql, "STRING_AGG(product_name, ', ' ORDER BY revenue DESC)");
    }

    #[test]
    fn test_string_agg_mysql() {
        let generator = AggregationSqlGenerator::new(DatabaseType::MySQL);

        let order_by = vec![OrderByClause {
            field: "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert_eq!(sql, "GROUP_CONCAT(product_name ORDER BY revenue DESC SEPARATOR ', ')");
    }

    #[test]
    fn test_string_agg_sqlserver() {
        let generator = AggregationSqlGenerator::new(DatabaseType::SQLServer);

        let order_by = vec![OrderByClause {
            field: "revenue".to_string(),
            direction: OrderDirection::Desc,
        }];
        let sql = generator.generate_string_agg_sql("product_name", ", ", Some(&order_by));
        assert!(sql.contains("STRING_AGG(CAST(product_name AS NVARCHAR(MAX)), ', ')"));
        assert!(sql.contains("WITHIN GROUP (ORDER BY revenue DESC)"));
    }

    #[test]
    fn test_json_agg_postgres() {
        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate_json_agg_sql("data", None);
        assert_eq!(sql, "JSON_AGG(data)");

        let order_by = vec![OrderByClause {
            field: "created_at".to_string(),
            direction: OrderDirection::Asc,
        }];
        let sql = generator.generate_json_agg_sql("data", Some(&order_by));
        assert_eq!(sql, "JSON_AGG(data ORDER BY created_at ASC)");
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
            column: "product_id".to_string(),
            function: AggregateFunction::ArrayAgg,
            alias: "products".to_string(),
            delimiter: None,
            order_by: Some(vec![OrderByClause {
                field: "revenue".to_string(),
                direction: OrderDirection::Desc,
            }]),
        });

        // Add a STRING_AGG aggregate
        plan.aggregate_expressions.push(AggregateExpression::AdvancedAggregate {
            column: "product_name".to_string(),
            function: AggregateFunction::StringAgg,
            alias: "product_names".to_string(),
            delimiter: Some(", ".to_string()),
            order_by: None,
        });

        let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("ARRAY_AGG(product_id ORDER BY revenue DESC)"));
        assert!(sql.complete_sql.contains("STRING_AGG(product_name, ', ')"));
    }
}
