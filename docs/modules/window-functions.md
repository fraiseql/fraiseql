# Window Functions Module

**Source file**: `crates/fraiseql-core/src/compiler/window_functions.rs` (~1,926 lines)

**Tests**: 50+ unit tests in `#[cfg(test)] mod tests` at the bottom of the file. Run with:
```bash
cargo nextest run -p fraiseql-core --lib compiler::window_functions
```

---

## Overview

Window functions let FraiseQL compute running totals, rankings, lag/lead values, and other
analytics directly in SQL. The module implements a **3-stage compilation pipeline** that
translates user-facing semantic requests into database-specific SQL.

---

## 3-Stage Pipeline

```
GraphQL query arguments
      ↓ parse_window_request()
WindowRequest { fn_type, partition_by, order_by, frame, … }  ← semantic names
      ↓ WindowPlanner::plan(request, fact_table_metadata)
WindowExecutionPlan { sql_expressions, … }                    ← SQL expressions
      ↓ WindowSqlGenerator::generate(plan, db_type)
"ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY occurred_at DESC)"
```

### Stage 1: `WindowRequest` (semantic)

User-facing API. Field names are semantic (`"revenue"`, `"category"`) — not SQL.

```rust
pub struct WindowRequest {
    pub table_name: String,
    pub select: Vec<WindowSelectColumn>,     // Measure | Dimension | Filter
    pub windows: Vec<WindowFunctionRequest>,
    pub where_clause: Option<WhereClause>,
    pub order_by: Vec<WindowOrderBy>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub enum WindowSelectColumn {
    Measure { name: String, alias: String },   // "revenue"
    Dimension { path: String, alias: String }, // "category" (from JSONB)
    Filter { name: String, alias: String },    // "customer_id" (SQL column)
}
```

### Stage 2: `WindowExecutionPlan` (low-level)

Semantic names are resolved to SQL expressions against `FactTableMetadata`. The planner
validates that every referenced measure/dimension exists on the target table.

```rust
pub struct WindowExecutionPlan {
    pub table: String,
    pub select: Vec<SelectColumn>,    // SQL expressions like "dimensions->>'category'"
    pub windows: Vec<WindowFunction>,
    pub where_clause: Option<WhereClause>,
    pub order_by: Vec<OrderByClause>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub struct SelectColumn {
    pub expression: String, // e.g. "revenue" or "dimensions->>'category'"
    pub alias: String,
}
```

`WindowPlanner::plan()` is where validation errors surface. Common errors:
- Measure name not found on table
- Dimension path not found in JSONB schema
- Window function not supported on target database dialect

### Stage 3: `WindowSqlGenerator` (database-specific)

Converts the execution plan into a dialect-correct SQL string. The generator is the
only place where database differences are handled.

---

## Supported Function Categories

### 1. Ranking Functions (no field reference required)

| Function | PostgreSQL | MySQL 8+ | SQL Server | SQLite |
|----------|:----------:|:--------:|:----------:|:------:|
| `ROW_NUMBER()` | ✅ | ✅ | ✅ | ✅ |
| `RANK()` | ✅ | ✅ | ✅ | ✅ |
| `DENSE_RANK()` | ✅ | ✅ | ✅ | ✅ |
| `NTILE(n)` | ✅ | ✅ | ✅ | ✅ |
| `PERCENT_RANK()` | ✅ | ❌ | ✅ | ❌ |
| `CUME_DIST()` | ✅ | ❌ | ✅ | ❌ |

### 2. Value Functions (require a field reference)

| Function | PostgreSQL | MySQL 8+ | SQL Server | SQLite |
|----------|:----------:|:--------:|:----------:|:------:|
| `LAG(field, offset, default)` | ✅ | ✅ | ✅ | ✅ |
| `LEAD(field, offset, default)` | ✅ | ✅ | ✅ | ✅ |
| `FIRST_VALUE(field)` | ✅ | ✅ | ✅ | ✅ |
| `LAST_VALUE(field)` | ✅ | ✅ | ✅ | ✅ |
| `NTH_VALUE(field, n)` | ✅ | ✅ | ✅ | ✅ |

### 3. Aggregate-as-Window Functions

| Function | PostgreSQL | MySQL 8+ | SQL Server | SQLite |
|----------|:----------:|:--------:|:----------:|:------:|
| `SUM(measure) OVER (...)` | ✅ | ✅ | ✅ | ✅ |
| `AVG(measure) OVER (...)` | ✅ | ✅ | ✅ | ⚠️ |
| `COUNT(*) OVER (...)` | ✅ | ✅ | ✅ | ✅ |
| `MIN(measure) OVER (...)` | ✅ | ✅ | ✅ | ⚠️ |
| `MAX(measure) OVER (...)` | ✅ | ✅ | ✅ | ⚠️ |
| `STDDEV(measure) OVER (...)` | ✅ | ❌ | ✅ | ❌ |
| `VARIANCE(measure) OVER (...)` | ✅ | ❌ | ✅ | ❌ |

---

## Database Dialect Differences

### PostgreSQL (full support)

Supports all functions, all frame types (`ROWS`, `RANGE`, `GROUPS`), frame exclusion
(`CURRENT ROW`, `GROUP`, `TIES`, `NO OTHERS`), and the `FILTER (WHERE ...)` clause
on aggregate-as-window functions.

```sql
SELECT
    category,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY category
        ORDER BY revenue DESC
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) AS running_total
FROM tf_sales;
```

### MySQL 8+ (partial)

Supports ranking and value functions, and basic aggregate-as-window. **Missing**:
- `PERCENT_RANK()`, `CUME_DIST()`
- `STDDEV()`, `VARIANCE()` OVER
- `GROUPS` frame type (only `ROWS` and `RANGE`)
- Frame exclusion

The planner raises an error at `plan()` time if an unsupported function is requested
for a MySQL target.

### SQL Server (mostly full)

Supports most functions. Frame type differences: `ROWS`/`RANGE` only (no `GROUPS`).
Some versions lack `PERCENT_RANK`.

### SQLite (very limited)

Supports ranking and value functions, and basic `SUM`/`COUNT` OVER. Missing:
- `PERCENT_RANK()`, `CUME_DIST()`
- `STDDEV()`, `VARIANCE()`
- Frame exclusion

Validated at plan time; unsupported functions produce a `FraiseQLError::Unsupported`.

---

## Adding a New Window Function

1. Add a variant to `WindowFunctionSpec` (semantic) and `WindowFunctionType` (low-level)
2. Add mapping in `WindowPlanner::plan_function()` — converts Spec to Type
3. Add per-database SQL generation in `WindowSqlGenerator::generate_function()`
4. Add validation in each database dialect validator (MySQL/SQLite restriction lists)
5. Add a snapshot test in `tests/sql_snapshots.rs`
6. Add a behavioral integration test for at least PostgreSQL

---

## Testing

SQL snapshots for window function SQL generation live in:
```
crates/fraiseql-core/tests/sql_snapshots.rs  — section: window_functions
```

To update snapshots after changing SQL generation:
```bash
INSTA_UPDATE=accept cargo nextest run --test sql_snapshots
cargo insta review
```
