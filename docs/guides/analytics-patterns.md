# Analytics Patterns Guide

**Version:** 1.0
**Status:** Complete (Phase 1-2 patterns)
**Audience:** Developers, data analysts, analytics engineers
**Date:** January 12, 2026

---

## Overview

This guide provides practical examples of common analytical query patterns in FraiseQL v2, showing GraphQL queries and their corresponding SQL execution.

**Key Principle**: All examples use fact tables with measures (SQL columns) + dimensions (`data` JSONB column). No joins.

---

## Pattern 1: Simple Aggregation

**Use Case**: Total revenue and average order value

### GraphQL Query

```graphql
query {
  sales_aggregate {
    count
    revenue_sum
    revenue_avg
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    AVG(revenue) AS revenue_avg
FROM tf_sales;
```

**Performance**: ~0.2ms for 1M rows (with no WHERE clause, uses table statistics)

---

## Pattern 2: GROUP BY Single Dimension

**Use Case**: Revenue by category

### GraphQL Query

```graphql
query {
  sales_aggregate(groupBy: { category: true }) {
    category
    revenue_sum
    count
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY dimensions->>'category';
```

**Performance**: ~1-2ms for 1M rows (with GIN index on `data`)

---

## Pattern 3: GROUP BY Multiple Dimensions

**Use Case**: Revenue by category and region

### GraphQL Query

```graphql
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
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    dimensions->>'category' AS category,
    dimensions->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    SUM(quantity) AS quantity_sum
FROM tf_sales
GROUP BY dimensions->>'category', dimensions->>'region';
```

**Performance**: ~2-3ms for 1M rows

---

## Pattern 4: Temporal Bucketing (Daily)

**Use Case**: Daily sales trend

### GraphQL Query

```graphql
query {
  sales_aggregate(
    groupBy: { occurred_at_day: true }
  ) {
    occurred_at_day
    revenue_sum
    count
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    DATE_TRUNC('day', occurred_at) AS occurred_at_day,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY DATE_TRUNC('day', occurred_at)
ORDER BY occurred_at_day;
```

**Performance**: ~5-10ms for 1M rows (with index on `occurred_at`)

---

## Pattern 5: Filtered Aggregation

**Use Case**: Revenue for specific customer

### GraphQL Query

```graphql
query {
  sales_aggregate(
    where: { customer_id: { _eq: "uuid-123" } }
  ) {
    count
    revenue_sum
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum
FROM tf_sales
WHERE customer_id = $1;
-- Parameters: ["uuid-123"]
```

**Performance**: ~0.05ms (using B-tree index on `customer_id`)

---

## Pattern 6: HAVING Clause

**Use Case**: Categories with revenue > $10,000

### GraphQL Query

```graphql
query {
  sales_aggregate(
    groupBy: { category: true },
    having: { revenue_sum_gt: 10000 }
  ) {
    category
    revenue_sum
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY dimensions->>'category'
HAVING SUM(revenue) > $1;
-- Parameters: [10000]
```

**Performance**: ~1-2ms for 1M rows

---

## Pattern 7: Conditional Aggregates (PostgreSQL)

**Use Case**: Revenue by payment method using FILTER

### GraphQL Query

```graphql
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
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    SUM(revenue) FILTER (WHERE dimensions->>'payment_method' = 'credit_card') AS revenue_sum_credit_card,
    SUM(revenue) FILTER (WHERE dimensions->>'payment_method' = 'paypal') AS revenue_sum_paypal
FROM tf_sales;
```

**MySQL/SQLite/SQL Server** (emulated with CASE WHEN):

```sql
SELECT
    COUNT(*) AS count,
    SUM(revenue) AS revenue_sum,
    SUM(CASE WHEN dimensions->>'payment_method' = 'credit_card' THEN revenue ELSE 0 END) AS revenue_sum_credit_card,
    SUM(CASE WHEN dimensions->>'payment_method' = 'paypal' THEN revenue ELSE 0 END) AS revenue_sum_paypal
FROM tf_sales;
```

---

## Pattern 8: Time-Series with Multiple Dimensions

**Use Case**: Monthly revenue by category and region

### GraphQL Query

```graphql
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
```

### SQL Execution (PostgreSQL)

```sql
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
```

**Performance**: ~10-20ms for 1M rows

---

## Pattern 9: Nested Dimension Paths

**Use Case**: Revenue by customer segment (nested JSONB)

### Schema Definition

```python
@schema.type
class Sales:
    # ...
    customer_segment: str  # Maps to dimensions#>>'{customer,segment}'
```

### GraphQL Query

```graphql
query {
  sales_aggregate(
    groupBy: { customer_segment: true }
  ) {
    customer_segment
    revenue_sum
  }
}
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    dimensions#>>'{customer,segment}' AS customer_segment,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY dimensions#>>'{customer,segment}';
```

---

## Pattern 10: Combining Filters and Grouping

**Use Case**: Revenue by region for Q1 2024

### GraphQL Query

```graphql
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
```

### SQL Execution (PostgreSQL)

```sql
SELECT
    dimensions->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    SUM(quantity) AS quantity_sum
FROM tf_sales
WHERE occurred_at >= $1 AND occurred_at < $2
GROUP BY dimensions->>'region';
-- Parameters: ["2024-01-01", "2024-04-01"]
```

**Performance**: ~0.5-1ms (using index on `occurred_at`)

---

## Performance Optimization

### Use Denormalized Columns for Filters

❌ **SLOW** (JSONB filter):

```sql
WHERE dimensions->>'customer_id' = 'uuid-123'
-- ~5-10ms (even with GIN index)
```

✅ **FAST** (indexed SQL column):

```sql
WHERE customer_id = 'uuid-123'
-- ~0.05ms (B-tree index)
```

**100-200x faster!**

### Pre-Compute Common Aggregates

Create pre-aggregated fact tables for frequently-used rollups:

```sql
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
```

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
SELECT
    dimensions->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    STDDEV(revenue) AS revenue_stddev
FROM tf_sales
WHERE dimensions @> '{"region": "North America"}'
GROUP BY dimensions->>'category';
```

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
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category');
```

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
SELECT
    json_extract(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum
FROM tf_sales
GROUP BY json_extract(data, '$.category');
```

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
SELECT
    JSON_VALUE(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum,
    STDEV(revenue) AS revenue_stdev
FROM tf_sales
GROUP BY JSON_VALUE(data, '$.category');
```

---

## Common Use Cases

### E-Commerce Analytics

**Daily Sales Trend**:

```graphql
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
```

**Top Products by Revenue**:

```graphql
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
```

### SaaS Metrics

**Monthly Recurring Revenue by Plan**:

```graphql
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
```

**Churn Rate**:

```graphql
query {
  subscriptions_aggregate(
    where: { status: { _eq: "cancelled" } }
    groupBy: { occurred_at_month: true }
  ) {
    occurred_at_month
    count
  }
}
```

### API Monitoring

**Requests by Endpoint**:

```graphql
query {
  api_requests_aggregate(
    groupBy: { endpoint: true }
  ) {
    endpoint
    count
    duration_ms_avg
  }
}
```

**Error Rate by Status Code**:

```graphql
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
```

---

## Related Documentation

- **Aggregation Model** (`../architecture/analytics/aggregation-model.md`) - Compilation and execution
- **Fact-Dimension Pattern** (`../architecture/analytics/fact-dimension-pattern.md`) - Table structure
- **Analytical Schema Conventions** (`../specs/analytical-schema-conventions.md`) - Naming patterns
- **Aggregation Operators** (`../specs/aggregation-operators.md`) - Available functions

---

*End of Analytics Patterns Guide*
