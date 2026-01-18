//! Window Function SQL Generation
//!
//! Generates database-specific SQL for window functions.
//!
//! # Supported Databases
//!
//! - **PostgreSQL**: Full support (all functions + GROUPS frames + frame exclusion)
//! - **MySQL 8.0+**: Full support (no GROUPS, no frame exclusion)
//! - **SQLite 3.25+**: Basic support (no GROUPS, no PERCENT_RANK/CUME_DIST)
//! - **SQL Server**: Full support (STDEV/VAR naming difference)

use crate::{
    compiler::{
        aggregation::OrderDirection,
        window_functions::{
            FrameBoundary, FrameExclusion, FrameType, WindowExecutionPlan, WindowFrame,
            WindowFunction, WindowFunctionType,
        },
    },
    db::types::DatabaseType,
    error::{FraiseQLError, Result},
};

/// Generated SQL for window function query
#[derive(Debug, Clone)]
pub struct WindowSql {
    /// Complete SQL query
    pub complete_sql: String,

    /// Parameterized values (for WHERE clause)
    pub parameters: Vec<serde_json::Value>,
}

/// Window function SQL generator
pub struct WindowSqlGenerator {
    database_type: DatabaseType,
}

impl WindowSqlGenerator {
    /// Create new generator for database type
    #[must_use]
    pub const fn new(database_type: DatabaseType) -> Self {
        Self { database_type }
    }

    /// Generate SQL from window execution plan
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Unsupported function for database
    /// - Invalid frame specification
    /// - WHERE clause generation fails
    pub fn generate(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        match self.database_type {
            DatabaseType::PostgreSQL => self.generate_postgres(plan),
            DatabaseType::MySQL => self.generate_mysql(plan),
            DatabaseType::SQLite => self.generate_sqlite(plan),
            DatabaseType::SQLServer => self.generate_sqlserver(plan),
        }
    }

    /// Generate PostgreSQL window function SQL
    fn generate_postgres(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        let mut sql = String::from("SELECT ");
        let parameters = Vec::new();

        // Add regular SELECT columns
        for (i, col) in plan.select.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} AS {}", col.expression, col.alias));
        }

        // Add window functions
        for window in &plan.windows {
            if !plan.select.is_empty() || sql.len() > "SELECT ".len() {
                sql.push_str(", ");
            }
            sql.push_str(&self.generate_window_function(window)?);
        }

        // FROM clause
        sql.push_str(&format!(" FROM {}", plan.table));

        // WHERE clause (if any)
        if plan.where_clause.is_some() {
            sql.push_str(" WHERE 1=1"); // Placeholder
        }

        // ORDER BY clause
        if !plan.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            for (i, order) in plan.order_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                let dir = match order.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };
                sql.push_str(&format!("{} {}", order.field, dir));
            }
        }

        // LIMIT / OFFSET
        if let Some(limit) = plan.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = plan.offset {
            sql.push_str(&format!(" OFFSET {offset}"));
        }

        Ok(WindowSql {
            complete_sql: sql,
            parameters,
        })
    }

    /// Generate window function expression
    fn generate_window_function(&self, window: &WindowFunction) -> Result<String> {
        let func_sql = self.generate_function_call(&window.function)?;
        let mut sql = format!("{func_sql} OVER (");

        // PARTITION BY
        if !window.partition_by.is_empty() {
            sql.push_str("PARTITION BY ");
            sql.push_str(&window.partition_by.join(", "));
        }

        // ORDER BY
        if !window.order_by.is_empty() {
            if !window.partition_by.is_empty() {
                sql.push(' ');
            }
            sql.push_str("ORDER BY ");
            for (i, order) in window.order_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                let dir = match order.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };
                sql.push_str(&format!("{} {}", order.field, dir));
            }
        }

        // Frame clause
        if let Some(frame) = &window.frame {
            if !window.partition_by.is_empty() || !window.order_by.is_empty() {
                sql.push(' ');
            }
            sql.push_str(&self.generate_frame_clause(frame)?);
        }

        sql.push(')');
        sql.push_str(&format!(" AS {}", window.alias));

        Ok(sql)
    }

    /// Generate function call SQL
    fn generate_function_call(&self, function: &WindowFunctionType) -> Result<String> {
        let sql = match function {
            WindowFunctionType::RowNumber => "ROW_NUMBER()".to_string(),
            WindowFunctionType::Rank => "RANK()".to_string(),
            WindowFunctionType::DenseRank => "DENSE_RANK()".to_string(),
            WindowFunctionType::Ntile { n } => format!("NTILE({n})"),
            WindowFunctionType::PercentRank => "PERCENT_RANK()".to_string(),
            WindowFunctionType::CumeDist => "CUME_DIST()".to_string(),

            WindowFunctionType::Lag {
                field,
                offset,
                default,
            } => {
                if let Some(default_val) = default {
                    format!("LAG({field}, {offset}, {default_val})")
                } else {
                    format!("LAG({field}, {offset})")
                }
            },
            WindowFunctionType::Lead {
                field,
                offset,
                default,
            } => {
                if let Some(default_val) = default {
                    format!("LEAD({field}, {offset}, {default_val})")
                } else {
                    format!("LEAD({field}, {offset})")
                }
            },
            WindowFunctionType::FirstValue { field } => format!("FIRST_VALUE({field})"),
            WindowFunctionType::LastValue { field } => format!("LAST_VALUE({field})"),
            WindowFunctionType::NthValue { field, n } => format!("NTH_VALUE({field}, {n})"),

            WindowFunctionType::Sum { field } => format!("SUM({field})"),
            WindowFunctionType::Avg { field } => format!("AVG({field})"),
            WindowFunctionType::Count { field: Some(field) } => format!("COUNT({field})"),
            WindowFunctionType::Count { field: None } => "COUNT(*)".to_string(),
            WindowFunctionType::Min { field } => format!("MIN({field})"),
            WindowFunctionType::Max { field } => format!("MAX({field})"),
            WindowFunctionType::Stddev { field } => {
                // PostgreSQL/MySQL use STDDEV, SQL Server uses STDEV
                match self.database_type {
                    DatabaseType::SQLServer => format!("STDEV({field})"),
                    _ => format!("STDDEV({field})"),
                }
            },
            WindowFunctionType::Variance { field } => {
                // PostgreSQL/MySQL use VARIANCE, SQL Server uses VAR
                match self.database_type {
                    DatabaseType::SQLServer => format!("VAR({field})"),
                    _ => format!("VARIANCE({field})"),
                }
            },
        };

        Ok(sql)
    }

    /// Generate window frame clause
    fn generate_frame_clause(&self, frame: &WindowFrame) -> Result<String> {
        let frame_type = match frame.frame_type {
            FrameType::Rows => "ROWS",
            FrameType::Range => "RANGE",
            FrameType::Groups => {
                if !matches!(self.database_type, DatabaseType::PostgreSQL) {
                    return Err(FraiseQLError::validation(
                        "GROUPS frame type only supported on PostgreSQL",
                    ));
                }
                "GROUPS"
            },
        };

        let start = self.format_frame_boundary(&frame.start);
        let end = self.format_frame_boundary(&frame.end);

        let mut sql = format!("{frame_type} BETWEEN {start} AND {end}");

        // Frame exclusion (PostgreSQL only)
        if let Some(exclusion) = &frame.exclusion {
            if matches!(self.database_type, DatabaseType::PostgreSQL) {
                let excl = match exclusion {
                    FrameExclusion::CurrentRow => "EXCLUDE CURRENT ROW",
                    FrameExclusion::Group => "EXCLUDE GROUP",
                    FrameExclusion::Ties => "EXCLUDE TIES",
                    FrameExclusion::NoOthers => "EXCLUDE NO OTHERS",
                };
                sql.push_str(&format!(" {excl}"));
            }
        }

        Ok(sql)
    }

    /// Format frame boundary
    #[must_use]
    pub fn format_frame_boundary(&self, boundary: &FrameBoundary) -> String {
        match boundary {
            FrameBoundary::UnboundedPreceding => "UNBOUNDED PRECEDING".to_string(),
            FrameBoundary::NPreceding { n } => format!("{n} PRECEDING"),
            FrameBoundary::CurrentRow => "CURRENT ROW".to_string(),
            FrameBoundary::NFollowing { n } => format!("{n} FOLLOWING"),
            FrameBoundary::UnboundedFollowing => "UNBOUNDED FOLLOWING".to_string(),
        }
    }

    /// Generate MySQL window function SQL
    fn generate_mysql(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // MySQL 8.0+ supports window functions similar to PostgreSQL
        // Main differences handled in generate_function_call (no STDEV/VAR differences for window
        // functions)
        self.generate_postgres(plan)
    }

    /// Generate SQLite window function SQL
    fn generate_sqlite(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // SQLite 3.25+ supports window functions
        // Similar to PostgreSQL but no PERCENT_RANK, CUME_DIST validation done in planner
        self.generate_postgres(plan)
    }

    /// Generate SQL Server window function SQL
    fn generate_sqlserver(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // SQL Server supports window functions with minor differences (STDEV/VAR naming)
        self.generate_postgres(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{
        aggregation::{OrderByClause, OrderDirection},
        window_functions::*,
    };

    #[test]
    fn test_generate_row_number() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![SelectColumn {
                expression: "revenue".to_string(),
                alias:      "revenue".to_string(),
            }],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec!["data->>'category'".to_string()],
                order_by:     vec![OrderByClause {
                    field:     "revenue".to_string(),
                    direction: OrderDirection::Desc,
                }],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("ROW_NUMBER()"));
        assert!(sql.complete_sql.contains("PARTITION BY data->>'category'"));
        assert!(sql.complete_sql.contains("ORDER BY revenue DESC"));
    }

    #[test]
    fn test_generate_running_total() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![
                SelectColumn {
                    expression: "occurred_at".to_string(),
                    alias:      "date".to_string(),
                },
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias:      "revenue".to_string(),
                },
            ],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::Sum {
                    field: "revenue".to_string(),
                },
                alias:        "running_total".to_string(),
                partition_by: vec![],
                order_by:     vec![OrderByClause {
                    field:     "occurred_at".to_string(),
                    direction: OrderDirection::Asc,
                }],
                frame:        Some(WindowFrame {
                    frame_type: FrameType::Rows,
                    start:      FrameBoundary::UnboundedPreceding,
                    end:        FrameBoundary::CurrentRow,
                    exclusion:  None,
                }),
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("SUM(revenue) OVER"));
        assert!(sql.complete_sql.contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_generate_lag_lead() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![
                WindowFunction {
                    function:     WindowFunctionType::Lag {
                        field:   "revenue".to_string(),
                        offset:  1,
                        default: Some(serde_json::json!(0)),
                    },
                    alias:        "prev_revenue".to_string(),
                    partition_by: vec![],
                    order_by:     vec![OrderByClause {
                        field:     "occurred_at".to_string(),
                        direction: OrderDirection::Asc,
                    }],
                    frame:        None,
                },
                WindowFunction {
                    function:     WindowFunctionType::Lead {
                        field:   "revenue".to_string(),
                        offset:  1,
                        default: None,
                    },
                    alias:        "next_revenue".to_string(),
                    partition_by: vec![],
                    order_by:     vec![OrderByClause {
                        field:     "occurred_at".to_string(),
                        direction: OrderDirection::Asc,
                    }],
                    frame:        None,
                },
            ],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("LAG(revenue, 1, 0)"));
        assert!(sql.complete_sql.contains("LEAD(revenue, 1)"));
    }

    #[test]
    fn test_frame_boundary_formatting() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::UnboundedPreceding),
            "UNBOUNDED PRECEDING"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::NPreceding { n: 5 }),
            "5 PRECEDING"
        );
        assert_eq!(generator.format_frame_boundary(&FrameBoundary::CurrentRow), "CURRENT ROW");
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::NFollowing { n: 3 }),
            "3 FOLLOWING"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::UnboundedFollowing),
            "UNBOUNDED FOLLOWING"
        );
    }

    #[test]
    fn test_moving_average() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::Avg {
                    field: "revenue".to_string(),
                },
                alias:        "moving_avg_7d".to_string(),
                partition_by: vec![],
                order_by:     vec![OrderByClause {
                    field:     "occurred_at".to_string(),
                    direction: OrderDirection::Asc,
                }],
                frame:        Some(WindowFrame {
                    frame_type: FrameType::Rows,
                    start:      FrameBoundary::NPreceding { n: 6 },
                    end:        FrameBoundary::CurrentRow,
                    exclusion:  None,
                }),
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("AVG(revenue) OVER"));
        assert!(sql.complete_sql.contains("ROWS BETWEEN 6 PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_sqlserver_stddev_variance() {
        let generator = WindowSqlGenerator::new(DatabaseType::SQLServer);

        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![
                WindowFunction {
                    function:     WindowFunctionType::Stddev {
                        field: "revenue".to_string(),
                    },
                    alias:        "stddev".to_string(),
                    partition_by: vec![],
                    order_by:     vec![],
                    frame:        None,
                },
                WindowFunction {
                    function:     WindowFunctionType::Variance {
                        field: "revenue".to_string(),
                    },
                    alias:        "variance".to_string(),
                    partition_by: vec![],
                    order_by:     vec![],
                    frame:        None,
                },
            ],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let sql = generator.generate(&plan).unwrap();

        // SQL Server uses STDEV/VAR instead of STDDEV/VARIANCE
        assert!(sql.complete_sql.contains("STDEV(revenue)"));
        assert!(sql.complete_sql.contains("VAR(revenue)"));
    }
}
