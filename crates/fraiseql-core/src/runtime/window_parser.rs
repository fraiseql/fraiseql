//! Window Query Parser
//!
//! Parses GraphQL window queries into `WindowRequest` for execution.
//!
//! # GraphQL Query Format
//!
//! ```graphql
//! query {
//!   sales_window(
//!     where: { customer_id: { _eq: "uuid-123" } }
//!     orderBy: { occurred_at: ASC }
//!     limit: 100
//!   ) {
//!     revenue
//!     category
//!     rank: row_number(partitionBy: ["category"], orderBy: { revenue: DESC })
//!     running_total: sum(field: "revenue", orderBy: { occurred_at: ASC })
//!     prev_revenue: lag(field: "revenue", offset: 1, default: 0)
//!   }
//! }
//! ```
//!
//! # JSON Query Format
//!
//! ```json
//! {
//!   "table": "tf_sales",
//!   "select": [
//!     {"type": "measure", "name": "revenue", "alias": "revenue"},
//!     {"type": "dimension", "path": "category", "alias": "category"}
//!   ],
//!   "windows": [
//!     {
//!       "function": {"type": "row_number"},
//!       "alias": "rank",
//!       "partitionBy": [{"type": "dimension", "path": "category"}],
//!       "orderBy": [{"field": "revenue", "direction": "DESC"}]
//!     },
//!     {
//!       "function": {"type": "running_sum", "measure": "revenue"},
//!       "alias": "running_total",
//!       "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
//!       "frame": {"frame_type": "ROWS", "start": {"type": "unbounded_preceding"}, "end": {"type": "current_row"}}
//!     }
//!   ],
//!   "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
//!   "limit": 100
//! }
//! ```

use crate::compiler::aggregation::OrderDirection;
use crate::compiler::fact_table::FactTableMetadata;
use crate::compiler::window_functions::{
    FrameBoundary, FrameExclusion, FrameType, PartitionByColumn, WindowFrame, WindowFunctionRequest,
    WindowFunctionSpec, WindowOrderBy, WindowRequest, WindowSelectColumn,
};
use crate::db::where_clause::{WhereClause, WhereOperator};
use crate::error::{FraiseQLError, Result};
use serde_json::Value;

/// Window query parser
pub struct WindowQueryParser;

impl WindowQueryParser {
    /// Parse a window query JSON into `WindowRequest`.
    ///
    /// # Arguments
    ///
    /// * `query_json` - JSON representation of the window query
    /// * `_metadata` - Fact table metadata (for validation, optional future use)
    ///
    /// # Errors
    ///
    /// Returns error if the query structure is invalid.
    pub fn parse(query_json: &Value, _metadata: &FactTableMetadata) -> Result<WindowRequest> {
        // Extract table name
        let table_name = query_json
            .get("table")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'table' field in window query".to_string(),
                path: None,
            })?
            .to_string();

        // Parse SELECT columns
        let select = if let Some(select_array) = query_json.get("select") {
            Self::parse_select_columns(select_array)?
        } else {
            vec![]
        };

        // Parse window functions
        let windows = if let Some(windows_array) = query_json.get("windows") {
            Self::parse_window_functions(windows_array)?
        } else {
            vec![]
        };

        // Parse WHERE clause
        let where_clause = if let Some(where_obj) = query_json.get("where") {
            Some(Self::parse_where_clause(where_obj)?)
        } else {
            None
        };

        // Parse final ORDER BY
        let order_by = if let Some(order_array) = query_json.get("orderBy") {
            Self::parse_order_by(order_array)?
        } else {
            vec![]
        };

        // Parse LIMIT/OFFSET
        let limit = query_json
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        let offset = query_json
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        Ok(WindowRequest {
            table_name,
            select,
            windows,
            where_clause,
            order_by,
            limit,
            offset,
        })
    }

    /// Parse SELECT columns from JSON array.
    fn parse_select_columns(select_array: &Value) -> Result<Vec<WindowSelectColumn>> {
        let Some(arr) = select_array.as_array() else {
            return Ok(vec![]);
        };

        arr.iter().map(Self::parse_single_select_column).collect()
    }

    fn parse_single_select_column(col: &Value) -> Result<WindowSelectColumn> {
        let col_type = col
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'type' in select column".to_string(),
                path: None,
            })?;

        let alias = col
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'alias' in select column".to_string(),
                path: None,
            })?
            .to_string();

        match col_type {
            "measure" => {
                let name = col
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'name' in measure select column".to_string(),
                        path: None,
                    })?
                    .to_string();
                Ok(WindowSelectColumn::Measure { name, alias })
            }
            "dimension" => {
                let path = col
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'path' in dimension select column".to_string(),
                        path: None,
                    })?
                    .to_string();
                Ok(WindowSelectColumn::Dimension { path, alias })
            }
            "filter" => {
                let name = col
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'name' in filter select column".to_string(),
                        path: None,
                    })?
                    .to_string();
                Ok(WindowSelectColumn::Filter { name, alias })
            }
            _ => Err(FraiseQLError::Validation {
                message: format!("Unknown select column type: {col_type}"),
                path: None,
            }),
        }
    }

    /// Parse window functions from JSON array.
    fn parse_window_functions(windows_array: &Value) -> Result<Vec<WindowFunctionRequest>> {
        let Some(arr) = windows_array.as_array() else {
            return Ok(vec![]);
        };

        arr.iter().map(Self::parse_single_window_function).collect()
    }

    fn parse_single_window_function(window: &Value) -> Result<WindowFunctionRequest> {
        // Parse function spec
        let function = window
            .get("function")
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'function' in window definition".to_string(),
                path: None,
            })
            .and_then(Self::parse_function_spec)?;

        // Parse alias
        let alias = window
            .get("alias")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'alias' in window definition".to_string(),
                path: None,
            })?
            .to_string();

        // Parse PARTITION BY
        let partition_by = if let Some(partition_array) = window.get("partitionBy") {
            Self::parse_partition_by(partition_array)?
        } else {
            vec![]
        };

        // Parse ORDER BY within window
        let order_by = if let Some(order_array) = window.get("orderBy") {
            Self::parse_order_by(order_array)?
        } else {
            vec![]
        };

        // Parse frame
        let frame = window.get("frame").map(Self::parse_frame).transpose()?;

        Ok(WindowFunctionRequest {
            function,
            alias,
            partition_by,
            order_by,
            frame,
        })
    }

    /// Parse window function specification.
    fn parse_function_spec(func: &Value) -> Result<WindowFunctionSpec> {
        let func_type = func
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'type' in function spec".to_string(),
                path: None,
            })?;

        match func_type {
            // Ranking functions
            "row_number" => Ok(WindowFunctionSpec::RowNumber),
            "rank" => Ok(WindowFunctionSpec::Rank),
            "dense_rank" => Ok(WindowFunctionSpec::DenseRank),
            "ntile" => {
                let n = func
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'n' in NTILE function".to_string(),
                        path: None,
                    })? as u32;
                Ok(WindowFunctionSpec::Ntile { n })
            }
            "percent_rank" => Ok(WindowFunctionSpec::PercentRank),
            "cume_dist" => Ok(WindowFunctionSpec::CumeDist),

            // Value functions
            "lag" => {
                let field = Self::extract_string_field(func, "field")?;
                let offset = func.get("offset").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                let default = func.get("default").cloned();
                Ok(WindowFunctionSpec::Lag {
                    field,
                    offset,
                    default,
                })
            }
            "lead" => {
                let field = Self::extract_string_field(func, "field")?;
                let offset = func.get("offset").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                let default = func.get("default").cloned();
                Ok(WindowFunctionSpec::Lead {
                    field,
                    offset,
                    default,
                })
            }
            "first_value" => {
                let field = Self::extract_string_field(func, "field")?;
                Ok(WindowFunctionSpec::FirstValue { field })
            }
            "last_value" => {
                let field = Self::extract_string_field(func, "field")?;
                Ok(WindowFunctionSpec::LastValue { field })
            }
            "nth_value" => {
                let field = Self::extract_string_field(func, "field")?;
                let n = func
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'n' in NTH_VALUE function".to_string(),
                        path: None,
                    })? as u32;
                Ok(WindowFunctionSpec::NthValue { field, n })
            }

            // Aggregate as window functions
            "running_sum" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningSum { measure })
            }
            "running_avg" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningAvg { measure })
            }
            "running_count" => {
                if let Some(field) = func.get("field").and_then(|v| v.as_str()) {
                    Ok(WindowFunctionSpec::RunningCountField {
                        field: field.to_string(),
                    })
                } else {
                    Ok(WindowFunctionSpec::RunningCount)
                }
            }
            "running_min" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningMin { measure })
            }
            "running_max" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningMax { measure })
            }
            "running_stddev" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningStddev { measure })
            }
            "running_variance" => {
                let measure = Self::extract_string_field(func, "measure")?;
                Ok(WindowFunctionSpec::RunningVariance { measure })
            }

            _ => Err(FraiseQLError::Validation {
                message: format!("Unknown window function type: {func_type}"),
                path: None,
            }),
        }
    }

    /// Extract a required string field from JSON object.
    fn extract_string_field(obj: &Value, field_name: &str) -> Result<String> {
        obj.get(field_name)
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!("Missing '{field_name}' in function spec"),
                path: None,
            })
    }

    /// Parse PARTITION BY from JSON array.
    fn parse_partition_by(partition_array: &Value) -> Result<Vec<PartitionByColumn>> {
        let Some(arr) = partition_array.as_array() else {
            return Ok(vec![]);
        };

        arr.iter()
            .map(|item| {
                let col_type = item
                    .get("type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'type' in partitionBy column".to_string(),
                        path: None,
                    })?;

                match col_type {
                    "dimension" => {
                        let path = item
                            .get("path")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| FraiseQLError::Validation {
                                message: "Missing 'path' in dimension partition column".to_string(),
                                path: None,
                            })?
                            .to_string();
                        Ok(PartitionByColumn::Dimension { path })
                    }
                    "filter" => {
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| FraiseQLError::Validation {
                                message: "Missing 'name' in filter partition column".to_string(),
                                path: None,
                            })?
                            .to_string();
                        Ok(PartitionByColumn::Filter { name })
                    }
                    "measure" => {
                        let name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| FraiseQLError::Validation {
                                message: "Missing 'name' in measure partition column".to_string(),
                                path: None,
                            })?
                            .to_string();
                        Ok(PartitionByColumn::Measure { name })
                    }
                    _ => Err(FraiseQLError::Validation {
                        message: format!("Unknown partition column type: {col_type}"),
                        path: None,
                    }),
                }
            })
            .collect()
    }

    /// Parse ORDER BY from JSON array.
    fn parse_order_by(order_array: &Value) -> Result<Vec<WindowOrderBy>> {
        let Some(arr) = order_array.as_array() else {
            return Ok(vec![]);
        };

        arr.iter()
            .map(|item| {
                let field = item
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'field' in orderBy".to_string(),
                        path: None,
                    })?
                    .to_string();

                let direction = match item.get("direction").and_then(|v| v.as_str()) {
                    Some("DESC" | "desc") => OrderDirection::Desc,
                    _ => OrderDirection::Asc,
                };

                Ok(WindowOrderBy { field, direction })
            })
            .collect()
    }

    /// Parse WHERE clause from JSON.
    fn parse_where_clause(where_obj: &Value) -> Result<WhereClause> {
        let Some(obj) = where_obj.as_object() else {
            return Ok(WhereClause::And(vec![]));
        };

        let mut conditions = Vec::new();

        for (key, value) in obj {
            // Parse field_operator format (e.g., "customer_id_eq" -> field="customer_id", operator="eq")
            if let Some((field, operator_str)) = Self::parse_where_field_and_operator(key)? {
                let operator = WhereOperator::from_str(operator_str)?;

                conditions.push(WhereClause::Field {
                    path: vec![field.to_string()],
                    operator,
                    value: value.clone(),
                });
            }
        }

        Ok(WhereClause::And(conditions))
    }

    /// Parse WHERE field and operator from key.
    fn parse_where_field_and_operator(key: &str) -> Result<Option<(&str, &str)>> {
        if let Some(last_underscore) = key.rfind('_') {
            let field = &key[..last_underscore];
            let operator = &key[last_underscore + 1..];

            match WhereOperator::from_str(operator) {
                Ok(_) => Ok(Some((field, operator))),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Parse window frame from JSON.
    fn parse_frame(frame: &Value) -> Result<WindowFrame> {
        let frame_type = match frame.get("frame_type").and_then(|v| v.as_str()) {
            Some("ROWS") => FrameType::Rows,
            Some("RANGE") => FrameType::Range,
            Some("GROUPS") => FrameType::Groups,
            _ => {
                return Err(FraiseQLError::Validation {
                    message: "Invalid or missing 'frame_type' in frame".to_string(),
                    path: None,
                })
            }
        };

        let start = frame
            .get("start")
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'start' in frame".to_string(),
                path: None,
            })
            .and_then(Self::parse_frame_boundary)?;

        let end = frame
            .get("end")
            .ok_or_else(|| FraiseQLError::Validation {
                message: "Missing 'end' in frame".to_string(),
                path: None,
            })
            .and_then(Self::parse_frame_boundary)?;

        let exclusion = frame
            .get("exclusion")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "current_row" => FrameExclusion::CurrentRow,
                "group" => FrameExclusion::Group,
                "ties" => FrameExclusion::Ties,
                _ => FrameExclusion::NoOthers,
            });

        Ok(WindowFrame {
            frame_type,
            start,
            end,
            exclusion,
        })
    }

    /// Parse frame boundary from JSON.
    fn parse_frame_boundary(boundary: &Value) -> Result<FrameBoundary> {
        let boundary_type = boundary.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
            FraiseQLError::Validation {
                message: "Missing 'type' in frame boundary".to_string(),
                path: None,
            }
        })?;

        match boundary_type {
            "unbounded_preceding" => Ok(FrameBoundary::UnboundedPreceding),
            "n_preceding" => {
                let n = boundary
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'n' in N PRECEDING boundary".to_string(),
                        path: None,
                    })? as u32;
                Ok(FrameBoundary::NPreceding { n })
            }
            "current_row" => Ok(FrameBoundary::CurrentRow),
            "n_following" => {
                let n = boundary
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: "Missing 'n' in N FOLLOWING boundary".to_string(),
                        path: None,
                    })? as u32;
                Ok(FrameBoundary::NFollowing { n })
            }
            "unbounded_following" => Ok(FrameBoundary::UnboundedFollowing),
            _ => Err(FraiseQLError::Validation {
                message: format!("Unknown frame boundary type: {boundary_type}"),
                path: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};
    use serde_json::json;

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![
                MeasureColumn {
                    name: "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name: "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions: DimensionColumn {
                name: "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name: "customer_id".to_string(),
                    sql_type: SqlType::Uuid,
                    indexed: true,
                },
                FilterColumn {
                    name: "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed: true,
                },
            ],
            calendar_dimensions: vec![],
        }
    }

    #[test]
    fn test_parse_simple_window_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "revenue"}
            ],
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.select.len(), 1);
        assert_eq!(request.windows.len(), 1);
        assert_eq!(request.windows[0].alias, "rank");
        assert!(matches!(
            request.windows[0].function,
            WindowFunctionSpec::RowNumber
        ));
    }

    #[test]
    fn test_parse_running_sum() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [],
            "windows": [
                {
                    "function": {"type": "running_sum", "measure": "revenue"},
                    "alias": "running_total",
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                    "frame": {
                        "frame_type": "ROWS",
                        "start": {"type": "unbounded_preceding"},
                        "end": {"type": "current_row"}
                    }
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.windows.len(), 1);
        match &request.windows[0].function {
            WindowFunctionSpec::RunningSum { measure } => {
                assert_eq!(measure, "revenue");
            }
            _ => panic!("Expected RunningSum function"),
        }
        assert!(request.windows[0].frame.is_some());
    }

    #[test]
    fn test_parse_lag_function() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "lag", "field": "revenue", "offset": 1, "default": 0},
                    "alias": "prev_revenue",
                    "orderBy": [{"field": "occurred_at"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        match &request.windows[0].function {
            WindowFunctionSpec::Lag {
                field,
                offset,
                default,
            } => {
                assert_eq!(field, "revenue");
                assert_eq!(*offset, 1);
                assert!(default.is_some());
            }
            _ => panic!("Expected Lag function"),
        }
    }

    #[test]
    fn test_parse_ntile_function() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "ntile", "n": 4},
                    "alias": "quartile",
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        match &request.windows[0].function {
            WindowFunctionSpec::Ntile { n } => {
                assert_eq!(*n, 4);
            }
            _ => panic!("Expected Ntile function"),
        }
    }

    #[test]
    fn test_parse_select_columns() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "rev"},
                {"type": "dimension", "path": "category", "alias": "cat"},
                {"type": "filter", "name": "occurred_at", "alias": "date"}
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.select.len(), 3);
        assert!(matches!(
            &request.select[0],
            WindowSelectColumn::Measure { name, alias } if name == "revenue" && alias == "rev"
        ));
        assert!(matches!(
            &request.select[1],
            WindowSelectColumn::Dimension { path, alias } if path == "category" && alias == "cat"
        ));
        assert!(matches!(
            &request.select[2],
            WindowSelectColumn::Filter { name, alias } if name == "occurred_at" && alias == "date"
        ));
    }

    #[test]
    fn test_parse_partition_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [
                        {"type": "dimension", "path": "category"},
                        {"type": "filter", "name": "customer_id"}
                    ],
                    "orderBy": []
                }
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.windows[0].partition_by.len(), 2);
        assert!(matches!(
            &request.windows[0].partition_by[0],
            PartitionByColumn::Dimension { path } if path == "category"
        ));
        assert!(matches!(
            &request.windows[0].partition_by[1],
            PartitionByColumn::Filter { name } if name == "customer_id"
        ));
    }

    #[test]
    fn test_parse_limit_offset() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "limit": 100,
            "offset": 50
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.limit, Some(100));
        assert_eq!(request.offset, Some(50));
    }

    #[test]
    fn test_parse_final_order_by() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "orderBy": [
                {"field": "revenue", "direction": "DESC"},
                {"field": "occurred_at", "direction": "ASC"}
            ]
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.order_by[0].field, "revenue");
        assert_eq!(request.order_by[0].direction, OrderDirection::Desc);
        assert_eq!(request.order_by[1].field, "occurred_at");
        assert_eq!(request.order_by[1].direction, OrderDirection::Asc);
    }

    #[test]
    fn test_parse_complex_window_query() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "select": [
                {"type": "measure", "name": "revenue", "alias": "revenue"},
                {"type": "dimension", "path": "category", "alias": "category"}
            ],
            "windows": [
                {
                    "function": {"type": "row_number"},
                    "alias": "rank",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "revenue", "direction": "DESC"}]
                },
                {
                    "function": {"type": "running_sum", "measure": "revenue"},
                    "alias": "running_total",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}],
                    "frame": {
                        "frame_type": "ROWS",
                        "start": {"type": "unbounded_preceding"},
                        "end": {"type": "current_row"}
                    }
                },
                {
                    "function": {"type": "lag", "field": "revenue", "offset": 1},
                    "alias": "prev_revenue",
                    "partitionBy": [{"type": "dimension", "path": "category"}],
                    "orderBy": [{"field": "occurred_at", "direction": "ASC"}]
                }
            ],
            "orderBy": [
                {"field": "category", "direction": "ASC"},
                {"field": "revenue", "direction": "DESC"}
            ],
            "limit": 100
        });

        let request = WindowQueryParser::parse(&query, &metadata).unwrap();

        assert_eq!(request.table_name, "tf_sales");
        assert_eq!(request.select.len(), 2);
        assert_eq!(request.windows.len(), 3);
        assert_eq!(request.order_by.len(), 2);
        assert_eq!(request.limit, Some(100));
    }

    #[test]
    fn test_parse_error_missing_table() {
        let metadata = create_test_metadata();
        let query = json!({
            "select": [],
            "windows": []
        });

        let result = WindowQueryParser::parse(&query, &metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("table"));
    }

    #[test]
    fn test_parse_error_invalid_function_type() {
        let metadata = create_test_metadata();
        let query = json!({
            "table": "tf_sales",
            "windows": [
                {
                    "function": {"type": "invalid_function"},
                    "alias": "test"
                }
            ]
        });

        let result = WindowQueryParser::parse(&query, &metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown"));
    }
}
