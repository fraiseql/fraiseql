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
use crate::db::types::DatabaseType;
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
            Some(self.build_where_clause(where_clause)?)
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
                | AggregateExpression::MeasureAggregate { alias, .. } => alias,
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
        }
    }

    /// Build WHERE clause
    fn build_where_clause(&self, _where_clause: &crate::db::where_clause::WhereClause) -> Result<String> {
        // TODO: Use WhereClauseGenerator from db module
        // For now, return a placeholder
        Ok("WHERE 1=1".to_string())
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
        use crate::compiler::aggregation::HavingCondition;

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
}
