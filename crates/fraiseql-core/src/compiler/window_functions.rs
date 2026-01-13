//! Window Function Planning Module
//!
//! Generates execution plans for SQL window functions.
//!
//! # Window Functions
//!
//! Window functions perform calculations across sets of table rows that are related
//! to the current row, without collapsing them into a single output row like GROUP BY.
//!
//! ## Function Types
//!
//! ### Ranking Functions
//! - `ROW_NUMBER()` - Sequential number within partition
//! - `RANK()` - Rank with gaps for ties
//! - `DENSE_RANK()` - Rank without gaps
//! - `NTILE(n)` - Divide rows into n groups
//! - `PERCENT_RANK()` - Relative rank (0 to 1)
//! - `CUME_DIST()` - Cumulative distribution
//!
//! ### Value Functions
//! - `LAG(field, offset)` - Value from previous row
//! - `LEAD(field, offset)` - Value from next row
//! - `FIRST_VALUE(field)` - First value in window
//! - `LAST_VALUE(field)` - Last value in window
//! - `NTH_VALUE(field, n)` - Nth value in window
//!
//! ### Aggregate as Window
//! - `SUM(field) OVER (...)` - Running total
//! - `AVG(field) OVER (...)` - Moving average
//! - `COUNT(*) OVER (...)` - Running count
//!
//! # Example
//!
//! ```sql
//! -- Running total
//! SELECT
//!     occurred_at,
//!     revenue,
//!     SUM(revenue) OVER (
//!         ORDER BY occurred_at
//!         ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
//!     ) as running_total
//! FROM tf_sales;
//!
//! -- Ranking
//! SELECT
//!     category,
//!     revenue,
//!     ROW_NUMBER() OVER (
//!         PARTITION BY category
//!         ORDER BY revenue DESC
//!     ) as rank
//! FROM tf_sales;
//! ```

use crate::compiler::aggregation::{OrderByClause, OrderDirection};
use crate::compiler::fact_table::FactTableMetadata;
use crate::db::where_clause::WhereClause;
use crate::error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

/// Window function execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowExecutionPlan {
    /// Source table name
    pub table: String,

    /// Regular SELECT columns (non-window)
    pub select: Vec<SelectColumn>,

    /// Window function expressions
    pub windows: Vec<WindowFunction>,

    /// WHERE clause filter
    pub where_clause: Option<WhereClause>,

    /// Final ORDER BY (after window computation)
    pub order_by: Vec<OrderByClause>,

    /// Result limit
    pub limit: Option<u32>,

    /// Result offset
    pub offset: Option<u32>,
}

/// Regular SELECT column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectColumn {
    /// Column expression (e.g., "revenue", "data->>'category'")
    pub expression: String,

    /// Result alias
    pub alias: String,
}

/// Window function specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowFunction {
    /// Window function type
    pub function: WindowFunctionType,

    /// Result column alias
    pub alias: String,

    /// PARTITION BY columns
    pub partition_by: Vec<String>,

    /// ORDER BY within window
    pub order_by: Vec<OrderByClause>,

    /// Window frame specification
    pub frame: Option<WindowFrame>,
}

/// Window function types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WindowFunctionType {
    // Ranking functions
    /// ROW_NUMBER() - Sequential number within partition
    RowNumber,

    /// RANK() - Rank with gaps for ties
    Rank,

    /// DENSE_RANK() - Rank without gaps
    DenseRank,

    /// NTILE(n) - Divide rows into n groups
    Ntile { n: u32 },

    /// PERCENT_RANK() - Relative rank (0 to 1)
    PercentRank,

    /// CUME_DIST() - Cumulative distribution
    CumeDist,

    // Value functions
    /// LAG(field, offset, default) - Value from previous row
    Lag {
        field: String,
        offset: i32,
        default: Option<serde_json::Value>,
    },

    /// LEAD(field, offset, default) - Value from next row
    Lead {
        field: String,
        offset: i32,
        default: Option<serde_json::Value>,
    },

    /// FIRST_VALUE(field) - First value in window
    FirstValue { field: String },

    /// LAST_VALUE(field) - Last value in window
    LastValue { field: String },

    /// NTH_VALUE(field, n) - Nth value in window
    NthValue { field: String, n: u32 },

    // Aggregate as window functions
    /// SUM(field) OVER (...) - Running total
    Sum { field: String },

    /// AVG(field) OVER (...) - Moving average
    Avg { field: String },

    /// COUNT(*) OVER (...) - Running count
    Count { field: Option<String> },

    /// MIN(field) OVER (...) - Running minimum
    Min { field: String },

    /// MAX(field) OVER (...) - Running maximum
    Max { field: String },

    /// STDDEV(field) OVER (...) - Running standard deviation
    Stddev { field: String },

    /// VARIANCE(field) OVER (...) - Running variance
    Variance { field: String },
}

/// Window frame specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowFrame {
    /// Frame type (ROWS, RANGE, GROUPS)
    pub frame_type: FrameType,

    /// Frame start boundary
    pub start: FrameBoundary,

    /// Frame end boundary
    pub end: FrameBoundary,

    /// Frame exclusion (PostgreSQL only)
    pub exclusion: Option<FrameExclusion>,
}

/// Window frame type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameType {
    /// ROWS frame - Physical rows
    Rows,

    /// RANGE frame - Logical range based on ORDER BY
    Range,

    /// GROUPS frame - Peer groups (PostgreSQL only)
    Groups,
}

/// Window frame boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrameBoundary {
    /// UNBOUNDED PRECEDING
    UnboundedPreceding,

    /// N PRECEDING
    NPreceding { n: u32 },

    /// CURRENT ROW
    CurrentRow,

    /// N FOLLOWING
    NFollowing { n: u32 },

    /// UNBOUNDED FOLLOWING
    UnboundedFollowing,
}

/// Frame exclusion mode (PostgreSQL)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameExclusion {
    /// EXCLUDE CURRENT ROW
    CurrentRow,

    /// EXCLUDE GROUP
    Group,

    /// EXCLUDE TIES
    Ties,

    /// EXCLUDE NO OTHERS
    NoOthers,
}

/// Window function plan generator
pub struct WindowFunctionPlanner;

impl WindowFunctionPlanner {
    /// Generate window function execution plan from JSON query
    ///
    /// # Example Query Format
    ///
    /// ```json
    /// {
    ///   "table": "tf_sales",
    ///   "select": ["revenue", "category"],
    ///   "windows": [
    ///     {
    ///       "function": {"row_number": {}},
    ///       "alias": "rank",
    ///       "partitionBy": ["data->>'category'"],
    ///       "orderBy": [{"field": "revenue", "direction": "DESC"}]
    ///     }
    ///   ],
    ///   "limit": 10
    /// }
    /// ```
    pub fn plan(
        query: &serde_json::Value,
        _metadata: &FactTableMetadata,
    ) -> Result<WindowExecutionPlan> {
        // Parse table name
        let table = query["table"]
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Missing 'table' field"))?
            .to_string();

        // Parse SELECT columns
        let select = Self::parse_select_columns(query)?;

        // Parse window functions
        let windows = Self::parse_window_functions(query)?;

        // Parse WHERE clause (placeholder - full implementation would parse actual conditions)
        let where_clause = query.get("where").map(|_| WhereClause::And(vec![]));

        // Parse ORDER BY
        let order_by = query
            .get("orderBy")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let direction = match item.get("direction").and_then(|d| d.as_str()) {
                            Some("DESC") => OrderDirection::Desc,
                            _ => OrderDirection::Asc,
                        };
                        Some(OrderByClause {
                            field: item["field"].as_str()?.to_string(),
                            direction,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse LIMIT/OFFSET
        let limit = query.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32);
        let offset = query.get("offset").and_then(|v| v.as_u64()).map(|n| n as u32);

        Ok(WindowExecutionPlan {
            table,
            select,
            windows,
            where_clause,
            order_by,
            limit,
            offset,
        })
    }

    fn parse_select_columns(query: &serde_json::Value) -> Result<Vec<SelectColumn>> {
        let default_array = vec![];
        let select = query
            .get("select")
            .and_then(|s| s.as_array())
            .unwrap_or(&default_array);

        let columns = select
            .iter()
            .filter_map(|col| {
                if let Some(col_str) = col.as_str() {
                    Some(SelectColumn {
                        expression: col_str.to_string(),
                        alias: col_str.to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(columns)
    }

    fn parse_window_functions(query: &serde_json::Value) -> Result<Vec<WindowFunction>> {
        let default_array = vec![];
        let windows = query
            .get("windows")
            .and_then(|w| w.as_array())
            .unwrap_or(&default_array);

        windows
            .iter()
            .map(|window| Self::parse_single_window(window))
            .collect()
    }

    fn parse_single_window(window: &serde_json::Value) -> Result<WindowFunction> {
        let function = Self::parse_window_function_type(&window["function"])?;
        let alias = window["alias"]
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Missing 'alias' in window function"))?
            .to_string();

        let partition_by = window
            .get("partitionBy")
            .and_then(|p| p.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let order_by = window
            .get("orderBy")
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let direction = match item.get("direction").and_then(|d| d.as_str()) {
                            Some("DESC") => OrderDirection::Desc,
                            _ => OrderDirection::Asc,
                        };
                        Some(OrderByClause {
                            field: item["field"].as_str()?.to_string(),
                            direction,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let frame = window.get("frame").map(Self::parse_window_frame).transpose()?;

        Ok(WindowFunction {
            function,
            alias,
            partition_by,
            order_by,
            frame,
        })
    }

    fn parse_window_function_type(func: &serde_json::Value) -> Result<WindowFunctionType> {
        // Try each function type
        if func.get("row_number").is_some() {
            return Ok(WindowFunctionType::RowNumber);
        }
        if func.get("rank").is_some() {
            return Ok(WindowFunctionType::Rank);
        }
        if func.get("dense_rank").is_some() {
            return Ok(WindowFunctionType::DenseRank);
        }
        if let Some(ntile) = func.get("ntile") {
            let n = ntile["n"].as_u64().ok_or_else(|| {
                FraiseQLError::validation("Missing 'n' in NTILE function")
            })? as u32;
            return Ok(WindowFunctionType::Ntile { n });
        }
        if func.get("percent_rank").is_some() {
            return Ok(WindowFunctionType::PercentRank);
        }
        if func.get("cume_dist").is_some() {
            return Ok(WindowFunctionType::CumeDist);
        }

        // Value functions
        if let Some(lag) = func.get("lag") {
            let field = lag["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in LAG"))?
                .to_string();
            let offset = lag.get("offset").and_then(|o| o.as_i64()).unwrap_or(1) as i32;
            let default = lag.get("default").cloned();
            return Ok(WindowFunctionType::Lag {
                field,
                offset,
                default,
            });
        }
        if let Some(lead) = func.get("lead") {
            let field = lead["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in LEAD"))?
                .to_string();
            let offset = lead.get("offset").and_then(|o| o.as_i64()).unwrap_or(1) as i32;
            let default = lead.get("default").cloned();
            return Ok(WindowFunctionType::Lead {
                field,
                offset,
                default,
            });
        }
        if let Some(first_val) = func.get("first_value") {
            let field = first_val["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in FIRST_VALUE"))?
                .to_string();
            return Ok(WindowFunctionType::FirstValue { field });
        }
        if let Some(last_val) = func.get("last_value") {
            let field = last_val["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in LAST_VALUE"))?
                .to_string();
            return Ok(WindowFunctionType::LastValue { field });
        }
        if let Some(nth_val) = func.get("nth_value") {
            let field = nth_val["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in NTH_VALUE"))?
                .to_string();
            let n = nth_val["n"]
                .as_u64()
                .ok_or_else(|| FraiseQLError::validation("Missing 'n' in NTH_VALUE"))?
                as u32;
            return Ok(WindowFunctionType::NthValue { field, n });
        }

        // Aggregate as window
        if let Some(sum) = func.get("sum") {
            let field = sum["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in SUM"))?
                .to_string();
            return Ok(WindowFunctionType::Sum { field });
        }
        if let Some(avg) = func.get("avg") {
            let field = avg["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in AVG"))?
                .to_string();
            return Ok(WindowFunctionType::Avg { field });
        }
        if let Some(count) = func.get("count") {
            let field = count.get("field").and_then(|f| f.as_str()).map(String::from);
            return Ok(WindowFunctionType::Count { field });
        }
        if let Some(min) = func.get("min") {
            let field = min["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in MIN"))?
                .to_string();
            return Ok(WindowFunctionType::Min { field });
        }
        if let Some(max) = func.get("max") {
            let field = max["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in MAX"))?
                .to_string();
            return Ok(WindowFunctionType::Max { field });
        }
        if let Some(stddev) = func.get("stddev") {
            let field = stddev["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in STDDEV"))?
                .to_string();
            return Ok(WindowFunctionType::Stddev { field });
        }
        if let Some(variance) = func.get("variance") {
            let field = variance["field"]
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Missing 'field' in VARIANCE"))?
                .to_string();
            return Ok(WindowFunctionType::Variance { field });
        }

        Err(FraiseQLError::validation("Unknown window function type"))
    }

    fn parse_window_frame(frame: &serde_json::Value) -> Result<WindowFrame> {
        let frame_type = match frame["frame_type"].as_str() {
            Some("ROWS") => FrameType::Rows,
            Some("RANGE") => FrameType::Range,
            Some("GROUPS") => FrameType::Groups,
            _ => {
                return Err(FraiseQLError::validation(
                    "Invalid or missing 'frame_type'",
                ))
            }
        };

        let start = Self::parse_frame_boundary(&frame["start"])?;
        let end = Self::parse_frame_boundary(&frame["end"])?;
        let exclusion = frame.get("exclusion").map(|e| match e.as_str() {
            Some("current_row") => FrameExclusion::CurrentRow,
            Some("group") => FrameExclusion::Group,
            Some("ties") => FrameExclusion::Ties,
            Some("no_others") => FrameExclusion::NoOthers,
            _ => FrameExclusion::NoOthers,
        });

        Ok(WindowFrame {
            frame_type,
            start,
            end,
            exclusion,
        })
    }

    fn parse_frame_boundary(boundary: &serde_json::Value) -> Result<FrameBoundary> {
        match boundary["type"].as_str() {
            Some("unbounded_preceding") => Ok(FrameBoundary::UnboundedPreceding),
            Some("n_preceding") => {
                let n = boundary["n"]
                    .as_u64()
                    .ok_or_else(|| FraiseQLError::validation("Missing 'n' in N PRECEDING"))?
                    as u32;
                Ok(FrameBoundary::NPreceding { n })
            }
            Some("current_row") => Ok(FrameBoundary::CurrentRow),
            Some("n_following") => {
                let n = boundary["n"]
                    .as_u64()
                    .ok_or_else(|| FraiseQLError::validation("Missing 'n' in N FOLLOWING"))?
                    as u32;
                Ok(FrameBoundary::NFollowing { n })
            }
            Some("unbounded_following") => Ok(FrameBoundary::UnboundedFollowing),
            _ => Err(FraiseQLError::validation("Invalid frame boundary type")),
        }
    }

    /// Validate window function plan
    pub fn validate(
        plan: &WindowExecutionPlan,
        _metadata: &FactTableMetadata,
        database_target: crate::db::types::DatabaseType,
    ) -> Result<()> {
        use crate::db::types::DatabaseType;

        // Validate frame type supported by database
        for window in &plan.windows {
            if let Some(frame) = &window.frame {
                if frame.frame_type == FrameType::Groups {
                    if !matches!(database_target, DatabaseType::PostgreSQL) {
                        return Err(FraiseQLError::validation(
                            "GROUPS frame type only supported on PostgreSQL",
                        ));
                    }
                }

                // Validate frame exclusion (PostgreSQL only)
                if frame.exclusion.is_some() && !matches!(database_target, DatabaseType::PostgreSQL)
                {
                    return Err(FraiseQLError::validation(
                        "Frame exclusion only supported on PostgreSQL",
                    ));
                }
            }

            // Validate PERCENT_RANK and CUME_DIST (not in SQLite)
            match window.function {
                WindowFunctionType::PercentRank | WindowFunctionType::CumeDist => {
                    if matches!(database_target, DatabaseType::SQLite) {
                        return Err(FraiseQLError::validation(
                            "PERCENT_RANK and CUME_DIST not supported on SQLite",
                        ));
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, MeasureColumn, SqlType};

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
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
            denormalized_filters: vec![],
        }
    }

    #[test]
    fn test_window_function_type_serialization() {
        let func = WindowFunctionType::RowNumber;
        let json = serde_json::to_string(&func).unwrap();
        assert_eq!(json, r#"{"type":"row_number"}"#);
    }

    #[test]
    fn test_frame_type_serialization() {
        let frame_type = FrameType::Rows;
        let json = serde_json::to_string(&frame_type).unwrap();
        assert_eq!(json, r#""ROWS""#);
    }

    #[test]
    fn test_frame_boundary_unbounded() {
        let boundary = FrameBoundary::UnboundedPreceding;
        let json = serde_json::to_string(&boundary).unwrap();
        assert!(json.contains("unbounded_preceding"));
    }

    #[test]
    fn test_frame_boundary_n_preceding() {
        let boundary = FrameBoundary::NPreceding { n: 5 };
        let json = serde_json::to_string(&boundary).unwrap();
        assert!(json.contains("n_preceding"));
        assert!(json.contains("\"n\":5"));
    }

    #[test]
    fn test_parse_row_number_query() {
        let metadata = create_test_metadata();
        let query = serde_json::json!({
            "table": "tf_sales",
            "select": ["revenue"],
            "windows": [{
                "function": {"row_number": {}},
                "alias": "rank",
                "partitionBy": ["category"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }]
        });

        let plan = WindowFunctionPlanner::plan(&query, &metadata).unwrap();

        assert_eq!(plan.table, "tf_sales");
        assert_eq!(plan.windows.len(), 1);
        assert_eq!(plan.windows[0].alias, "rank");
        assert!(matches!(
            plan.windows[0].function,
            WindowFunctionType::RowNumber
        ));
    }

    #[test]
    fn test_parse_lag_function() {
        let metadata = create_test_metadata();
        let query = serde_json::json!({
            "table": "tf_sales",
            "windows": [{
                "function": {
                    "lag": {
                        "field": "revenue",
                        "offset": 1,
                        "default": 0
                    }
                },
                "alias": "prev_revenue",
                "orderBy": [{"field": "occurred_at"}]
            }]
        });

        let plan = WindowFunctionPlanner::plan(&query, &metadata).unwrap();

        match &plan.windows[0].function {
            WindowFunctionType::Lag {
                field,
                offset,
                default,
            } => {
                assert_eq!(field, "revenue");
                assert_eq!(*offset, 1);
                assert!(default.is_some());
            }
            _ => panic!("Expected LAG function"),
        }
    }

    #[test]
    fn test_validate_groups_frame_postgres_only() {
        use crate::db::types::DatabaseType;

        let metadata = create_test_metadata();
        let plan = WindowExecutionPlan {
            table: "tf_sales".to_string(),
            select: vec![],
            windows: vec![WindowFunction {
                function: WindowFunctionType::RowNumber,
                alias: "rank".to_string(),
                partition_by: vec![],
                order_by: vec![],
                frame: Some(WindowFrame {
                    frame_type: FrameType::Groups,
                    start: FrameBoundary::UnboundedPreceding,
                    end: FrameBoundary::CurrentRow,
                    exclusion: None,
                }),
            }],
            where_clause: None,
            order_by: vec![],
            limit: None,
            offset: None,
        };

        // Should pass for PostgreSQL
        assert!(WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::PostgreSQL)
            .is_ok());

        // Should fail for MySQL
        assert!(
            WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::MySQL).is_err()
        );
    }
}
