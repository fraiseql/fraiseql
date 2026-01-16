# FraiseQL v2 - Analytics Phase 7: Window Functions

**Status**: ⏳ Not Started
**Priority**: Medium (Optional)
**Estimated Effort**: 3-4 days
**Dependencies**: Phases 1-6 complete ✅

---

## Objective

Implement SQL window functions for advanced analytical queries:
- Ranking functions (ROW_NUMBER, RANK, DENSE_RANK, NTILE)
- Value functions (LAG, LEAD, FIRST_VALUE, LAST_VALUE, NTH_VALUE)
- Aggregate window functions (running totals, moving averages)
- Window frames (ROWS, RANGE, GROUPS)

---

## Context

Window functions enable powerful analytical queries that cannot be expressed with GROUP BY:

**Examples**:
```sql
-- Running total by category
SELECT
    category,
    occurred_at,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY category
        ORDER BY occurred_at
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) as running_total
FROM tf_sales;

-- Ranking products by revenue
SELECT
    product,
    revenue,
    ROW_NUMBER() OVER (ORDER BY revenue DESC) as rank
FROM tf_sales;

-- Compare with previous period
SELECT
    occurred_at,
    revenue,
    LAG(revenue, 1) OVER (ORDER BY occurred_at) as prev_revenue,
    revenue - LAG(revenue, 1) OVER (ORDER BY occurred_at) as change
FROM tf_sales;
```

**Database Support**:
- PostgreSQL: Full support (all functions + GROUPS frame)
- MySQL 8.0+: Full support (no GROUPS frame)
- SQLite 3.25+: Full support (no GROUPS frame)
- SQL Server: Full support

---

## Files to Create

### Compiler Module
```
crates/fraiseql-core/src/compiler/window_functions.rs
```

### Runtime Module
```
crates/fraiseql-core/src/runtime/window.rs
```

### Tests
```
crates/fraiseql-core/src/compiler/window_functions.rs (unit tests)
crates/fraiseql-core/src/runtime/window.rs (unit tests)
tests/integration/window_functions_test.rs (integration tests)
```

---

## Implementation Steps

### Step 1: Create Window Function Types (compiler/window_functions.rs)

**Duration**: 4 hours

**Code**:
```rust
//! Window Function Planning Module
//!
//! Generates execution plans for SQL window functions.

use crate::compiler::fact_table::FactTableMetadata;
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
    RowNumber,
    Rank,
    DenseRank,
    Ntile { n: u32 },
    PercentRank,
    CumeDist,

    // Value functions
    Lag {
        field: String,
        offset: i32,
        default: Option<serde_json::Value>,
    },
    Lead {
        field: String,
        offset: i32,
        default: Option<serde_json::Value>,
    },
    FirstValue { field: String },
    LastValue { field: String },
    NthValue { field: String, n: u32 },

    // Aggregate as window functions
    Sum { field: String },
    Avg { field: String },
    Count { field: Option<String> },
    Min { field: String },
    Max { field: String },
    Stddev { field: String },
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
    Rows,
    Range,
    Groups, // PostgreSQL only
}

/// Window frame boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrameBoundary {
    UnboundedPreceding,
    NPreceding { n: u32 },
    CurrentRow,
    NFollowing { n: u32 },
    UnboundedFollowing,
}

/// Frame exclusion mode (PostgreSQL)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameExclusion {
    CurrentRow,
    Group,
    Ties,
    NoOthers,
}

/// WHERE clause (reuse from aggregation module)
pub use crate::compiler::aggregation::WhereClause;

/// ORDER BY clause (reuse from aggregation module)
pub use crate::compiler::aggregation::OrderByClause;

/// Window function plan generator
pub struct WindowFunctionPlanner;

impl WindowFunctionPlanner {
    /// Generate window function execution plan from JSON query
    pub fn plan(
        query: &serde_json::Value,
        metadata: &FactTableMetadata,
    ) -> Result<WindowExecutionPlan> {
        // Parse window function query
        let table = query["table"]
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Missing 'table' field"))?
            .to_string();

        // Parse SELECT columns
        let select = Self::parse_select_columns(query, metadata)?;

        // Parse window functions
        let windows = Self::parse_window_functions(query, metadata)?;

        // Parse WHERE clause
        let where_clause = query.get("where").map(Self::parse_where_clause).transpose()?;

        // Parse ORDER BY
        let order_by = query
            .get("orderBy")
            .map(Self::parse_order_by)
            .transpose()?
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

    fn parse_select_columns(
        query: &serde_json::Value,
        metadata: &FactTableMetadata,
    ) -> Result<Vec<SelectColumn>> {
        // Parse "select": ["revenue", "category", "occurred_at"]
        // Or "select": {"revenue": "total_revenue", "category": "cat"}
        todo!("Parse SELECT columns")
    }

    fn parse_window_functions(
        query: &serde_json::Value,
        metadata: &FactTableMetadata,
    ) -> Result<Vec<WindowFunction>> {
        // Parse "windows": [
        //   {
        //     "function": {"row_number": {}},
        //     "alias": "rank",
        //     "partitionBy": ["category"],
        //     "orderBy": [{"field": "revenue", "direction": "DESC"}]
        //   }
        // ]
        todo!("Parse window functions")
    }

    fn parse_where_clause(value: &serde_json::Value) -> Result<WhereClause> {
        todo!("Parse WHERE clause")
    }

    fn parse_order_by(value: &serde_json::Value) -> Result<Vec<OrderByClause>> {
        todo!("Parse ORDER BY")
    }

    /// Validate window function plan
    pub fn validate(
        plan: &WindowExecutionPlan,
        metadata: &FactTableMetadata,
        database_target: crate::compiler::lowering::DatabaseTarget,
    ) -> Result<()> {
        // 1. Validate PARTITION BY columns exist
        for window in &plan.windows {
            for col in &window.partition_by {
                Self::validate_column_exists(col, metadata)?;
            }
        }

        // 2. Validate ORDER BY columns exist
        for window in &plan.windows {
            for order in &window.order_by {
                Self::validate_column_exists(&order.field, metadata)?;
            }
        }

        // 3. Validate frame type supported by database
        for window in &plan.windows {
            if let Some(frame) = &window.frame {
                if frame.frame_type == FrameType::Groups {
                    if !matches!(database_target, crate::compiler::lowering::DatabaseTarget::PostgreSQL) {
                        return Err(FraiseQLError::validation(
                            "GROUPS frame type only supported on PostgreSQL"
                        ));
                    }
                }
            }
        }

        // 4. Validate function fields exist
        for window in &plan.windows {
            Self::validate_window_function_fields(&window.function, metadata)?;
        }

        Ok(())
    }

    fn validate_column_exists(col: &str, metadata: &FactTableMetadata) -> Result<()> {
        // Check if column is a measure, dimension path, or filter column
        todo!("Validate column exists")
    }

    fn validate_window_function_fields(
        function: &WindowFunctionType,
        metadata: &FactTableMetadata,
    ) -> Result<()> {
        match function {
            WindowFunctionType::Lag { field, .. }
            | WindowFunctionType::Lead { field, .. }
            | WindowFunctionType::FirstValue { field }
            | WindowFunctionType::LastValue { field }
            | WindowFunctionType::NthValue { field, .. }
            | WindowFunctionType::Sum { field }
            | WindowFunctionType::Avg { field }
            | WindowFunctionType::Min { field }
            | WindowFunctionType::Max { field }
            | WindowFunctionType::Stddev { field }
            | WindowFunctionType::Variance { field } => {
                Self::validate_column_exists(field, metadata)?;
            }
            WindowFunctionType::Count { field: Some(field) } => {
                Self::validate_column_exists(field, metadata)?;
            }
            _ => {}
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
}
```

**Tests to Add**:
- Serialization/deserialization of all types
- Plan generation from JSON queries
- Validation of PARTITION BY columns
- Validation of frame types per database

**Verification**:
```bash
cargo test -p fraiseql-core window_functions
```

---

### Step 2: Create Window SQL Generator (runtime/window.rs)

**Duration**: 6 hours

**Code**:
```rust
//! Window Function SQL Generation
//!
//! Generates database-specific SQL for window functions.

use crate::compiler::window_functions::{
    FrameBoundary, FrameExclusion, FrameType, WindowExecutionPlan, WindowFunction,
    WindowFunctionType,
};
use crate::db::types::DatabaseType;
use crate::error::{FraiseQLError, Result};

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
    pub fn new(database_type: DatabaseType) -> Self {
        Self { database_type }
    }

    /// Generate SQL from window execution plan
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
        let mut parameters = Vec::new();

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
            sql.push_str(&self.generate_window_function_postgres(window)?);
        }

        // FROM clause
        sql.push_str(&format!(" FROM {}", plan.table));

        // WHERE clause
        if let Some(where_clause) = &plan.where_clause {
            let (where_sql, where_params) = self.generate_where_clause(where_clause)?;
            sql.push_str(&format!(" WHERE {}", where_sql));
            parameters.extend(where_params);
        }

        // ORDER BY clause
        if !plan.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            for (i, order) in plan.order_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(&format!("{} {}", order.field, order.direction));
            }
        }

        // LIMIT / OFFSET
        if let Some(limit) = plan.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = plan.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        Ok(WindowSql {
            complete_sql: sql,
            parameters,
        })
    }

    /// Generate window function expression (PostgreSQL)
    fn generate_window_function_postgres(&self, window: &WindowFunction) -> Result<String> {
        let func_sql = match &window.function {
            WindowFunctionType::RowNumber => "ROW_NUMBER()".to_string(),
            WindowFunctionType::Rank => "RANK()".to_string(),
            WindowFunctionType::DenseRank => "DENSE_RANK()".to_string(),
            WindowFunctionType::Ntile { n } => format!("NTILE({})", n),
            WindowFunctionType::PercentRank => "PERCENT_RANK()".to_string(),
            WindowFunctionType::CumeDist => "CUME_DIST()".to_string(),

            WindowFunctionType::Lag { field, offset, default } => {
                if let Some(default_val) = default {
                    format!("LAG({}, {}, {})", field, offset, default_val)
                } else {
                    format!("LAG({}, {})", field, offset)
                }
            }
            WindowFunctionType::Lead { field, offset, default } => {
                if let Some(default_val) = default {
                    format!("LEAD({}, {}, {})", field, offset, default_val)
                } else {
                    format!("LEAD({}, {})", field, offset)
                }
            }
            WindowFunctionType::FirstValue { field } => format!("FIRST_VALUE({})", field),
            WindowFunctionType::LastValue { field } => format!("LAST_VALUE({})", field),
            WindowFunctionType::NthValue { field, n } => format!("NTH_VALUE({}, {})", field, n),

            WindowFunctionType::Sum { field } => format!("SUM({})", field),
            WindowFunctionType::Avg { field } => format!("AVG({})", field),
            WindowFunctionType::Count { field: Some(field) } => format!("COUNT({})", field),
            WindowFunctionType::Count { field: None } => "COUNT(*)".to_string(),
            WindowFunctionType::Min { field } => format!("MIN({})", field),
            WindowFunctionType::Max { field } => format!("MAX({})", field),
            WindowFunctionType::Stddev { field } => format!("STDDEV({})", field),
            WindowFunctionType::Variance { field } => format!("VARIANCE({})", field),
        };

        let mut sql = format!("{} OVER (", func_sql);

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
                sql.push_str(&format!("{} {}", order.field, order.direction));
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

    /// Generate window frame clause
    fn generate_frame_clause(&self, frame: &WindowFrame) -> Result<String> {
        let frame_type = match frame.frame_type {
            FrameType::Rows => "ROWS",
            FrameType::Range => "RANGE",
            FrameType::Groups => {
                if !matches!(self.database_type, DatabaseType::PostgreSQL) {
                    return Err(FraiseQLError::validation(
                        "GROUPS frame type only supported on PostgreSQL"
                    ));
                }
                "GROUPS"
            }
        };

        let start = self.format_frame_boundary(&frame.start);
        let end = self.format_frame_boundary(&frame.end);

        let mut sql = format!("{} BETWEEN {} AND {}", frame_type, start, end);

        // Frame exclusion (PostgreSQL only)
        if let Some(exclusion) = &frame.exclusion {
            if matches!(self.database_type, DatabaseType::PostgreSQL) {
                let excl = match exclusion {
                    FrameExclusion::CurrentRow => "EXCLUDE CURRENT ROW",
                    FrameExclusion::Group => "EXCLUDE GROUP",
                    FrameExclusion::Ties => "EXCLUDE TIES",
                    FrameExclusion::NoOthers => "EXCLUDE NO OTHERS",
                };
                sql.push_str(&format!(" {}", excl));
            }
        }

        Ok(sql)
    }

    /// Format frame boundary
    fn format_frame_boundary(&self, boundary: &FrameBoundary) -> String {
        match boundary {
            FrameBoundary::UnboundedPreceding => "UNBOUNDED PRECEDING".to_string(),
            FrameBoundary::NPreceding { n } => format!("{} PRECEDING", n),
            FrameBoundary::CurrentRow => "CURRENT ROW".to_string(),
            FrameBoundary::NFollowing { n } => format!("{} FOLLOWING", n),
            FrameBoundary::UnboundedFollowing => "UNBOUNDED FOLLOWING".to_string(),
        }
    }

    /// Generate MySQL window function SQL
    fn generate_mysql(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // MySQL 8.0+ supports window functions similar to PostgreSQL
        // Main differences:
        // - No GROUPS frame type
        // - No frame exclusion
        // - STDDEV_POP instead of STDDEV
        // - VAR_POP instead of VARIANCE
        self.generate_postgres(plan) // Reuse PostgreSQL logic with adjustments
    }

    /// Generate SQLite window function SQL
    fn generate_sqlite(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // SQLite 3.25+ supports window functions
        // Similar to PostgreSQL but:
        // - No GROUPS frame type
        // - No frame exclusion
        // - No PERCENT_RANK, CUME_DIST
        self.generate_postgres(plan) // Reuse PostgreSQL logic with adjustments
    }

    /// Generate SQL Server window function SQL
    fn generate_sqlserver(&self, plan: &WindowExecutionPlan) -> Result<WindowSql> {
        // SQL Server supports window functions
        // Main differences:
        // - STDEV instead of STDDEV
        // - VAR instead of VARIANCE
        // - TOP instead of LIMIT
        self.generate_postgres(plan) // Reuse PostgreSQL logic with adjustments
    }

    fn generate_where_clause(
        &self,
        where_clause: &crate::compiler::window_functions::WhereClause,
    ) -> Result<(String, Vec<serde_json::Value>)> {
        // Reuse WHERE clause generation from aggregation module
        todo!("Generate WHERE clause")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::window_functions::*;

    #[test]
    fn test_generate_row_number() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table: "tf_sales".to_string(),
            select: vec![
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias: "revenue".to_string(),
                },
            ],
            windows: vec![WindowFunction {
                function: WindowFunctionType::RowNumber,
                alias: "rank".to_string(),
                partition_by: vec!["data->>'category'".to_string()],
                order_by: vec![OrderByClause {
                    field: "revenue".to_string(),
                    direction: "DESC".to_string(),
                }],
                frame: None,
            }],
            where_clause: None,
            order_by: vec![],
            limit: None,
            offset: None,
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
            table: "tf_sales".to_string(),
            select: vec![
                SelectColumn {
                    expression: "occurred_at".to_string(),
                    alias: "date".to_string(),
                },
                SelectColumn {
                    expression: "revenue".to_string(),
                    alias: "revenue".to_string(),
                },
            ],
            windows: vec![WindowFunction {
                function: WindowFunctionType::Sum {
                    field: "revenue".to_string(),
                },
                alias: "running_total".to_string(),
                partition_by: vec![],
                order_by: vec![OrderByClause {
                    field: "occurred_at".to_string(),
                    direction: "ASC".to_string(),
                }],
                frame: Some(WindowFrame {
                    frame_type: FrameType::Rows,
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

        let sql = generator.generate(&plan).unwrap();

        assert!(sql.complete_sql.contains("SUM(revenue) OVER"));
        assert!(sql.complete_sql.contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_generate_lag_lead() {
        let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);

        let plan = WindowExecutionPlan {
            table: "tf_sales".to_string(),
            select: vec![],
            windows: vec![
                WindowFunction {
                    function: WindowFunctionType::Lag {
                        field: "revenue".to_string(),
                        offset: 1,
                        default: Some(serde_json::json!(0)),
                    },
                    alias: "prev_revenue".to_string(),
                    partition_by: vec![],
                    order_by: vec![OrderByClause {
                        field: "occurred_at".to_string(),
                        direction: "ASC".to_string(),
                    }],
                    frame: None,
                },
                WindowFunction {
                    function: WindowFunctionType::Lead {
                        field: "revenue".to_string(),
                        offset: 1,
                        default: None,
                    },
                    alias: "next_revenue".to_string(),
                    partition_by: vec![],
                    order_by: vec![OrderByClause {
                        field: "occurred_at".to_string(),
                        direction: "ASC".to_string(),
                    }],
                    frame: None,
                },
            ],
            where_clause: None,
            order_by: vec![],
            limit: None,
            offset: None,
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
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::CurrentRow),
            "CURRENT ROW"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::NFollowing { n: 3 }),
            "3 FOLLOWING"
        );
        assert_eq!(
            generator.format_frame_boundary(&FrameBoundary::UnboundedFollowing),
            "UNBOUNDED FOLLOWING"
        );
    }
}
```

**Tests to Add**:
- ROW_NUMBER, RANK, DENSE_RANK generation
- LAG/LEAD with defaults
- Running totals with frames
- Moving averages
- Multi-database SQL generation

**Verification**:
```bash
cargo test -p fraiseql-core runtime::window
```

---

### Step 3: Integration with Executor

**Duration**: 2 hours

Update `runtime/executor.rs` to handle window function queries:

```rust
impl Executor {
    /// Execute window function query
    pub async fn execute_window_query(
        &self,
        query_json: &serde_json::Value,
        query_name: &str,
        metadata: &FactTableMetadata,
    ) -> Result<String> {
        // 1. Parse and plan window query
        let plan = WindowFunctionPlanner::plan(query_json, metadata)?;

        // 2. Validate plan
        WindowFunctionPlanner::validate(&plan, metadata, self.db.database_type())?;

        // 3. Generate SQL
        let generator = WindowSqlGenerator::new(self.db.database_type());
        let sql = generator.generate(&plan)?;

        // 4. Execute query
        let rows = self.db.execute_query(&sql.complete_sql, &sql.parameters).await?;

        // 5. Project results (convert to GraphQL format)
        let projected = Self::project_window_results(rows, &plan)?;

        // 6. Wrap in GraphQL envelope
        let response = serde_json::json!({
            "data": {
                query_name: projected
            }
        });

        Ok(serde_json::to_string(&response)?)
    }

    fn project_window_results(
        rows: Vec<std::collections::HashMap<String, serde_json::Value>>,
        plan: &WindowExecutionPlan,
    ) -> Result<serde_json::Value> {
        // Convert database rows to GraphQL result format
        Ok(serde_json::to_value(rows)?)
    }
}
```

---

### Step 4: Integration Tests

**Duration**: 4 hours

Create `tests/integration/window_functions_test.rs`:

```rust
//! Integration tests for window functions

use fraiseql_core::compiler::fact_table::*;
use fraiseql_core::compiler::window_functions::*;
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::runtime::{Executor, WindowSqlGenerator};
use serde_json::json;
use std::sync::Arc;

const TEST_DB_URL: &str = "postgresql://fraiseql_test:password@localhost:5433/test_fraiseql";

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
            name: "data".to_string(),
            paths: vec![
                DimensionPath {
                    name: "category".to_string(),
                    json_path: "data->>'category'".to_string(),
                    data_type: "text".to_string(),
                },
            ],
        },
        denormalized_filters: vec![
            FilterColumn {
                name: "occurred_at".to_string(),
                sql_type: SqlType::Timestamp,
                indexed: true,
            },
        ],
    }
}

#[test]
fn test_parse_row_number_query() {
    let metadata = create_test_metadata();
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue", "category"],
        "windows": [
            {
                "function": {"row_number": {}},
                "alias": "rank",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }
        ]
    });

    let plan = WindowFunctionPlanner::plan(&query, &metadata).unwrap();

    assert_eq!(plan.windows.len(), 1);
    assert_eq!(plan.windows[0].alias, "rank");
    assert!(matches!(plan.windows[0].function, WindowFunctionType::RowNumber));
}

#[test]
fn test_generate_row_number_sql() {
    use fraiseql_core::db::types::DatabaseType;

    let metadata = create_test_metadata();
    let query = json!({
        "table": "tf_sales",
        "select": ["revenue"],
        "windows": [
            {
                "function": {"row_number": {}},
                "alias": "rank",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }
        ],
        "limit": 10
    });

    let plan = WindowFunctionPlanner::plan(&query, &metadata).unwrap();
    let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate(&plan).unwrap();

    assert!(sql.complete_sql.contains("ROW_NUMBER()"));
    assert!(sql.complete_sql.contains("PARTITION BY data->>'category'"));
    assert!(sql.complete_sql.contains("ORDER BY revenue DESC"));
    assert!(sql.complete_sql.contains("LIMIT 10"));
}

#[test]
fn test_generate_running_total_sql() {
    use fraiseql_core::db::types::DatabaseType;

    let metadata = create_test_metadata();
    let query = json!({
        "table": "tf_sales",
        "select": ["occurred_at", "revenue"],
        "windows": [
            {
                "function": {"sum": {"field": "revenue"}},
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

    let plan = WindowFunctionPlanner::plan(&query, &metadata).unwrap();
    let generator = WindowSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate(&plan).unwrap();

    assert!(sql.complete_sql.contains("SUM(revenue) OVER"));
    assert!(sql.complete_sql.contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"));
}

#[tokio::test]
#[ignore] // Requires database
async fn test_execute_window_query() {
    let adapter = Arc::new(PostgresAdapter::new(TEST_DB_URL).await.unwrap());
    let executor = Executor::new(Default::default(), adapter);
    let metadata = create_test_metadata();

    let query = json!({
        "table": "tf_sales",
        "select": ["revenue", "category"],
        "windows": [
            {
                "function": {"row_number": {}},
                "alias": "rank",
                "partitionBy": ["data->>'category'"],
                "orderBy": [{"field": "revenue", "direction": "DESC"}]
            }
        ],
        "limit": 5
    });

    let result = executor
        .execute_window_query(&query, "sales_ranked", &metadata)
        .await
        .unwrap();

    let response: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert!(response["data"]["sales_ranked"].is_array());
}
```

**Verification**:
```bash
# Unit tests
cargo test -p fraiseql-core window

# Integration tests (requires PostgreSQL)
cargo test --test window_functions_test -- --ignored
```

---

### Step 5: Update Module Exports

**Duration**: 30 minutes

Update `compiler/mod.rs`:
```rust
pub mod window_functions;

pub use window_functions::{WindowFunctionPlanner, WindowExecutionPlan, WindowFunction};
```

Update `runtime/mod.rs`:
```rust
pub mod window;

pub use window::{WindowSqlGenerator, WindowSql};
```

---

### Step 6: Documentation

**Duration**: 2 hours

Add examples to `docs/guides/analytics-patterns.md`:

```markdown
## Window Functions

### Ranking

```graphql
query TopSellingProducts {
  sales_ranked(
    select: ["product", "revenue"]
    windows: [
      {
        function: { row_number: {} }
        alias: "rank"
        partitionBy: ["data->>'category'"]
        orderBy: [{ field: "revenue", direction: DESC }]
      }
    ]
    limit: 10
  ) {
    product
    revenue
    rank
  }
}
```

### Running Totals

```graphql
query DailyRunningTotals {
  sales_window(
    select: ["occurred_at", "revenue"]
    windows: [
      {
        function: { sum: { field: "revenue" } }
        alias: "running_total"
        orderBy: [{ field: "occurred_at", direction: ASC }]
        frame: {
          frame_type: ROWS
          start: { type: "unbounded_preceding" }
          end: { type: "current_row" }
        }
      }
    ]
  ) {
    occurred_at
    revenue
    running_total
  }
}
```

### Moving Averages

```graphql
query SevenDayMovingAverage {
  sales_window(
    select: ["occurred_at", "revenue"]
    windows: [
      {
        function: { avg: { field: "revenue" } }
        alias: "moving_avg_7d"
        orderBy: [{ field: "occurred_at", direction: ASC }]
        frame: {
          frame_type: ROWS
          start: { type: "n_preceding", n: 6 }
          end: { type: "current_row" }
        }
      }
    ]
  ) {
    occurred_at
    revenue
    moving_avg_7d
  }
}
```

### Period Comparisons

```graphql
query PeriodOverPeriod {
  sales_window(
    select: ["occurred_at", "revenue"]
    windows: [
      {
        function: {
          lag: {
            field: "revenue",
            offset: 1,
            default: 0
          }
        }
        alias: "prev_period_revenue"
        orderBy: [{ field: "occurred_at", direction: ASC }]
      }
    ]
  ) {
    occurred_at
    revenue
    prev_period_revenue
    change: "revenue - prev_period_revenue"
  }
}
```
```

---

## Acceptance Criteria

- [ ] `compiler/window_functions.rs` module created with all types
- [ ] `runtime/window.rs` module created with SQL generation
- [ ] PostgreSQL window SQL generation working
- [ ] MySQL 8.0+ window SQL generation working
- [ ] SQLite 3.25+ window SQL generation working
- [ ] SQL Server window SQL generation working
- [ ] All ranking functions supported (ROW_NUMBER, RANK, DENSE_RANK, NTILE)
- [ ] All value functions supported (LAG, LEAD, FIRST_VALUE, LAST_VALUE, NTH_VALUE)
- [ ] Aggregate window functions working (SUM, AVG, MIN, MAX)
- [ ] Window frames supported (ROWS, RANGE, GROUPS for PostgreSQL)
- [ ] Unit tests pass (20+ tests)
- [ ] Integration tests pass with PostgreSQL
- [ ] Documentation updated with examples
- [ ] Executor integration complete

---

## Verification Commands

```bash
# Build check
cargo check -p fraiseql-core

# Lint
cargo clippy -p fraiseql-core -- -D warnings

# Unit tests
cargo test -p fraiseql-core window

# Integration tests (requires database)
cargo test --test window_functions_test

# Full test suite
cargo test -p fraiseql-core
```

**Expected Output**:
```
running 25 tests
test compiler::window_functions::tests::test_window_function_type_serialization ... ok
test compiler::window_functions::tests::test_frame_boundary_n_preceding ... ok
test runtime::window::tests::test_generate_row_number ... ok
test runtime::window::tests::test_generate_running_total ... ok
test runtime::window::tests::test_generate_lag_lead ... ok
...
test result: ok. 25 passed; 0 failed; 0 ignored
```

---

## DO NOT

- ❌ Don't implement window functions in WHERE clauses (not supported by SQL)
- ❌ Don't mix window functions with GROUP BY in same query
- ❌ Don't use GROUPS frame type for non-PostgreSQL databases
- ❌ Don't forget to validate PARTITION BY columns exist
- ❌ Don't skip integration tests with real databases

---

## Notes

**Database Compatibility**:
- PostgreSQL: Full support including GROUPS frames
- MySQL 8.0+: No GROUPS, no EXCLUDE
- SQLite 3.25+: No GROUPS, no EXCLUDE, no PERCENT_RANK/CUME_DIST
- SQL Server: STDEV/VAR instead of STDDEV/VARIANCE

**Performance Considerations**:
- Window functions can be expensive on large datasets
- Add WHERE clause filters before window computation
- Use LIMIT to restrict result size
- Consider materialized views for frequently-used windows

**Common Use Cases**:
- Top-N per category (ROW_NUMBER with PARTITION BY)
- Running totals / cumulative sums
- Moving averages (time series)
- Period-over-period comparisons (LAG/LEAD)
- Percentile analysis (NTILE, PERCENT_RANK)
