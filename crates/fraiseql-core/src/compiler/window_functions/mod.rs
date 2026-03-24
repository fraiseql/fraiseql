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
//! ```ignore
//! // Requires: FactTableMetadata from compiled schema.
//! use fraiseql_core::compiler::window_functions::*;
//! use fraiseql_core::compiler::fact_table::FactTableMetadata;
//! # use fraiseql_core::error::Result;
//! # fn example() -> Result<()> {
//! let metadata = FactTableMetadata::default();
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
//! # Ok(())
//! # }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

mod codegen;
mod planner;
pub use self::{codegen::WindowPlanner, planner::WindowFunctionPlanner};

#[cfg(test)]
mod tests;
