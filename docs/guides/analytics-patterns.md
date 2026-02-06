<!-- Skip to main content -->
---

title: Analytics Patterns Guide
description: - SQL aggregation functions (SUM, AVG, COUNT, GROUP BY, HAVING)
keywords: ["workflow", "debugging", "implementation", "best-practices", "deployment", "saas", "realtime", "ecommerce"]
tags: ["documentation", "reference"]
---

# Analytics Patterns Guide

**Status:** ✅ Production Ready
**Audience:** Developers, Data Engineers, Architects
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

---

## Prerequisites

### Required Knowledge

- SQL aggregation functions (SUM, AVG, COUNT, GROUP BY, HAVING)
- Fact tables and dimension tables (star schema/data warehouse concepts)
- JSONB/JSON data types and querying
- Window functions (ROW_NUMBER, RANK, LAG, LEAD)
- Time-series analysis and bucketing
- Filtering and WHERE clause optimization
- Query performance considerations
- GraphQL query syntax and execution

### Required Software

- FraiseQL v2.0.0-alpha.1 or later with Arrow Flight support (for columnar queries)
- Your chosen SDK language (Python, TypeScript, Go, Java, etc.)
- PostgreSQL 14+, MySQL 8.0+, or ClickHouse (analytics-optimized)
- SQL client for schema inspection (psql, mysql, etc.)
- A code editor for defining GraphQL queries
- Optional: BI tool (Tableau, Looker, Metabase, Apache Superset)

### Required Infrastructure

- FraiseQL server with analytics schema deployed
- Fact tables with measures (numeric columns) and dimensions (JSONB)
- PostgreSQL/MySQL database with analytical indexes
- Optional: Analytics database (ClickHouse) for large-scale analytics
- Optional: Arrow Flight endpoint for columnar data export
- Sample data loaded in analytics tables

#### Optional but Recommended

- Data warehouse ETL tool (dbt, Airflow)
- BI platform for visualization and dashboarding
- Query performance profiling tools
- Data modeling documentation
- Arrow/Parquet export infrastructure for external analysis
- Caching layer (Redis) for repeated aggregations

**Time Estimate:** 20-40 minutes per pattern example, 2-4 hours to adapt patterns to your schema

## Overview

This guide provides practical examples of common analytical query patterns in FraiseQL v2, showing GraphQL queries and their corresponding SQL execution.

**Key Principle**: All examples use fact tables with measures (SQL columns) + dimensions (`data` JSONB column). No joins.

---

## Pattern 1: Simple Aggregation

**Use Case**: Total revenue and average order value

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate {
    count
    revenue_sum
    revenue_avg
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    AVG(revenue) AS revenue_avg
FROM tf_sales;
```text
<!-- Code example in TEXT -->

**Performance**: ~0.2ms for 1M rows (with no WHERE clause, uses table statistics)

---

## Pattern 2: GROUP BY Single Dimension

**Use Case**: Revenue by category

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(groupBy: { category: true }) {
    category
    revenue_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY dimensions->>'category';
```text
<!-- Code example in TEXT -->

**Performance**: ~1-2ms for 1M rows (with GIN index on `data`)

---

## Pattern 3: GROUP BY Multiple Dimensions

**Use Case**: Revenue by category and region

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: {
      category: true,
      region: true
    }
  ) {
    category
    region
    revenue_sum
    quantity_sum
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    dimensions->>'category' AS category,
    dimensions->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    SUM(quantity) AS quantity_sum
FROM tf_sales
GROUP BY dimensions->>'category', dimensions->>'region';
```text
<!-- Code example in TEXT -->

**Performance**: ~2-3ms for 1M rows

---

## Pattern 4: Temporal Bucketing (Daily)

**Use Case**: Daily sales trend

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { occurred_at_day: true }
  ) {
    occurred_at_day
    revenue_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    DATE_TRUNC('day', occurred_at) AS occurred_at_day,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY DATE_TRUNC('day', occurred_at)
ORDER BY occurred_at_day;
```text
<!-- Code example in TEXT -->

**Performance**: ~5-10ms for 1M rows (with index on `occurred_at`)

---

## Pattern 5: Filtered Aggregation

**Use Case**: Revenue for specific customer

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    where: { customer_id: { _eq: "uuid-123" } }
  ) {
    count
    revenue_sum
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum
FROM tf_sales
WHERE customer_id = $1;
-- Parameters: ["uuid-123"]
```text
<!-- Code example in TEXT -->

**Performance**: ~0.05ms (using B-tree index on `customer_id`)

---

## Pattern 6: HAVING Clause

**Use Case**: Categories with revenue > $10,000

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { category: true },
    having: { revenue_sum_gt: 10000 }
  ) {
    category
    revenue_sum
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY dimensions->>'category'
HAVING SUM(revenue) > $1;
-- Parameters: [10000]
```text
<!-- Code example in TEXT -->

**Performance**: ~1-2ms for 1M rows

---

## Pattern 7: Conditional Aggregates (PostgreSQL)

**Use Case**: Revenue by payment method using FILTER

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate {
    count
    revenue_sum
    revenue_sum_credit_card: revenue_sum(
      filter: { payment_method: { _eq: "credit_card" } }
    )
    revenue_sum_paypal: revenue_sum(
      filter: { payment_method: { _eq: "paypal" } }
    )
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    SUM(revenue) FILTER (WHERE dimensions->>'payment_method' = 'credit_card') AS revenue_sum_credit_card,
    SUM(revenue) FILTER (WHERE dimensions->>'payment_method' = 'paypal') AS revenue_sum_paypal
FROM tf_sales;
```text
<!-- Code example in TEXT -->

**MySQL/SQLite/SQL Server** (emulated with CASE WHEN):

```sql
<!-- Code example in SQL -->
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    SUM(CASE WHEN dimensions->>'payment_method' = 'credit_card' THEN revenue ELSE 0 END) AS revenue_sum_credit_card,
    SUM(CASE WHEN dimensions->>'payment_method' = 'paypal' THEN revenue ELSE 0 END) AS revenue_sum_paypal
FROM tf_sales;
```text
<!-- Code example in TEXT -->

---

## Pattern 8: Time-Series with Multiple Dimensions

**Use Case**: Monthly revenue by category and region

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: {
      occurred_at_month: true,
      category: true,
      region: true
    }
  ) {
    occurred_at_month
    category
    region
    revenue_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    DATE_TRUNC('month', occurred_at) AS occurred_at_month,
    dimensions->>'category' AS category,
    dimensions->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY
    DATE_TRUNC('month', occurred_at),
    dimensions->>'category',
    dimensions->>'region'
ORDER BY occurred_at_month, category, region;
```text
<!-- Code example in TEXT -->

**Performance**: ~10-20ms for 1M rows

---

## Pattern 9: Nested Dimension Paths

**Use Case**: Revenue by customer segment (nested JSONB)

### Schema Definition

```python
<!-- Code example in Python -->
@schema.type
class Sales:
    # ...
    customer_segment: str  # Maps to dimensions#>>'{customer,segment}'
```text
<!-- Code example in TEXT -->

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { customer_segment: true }
  ) {
    customer_segment
    revenue_sum
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    dimensions#>>'{customer,segment}' AS customer_segment,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY dimensions#>>'{customer,segment}';
```text
<!-- Code example in TEXT -->

---

## Pattern 10: Combining Filters and Grouping

**Use Case**: Revenue by region for Q1 2024

### GraphQL Query

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    where: {
      occurred_at: {
        _gte: "2024-01-01",
        _lt: "2024-04-01"
      }
    },
    groupBy: { region: true }
  ) {
    region
    revenue_sum
    quantity_sum
  }
}
```text
<!-- Code example in TEXT -->

### SQL Execution (PostgreSQL)

```sql
<!-- Code example in SQL -->
SELECT
    dimensions->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    SUM(quantity) AS quantity_sum
FROM tf_sales
WHERE occurred_at >= $1 AND occurred_at < $2
GROUP BY dimensions->>'region';
-- Parameters: ["2024-01-01", "2024-04-01"]
```text
<!-- Code example in TEXT -->

**Performance**: ~0.5-1ms (using index on `occurred_at`)

---

## Performance Optimization

### Use Denormalized Columns for Filters

❌ **SLOW** (JSONB filter):

```sql
<!-- Code example in SQL -->
WHERE dimensions->>'customer_id' = 'uuid-123'
-- ~5-10ms (even with GIN index)
```text
<!-- Code example in TEXT -->

✅ **FAST** (indexed SQL column):

```sql
<!-- Code example in SQL -->
WHERE customer_id = 'uuid-123'
-- ~0.05ms (B-tree index)
```text
<!-- Code example in TEXT -->

### 100-200x faster

### Pre-Compute Common Aggregates

Create pre-aggregated fact tables for frequently-used rollups:

```sql
<!-- Code example in SQL -->
-- Create pre-aggregated fact table (daily granularity)
CREATE TABLE tf_sales_daily (
    id BIGSERIAL PRIMARY KEY,
    day DATE NOT NULL UNIQUE,
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    transaction_count INT NOT NULL,
    dimensions JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Populate via scheduled job (ETL)
INSERT INTO tf_sales_daily (day, revenue, quantity, transaction_count, data)
SELECT
    DATE_TRUNC('day', occurred_at)::DATE AS day,
    SUM(revenue) AS revenue,
    SUM(quantity) AS quantity,
    COUNT(*) AS transaction_count,
    jsonb_build_object() AS data
FROM tf_sales
GROUP BY DATE_TRUNC('day', occurred_at)::DATE
ON CONFLICT (day) DO UPDATE SET
    revenue = EXCLUDED.revenue,
    quantity = EXCLUDED.quantity,
    transaction_count = EXCLUDED.transaction_count;
```text
<!-- Code example in TEXT -->

**Query Speed**: ~0.1ms (reading from pre-aggregated table vs ~10ms from raw fact table)

### Leverage Arrow Plane for BI Tools

FraiseQL's Arrow plane automatically optimizes columnar data transfer for pre-aggregated views, providing efficient bulk data export for BI tools.

---

## Database-Specific Notes

### PostgreSQL

**Strengths**:

- Full JSONB support: `@>`, `?`, `?&` for complex filters
- Native `DATE_TRUNC` for all temporal buckets
- `FILTER (WHERE ...)` for conditional aggregates
- Statistical functions (STDDEV, VARIANCE)

**Example**:

```sql
<!-- Code example in SQL -->
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    STDDEV(revenue) AS revenue_stddev
FROM tf_sales
WHERE dimensions @> '{"region": "North America"}'
GROUP BY dimensions->>'category';
```text
<!-- Code example in TEXT -->

### MySQL

**Strengths**:

- JSON_EXTRACT, JSON_CONTAINS for JSON handling
- DATE_FORMAT for temporal bucketing
- GROUP_CONCAT for string aggregation

**Limitations**:

- No ILIKE (case-insensitive)
- No regex operators
- Emulate FILTER with CASE WHEN

**Example**:

```sql
<!-- Code example in SQL -->
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category');
```text
<!-- Code example in TEXT -->

### SQLite

**Strengths**:

- Lightweight, embedded
- json_extract for basic JSON
- strftime for temporal bucketing

**Limitations**:

- No statistical functions
- Limited JSON operators
- Use pre-aggregated views for performance

**Example**:

```sql
<!-- Code example in SQL -->
SELECT
    json_extract(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY json_extract(data, '$.category');
```text
<!-- Code example in TEXT -->

### SQL Server

**Strengths**:

- JSON_VALUE, JSON_QUERY for JSON handling
- DATEPART for temporal bucketing
- Statistical functions (STDEV, VAR)
- FOR JSON clause for output formatting

**Limitations**:

- Emulate FILTER with CASE WHEN
- OPENJSON required for complex queries

**Example**:

```sql
<!-- Code example in SQL -->
SELECT
    JSON_VALUE(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum,
    STDEV(revenue) AS revenue_stdev
FROM tf_sales
GROUP BY JSON_VALUE(data, '$.category');
```text
<!-- Code example in TEXT -->

---

## Common Use Cases

### E-Commerce Analytics

**Daily Sales Trend**:

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { occurred_at_day: true }
    orderBy: { occurred_at_day: ASC }
  ) {
    occurred_at_day
    revenue_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

**Top Products by Revenue**:

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { product_name: true }
    orderBy: { revenue_sum: DESC }
    limit: 10
  ) {
    product_name
    revenue_sum
    quantity_sum
  }
}
```text
<!-- Code example in TEXT -->

### SaaS Metrics

**Monthly Recurring Revenue by Plan**:

```graphql
<!-- Code example in GraphQL -->
query {
  subscriptions_aggregate(
    where: { status: { _eq: "active" } }
    groupBy: { plan: true, occurred_at_month: true }
  ) {
    plan
    occurred_at_month
    revenue_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

**Churn Rate**:

```graphql
<!-- Code example in GraphQL -->
query {
  subscriptions_aggregate(
    where: { status: { _eq: "cancelled" } }
    groupBy: { occurred_at_month: true }
  ) {
    occurred_at_month
    count
  }
}
```text
<!-- Code example in TEXT -->

### API Monitoring

**Requests by Endpoint**:

```graphql
<!-- Code example in GraphQL -->
query {
  api_requests_aggregate(
    groupBy: { endpoint: true }
  ) {
    endpoint
    count
    duration_ms_avg
  }
}
```text
<!-- Code example in TEXT -->

**Error Rate by Status Code**:

```graphql
<!-- Code example in GraphQL -->
query {
  api_requests_aggregate(
    groupBy: { status_code: true }
    having: { count_gte: 100 }
  ) {
    status_code
    count
    duration_ms_avg
  }
}
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### "Aggregation query returns zero rows"

**Cause:** Usually a schema mismatch or missing data in fact table.

#### Diagnosis

1. Verify fact table exists: `SELECT COUNT(*) FROM fact_table_name;`
2. Check column names match schema: `SELECT column_name FROM information_schema.columns WHERE table_name = 'fact_table_name';`
3. Verify date range has data: `SELECT COUNT(*) FROM fact_table WHERE created_at > NOW() - INTERVAL '30 days';`

#### Solutions

- Ensure fact table is populated with data
- Verify table name matches exactly (case-sensitive in some databases)
- Check date/time filters in query
- Ensure dimension JSON exists for grouping columns

### "Aggregation query is very slow (>30 seconds)"

**Cause:** Missing indexes on GROUP BY or WHERE clause columns.

#### Diagnosis

1. Run `EXPLAIN (ANALYZE, BUFFERS)` on the aggregation query
2. Look for "Seq Scan" on fact table - indicates missing index
3. Check cardinality of grouping columns: `SELECT COUNT(DISTINCT column_name) FROM fact_table;`

#### Solutions

- Add composite index on fact table: `CREATE INDEX idx_fact_date_col ON fact_table(created_at, groupby_column);`
- Partition large fact tables by date
- Use materialized views for pre-aggregated data (table-backed views)
- Reduce date range or add more specific WHERE filters
- For Arrow Flight: Use ClickHouse for columnar aggregations

### "JSON dimension data not being extracted in aggregation"

**Cause:** Dimension data stored in JSONB but query doesn't specify extraction path.

#### Diagnosis

1. Check data exists: `SELECT data FROM fact_table LIMIT 1;`
2. Verify JSON structure: `SELECT jsonb_pretty(data) FROM fact_table LIMIT 1;`
3. Test extraction: `SELECT data->>'customer_id' FROM fact_table LIMIT 1;`

#### Solutions

- In WHERE clause, extract JSON: `WHERE data->>'customer_type' = 'premium'`
- In GROUP BY, extract JSON: `GROUP BY data->>'region'`
- For complex JSON: use `jsonb_to_record()` for deeper access
- Consider denormalizing frequently-accessed fields to actual columns

### "GROUP BY returning too many rows (millions)"

**Cause:** Grouping by high-cardinality dimension (unique values per row).

#### Diagnosis

1. Check cardinality: `SELECT COUNT(DISTINCT groupby_column) FROM fact_table;`
2. If > 100K distinct values, likely too granular

#### Solutions

- Use `HAVING COUNT(*) > N` to filter small groups
- Add grouping hierarchy (day → week → month)
- Use top-K pattern: limit to top 100 results by count
- Consider if grouping by customer_id makes sense (should group by customer_type instead)

### "Window function query fails with 'not supported'"

**Cause:** FraiseQL uses SQL window functions but not all are compiled for your target database.

#### Diagnosis

1. Check FraiseQL logs for specific error
2. Verify database version supports window functions (PostgreSQL 8.4+, MySQL 8.0+)
3. Test window function directly: `SELECT id, ROW_NUMBER() OVER (ORDER BY created_at) FROM table LIMIT 1;`

#### Solutions

- Use supported functions: ROW_NUMBER(), RANK(), DENSE_RANK(), LAG(), LEAD()
- Avoid NTILE if unsupported in your database
- For SQL Server: ensure compatibility level 2012+
- Consider pre-aggregating results and using post-aggregation window functions

### "Arrow Flight aggregation returns different results than JSON"

**Cause:** Arrow schema doesn't include necessary fields for aggregation.

#### Diagnosis

1. Compare row counts: JSON vs Arrow should be identical
2. Check if NULL values handled differently
3. Verify data type conversions (string vs int)

#### Solutions

- Ensure all grouping columns are included in Arrow schema
- Handle NULL values explicitly in GROUP BY: `GROUP BY COALESCE(column, 'unknown')`
- Verify date/timestamp conversions between JSON and Arrow
- Use same aggregation function in both planes

### "Timeouts in analytics queries"

**Cause:** Query scans too much data or database is under load.

#### Diagnosis

1. Check query complexity: `EXPLAIN` on aggregation
2. Verify database server resources: CPU, memory, disk I/O
3. Check if other queries are running: `SELECT COUNT(*) FROM pg_stat_activity;`

#### Solutions

- Add date range filter to limit data scanned
- Pre-aggregate using table-backed views (tv_*)
- Use materialized views for common aggregations
- Consider Arrow Flight for better columnar performance
- Scale database (add resources or replicas)

---

## See Also

### Architecture & Design

- **[Aggregation Model](../architecture/analytics/aggregation-model.md)** — Compilation and execution of aggregations
- **[Fact-Dimension Pattern](../architecture/analytics/fact-dimension-pattern.md)** — Table structure and relationships
- **[Arrow Plane Architecture](../architecture/database/arrow-plane.md)** — Columnar data plane for analytics

### Schema & Specifications

- **[Analytical Schema Conventions](../specs/analytical-schema-conventions.md)** — Naming patterns for analytics tables
- **[Aggregation Operators](../specs/aggregation-operators.md)** — Available aggregate functions
- **[Scalar Types Reference](../reference/scalars.md)** — Data types for analytical fields

### Related Guides

- **[Common Patterns](./patterns.md)** — Real-world patterns including analytics
- **[Arrow Flight Quick Start](./arrow-flight-quick-start.md)** — Exporting analytics results
- **[Arrow vs JSON Guide](./arrow-vs-json-guide.md)** — Choosing optimal data format for analytics
- **[Database Selection Guide](./database-selection-guide.md)** — Choosing database for analytics workloads
- **[View Selection Guide](./view-selection-performance-testing.md)** — Optimizing view types for performance

### Operations & Optimization

- **[Performance Tuning Runbook](../operations/performance-tuning-runbook.md)** — Optimizing slow queries
- **[Observability Architecture](../operations/observability-architecture.md)** — Monitoring analytics performance
- **[Monitoring Guide](./monitoring.md)** — Observing analytics in production

### Troubleshooting

- **[Common Gotchas](./common-gotchas.md)** — Analytics pitfalls and solutions
- **[Troubleshooting Decision Tree](./troubleshooting-decision-tree.md)** — Route to correct guide
- **[Troubleshooting Guide](../troubleshooting.md)** — FAQ and solutions

---
