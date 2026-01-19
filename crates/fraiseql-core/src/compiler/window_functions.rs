//! Window Function Planning Module
//!
//! Generates execution plans for SQL window functions.
//!
//! # Architecture
//!
//! ```text
//! WindowRequest (high-level, semantic)
//!      ↓
//! WindowPlanner::plan() (validates against FactTableMetadata)
//!      ↓
//! WindowExecutionPlan (low-level, SQL expressions)
//!      ↓
//! WindowSqlGenerator (database-specific SQL)
//! ```
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
//! # High-Level Example (WindowRequest)
//!
//! ```rust,ignore
//! use fraiseql_core::compiler::window_functions::*;
//!
//! let request = WindowRequest {
//!     table_name: "tf_sales".to_string(),
//!     select: vec![
//!         WindowSelectColumn::Measure { name: "revenue".to_string(), alias: "revenue".to_string() },
//!         WindowSelectColumn::Dimension { path: "category".to_string(), alias: "category".to_string() },
//!     ],
//!     windows: vec![
//!         WindowFunctionRequest {
//!             function: WindowFunctionSpec::RowNumber,
//!             alias: "rank".to_string(),
//!             partition_by: vec![PartitionByColumn::Dimension { path: "category".to_string() }],
//!             order_by: vec![WindowOrderBy { field: "revenue".to_string(), direction: OrderDirection::Desc }],
//!             frame: None,
//!         },
//!     ],
//!     where_clause: None,
//!     order_by: vec![],
//!     limit: Some(100),
//!     offset: None,
//! };
//!
//! let plan = WindowPlanner::plan(request, metadata)?;
//! ```
//!
//! # SQL Example (WindowExecutionPlan output)
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

use serde::{Deserialize, Serialize};

use crate::{
    compiler::{
        aggregation::{OrderByClause, OrderDirection},
        fact_table::FactTableMetadata,
    },
    db::where_clause::WhereClause,
    error::{FraiseQLError, Result},
};

// =============================================================================
// High-Level Types (WindowRequest) - Semantic names, validated against metadata
// =============================================================================

/// High-level window query request using semantic field names.
///
/// This is the user-facing API that uses measure names and dimension paths
/// instead of raw SQL expressions. It gets validated and converted to
/// `WindowExecutionPlan` by `WindowPlanner::plan()`.
///
/// # Example
///
/// ```rust,ignore
/// let request = WindowRequest {
///     table_name: "tf_sales".to_string(),
///     select: vec![
///         WindowSelectColumn::Measure { name: "revenue".to_string(), alias: "revenue".to_string() },
///         WindowSelectColumn::Dimension { path: "category".to_string(), alias: "category".to_string() },
///     ],
///     windows: vec![WindowFunctionRequest {
///         function: WindowFunctionSpec::RunningSum { measure: "revenue".to_string() },
///         alias: "running_total".to_string(),
///         partition_by: vec![],
///         order_by: vec![WindowOrderBy { field: "occurred_at".to_string(), direction: OrderDirection::Asc }],
///         frame: Some(WindowFrame { ... }),
///     }],
///     where_clause: None,
///     order_by: vec![],
///     limit: Some(100),
///     offset: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowRequest {
    /// Fact table name (e.g., "tf_sales")
    pub table_name: String,

    /// Columns to select (measures, dimensions, filters)
    pub select: Vec<WindowSelectColumn>,

    /// Window function specifications
    pub windows: Vec<WindowFunctionRequest>,

    /// WHERE clause filters (applied before window computation)
    pub where_clause: Option<WhereClause>,

    /// Final ORDER BY (after window computation)
    pub order_by: Vec<WindowOrderBy>,

    /// Result limit
    pub limit: Option<u32>,

    /// Result offset
    pub offset: Option<u32>,
}

/// Column selection for window query (semantic names).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WindowSelectColumn {
    /// Select a measure column (e.g., "revenue")
    Measure {
        /// Measure name from FactTableMetadata
        name:  String,
        /// Result alias
        alias: String,
    },

    /// Select a dimension from JSONB (e.g., "category")
    Dimension {
        /// Dimension path in JSONB
        path:  String,
        /// Result alias
        alias: String,
    },

    /// Select a denormalized filter column (e.g., "customer_id", "occurred_at")
    Filter {
        /// Filter column name
        name:  String,
        /// Result alias
        alias: String,
    },
}

impl WindowSelectColumn {
    /// Get the result alias for this selection.
    #[must_use]
    pub fn alias(&self) -> &str {
        match self {
            Self::Measure { alias, .. }
            | Self::Dimension { alias, .. }
            | Self::Filter { alias, .. } => alias,
        }
    }
}

/// Window function request (high-level, semantic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowFunctionRequest {
    /// Window function type and parameters
    pub function: WindowFunctionSpec,

    /// Result column alias
    pub alias: String,

    /// PARTITION BY columns (semantic names)
    pub partition_by: Vec<PartitionByColumn>,

    /// ORDER BY within window
    pub order_by: Vec<WindowOrderBy>,

    /// Window frame specification
    pub frame: Option<WindowFrame>,
}

/// Window function specification using semantic field names.
///
/// Unlike `WindowFunctionType` which uses raw SQL expressions,
/// this uses measure/dimension names that get validated against metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WindowFunctionSpec {
    // =========================================================================
    // Ranking Functions (no field reference needed)
    // =========================================================================
    /// ROW_NUMBER() - Sequential number within partition
    RowNumber,

    /// RANK() - Rank with gaps for ties
    Rank,

    /// DENSE_RANK() - Rank without gaps
    DenseRank,

    /// NTILE(n) - Divide rows into n groups
    Ntile {
        /// Number of groups
        n: u32,
    },

    /// PERCENT_RANK() - Relative rank (0 to 1)
    PercentRank,

    /// CUME_DIST() - Cumulative distribution
    CumeDist,

    // =========================================================================
    // Value Functions (reference measures or dimensions)
    // =========================================================================
    /// LAG(field, offset, default) - Value from previous row
    Lag {
        /// Measure or dimension name
        field:   String,
        /// Row offset (default: 1)
        offset:  i32,
        /// Default value when no previous row
        default: Option<serde_json::Value>,
    },

    /// LEAD(field, offset, default) - Value from next row
    Lead {
        /// Measure or dimension name
        field:   String,
        /// Row offset (default: 1)
        offset:  i32,
        /// Default value when no next row
        default: Option<serde_json::Value>,
    },

    /// FIRST_VALUE(field) - First value in window frame
    FirstValue {
        /// Measure or dimension name
        field: String,
    },

    /// LAST_VALUE(field) - Last value in window frame
    LastValue {
        /// Measure or dimension name
        field: String,
    },

    /// NTH_VALUE(field, n) - Nth value in window frame
    NthValue {
        /// Measure or dimension name
        field: String,
        /// Position (1-indexed)
        n:     u32,
    },

    // =========================================================================
    // Aggregate as Window Functions (reference measures)
    // =========================================================================
    /// SUM(measure) OVER (...) - Running total
    RunningSum {
        /// Measure name
        measure: String,
    },

    /// AVG(measure) OVER (...) - Moving average
    RunningAvg {
        /// Measure name
        measure: String,
    },

    /// COUNT(*) OVER (...) - Running count
    RunningCount,

    /// COUNT(field) OVER (...) - Running count of non-null values
    RunningCountField {
        /// Measure or dimension name
        field: String,
    },

    /// MIN(measure) OVER (...) - Running minimum
    RunningMin {
        /// Measure name
        measure: String,
    },

    /// MAX(measure) OVER (...) - Running maximum
    RunningMax {
        /// Measure name
        measure: String,
    },

    /// STDDEV(measure) OVER (...) - Running standard deviation
    RunningStddev {
        /// Measure name
        measure: String,
    },

    /// VARIANCE(measure) OVER (...) - Running variance
    RunningVariance {
        /// Measure name
        measure: String,
    },
}

/// PARTITION BY column specification (semantic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PartitionByColumn {
    /// Partition by dimension from JSONB
    Dimension {
        /// Dimension path
        path: String,
    },

    /// Partition by denormalized filter column
    Filter {
        /// Filter column name
        name: String,
    },

    /// Partition by measure (rare but valid)
    Measure {
        /// Measure name
        name: String,
    },
}

/// ORDER BY clause for window functions (semantic field names).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowOrderBy {
    /// Field name (measure, dimension, or filter)
    pub field: String,

    /// Sort direction
    pub direction: OrderDirection,
}

// =============================================================================
// Low-Level Types (WindowExecutionPlan) - SQL expressions, ready for execution
// =============================================================================

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
    Ntile {
        /// Number of groups
        n: u32,
    },

    /// PERCENT_RANK() - Relative rank (0 to 1)
    PercentRank,

    /// CUME_DIST() - Cumulative distribution
    CumeDist,

    // Value functions
    /// LAG(field, offset, default) - Value from previous row
    Lag {
        /// Field name
        field:   String,
        /// Row offset
        offset:  i32,
        /// Default value
        default: Option<serde_json::Value>,
    },

    /// LEAD(field, offset, default) - Value from next row
    Lead {
        /// Field name
        field:   String,
        /// Row offset
        offset:  i32,
        /// Default value
        default: Option<serde_json::Value>,
    },

    /// FIRST_VALUE(field) - First value in window
    FirstValue {
        /// Field name
        field: String,
    },

    /// LAST_VALUE(field) - Last value in window
    LastValue {
        /// Field name
        field: String,
    },

    /// NTH_VALUE(field, n) - Nth value in window
    NthValue {
        /// Field name
        field: String,
        /// Position
        n:     u32,
    },

    // Aggregate as window functions
    /// SUM(field) OVER (...) - Running total
    Sum {
        /// Field name
        field: String,
    },

    /// AVG(field) OVER (...) - Moving average
    Avg {
        /// Field name
        field: String,
    },

    /// COUNT(*) OVER (...) - Running count
    Count {
        /// Field name
        field: Option<String>,
    },

    /// MIN(field) OVER (...) - Running minimum
    Min {
        /// Field name
        field: String,
    },

    /// MAX(field) OVER (...) - Running maximum
    Max {
        /// Field name
        field: String,
    },

    /// STDDEV(field) OVER (...) - Running standard deviation
    Stddev {
        /// Field name
        field: String,
    },

    /// VARIANCE(field) OVER (...) - Running variance
    Variance {
        /// Field name
        field: String,
    },
}

/// Window frame specification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    NPreceding {
        /// Number of rows
        n: u32,
    },

    /// CURRENT ROW
    CurrentRow,

    /// N FOLLOWING
    NFollowing {
        /// Number of rows
        n: u32,
    },

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
        let select = query.get("select").and_then(|s| s.as_array()).unwrap_or(&default_array);

        let columns = select
            .iter()
            .filter_map(|col| {
                if let Some(col_str) = col.as_str() {
                    Some(SelectColumn {
                        expression: col_str.to_string(),
                        alias:      col_str.to_string(),
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
        let windows = query.get("windows").and_then(|w| w.as_array()).unwrap_or(&default_array);

        windows.iter().map(|window| Self::parse_single_window(window)).collect()
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
            let n = ntile["n"]
                .as_u64()
                .ok_or_else(|| FraiseQLError::validation("Missing 'n' in NTILE function"))?
                as u32;
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
            _ => return Err(FraiseQLError::validation("Invalid or missing 'frame_type'")),
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
            },
            Some("current_row") => Ok(FrameBoundary::CurrentRow),
            Some("n_following") => {
                let n = boundary["n"]
                    .as_u64()
                    .ok_or_else(|| FraiseQLError::validation("Missing 'n' in N FOLLOWING"))?
                    as u32;
                Ok(FrameBoundary::NFollowing { n })
            },
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
                },
                _ => {},
            }
        }

        Ok(())
    }
}

// =============================================================================
// WindowPlanner - Converts high-level WindowRequest to WindowExecutionPlan
// =============================================================================

/// High-level window planner that validates semantic names against metadata.
///
/// Converts `WindowRequest` (user-friendly semantic names) to `WindowExecutionPlan`
/// (SQL expressions ready for execution).
///
/// # Example
///
/// ```rust,ignore
/// let request = WindowRequest { ... };
/// let metadata = FactTableMetadata { ... };
/// let plan = WindowPlanner::plan(request, metadata)?;
/// // plan now has SQL expressions like "dimensions->>'category'" instead of "category"
/// ```
pub struct WindowPlanner;

impl WindowPlanner {
    /// Convert high-level WindowRequest to executable WindowExecutionPlan.
    ///
    /// # Arguments
    ///
    /// * `request` - High-level window request with semantic names
    /// * `metadata` - Fact table metadata for validation and expression generation
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Referenced measures don't exist in metadata
    /// - Referenced filter columns don't exist
    /// - Window function field references are invalid
    pub fn plan(
        request: WindowRequest,
        metadata: FactTableMetadata,
    ) -> Result<WindowExecutionPlan> {
        // Convert select columns to SQL expressions
        let select = Self::convert_select_columns(&request.select, &metadata)?;

        // Convert window functions to SQL expressions
        let windows = Self::convert_window_functions(&request.windows, &metadata)?;

        // Convert final ORDER BY to SQL expressions
        let order_by = Self::convert_order_by(&request.order_by, &metadata)?;

        Ok(WindowExecutionPlan {
            table: request.table_name,
            select,
            windows,
            where_clause: request.where_clause,
            order_by,
            limit: request.limit,
            offset: request.offset,
        })
    }

    /// Convert semantic select columns to SQL expressions.
    fn convert_select_columns(
        columns: &[WindowSelectColumn],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<SelectColumn>> {
        columns
            .iter()
            .map(|col| Self::convert_single_select_column(col, metadata))
            .collect()
    }

    fn convert_single_select_column(
        column: &WindowSelectColumn,
        metadata: &FactTableMetadata,
    ) -> Result<SelectColumn> {
        match column {
            WindowSelectColumn::Measure { name, alias } => {
                // Validate measure exists
                if !metadata.measures.iter().any(|m| m.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Measure '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                // Measure columns are direct SQL columns
                Ok(SelectColumn {
                    expression: name.clone(),
                    alias:      alias.clone(),
                })
            },
            WindowSelectColumn::Dimension { path, alias } => {
                // Dimension from JSONB - generate extraction expression
                let expression = format!("{}->>'{}'", metadata.dimensions.name, path);
                Ok(SelectColumn {
                    expression,
                    alias: alias.clone(),
                })
            },
            WindowSelectColumn::Filter { name, alias } => {
                // Validate filter column exists
                if !metadata.denormalized_filters.iter().any(|f| f.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Filter column '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                // Filter columns are direct SQL columns
                Ok(SelectColumn {
                    expression: name.clone(),
                    alias:      alias.clone(),
                })
            },
        }
    }

    /// Convert semantic window functions to SQL expressions.
    fn convert_window_functions(
        windows: &[WindowFunctionRequest],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<WindowFunction>> {
        windows
            .iter()
            .map(|w| Self::convert_single_window_function(w, metadata))
            .collect()
    }

    fn convert_single_window_function(
        request: &WindowFunctionRequest,
        metadata: &FactTableMetadata,
    ) -> Result<WindowFunction> {
        // Convert function spec to function type
        let function = Self::convert_function_spec(&request.function, metadata)?;

        // Convert PARTITION BY columns to SQL expressions
        let partition_by = request
            .partition_by
            .iter()
            .map(|p| Self::convert_partition_by(p, metadata))
            .collect::<Result<Vec<_>>>()?;

        // Convert ORDER BY within window to SQL expressions
        let order_by = request
            .order_by
            .iter()
            .map(|o| Self::convert_window_order_by(o, metadata))
            .collect::<Result<Vec<_>>>()?;

        Ok(WindowFunction {
            function,
            alias: request.alias.clone(),
            partition_by,
            order_by,
            frame: request.frame.clone(),
        })
    }

    /// Convert high-level function spec to low-level function type with SQL expressions.
    fn convert_function_spec(
        spec: &WindowFunctionSpec,
        metadata: &FactTableMetadata,
    ) -> Result<WindowFunctionType> {
        match spec {
            // Ranking functions - no field conversion needed
            WindowFunctionSpec::RowNumber => Ok(WindowFunctionType::RowNumber),
            WindowFunctionSpec::Rank => Ok(WindowFunctionType::Rank),
            WindowFunctionSpec::DenseRank => Ok(WindowFunctionType::DenseRank),
            WindowFunctionSpec::Ntile { n } => Ok(WindowFunctionType::Ntile { n: *n }),
            WindowFunctionSpec::PercentRank => Ok(WindowFunctionType::PercentRank),
            WindowFunctionSpec::CumeDist => Ok(WindowFunctionType::CumeDist),

            // Value functions - need field conversion
            WindowFunctionSpec::Lag {
                field,
                offset,
                default,
            } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Lag {
                    field:   sql_field,
                    offset:  *offset,
                    default: default.clone(),
                })
            },
            WindowFunctionSpec::Lead {
                field,
                offset,
                default,
            } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Lead {
                    field:   sql_field,
                    offset:  *offset,
                    default: default.clone(),
                })
            },
            WindowFunctionSpec::FirstValue { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::FirstValue { field: sql_field })
            },
            WindowFunctionSpec::LastValue { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::LastValue { field: sql_field })
            },
            WindowFunctionSpec::NthValue { field, n } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::NthValue {
                    field: sql_field,
                    n:     *n,
                })
            },

            // Aggregate as window functions - need measure conversion
            WindowFunctionSpec::RunningSum { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Sum {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningAvg { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Avg {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningCount => Ok(WindowFunctionType::Count { field: None }),
            WindowFunctionSpec::RunningCountField { field } => {
                let sql_field = Self::resolve_field_to_sql(field, metadata)?;
                Ok(WindowFunctionType::Count {
                    field: Some(sql_field),
                })
            },
            WindowFunctionSpec::RunningMin { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Min {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningMax { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Max {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningStddev { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Stddev {
                    field: measure.clone(),
                })
            },
            WindowFunctionSpec::RunningVariance { measure } => {
                Self::validate_measure(measure, metadata)?;
                Ok(WindowFunctionType::Variance {
                    field: measure.clone(),
                })
            },
        }
    }

    /// Convert PARTITION BY column to SQL expression.
    fn convert_partition_by(
        partition: &PartitionByColumn,
        metadata: &FactTableMetadata,
    ) -> Result<String> {
        match partition {
            PartitionByColumn::Dimension { path } => {
                Ok(format!("{}->>'{}'", metadata.dimensions.name, path))
            },
            PartitionByColumn::Filter { name } => {
                if !metadata.denormalized_filters.iter().any(|f| f.name == *name) {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Filter column '{}' not found in fact table '{}'",
                            name, metadata.table_name
                        ),
                        path:    None,
                    });
                }
                Ok(name.clone())
            },
            PartitionByColumn::Measure { name } => {
                Self::validate_measure(name, metadata)?;
                Ok(name.clone())
            },
        }
    }

    /// Convert window ORDER BY to SQL expression.
    fn convert_window_order_by(
        order: &WindowOrderBy,
        metadata: &FactTableMetadata,
    ) -> Result<OrderByClause> {
        let field = Self::resolve_field_to_sql(&order.field, metadata)?;
        Ok(OrderByClause {
            field,
            direction: order.direction,
        })
    }

    /// Convert final ORDER BY to SQL expressions.
    fn convert_order_by(
        orders: &[WindowOrderBy],
        metadata: &FactTableMetadata,
    ) -> Result<Vec<OrderByClause>> {
        orders.iter().map(|o| Self::convert_window_order_by(o, metadata)).collect()
    }

    /// Resolve a semantic field name to its SQL expression.
    ///
    /// Priority:
    /// 1. Check if it's a measure (direct column)
    /// 2. Check if it's a filter column (direct column)
    /// 3. Assume it's a dimension path (JSONB extraction)
    fn resolve_field_to_sql(field: &str, metadata: &FactTableMetadata) -> Result<String> {
        // Check if it's a measure
        if metadata.measures.iter().any(|m| m.name == field) {
            return Ok(field.to_string());
        }

        // Check if it's a filter column
        if metadata.denormalized_filters.iter().any(|f| f.name == field) {
            return Ok(field.to_string());
        }

        // Assume it's a dimension path
        Ok(format!("{}->>'{}'", metadata.dimensions.name, field))
    }

    /// Validate that a measure exists in metadata.
    fn validate_measure(measure: &str, metadata: &FactTableMetadata) -> Result<()> {
        if !metadata.measures.iter().any(|m| m.name == *measure) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Measure '{}' not found in fact table '{}'",
                    measure, metadata.table_name
                ),
                path:    None,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::fact_table::{DimensionColumn, FilterColumn, MeasureColumn, SqlType};

    fn create_test_metadata() -> FactTableMetadata {
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:           DimensionColumn {
                name:  "dimensions".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name:     "customer_id".to_string(),
                    sql_type: SqlType::Uuid,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "occurred_at".to_string(),
                    sql_type: SqlType::Timestamp,
                    indexed:  true,
                },
            ],
            calendar_dimensions:  vec![],
        }
    }

    // =============================================================================
    // Test Helpers
    // =============================================================================

    /// Helper to serialize test objects without panicking
    fn serialize_json<T: serde::Serialize>(value: &T) -> String {
        serde_json::to_string(value)
            .expect("serialization should succeed for test objects")
    }

    /// Helper to deserialize test JSON without panicking
    fn deserialize_json<'a, T: serde::Deserialize<'a>>(json: &'a str) -> T {
        serde_json::from_str(json)
            .expect("deserialization should succeed for valid test JSON")
    }

    // =============================================================================
    // Tests
    // =============================================================================

    #[test]
    fn test_window_function_type_serialization() {
        let func = WindowFunctionType::RowNumber;
        let json = serialize_json(&func);
        assert_eq!(json, r#"{"type":"row_number"}"#);
    }

    #[test]
    fn test_frame_type_serialization() {
        let frame_type = FrameType::Rows;
        let json = serialize_json(&frame_type);
        assert_eq!(json, r#""ROWS""#);
    }

    #[test]
    fn test_frame_boundary_unbounded() {
        let boundary = FrameBoundary::UnboundedPreceding;
        let json = serialize_json(&boundary);
        assert!(json.contains("unbounded_preceding"));
    }

    #[test]
    fn test_frame_boundary_n_preceding() {
        let boundary = FrameBoundary::NPreceding { n: 5 };
        let json = serialize_json(&boundary);
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

        let plan = WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

        assert_eq!(plan.table, "tf_sales");
        assert_eq!(plan.windows.len(), 1);
        assert_eq!(plan.windows[0].alias, "rank");
        assert!(matches!(plan.windows[0].function, WindowFunctionType::RowNumber));
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

        let plan = WindowFunctionPlanner::plan(&query, &metadata).expect("window plan should succeed");

        match &plan.windows[0].function {
            WindowFunctionType::Lag {
                field,
                offset,
                default,
            } => {
                assert_eq!(field, "revenue");
                assert_eq!(*offset, 1);
                assert!(default.is_some());
            },
            _ => panic!("Expected LAG function"),
        }
    }

    #[test]
    fn test_validate_groups_frame_postgres_only() {
        use crate::db::types::DatabaseType;

        let metadata = create_test_metadata();
        let plan = WindowExecutionPlan {
            table:        "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunction {
                function:     WindowFunctionType::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        Some(WindowFrame {
                    frame_type: FrameType::Groups,
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

        // Should pass for PostgreSQL
        assert!(
            WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::PostgreSQL).is_ok()
        );

        // Should fail for MySQL
        assert!(WindowFunctionPlanner::validate(&plan, &metadata, DatabaseType::MySQL).is_err());
    }

    // =============================================================================
    // WindowPlanner Tests (High-Level -> Low-Level conversion)
    // =============================================================================

    #[test]
    fn test_window_planner_basic_request() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![
                WindowSelectColumn::Measure {
                    name:  "revenue".to_string(),
                    alias: "revenue".to_string(),
                },
                WindowSelectColumn::Dimension {
                    path:  "category".to_string(),
                    alias: "category".to_string(),
                },
            ],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![PartitionByColumn::Dimension {
                    path: "category".to_string(),
                }],
                order_by:     vec![WindowOrderBy {
                    field:     "revenue".to_string(),
                    direction: OrderDirection::Desc,
                }],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        Some(100),
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        assert_eq!(plan.table, "tf_sales");
        assert_eq!(plan.select.len(), 2);
        assert_eq!(plan.select[0].expression, "revenue");
        assert_eq!(plan.select[0].alias, "revenue");
        assert_eq!(plan.select[1].expression, "dimensions->>'category'");
        assert_eq!(plan.select[1].alias, "category");

        assert_eq!(plan.windows.len(), 1);
        assert_eq!(plan.windows[0].alias, "rank");
        assert!(matches!(plan.windows[0].function, WindowFunctionType::RowNumber));
        assert_eq!(plan.windows[0].partition_by, vec!["dimensions->>'category'"]);
        assert_eq!(plan.windows[0].order_by.len(), 1);
        assert_eq!(plan.windows[0].order_by[0].field, "revenue");
        assert_eq!(plan.windows[0].order_by[0].direction, OrderDirection::Desc);

        assert_eq!(plan.limit, Some(100));
    }

    #[test]
    fn test_window_planner_running_sum() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Measure {
                name:  "revenue".to_string(),
                alias: "revenue".to_string(),
            }],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::RunningSum {
                    measure: "revenue".to_string(),
                },
                alias:        "running_total".to_string(),
                partition_by: vec![],
                order_by:     vec![WindowOrderBy {
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

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        assert_eq!(plan.windows.len(), 1);
        match &plan.windows[0].function {
            WindowFunctionType::Sum { field } => {
                assert_eq!(field, "revenue");
            },
            _ => panic!("Expected Sum function"),
        }
        assert_eq!(plan.windows[0].alias, "running_total");
        assert!(plan.windows[0].frame.is_some());
    }

    #[test]
    fn test_window_planner_filter_column() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Filter {
                name:  "occurred_at".to_string(),
                alias: "date".to_string(),
            }],
            windows:      vec![],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        assert_eq!(plan.select.len(), 1);
        assert_eq!(plan.select[0].expression, "occurred_at");
        assert_eq!(plan.select[0].alias, "date");
    }

    #[test]
    fn test_window_planner_invalid_measure() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Measure {
                name:  "nonexistent".to_string(),
                alias: "alias".to_string(),
            }],
            windows:      vec![],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = WindowPlanner::plan(request, metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_window_planner_invalid_filter() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Filter {
                name:  "nonexistent_filter".to_string(),
                alias: "alias".to_string(),
            }],
            windows:      vec![],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let result = WindowPlanner::plan(request, metadata);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_window_planner_lag_function() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::Lag {
                    field:   "revenue".to_string(),
                    offset:  1,
                    default: Some(serde_json::json!(0)),
                },
                alias:        "prev_revenue".to_string(),
                partition_by: vec![],
                order_by:     vec![WindowOrderBy {
                    field:     "occurred_at".to_string(),
                    direction: OrderDirection::Asc,
                }],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        match &plan.windows[0].function {
            WindowFunctionType::Lag {
                field,
                offset,
                default,
            } => {
                assert_eq!(field, "revenue"); // measure stays as-is
                assert_eq!(*offset, 1);
                assert!(default.is_some());
            },
            _ => panic!("Expected Lag function"),
        }
    }

    #[test]
    fn test_window_planner_dimension_field_in_lag() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::Lag {
                    field:   "category".to_string(), // dimension path
                    offset:  1,
                    default: None,
                },
                alias:        "prev_category".to_string(),
                partition_by: vec![],
                order_by:     vec![WindowOrderBy {
                    field:     "occurred_at".to_string(),
                    direction: OrderDirection::Asc,
                }],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        match &plan.windows[0].function {
            WindowFunctionType::Lag { field, .. } => {
                // dimension gets converted to JSONB extraction
                assert_eq!(field, "dimensions->>'category'");
            },
            _ => panic!("Expected Lag function"),
        }
    }

    #[test]
    fn test_window_planner_partition_by_filter() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![PartitionByColumn::Filter {
                    name: "customer_id".to_string(),
                }],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        None,
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        assert_eq!(plan.windows[0].partition_by, vec!["customer_id"]);
    }

    #[test]
    fn test_window_planner_final_order_by() {
        let metadata = create_test_metadata();
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![],
            windows:      vec![],
            where_clause: None,
            order_by:     vec![
                WindowOrderBy {
                    field:     "revenue".to_string(),
                    direction: OrderDirection::Desc,
                },
                WindowOrderBy {
                    field:     "category".to_string(), // dimension
                    direction: OrderDirection::Asc,
                },
            ],
            limit:        None,
            offset:       None,
        };

        let plan = WindowPlanner::plan(request, metadata).expect("window plan should succeed");

        assert_eq!(plan.order_by.len(), 2);
        assert_eq!(plan.order_by[0].field, "revenue");
        assert_eq!(plan.order_by[0].direction, OrderDirection::Desc);
        assert_eq!(plan.order_by[1].field, "dimensions->>'category'");
        assert_eq!(plan.order_by[1].direction, OrderDirection::Asc);
    }

    #[test]
    fn test_window_request_serialization() {
        let request = WindowRequest {
            table_name:   "tf_sales".to_string(),
            select:       vec![WindowSelectColumn::Measure {
                name:  "revenue".to_string(),
                alias: "revenue".to_string(),
            }],
            windows:      vec![WindowFunctionRequest {
                function:     WindowFunctionSpec::RowNumber,
                alias:        "rank".to_string(),
                partition_by: vec![],
                order_by:     vec![],
                frame:        None,
            }],
            where_clause: None,
            order_by:     vec![],
            limit:        Some(10),
            offset:       None,
        };

        // Should serialize without panic
        let json = serialize_json(&request);
        assert!(json.contains("tf_sales"));
        assert!(json.contains("revenue"));
        assert!(json.contains("row_number"));

        // Should deserialize back
        let deserialized: WindowRequest = deserialize_json(&json);
        assert_eq!(deserialized.table_name, "tf_sales");
        assert_eq!(deserialized.limit, Some(10));
    }

    #[test]
    fn test_window_function_spec_serialization() {
        let spec = WindowFunctionSpec::RunningSum {
            measure: "revenue".to_string(),
        };
        let json = serialize_json(&spec);
        assert!(json.contains("running_sum"));
        assert!(json.contains("revenue"));

        let spec2 = WindowFunctionSpec::Ntile { n: 4 };
        let json2 = serialize_json(&spec2);
        assert!(json2.contains("ntile"));
        assert!(json2.contains("4"));
    }
}
