# Window Functions Architecture

**Version:** 1.0
**Status:** Implemented
**Audience:** Compiler developers, runtime engineers, SDK users
**Date:** January 18, 2026

---

## Overview

Window functions (analytical functions) perform calculations across rows related to the current row, using an OVER clause to define the window. Unlike aggregate functions with GROUP BY, window functions return a value for EVERY row.

**Status**: Implemented in FraiseQL v2

---

## Window Function Categories

### 1. Ranking Functions

Assign ranks to rows within partitions.

- `ROW_NUMBER()` - Unique sequential number (1, 2, 3, 4...)
- `RANK()` - Ranking with gaps for ties (1, 2, 2, 4...)
- `DENSE_RANK()` - Ranking without gaps (1, 2, 2, 3...)
- `NTILE(n)` - Divide rows into n buckets (quartiles, deciles, etc.)
- `PERCENT_RANK()` - Relative rank from 0.0 to 1.0
- `CUME_DIST()` - Cumulative distribution (0.0 to 1.0)

**Example**:

```sql
SELECT
    data->>'category' AS category,
    revenue,
    ROW_NUMBER() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC) AS row_num,
    RANK() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC) AS rank,
    DENSE_RANK() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC) AS dense_rank
FROM tf_sales;
```

### 2. Value Functions

Access values from other rows in the window.

- `LAG(field, offset, default)` - Access previous row value
- `LEAD(field, offset, default)` - Access next row value
- `FIRST_VALUE(field)` - First value in window
- `LAST_VALUE(field)` - Last value in window
- `NTH_VALUE(field, n)` - Nth value in window

**Example**:

```sql
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    LAG(revenue, 1) OVER (PARTITION BY data->>'category' ORDER BY occurred_at) AS prev_day_revenue,
    LEAD(revenue, 1) OVER (PARTITION BY data->>'category' ORDER BY occurred_at) AS next_day_revenue
FROM tf_sales;
```

### 3. Aggregate Functions as Windows

Apply aggregate functions with window semantics (running totals, moving averages).

- `SUM(field) OVER (...)` - Running total
- `AVG(field) OVER (...)` - Moving average
- `COUNT(*) OVER (...)` - Running count
- `MIN(field) OVER (...)` - Running minimum
- `MAX(field) OVER (...)` - Running maximum

**Example**:

```sql
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) AS running_total
FROM tf_sales;
```

---

## Window Specification

### PARTITION BY

Divides rows into partitions (groups). Window function applies separately to each partition.

**Syntax**:

```sql
OVER (PARTITION BY column1, column2, ...)
```

**Example**:

```sql
-- Row number within each category
ROW_NUMBER() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC)

-- No partition = single global window
ROW_NUMBER() OVER (ORDER BY revenue DESC)
```

### ORDER BY

Defines row ordering within each partition. Required for ranking functions and frame clauses.

**Syntax**:

```sql
OVER (PARTITION BY ... ORDER BY column1 [ASC|DESC], column2 [ASC|DESC], ...)
```

**Example**:

```sql
-- Rank by revenue descending within category
RANK() OVER (PARTITION BY data->>'category' ORDER BY revenue DESC)

-- Running total ordered by date
SUM(revenue) OVER (PARTITION BY data->>'category' ORDER BY occurred_at ASC)
```

### Frame Clauses

Define which rows are included in the window frame relative to current row. Used with aggregate window functions.

**Frame Types**:

- `ROWS` - Physical row-based window (count rows)
- `RANGE` - Logical value-based window (based on ORDER BY value)
- `GROUPS` - Group-based window (PostgreSQL only)

**Frame Boundaries**:

- `UNBOUNDED PRECEDING` - Start of partition
- `n PRECEDING` - n rows/range units before current
- `CURRENT ROW` - Current row
- `n FOLLOWING` - n rows/range units after current
- `UNBOUNDED FOLLOWING` - End of partition

**Default Frame** (if not specified):

- With ORDER BY: `RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW`
- Without ORDER BY: `ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING`

**Examples**:

```sql
-- Cumulative sum (all rows up to current)
SUM(revenue) OVER (
    PARTITION BY data->>'category'
    ORDER BY occurred_at
    ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
)

-- 7-day moving average (last 7 rows including current)
AVG(revenue) OVER (
    PARTITION BY data->>'category'
    ORDER BY occurred_at
    ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
)

-- Centered 3-row moving average (current ± 1 row)
AVG(revenue) OVER (
    PARTITION BY data->>'category'
    ORDER BY occurred_at
    ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING
)

-- All rows in partition (default without ORDER BY)
SUM(revenue) OVER (
    PARTITION BY data->>'category'
    ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
)
```

---

## Compilation Strategy

### Compiler Tasks

1. **Parse Window Function Specifications** from schema decorators
2. **Validate Columns**:
   - PARTITION BY columns exist in table
   - ORDER BY columns exist in table
   - Field references are valid
3. **Generate Window Clause SQL**:
   - Build OVER clause with PARTITION BY, ORDER BY, frame
4. **Database-Specific Lowering**:
   - Adjust syntax for target database
   - Validate frame clause support

### Runtime Execution

1. Apply WHERE filters first (before window functions)
2. Compute window functions (database-side)
3. Apply HAVING if present (after aggregates)
4. Apply final ORDER BY and LIMIT
5. Return results with window columns

**Execution Order**:

```
WHERE → GROUP BY → HAVING → Window Functions → ORDER BY → LIMIT
```

---

## Database-Specific Support

### PostgreSQL

**Support Level**: ✅ Full

**Features**:

- All ranking functions (ROW_NUMBER, RANK, DENSE_RANK, NTILE, PERCENT_RANK, CUME_DIST)
- All value functions (LAG, LEAD, FIRST_VALUE, LAST_VALUE, NTH_VALUE)
- All frame types (ROWS, RANGE, GROUPS)
- EXCLUDE clause (EXCLUDE CURRENT ROW, EXCLUDE GROUP, EXCLUDE TIES, EXCLUDE NO OTHERS)

**Example**:

```sql
SELECT
    data->>'category' AS category,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        EXCLUDE CURRENT ROW
    ) AS cumulative_revenue_excluding_current
FROM tf_sales;
```

### MySQL (8.0+)

**Support Level**: ✅ Near-full (requires MySQL 8.0+)

**Features**:

- All ranking functions
- All value functions
- Frame types: ROWS, RANGE
- ❌ No GROUPS frame type
- ❌ No EXCLUDE clause

**Example**:

```sql
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    revenue,
    ROW_NUMBER() OVER (PARTITION BY JSON_EXTRACT(data, '$.category') ORDER BY revenue DESC) AS rank
FROM tf_sales;
```

### SQLite (3.25+)

**Support Level**: ✅ Good (requires SQLite 3.25+, released 2018)

**Features**:

- All ranking functions
- All value functions
- Frame types: ROWS, RANGE
- ❌ No GROUPS frame type
- ❌ No EXCLUDE clause

**Example**:

```sql
SELECT
    json_extract(data, '$.category') AS category,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY json_extract(data, '$.category')
        ORDER BY occurred_at
        ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
    ) AS moving_avg_7d
FROM tf_sales;
```

### SQL Server

**Support Level**: ✅ Full

**Features**:

- All ranking functions
- All value functions (LAG, LEAD, FIRST_VALUE, LAST_VALUE)
- Frame types: ROWS, RANGE
- ❌ No GROUPS frame type
- ❌ No EXCLUDE clause

**Example**:

```sql
SELECT
    JSON_VALUE(data, '$.category') AS category,
    revenue,
    LAG(revenue, 1) OVER (
        PARTITION BY JSON_VALUE(data, '$.category')
        ORDER BY occurred_at
    ) AS prev_day_revenue
FROM tf_sales;
```

---

## Use Cases

### 1. Running Totals

Calculate cumulative sum up to current row.

```sql
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) AS cumulative_revenue
FROM tf_sales
ORDER BY data->>'category', occurred_at;
```

### 2. Moving Averages

Calculate average over sliding window (e.g., 7-day moving average).

```sql
SELECT
    data->>'category' AS category,
    occurred_at::DATE AS day,
    SUM(revenue) AS daily_revenue,
    AVG(SUM(revenue)) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at::DATE
        ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
    ) AS moving_avg_7d
FROM tf_sales
GROUP BY data->>'category', occurred_at::DATE
ORDER BY data->>'category', occurred_at::DATE;
```

### 3. Year-Over-Year Comparison

Compare current period to same period last year using LAG.

```sql
SELECT
    DATE_TRUNC('month', occurred_at) AS month,
    SUM(revenue) AS monthly_revenue,
    LAG(SUM(revenue), 12) OVER (ORDER BY DATE_TRUNC('month', occurred_at)) AS same_month_last_year,
    SUM(revenue) - LAG(SUM(revenue), 12) OVER (ORDER BY DATE_TRUNC('month', occurred_at)) AS yoy_change
FROM tf_sales
GROUP BY DATE_TRUNC('month', occurred_at)
ORDER BY month;
```

### 4. Top-N Per Category

Rank items within each category and filter to top N.

```sql
SELECT * FROM (
    SELECT
        data->>'category' AS category,
        data->>'product_name' AS product,
        SUM(revenue) AS total_revenue,
        ROW_NUMBER() OVER (
            PARTITION BY data->>'category'
            ORDER BY SUM(revenue) DESC
        ) AS rank
    FROM tf_sales
    GROUP BY data->>'category', data->>'product_name'
) ranked
WHERE rank <= 10
ORDER BY category, rank;
```

### 5. Percentile Ranking

Assign percentile ranks to rows.

```sql
SELECT
    data->>'product_name' AS product,
    SUM(revenue) AS total_revenue,
    PERCENT_RANK() OVER (ORDER BY SUM(revenue) DESC) AS percentile_rank,
    NTILE(4) OVER (ORDER BY SUM(revenue) DESC) AS quartile
FROM tf_sales
GROUP BY data->>'product_name'
ORDER BY total_revenue DESC;
```

### 6. Trend Analysis

Compare to previous period to identify trends.

```sql
SELECT
    occurred_at::DATE AS day,
    SUM(revenue) AS daily_revenue,
    LAG(SUM(revenue), 1) OVER (ORDER BY occurred_at::DATE) AS prev_day_revenue,
    SUM(revenue) - LAG(SUM(revenue), 1) OVER (ORDER BY occurred_at::DATE) AS day_over_day_change,
    ROUND(
        100.0 * (SUM(revenue) - LAG(SUM(revenue), 1) OVER (ORDER BY occurred_at::DATE)) /
        NULLIF(LAG(SUM(revenue), 1) OVER (ORDER BY occurred_at::DATE), 0),
        2
    ) AS day_over_day_pct
FROM tf_sales
GROUP BY occurred_at::DATE
ORDER BY occurred_at::DATE;
```

---

## Performance Considerations

### Indexing Strategy

**PARTITION BY Columns**:

```sql
-- Index columns used in PARTITION BY
CREATE INDEX idx_sales_category ON tf_sales ((dimensions->>'category'));
```

**ORDER BY Columns**:

```sql
-- Index columns used in ORDER BY within window
CREATE INDEX idx_sales_occurred ON tf_sales(occurred_at);
```

**Composite Indexes**:

```sql
-- Composite index for common window pattern
CREATE INDEX idx_sales_category_occurred
    ON tf_sales ((dimensions->>'category'), occurred_at);
```

### Window Function Evaluation

**Execution Order**:

1. WHERE clause filters rows
2. GROUP BY aggregates (if present)
3. HAVING filters aggregated results (if present)
4. **Window functions compute** ← Happens here
5. Final ORDER BY sorts results
6. LIMIT/OFFSET applies

**Performance Impact**:

- Window functions evaluated AFTER WHERE/GROUP BY/HAVING
- Can be expensive for large windows (UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING)
- Proper indexes on PARTITION BY and ORDER BY columns critical

### Optimization Tips

1. **Use Specific Frame Clauses**:

   ```sql
   -- ❌ SLOW: Large frame
   SUM(revenue) OVER (
       ORDER BY occurred_at
       ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
   )

   -- ✅ FAST: Bounded frame
   SUM(revenue) OVER (
       ORDER BY occurred_at
       ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
   )
   ```

2. **Partition Data Appropriately**:
   - Balance partition size (not too large, not too many)
   - Use meaningful partitions (category, region, etc.)

3. **Consider Materialized Views**:

   ```sql
   -- For frequently-used window calculations
   CREATE MATERIALIZED VIEW mv_sales_running_totals AS
   SELECT
       data->>'category' AS category,
       occurred_at,
       revenue,
       SUM(revenue) OVER (
           PARTITION BY data->>'category'
           ORDER BY occurred_at
           ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
       ) AS cumulative_revenue
   FROM tf_sales;

   CREATE INDEX idx_mv_category_occurred
       ON mv_sales_running_totals(category, occurred_at);
   ```

4. **Limit Window Size**:
   - Prefer `ROWS BETWEEN 6 PRECEDING` over `UNBOUNDED PRECEDING` when possible
   - Use WHERE clause to reduce data volume before window computation

---

## GraphQL API (Proposed)

**Status**: Proposed for future enhancement

### Query Structure

```graphql
input WindowFunctionInput {
  function: WindowFunction!
  field: String
  alias: String!
  partition_by: [String!]
  order_by: [OrderByInput!]
  frame: WindowFrameInput
  offset: Int  # For LAG/LEAD
  default: JSON  # For LAG/LEAD default value
}

enum WindowFunction {
  # Ranking
  ROW_NUMBER
  RANK
  DENSE_RANK
  NTILE
  PERCENT_RANK
  CUME_DIST

  # Value
  LAG
  LEAD
  FIRST_VALUE
  LAST_VALUE
  NTH_VALUE

  # Aggregates
  SUM
  AVG
  COUNT
  MIN
  MAX
}

input WindowFrameInput {
  type: WindowFrameType!
  start: WindowFrameBoundary!
  end: WindowFrameBoundary!
}

enum WindowFrameType {
  ROWS
  RANGE
  GROUPS  # PostgreSQL only
}

input WindowFrameBoundary {
  type: BoundaryType!
  offset: Int
}

enum BoundaryType {
  UNBOUNDED_PRECEDING
  N_PRECEDING
  CURRENT_ROW
  N_FOLLOWING
  UNBOUNDED_FOLLOWING
}
```

### Example Query

```graphql
query {
  sales_window(
    select: ["category", "occurred_at", "revenue"]
    windows: [
      {
        function: ROW_NUMBER
        alias: "rank_by_revenue"
        partition_by: ["category"]
        order_by: [{field: "revenue", direction: DESC}]
      }
      {
        function: SUM
        field: "revenue"
        alias: "running_total"
        partition_by: ["category"]
        order_by: [{field: "occurred_at", direction: ASC}]
        frame: {
          type: ROWS
          start: {type: UNBOUNDED_PRECEDING}
          end: {type: CURRENT_ROW}
        }
      }
      {
        function: AVG
        field: "revenue"
        alias: "moving_avg_7d"
        partition_by: ["category"]
        order_by: [{field: "occurred_at", direction: ASC}]
        frame: {
          type: ROWS
          start: {type: N_PRECEDING, offset: 6}
          end: {type: CURRENT_ROW}
        }
      }
      {
        function: LAG
        field: "revenue"
        offset: 1
        alias: "prev_day_revenue"
        partition_by: ["category"]
        order_by: [{field: "occurred_at", direction: ASC}]
      }
    ]
    where: {occurred_at: {_gte: "2026-01-01"}}
    orderBy: [{field: "category"}, {field: "occurred_at"}]
  ) {
    category
    occurred_at
    revenue
    rank_by_revenue
    running_total
    moving_avg_7d
    prev_day_revenue
  }
}
```

---

## Related Specifications

- **Aggregation Model** (`aggregation-model.md`) - GROUP BY and basic aggregates
- **Window Operators Reference** (`../specs/window-operators.md`) - Complete function reference
- **Analytics Patterns** (`../guides/analytics-patterns.md`) - Practical examples

---

*End of Window Functions Architecture*
