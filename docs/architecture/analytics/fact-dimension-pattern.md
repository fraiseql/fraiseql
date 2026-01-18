# Fact-Dimension Pattern

**Version:** 1.0
**Status:** Complete (Phase 1-2)
**Audience:** Database architects, data engineers, SDK users
**Date:** January 12, 2026

---

## Overview

FraiseQL enforces the **fact table pattern** for analytical workloads:
- One record = one immutable fact (transaction, measurement, event)
- Measures stored as SQL columns (10-100x faster aggregation)
- Dimensions stored in JSONB `data` column (flexible grouping)
- Denormalized filters as indexed SQL columns

**Critical Principle**: No joins. All dimensional data must be denormalized at ETL time.

---

## Fact Table Structure

### Required Columns

1. **Primary Key**: `id` (UUID or BIGSERIAL)
2. **Measure Columns**: Numeric types for aggregation
   - INT, BIGINT, DECIMAL, FLOAT, NUMERIC
   - Examples: `revenue`, `quantity`, `duration_ms`
3. **Dimensions Column**: `data` JSONB (default name, configurable)
   - Contains all grouping dimensions
   - Examples: category, region, customer_segment

### Optional Columns

4. **Denormalized Filter Columns**: Indexed SQL columns for fast WHERE filtering
   - UUIDs, VARCHAR, DATE, ENUM
   - Examples: `customer_id`, `product_id`, `occurred_at`, `status`
5. **Timestamps**: `created_at`, `occurred_at`, etc.

### Example: Sales Fact Table

```sql
CREATE TABLE tf_sales (
    -- Primary key
    id BIGSERIAL PRIMARY KEY,

    -- Measures (SQL columns for fast aggregation)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,

    -- Dimensions (JSONB for flexible grouping)
    dimensions JSONB NOT NULL,
    -- Example data content:
    -- {
    --   "category": "Electronics",
    --   "region": "North America",
    --   "product_name": "Laptop Pro",
    --   "customer_segment": "Enterprise"
    -- }

    -- Denormalized filters (indexed SQL columns for fast WHERE)
    customer_id UUID NOT NULL,
    product_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    status VARCHAR(50) NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for fast filtering
CREATE INDEX idx_sales_customer ON tf_sales(customer_id);
CREATE INDEX idx_sales_product ON tf_sales(product_id);
CREATE INDEX idx_sales_occurred ON tf_sales(occurred_at);
CREATE INDEX idx_sales_status ON tf_sales(status);

-- GIN index for JSONB dimensions (PostgreSQL)
CREATE INDEX idx_sales_data_gin ON tf_sales USING GIN(data);

-- Composite index for common query pattern
CREATE INDEX idx_sales_customer_occurred
    ON tf_sales(customer_id, occurred_at DESC);
```

---

## Measures vs Dimensions vs Filters

### Measures (SQL Columns)

**Purpose**: Aggregation targets (SUM, AVG, COUNT, etc.)

**Storage**: Dedicated SQL columns with numeric types

**Performance**: 10-100x faster than JSONB

**Examples**:
- `revenue DECIMAL(10,2)` - Total sale amount
- `quantity INT` - Number of items
- `duration_ms BIGINT` - Event duration in milliseconds
- `error_count INT` - Number of errors

**Why SQL Columns?**:
```sql
-- ✅ FAST: Direct aggregation on SQL column
SELECT SUM(revenue) FROM tf_sales WHERE customer_id = $1;
-- Execution: 0.3ms (1M rows with index)

-- ❌ SLOW: Aggregation on JSONB field (don't do this!)
SELECT SUM((dimensions->>'revenue')::numeric) FROM tf_sales WHERE customer_id = $1;
-- Execution: 52ms (1M rows)
-- 173x slower!
```

### Dimensions (JSONB Paths)

**Purpose**: GROUP BY grouping keys

**Storage**: JSONB `data` column with flexible schema

**Performance**: Slower than SQL columns, but flexible (no ALTER TABLE needed)

**Examples**:
- `dimensions->>'category'` - Product category
- `dimensions->>'region'` - Geographic region
- `dimensions->>'product_type'` - Product classification
- `data#>>'{customer,segment}'` - Nested path for customer segment

**Why JSONB?**:
- Schema flexibility (add dimensions without ALTER TABLE)
- Sparse dimensions (not all facts have all dimensions)
- Nested structures (hierarchical dimensions)
- No need to create columns for rarely-used dimensions

**Query Pattern**:
```sql
SELECT
    dimensions->>'category' AS category,
    dimensions->>'region' AS region,
    SUM(revenue) AS total_revenue
FROM tf_sales
GROUP BY dimensions->>'category', dimensions->>'region';
```

### Denormalized Filters (Indexed SQL Columns)

**Purpose**: Fast WHERE filtering (avoid JSONB for high-selectivity filters)

**Storage**: Dedicated indexed SQL columns

**Performance**: B-tree index access (microseconds)

**Examples**:
- `customer_id UUID` - Filter by customer (high cardinality)
- `product_id UUID` - Filter by product
- `occurred_at TIMESTAMPTZ` - Filter by time range
- `status VARCHAR(50)` - Filter by status (low cardinality but frequently filtered)

**Why Denormalized?**:
```sql
-- ✅ FAST: Indexed SQL column filter
SELECT * FROM tf_sales
WHERE customer_id = 'uuid-123' AND occurred_at >= '2024-01-01';
-- Uses composite index, execution: 0.05ms

-- ❌ SLOW: JSONB filter (don't do this for high-selectivity filters!)
SELECT * FROM tf_sales
WHERE dimensions->>'customer_id' = 'uuid-123';
-- GIN index is slower for exact matches, execution: 2-5ms
```

---

## No Joins Principle

**Critical Architecture Decision**: FraiseQL does not support joins between tables.

### Implications

1. All dimensional data must be denormalized into `data` JSONB at ETL time
2. Dimension tables (`td_*`) are used at ETL time only, never at query time
3. Each table is completely standalone
4. Aggregate tables follow the same pattern (not joined to anything)

### Example

```sql
-- ❌ NOT SUPPORTED: Joining dimension tables at query time
SELECT
    s.revenue,
    p.category,
    c.segment
FROM tf_sales s
JOIN td_products p ON s.product_id = p.id
JOIN td_customers c ON s.customer_id = c.id;
-- FraiseQL does not support this!

-- ✅ CORRECT: Denormalized dimensions in JSONB (done at ETL time)
SELECT
    revenue,
    dimensions->>'product_category' AS category,
    dimensions->>'customer_segment' AS segment
FROM tf_sales;
-- Category and segment already denormalized by ETL process
```

### ETL Process (Managed by DBA/Data Team)

```sql
-- Step 1: ETL loads raw transaction
INSERT INTO staging_sales (transaction_id, product_id, customer_id, revenue)
VALUES ('txn-001', 'prod-123', 'cust-456', 99.99);

-- Step 2: ETL enriches with dimensional data from td_* tables
INSERT INTO tf_sales (
    id,
    revenue,
    quantity,
    cost,
    data,  -- ← Dimensions denormalized from td_products, td_customers
    customer_id,
    product_id,
    occurred_at
)
SELECT
    gen_random_uuid(),
    s.revenue,
    s.quantity,
    s.cost,
    jsonb_build_object(
        'product_category', p.category,
        'product_name', p.name,
        'customer_segment', c.segment,
        'customer_region', c.region
    ) AS data,  -- ← Denormalization happens here
    s.customer_id,
    s.product_id,
    s.occurred_at
FROM staging_sales s
JOIN td_products p ON s.product_id = p.id
JOIN td_customers c ON s.customer_id = c.id;

-- Step 3: Staging table is truncated
TRUNCATE staging_sales;
```

**Important**: This ETL process is managed by the DBA/data team, NOT by FraiseQL.

---

## Compilation Strategy

### Phase 4 (Compiler)

When the compiler encounters a schema with `fact_table=True`:

1. **Introspect Fact Table Structure**:
   ```rust
   let columns = introspect_table("tf_sales");
   let measures = columns.filter(|col| col.is_numeric());
   let data_column = columns.find(|col| col.name == "dimensions" && col.type == "jsonb");
   let filters = columns.filter(|col| col.has_index());
   ```

2. **Identify Components**:
   - Measure columns: `revenue`, `quantity`, `cost` (numeric types)
   - JSONB dimensions column: `dimensions`
   - Denormalized filter columns: `customer_id`, `product_id`, `occurred_at`

3. **Generate GraphQL Types**:
   - `Sales` type with measure fields + dimension fields (from JSONB paths)
   - `SalesWhereInput` with filter columns + JSONB path filters
   - `SalesAggregateInput` with measure columns for aggregation
   - `SalesGroupByInput` with dimension paths + temporal buckets

### Phase 5 (Runtime)

When executing an aggregation query:

1. **Parse GROUP BY Request**:
   ```graphql
   query {
     sales_aggregate(
       groupBy: { category: true, region: true }
     ) {
       category
       region
       revenue_sum
       count
     }
   }
   ```

2. **Generate SELECT Statement**:
   ```sql
   SELECT
       dimensions->>'category' AS category,
       dimensions->>'region' AS region,
       SUM(revenue) AS revenue_sum,
       COUNT(*) AS count
   FROM tf_sales
   GROUP BY dimensions->>'category', dimensions->>'region';
   ```

3. **Execute and Return Results**

---

## Database-Specific Considerations

### PostgreSQL

**Strengths**:
- Full JSONB support: `->`, `->>`, `#>`, `#>>`, `@>`, `?`, `?&`
- Native DATE_TRUNC for temporal bucketing
- FILTER (WHERE ...) for conditional aggregates
- GIN indexes for efficient JSONB queries
- Statistical functions (STDDEV, VARIANCE)

**Example**:
```sql
-- Advanced JSONB queries
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) FILTER (WHERE data @> '{"region": "North America"}') AS na_revenue,
    SUM(revenue) FILTER (WHERE data @> '{"region": "Europe"}') AS eu_revenue
FROM tf_sales
WHERE data ? 'category'  -- Has 'category' key
GROUP BY dimensions->>'category';
```

### MySQL

**Strengths**:
- JSON_EXTRACT, JSON_CONTAINS for JSON handling
- DATE_FORMAT for temporal bucketing
- GROUP_CONCAT for string aggregation

**Limitations**:
- No ILIKE (case-insensitive like)
- No native regex operators
- CASE WHEN emulation for conditional aggregates

**Example**:
```sql
-- MySQL JSON extraction
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    SUM(revenue) AS total_revenue
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category');
```

### SQLite

**Strengths**:
- Lightweight, embedded
- json_extract for basic JSON handling
- strftime for temporal bucketing

**Limitations**:
- Limited JSON support (no JSON operators beyond extraction)
- No GIN indexes
- No statistical functions

**Example**:
```sql
-- SQLite JSON extraction
SELECT
    json_extract(data, '$.category') AS category,
    SUM(revenue) AS total_revenue
FROM tf_sales
GROUP BY json_extract(data, '$.category');
```

### SQL Server

**Strengths**:
- JSON_VALUE, JSON_QUERY for JSON handling
- DATEPART for temporal bucketing
- Statistical functions (STDEV, VAR with population variants)
- FOR JSON clause for output formatting

**Limitations**:
- OPENJSON required for complex JSON queries
- CASE WHEN emulation for conditional aggregates

**Example**:
```sql
-- SQL Server JSON extraction
SELECT
    JSON_VALUE(data, '$.category') AS category,
    SUM(revenue) AS total_revenue
FROM tf_sales
GROUP BY JSON_VALUE(data, '$.category');
```

---

## Pre-Aggregated Fact Tables = Same Structure, Different Granularity

**Key Insight**: Pre-aggregated tables follow the SAME pattern as fact tables, just with coarser granularity. Use `tf_` prefix with descriptive suffix.

### Example: Daily Aggregates

```sql
-- Pre-aggregated fact table: same structure as tf_sales, daily granularity
CREATE TABLE tf_sales_daily (
    id BIGSERIAL PRIMARY KEY,
    day DATE NOT NULL,  -- Granularity dimension

    -- Pre-aggregated measures
    revenue DECIMAL(10,2) NOT NULL,      -- SUM(revenue) from tf_sales
    quantity INT NOT NULL,               -- SUM(quantity) from tf_sales
    transaction_count INT NOT NULL,      -- COUNT(*) from tf_sales

    -- Dimensions (same JSONB pattern!)
    dimensions JSONB NOT NULL,
    -- Can still group by category, region, etc. from data column

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_sales_daily_day ON tf_sales_daily(day);
CREATE INDEX idx_sales_daily_data_gin ON tf_sales_daily USING GIN(data);
```

**Populated via ETL** (managed by DBA/data team):

```sql
INSERT INTO tf_sales_daily (day, revenue, quantity, transaction_count, data)
SELECT
    DATE_TRUNC('day', occurred_at)::DATE AS day,
    SUM(revenue) AS revenue,
    SUM(quantity) AS quantity,
    COUNT(*) AS transaction_count,
    jsonb_build_object() AS data  -- Can preserve dimensions if needed
FROM tf_sales
GROUP BY DATE_TRUNC('day', occurred_at)::DATE
ON CONFLICT (day) DO UPDATE SET
    revenue = EXCLUDED.revenue,
    quantity = EXCLUDED.quantity,
    transaction_count = EXCLUDED.transaction_count;
```

**Query Pattern** (FraiseQL treats it like any fact table):

```graphql
query {
  sales_daily_aggregate(
    where: { day: { _gte: "2024-01-01" } }
  ) {
    day
    revenue_sum
    transaction_count_sum
  }
}
```

---

## Best Practices

### When to Use Fact Tables (tf_*)

✅ **Use for**:
- High-volume transactional data (sales, events, logs)
- Any granularity (raw transactions or pre-aggregated rollups)
- Real-time or near-real-time data ingestion
- Data requiring full history retention

❌ **Don't use for**:
- Low-volume reference data (use regular tables)
- Frequently updated records (facts are immutable)
- Data requiring joins (FraiseQL doesn't support joins)

### When to Use Pre-Aggregated Fact Tables (tf_*_daily, tf_*_monthly, etc.)

✅ **Use for**:
- Pre-computed aggregates for common queries
- Coarser granularity (daily, monthly, per-category, etc.)
- Query performance optimization
- Materialized rollups refreshed periodically
- Same structure as fact tables (measures + `data` JSONB)

### When to Use Dimension Tables (td_*)

✅ **Use for**:
- Reference data for ETL denormalization (products, customers, locations)
- Lookup data used to enrich fact tables during data loading
- Master data management

❌ **Don't use for**:
- Query-time joins (FraiseQL doesn't support joins)
- Direct GraphQL exposure (use denormalized data in fact tables instead)

### Index Strategy

**Denormalized Filter Columns**:
```sql
-- High-cardinality filters
CREATE INDEX idx_sales_customer ON tf_sales(customer_id);
CREATE INDEX idx_sales_product ON tf_sales(product_id);

-- Temporal filters
CREATE INDEX idx_sales_occurred ON tf_sales(occurred_at);

-- Composite indexes for common patterns
CREATE INDEX idx_sales_customer_occurred
    ON tf_sales(customer_id, occurred_at DESC);
```

**JSONB Dimensions** (PostgreSQL):
```sql
-- GIN index for JSONB queries
CREATE INDEX idx_sales_data_gin ON tf_sales USING GIN(data);

-- Specific path index for frequently-queried dimension
CREATE INDEX idx_sales_category
    ON tf_sales ((dimensions->>'category'));
```

**Don't Over-Index**:
- Every index slows INSERT/UPDATE operations
- Index only frequently-filtered columns
- Monitor query patterns before adding indexes

---

## Related Specifications

- **Aggregation Model** (`aggregation-model.md`) - GROUP BY, aggregates, HAVING
- **Analytical Schema Conventions** (`../specs/analytical-schema-conventions.md`) - Naming patterns
- **Schema Conventions** (`../specs/schema-conventions.md`) - General schema patterns
- **Database Targeting** (`../database/database-targeting.md`) - Multi-database support

---

*End of Fact-Dimension Pattern Specification*
