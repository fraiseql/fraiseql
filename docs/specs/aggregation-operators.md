# Aggregation Operators Specification

**Version:** 1.0
**Status:** Complete (Phase 1-3)
**Audience:** Compiler developers, SDK maintainers
**Date:** January 12, 2026

---

## Overview

This specification defines the aggregation operators available in FraiseQL, organized by database target and phase of implementation.

**Phases**:
- **Phase 1-2** (✅ Complete): Basic aggregates (COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE)
- **Phase 3** (📋 Planned): Advanced aggregates (ARRAY_AGG, JSON_AGG, STRING_AGG, BOOL_AND/OR)
- **Phase 4** (📋 Planned): Auto-generated GraphQL types (perfect for v2 compiler!)

---

## Capability Manifest Extension

The capability manifest defines which operators are available for each database target.

### PostgreSQL Aggregation Operators

```json
{
  "postgresql": {
    "aggregation": {
      "basic": [
        { "function": "COUNT", "sql": "COUNT", "return_type": "Int" },
        { "function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $field)", "return_type": "Int" },
        { "function": "SUM", "sql": "SUM", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG", "return_type": "Float" },
        { "function": "MIN", "sql": "MIN", "return_type": "Same as input" },
        { "function": "MAX", "sql": "MAX", "return_type": "Same as input" }
      ],
      "statistical": [
        { "function": "STDDEV", "sql": "STDDEV", "return_type": "Float" },
        { "function": "STDDEV_POP", "sql": "STDDEV_POP", "return_type": "Float" },
        { "function": "STDDEV_SAMP", "sql": "STDDEV_SAMP", "return_type": "Float" },
        { "function": "VARIANCE", "sql": "VARIANCE", "return_type": "Float" },
        { "function": "VAR_POP", "sql": "VAR_POP", "return_type": "Float" },
        { "function": "VAR_SAMP", "sql": "VAR_SAMP", "return_type": "Float" },
        { "function": "PERCENTILE_CONT", "sql": "PERCENTILE_CONT($fraction) WITHIN GROUP (ORDER BY $field)", "return_type": "Float" },
        { "function": "PERCENTILE_DISC", "sql": "PERCENTILE_DISC($fraction) WITHIN GROUP (ORDER BY $field)", "return_type": "Same as input" }
      ],
      "advanced": [
        { "function": "ARRAY_AGG", "sql": "ARRAY_AGG", "return_type": "Array", "phase": 3 },
        { "function": "JSON_AGG", "sql": "JSON_AGG", "return_type": "JSON", "phase": 3 },
        { "function": "JSONB_AGG", "sql": "JSONB_AGG", "return_type": "JSONB", "phase": 3 },
        { "function": "STRING_AGG", "sql": "STRING_AGG($field, $separator)", "return_type": "String", "phase": 3 },
        { "function": "BOOL_AND", "sql": "BOOL_AND", "return_type": "Boolean", "phase": 3 },
        { "function": "BOOL_OR", "sql": "BOOL_OR", "return_type": "Boolean", "phase": 3 }
      ],
      "temporal_bucketing": [
        { "function": "DATE_TRUNC", "sql": "DATE_TRUNC", "buckets": ["second", "minute", "hour", "day", "week", "month", "quarter", "year"] }
      ],
      "conditional": [
        { "function": "FILTER", "sql": "... FILTER (WHERE ...)", "supported": true }
      ]
    }
  }
}
```

### MySQL Aggregation Operators

```json
{
  "mysql": {
    "aggregation": {
      "basic": [
        { "function": "COUNT", "sql": "COUNT", "return_type": "Int" },
        { "function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $field)", "return_type": "Int" },
        { "function": "SUM", "sql": "SUM", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG", "return_type": "Float" },
        { "function": "MIN", "sql": "MIN", "return_type": "Same as input" },
        { "function": "MAX", "sql": "MAX", "return_type": "Same as input" }
      ],
      "statistical": [
        { "function": "STDDEV", "sql": "STDDEV", "return_type": "Float", "note": "Sample standard deviation" },
        { "function": "STDDEV_POP", "sql": "STDDEV_POP", "return_type": "Float" },
        { "function": "STDDEV_SAMP", "sql": "STDDEV_SAMP", "return_type": "Float" },
        { "function": "VARIANCE", "sql": "VARIANCE", "return_type": "Float", "note": "Sample variance" },
        { "function": "VAR_POP", "sql": "VAR_POP", "return_type": "Float" },
        { "function": "VAR_SAMP", "sql": "VAR_SAMP", "return_type": "Float" }
      ],
      "advanced": [
        { "function": "GROUP_CONCAT", "sql": "GROUP_CONCAT($field SEPARATOR $separator)", "return_type": "String", "phase": 3, "note": "Similar to STRING_AGG" },
        { "function": "JSON_ARRAYAGG", "sql": "JSON_ARRAYAGG", "return_type": "JSON", "phase": 3 },
        { "function": "JSON_OBJECTAGG", "sql": "JSON_OBJECTAGG($key, $value)", "return_type": "JSON", "phase": 3 }
      ],
      "temporal_bucketing": [
        { "function": "DATE_FORMAT", "sql": "DATE_FORMAT", "buckets": ["day", "week", "month", "year"], "note": "Limited bucket support" }
      ],
      "conditional": [
        { "function": "FILTER", "sql": "CASE WHEN ... THEN ... END", "supported": "emulated" }
      ]
    }
  }
}
```

### SQLite Aggregation Operators

```json
{
  "sqlite": {
    "aggregation": {
      "basic": [
        { "function": "COUNT", "sql": "COUNT", "return_type": "Int" },
        { "function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $field)", "return_type": "Int" },
        { "function": "SUM", "sql": "SUM", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG", "return_type": "Float" },
        { "function": "MIN", "sql": "MIN", "return_type": "Same as input" },
        { "function": "MAX", "sql": "MAX", "return_type": "Same as input" }
      ],
      "statistical": [],
      "advanced": [
        { "function": "GROUP_CONCAT", "sql": "GROUP_CONCAT($field, $separator)", "return_type": "String", "phase": 3 }
      ],
      "temporal_bucketing": [
        { "function": "strftime", "sql": "strftime", "buckets": ["day", "week", "month", "year"] }
      ],
      "conditional": [
        { "function": "FILTER", "sql": "CASE WHEN ... THEN ... END", "supported": "emulated" }
      ]
    }
  }
}
```

### SQL Server Aggregation Operators

```json
{
  "sqlserver": {
    "aggregation": {
      "basic": [
        { "function": "COUNT", "sql": "COUNT", "return_type": "Int" },
        { "function": "COUNT_DISTINCT", "sql": "COUNT(DISTINCT $field)", "return_type": "Int" },
        { "function": "SUM", "sql": "SUM", "return_type": "Numeric" },
        { "function": "AVG", "sql": "AVG", "return_type": "Float" },
        { "function": "MIN", "sql": "MIN", "return_type": "Same as input" },
        { "function": "MAX", "sql": "MAX", "return_type": "Same as input" }
      ],
      "statistical": [
        { "function": "STDEV", "sql": "STDEV", "return_type": "Float", "note": "Sample standard deviation" },
        { "function": "STDEVP", "sql": "STDEVP", "return_type": "Float", "note": "Population standard deviation" },
        { "function": "VAR", "sql": "VAR", "return_type": "Float", "note": "Sample variance" },
        { "function": "VARP", "sql": "VARP", "return_type": "Float", "note": "Population variance" }
      ],
      "advanced": [
        { "function": "STRING_AGG", "sql": "STRING_AGG($field, $separator)", "return_type": "String", "phase": 3, "note": "Requires SQL Server 2017+" }
      ],
      "temporal_bucketing": [
        { "function": "DATEPART", "sql": "DATEPART", "buckets": ["day", "week", "month", "quarter", "year", "hour", "minute"] }
      ],
      "conditional": [
        { "function": "FILTER", "sql": "CASE WHEN ... THEN ... END", "supported": "emulated" }
      ],
      "json": [
        { "function": "JSON_VALUE", "sql": "JSON_VALUE", "supported": true },
        { "function": "JSON_QUERY", "sql": "JSON_QUERY", "supported": true },
        { "function": "FOR_JSON", "sql": "FOR JSON PATH", "supported": true, "phase": 3, "note": "Output formatting" }
      ]
    }
  }
}
```

---

## GraphQL Schema Generation

For a fact table `tf_sales` with measures `revenue`, `quantity`, compiler generates database-specific aggregate types.

### PostgreSQL Target (Full Support)

```graphql
type SalesAggregate {
  # Basic aggregates (Phase 1-2)
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  revenue_stddev: Float      # PostgreSQL only
  revenue_variance: Float    # PostgreSQL only
  quantity_sum: Int
  quantity_avg: Float
  quantity_min: Int
  quantity_max: Int

  # Grouped dimensions
  category: String
  region: String
  occurred_at_day: String
  occurred_at_week: String
  occurred_at_month: String
}

input SalesGroupByInput {
  category: Boolean
  region: Boolean
  customer_segment: Boolean
  occurred_at_day: Boolean
  occurred_at_week: Boolean
  occurred_at_month: Boolean
  occurred_at_quarter: Boolean
  occurred_at_year: Boolean
}

input SalesHavingInput {
  count_eq: Int
  count_gt: Int
  count_gte: Int
  revenue_sum_eq: Float
  revenue_sum_gt: Float
  revenue_sum_gte: Float
  revenue_avg_eq: Float
  revenue_avg_gt: Float
  revenue_avg_gte: Float
}

type Query {
  sales_aggregate(
    where: SalesWhereInput
    groupBy: SalesGroupByInput
    having: SalesHavingInput
    orderBy: [OrderByInput!]
    limit: Int
    offset: Int
  ): [SalesAggregate!]!
}
```

### MySQL Target (Good Support)

```graphql
type SalesAggregate {
  # Basic aggregates (same as PostgreSQL)
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  revenue_stddev: Float      # MySQL has STDDEV
  revenue_variance: Float    # MySQL has VARIANCE
  quantity_sum: Int
  quantity_avg: Float

  # NO advanced aggregates in Phase 1-2
  # Phase 3 will add GROUP_CONCAT, JSON_ARRAYAGG

  # Grouped dimensions
  category: String
  region: String
  occurred_at_day: String
  occurred_at_month: String
  occurred_at_year: String
  # NO quarter bucket
}
```

### SQLite Target (Basic Support)

```graphql
type SalesAggregate {
  # Basic aggregates only
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  # NO stddev/variance
  quantity_sum: Int
  quantity_avg: Float

  # Grouped dimensions
  category: String
  region: String
  occurred_at_day: String
  occurred_at_month: String
  occurred_at_year: String
}
```

### SQL Server Target (Enterprise Support)

```graphql
type SalesAggregate {
  # Basic aggregates
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  revenue_stdev: Float       # SQL Server: STDEV
  revenue_variance: Float    # SQL Server: VAR
  quantity_sum: Int
  quantity_avg: Float

  # Grouped dimensions
  category: String
  region: String
  occurred_at_day: String
  occurred_at_week: String
  occurred_at_month: String
  occurred_at_quarter: String
  occurred_at_year: String
}
```

---

## Phase 3: Advanced Aggregate Functions

**Status**: Implemented (database-dependent availability)

### ARRAY_AGG

Aggregate values into an array.

**PostgreSQL**:
```sql
SELECT
    data->>'category' AS category,
    ARRAY_AGG(data->>'product_name') AS product_names
FROM tf_sales
GROUP BY data->>'category';
```

**MySQL**:
```sql
-- Use JSON_ARRAYAGG instead
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    JSON_ARRAYAGG(JSON_EXTRACT(data, '$.product_name')) AS product_names
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category');
```

**SQLite**: Not supported

**SQL Server**: Not directly supported (use FOR JSON)

### JSON_AGG / JSONB_AGG

Aggregate rows into JSON.

**PostgreSQL**:
```sql
SELECT
    data->>'customer_id' AS customer_id,
    JSONB_AGG(jsonb_build_object(
        'product', data->>'product_name',
        'revenue', revenue
    )) AS orders
FROM tf_sales
GROUP BY data->>'customer_id';
```

**MySQL**:
```sql
SELECT
    JSON_EXTRACT(data, '$.customer_id') AS customer_id,
    JSON_ARRAYAGG(
        JSON_OBJECT(
            'product', JSON_EXTRACT(data, '$.product_name'),
            'revenue', revenue
        )
    ) AS orders
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.customer_id');
```

**SQLite**: Not supported

**SQL Server**:
```sql
-- Use FOR JSON PATH
SELECT
    JSON_VALUE(data, '$.customer_id') AS customer_id,
    (
        SELECT
            JSON_VALUE(data, '$.product_name') AS product,
            revenue
        FROM tf_sales s2
        WHERE JSON_VALUE(s2.data, '$.customer_id') = JSON_VALUE(s1.data, '$.customer_id')
        FOR JSON PATH
    ) AS orders
FROM tf_sales s1
GROUP BY JSON_VALUE(data, '$.customer_id');
```

### STRING_AGG / GROUP_CONCAT

Concatenate strings with delimiter.

**PostgreSQL**:
```sql
SELECT
    data->>'customer_id' AS customer_id,
    STRING_AGG(data->>'product_name', ', ' ORDER BY revenue DESC) AS products
FROM tf_sales
GROUP BY data->>'customer_id';
```

**MySQL**:
```sql
SELECT
    JSON_EXTRACT(data, '$.customer_id') AS customer_id,
    GROUP_CONCAT(JSON_EXTRACT(data, '$.product_name') ORDER BY revenue DESC SEPARATOR ', ') AS products
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.customer_id');
```

**SQLite**:
```sql
SELECT
    json_extract(data, '$.customer_id') AS customer_id,
    GROUP_CONCAT(json_extract(data, '$.product_name'), ', ') AS products
FROM tf_sales
GROUP BY json_extract(data, '$.customer_id');
```

**SQL Server**:
```sql
SELECT
    JSON_VALUE(data, '$.customer_id') AS customer_id,
    STRING_AGG(JSON_VALUE(data, '$.product_name'), ', ') AS products
FROM tf_sales
GROUP BY JSON_VALUE(data, '$.customer_id');
```

### BOOL_AND / BOOL_OR

Boolean aggregates (all true / any true).

**PostgreSQL**:
```sql
SELECT
    data->>'category' AS category,
    BOOL_AND((data->>'in_stock')::boolean) AS all_in_stock,
    BOOL_OR((data->>'on_sale')::boolean) AS any_on_sale
FROM tf_sales
GROUP BY data->>'category';
```

**MySQL/SQLite/SQL Server**: Emulate with MIN/MAX on boolean values or CASE WHEN.

---

## Compilation Rules

### Measure Column Detection

**Criteria**:
- Numeric type: INT, BIGINT, DECIMAL, FLOAT, NUMERIC, REAL, DOUBLE PRECISION
- Non-nullable preferred (but nullable allowed)
- Excluded by convention: `id`, `created_at`, `updated_at`

**Example**:
```rust
fn is_measure_column(column: &Column) -> bool {
    let numeric_types = ["int", "bigint", "decimal", "float", "numeric", "real", "double"];
    let excluded_names = ["id", "created_at", "updated_at"];

    numeric_types.contains(&column.data_type.to_lowercase().as_str())
        && !excluded_names.contains(&column.name.to_lowercase().as_str())
}
```

### Dimension Path Detection

**Criteria**:
- References JSONB column (default: `data`)
- Supports nested paths: `data->>'key'`, `data#>>'{path,to,key}'`
- Database-specific operators from capability manifest

**Example**:
```rust
fn extract_dimension(column_name: &str, jsonb_column: &str) -> String {
    match database_target {
        "postgresql" => format!("{}->>'{}' AS {}", jsonb_column, column_name, column_name),
        "mysql" => format!("JSON_EXTRACT({}, '$.{}') AS {}", jsonb_column, column_name, column_name),
        "sqlite" => format!("json_extract({}, '$.{}') AS {}", jsonb_column, column_name, column_name),
        "sqlserver" => format!("JSON_VALUE({}, '$.{}') AS {}", jsonb_column, column_name, column_name),
    }
}
```

### Temporal Bucketing

**PostgreSQL**:
```rust
fn temporal_bucket_postgres(field: &str, bucket: &str) -> String {
    format!("DATE_TRUNC('{}', {})", bucket, field)
}
```

**MySQL**:
```rust
fn temporal_bucket_mysql(field: &str, bucket: &str) -> String {
    let format = match bucket {
        "day" => "%Y-%m-%d",
        "week" => "%Y-%u",
        "month" => "%Y-%m",
        "year" => "%Y",
        _ => panic!("Unsupported bucket: {}", bucket),
    };
    format!("DATE_FORMAT({}, '{}')", field, format)
}
```

**SQLite**:
```rust
fn temporal_bucket_sqlite(field: &str, bucket: &str) -> String {
    let format = match bucket {
        "day" => "%Y-%m-%d",
        "week" => "%Y-%W",
        "month" => "%Y-%m",
        "year" => "%Y",
        _ => panic!("Unsupported bucket: {}", bucket),
    };
    format!("strftime('{}', {})", format, field)
}
```

**SQL Server**:
```rust
fn temporal_bucket_sqlserver(field: &str, bucket: &str) -> String {
    let part = bucket; // day, week, month, quarter, year
    format!("DATEPART({}, {})", part, field)
}
```

---

## Related Specifications

- **Capability Manifest** (`capability-manifest.md`) - Database-specific operator availability
- **Aggregation Model** (`../architecture/analytics/aggregation-model.md`) - Compilation and execution
- **Database Targeting** (`../architecture/database/database-targeting.md`) - Multi-database support
- **Window Operators** (`window-operators.md`) - Window function reference (Phase 5)

---

*End of Aggregation Operators Specification*
