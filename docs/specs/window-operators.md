# Window Operators Specification

**Version:** 1.0
**Status:** Planned
**Audience:** Compiler developers, SDK maintainers
**Date:** January 12, 2026

---

## Overview

This specification defines window function operators available in FraiseQL, organized by database target and function category.

**Status**: Phase 5 (planned)

For detailed architecture and use cases, see `../architecture/analytics/window-functions.md`.

---

## Capability Manifest Extension

### PostgreSQL Window Functions

```json
{
  "postgresql": {
    "window_functions": {
      "ranking": [
        { "function": "ROW_NUMBER", "sql": "ROW_NUMBER()", "return_type": "Int" },
        { "function": "RANK", "sql": "RANK()", "return_type": "Int" },
        { "function": "DENSE_RANK", "sql": "DENSE_RANK()", "return_type": "Int" },
        { "function": "NTILE", "sql": "NTILE($n)", "return_type": "Int", "params": ["n: Int"] },
        { "function": "PERCENT_RANK", "sql": "PERCENT_RANK()", "return_type": "Float" },
        { "function": "CUME_DIST", "sql": "CUME_DIST()", "return_type": "Float" }
      ],
      "value": [
        { "function": "LAG", "sql": "LAG($field, $offset, $default)", "return_type": "Same as field", "params": ["field: String", "offset: Int = 1", "default: Any = NULL"] },
        { "function": "LEAD", "sql": "LEAD($field, $offset, $default)", "return_type": "Same as field", "params": ["field: String", "offset: Int = 1", "default: Any = NULL"] },
        { "function": "FIRST_VALUE", "sql": "FIRST_VALUE($field)", "return_type": "Same as field", "params": ["field: String"] },
        { "function": "LAST_VALUE", "sql": "LAST_VALUE($field)", "return_type": "Same as field", "params": ["field: String"] },
        { "function": "NTH_VALUE", "sql": "NTH_VALUE($field, $n)", "return_type": "Same as field", "params": ["field: String", "n: Int"] }
      ],
      "aggregate_as_window": [
        { "function": "SUM", "sql": "SUM($field)", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG($field)", "return_type": "Float" },
        { "function": "COUNT", "sql": "COUNT($field)", "return_type": "Int" },
        { "function": "MIN", "sql": "MIN($field)", "return_type": "Same as field" },
        { "function": "MAX", "sql": "MAX($field)", "return_type": "Same as field" }
      ],
      "frame_types": ["ROWS", "RANGE", "GROUPS"],
      "frame_exclusion": ["EXCLUDE CURRENT ROW", "EXCLUDE GROUP", "EXCLUDE TIES", "EXCLUDE NO OTHERS"],
      "supported": true
    }
  }
}
```

### MySQL Window Functions (8.0+)

```json
{
  "mysql": {
    "window_functions": {
      "ranking": [
        { "function": "ROW_NUMBER", "sql": "ROW_NUMBER()", "return_type": "Int" },
        { "function": "RANK", "sql": "RANK()", "return_type": "Int" },
        { "function": "DENSE_RANK", "sql": "DENSE_RANK()", "return_type": "Int" },
        { "function": "NTILE", "sql": "NTILE($n)", "return_type": "Int" },
        { "function": "PERCENT_RANK", "sql": "PERCENT_RANK()", "return_type": "Float" },
        { "function": "CUME_DIST", "sql": "CUME_DIST()", "return_type": "Float" }
      ],
      "value": [
        { "function": "LAG", "sql": "LAG($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "LEAD", "sql": "LEAD($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "FIRST_VALUE", "sql": "FIRST_VALUE($field)", "return_type": "Same as field" },
        { "function": "LAST_VALUE", "sql": "LAST_VALUE($field)", "return_type": "Same as field" },
        { "function": "NTH_VALUE", "sql": "NTH_VALUE($field, $n)", "return_type": "Same as field" }
      ],
      "aggregate_as_window": [
        { "function": "SUM", "sql": "SUM($field)", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG($field)", "return_type": "Float" },
        { "function": "COUNT", "sql": "COUNT($field)", "return_type": "Int" },
        { "function": "MIN", "sql": "MIN($field)", "return_type": "Same as field" },
        { "function": "MAX", "sql": "MAX($field)", "return_type": "Same as field" }
      ],
      "frame_types": ["ROWS", "RANGE"],
      "frame_exclusion": [],
      "supported": true,
      "min_version": "8.0"
    }
  }
}
```

### SQLite Window Functions (3.25+)

```json
{
  "sqlite": {
    "window_functions": {
      "ranking": [
        { "function": "ROW_NUMBER", "sql": "ROW_NUMBER()", "return_type": "Int" },
        { "function": "RANK", "sql": "RANK()", "return_type": "Int" },
        { "function": "DENSE_RANK", "sql": "DENSE_RANK()", "return_type": "Int" },
        { "function": "NTILE", "sql": "NTILE($n)", "return_type": "Int" },
        { "function": "PERCENT_RANK", "sql": "PERCENT_RANK()", "return_type": "Float" },
        { "function": "CUME_DIST", "sql": "CUME_DIST()", "return_type": "Float" }
      ],
      "value": [
        { "function": "LAG", "sql": "LAG($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "LEAD", "sql": "LEAD($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "FIRST_VALUE", "sql": "FIRST_VALUE($field)", "return_type": "Same as field" },
        { "function": "LAST_VALUE", "sql": "LAST_VALUE($field)", "return_type": "Same as field" },
        { "function": "NTH_VALUE", "sql": "NTH_VALUE($field, $n)", "return_type": "Same as field" }
      ],
      "aggregate_as_window": [
        { "function": "SUM", "sql": "SUM($field)", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG($field)", "return_type": "Float" },
        { "function": "COUNT", "sql": "COUNT($field)", "return_type": "Int" },
        { "function": "MIN", "sql": "MIN($field)", "return_type": "Same as field" },
        { "function": "MAX", "sql": "MAX($field)", "return_type": "Same as field" }
      ],
      "frame_types": ["ROWS", "RANGE"],
      "frame_exclusion": [],
      "supported": true,
      "min_version": "3.25"
    }
  }
}
```

### SQL Server Window Functions

```json
{
  "sqlserver": {
    "window_functions": {
      "ranking": [
        { "function": "ROW_NUMBER", "sql": "ROW_NUMBER()", "return_type": "Int" },
        { "function": "RANK", "sql": "RANK()", "return_type": "Int" },
        { "function": "DENSE_RANK", "sql": "DENSE_RANK()", "return_type": "Int" },
        { "function": "NTILE", "sql": "NTILE($n)", "return_type": "Int" },
        { "function": "PERCENT_RANK", "sql": "PERCENT_RANK()", "return_type": "Float" },
        { "function": "CUME_DIST", "sql": "CUME_DIST()", "return_type": "Float" }
      ],
      "value": [
        { "function": "LAG", "sql": "LAG($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "LEAD", "sql": "LEAD($field, $offset, $default)", "return_type": "Same as field" },
        { "function": "FIRST_VALUE", "sql": "FIRST_VALUE($field)", "return_type": "Same as field" },
        { "function": "LAST_VALUE", "sql": "LAST_VALUE($field)", "return_type": "Same as field" }
      ],
      "aggregate_as_window": [
        { "function": "SUM", "sql": "SUM($field)", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG($field)", "return_type": "Float" },
        { "function": "COUNT", "sql": "COUNT($field)", "return_type": "Int" },
        { "function": "MIN", "sql": "MIN($field)", "return_type": "Same as field" },
        { "function": "MAX", "sql": "MAX($field)", "return_type": "Same as field" }
      ],
      "frame_types": ["ROWS", "RANGE"],
      "frame_exclusion": [],
      "supported": true
    }
  }
}
```

---

## GraphQL Schema Generation

### Window Function Input

```graphql
input WindowFunctionInput {
  function: WindowFunction!
  field: String              # Required for value/aggregate functions
  alias: String!
  partition_by: [String!]    # Optional partitioning
  order_by: [OrderByInput!]  # Required for ranking/frame functions
  frame: WindowFrameInput    # Optional frame clause
  offset: Int                # For LAG/LEAD (default: 1)
  default: JSON              # For LAG/LEAD default value
  n: Int                     # For NTILE, NTH_VALUE
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

  # Aggregates as windows
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
  exclusion: WindowFrameExclusion  # PostgreSQL only
}

enum WindowFrameType {
  ROWS
  RANGE
  GROUPS  # PostgreSQL only
}

input WindowFrameBoundary {
  type: BoundaryType!
  offset: Int  # For PRECEDING/FOLLOWING with offset
}

enum BoundaryType {
  UNBOUNDED_PRECEDING
  N_PRECEDING
  CURRENT_ROW
  N_FOLLOWING
  UNBOUNDED_FOLLOWING
}

enum WindowFrameExclusion {
  EXCLUDE_CURRENT_ROW
  EXCLUDE_GROUP
  EXCLUDE_TIES
  EXCLUDE_NO_OTHERS
}
```

### Query Type

```graphql
type Query {
  sales_window(
    select: [String!]!
    windows: [WindowFunctionInput!]!
    where: SalesWhereInput
    orderBy: [OrderByInput!]
    limit: Int
    offset: Int
  ): [JSON!]!
}
```

---

## SQL Generation Examples

### PostgreSQL

```sql
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    ROW_NUMBER() OVER (
        PARTITION BY data->>'category'
        ORDER BY revenue DESC
    ) AS rank_by_revenue,
    SUM(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) AS running_total,
    LAG(revenue, 1) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at
    ) AS prev_day_revenue
FROM tf_sales;
```

### MySQL

```sql
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    occurred_at,
    revenue,
    ROW_NUMBER() OVER (
        PARTITION BY JSON_EXTRACT(data, '$.category')
        ORDER BY revenue DESC
    ) AS rank_by_revenue
FROM tf_sales;
```

### SQLite

```sql
SELECT
    json_extract(data, '$.category') AS category,
    occurred_at,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY json_extract(data, '$.category')
        ORDER BY occurred_at
        ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
    ) AS moving_avg_7d
FROM tf_sales;
```

### SQL Server

```sql
SELECT
    JSON_VALUE(data, '$.category') AS category,
    occurred_at,
    revenue,
    LAG(revenue, 1) OVER (
        PARTITION BY JSON_VALUE(data, '$.category')
        ORDER BY occurred_at
    ) AS prev_day_revenue
FROM tf_sales;
```

---

## Related Specifications

- **Window Functions Architecture** (`../architecture/analytics/window-functions.md`) - Detailed architecture
- **Capability Manifest** (`capability-manifest.md`) - Database-specific operator availability
- **Aggregation Operators** (`aggregation-operators.md`) - Aggregate function reference

---


