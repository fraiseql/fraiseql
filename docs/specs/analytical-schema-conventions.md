<!-- Skip to main content -->
---

title: Analytical Schema Conventions
description: This document defines naming conventions and patterns for analytical tables in FraiseQL.
keywords: ["format", "compliance", "schema", "protocol", "specification", "standard"]
tags: ["documentation", "reference"]
---

# Analytical Schema Conventions

**Version:** 1.0
**Status:** Complete
**Audience:** Data engineers, DBAs, schema designers
**Date:** January 12, 2026

---

## Overview

This document defines naming conventions and patterns for analytical tables in FraiseQL.

**Core Principle**: No joins. All fact tables use the same pattern: measures (SQL columns) + dimensions (`dimensions` JSONB column).

---

## Table Naming Conventions

### Fact Tables (tf_)

**Prefix**: `tf_` (table fact)
**Pattern**: `tf_<domain>_<noun>`
**Purpose**: Raw, finest-granularity transactional data

**Examples**:

- `tf_sales` - Sales transactions (one row per sale)
- `tf_events` - Application events (one row per event)
- `tf_api_requests` - API usage (one row per request)
- `tf_user_sessions` - Session analytics (one row per session)
- `tf_orders` - Order transactions
- `tf_payments` - Payment records

**Structure**:

```sql
<!-- Code example in SQL -->
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,
    -- Measures (SQL columns)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    -- Dimensions (JSONB)
    dimensions JSONB NOT NULL,
    -- Denormalized filters (indexed)
    customer_id UUID NOT NULL,
    product_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```text
<!-- Code example in TEXT -->

### Pre-Aggregated Fact Tables (Different Granularity)

**Important**: Pre-aggregated tables are just **fact tables at a different granularity**. Use `tf_` prefix with a descriptive suffix indicating the granularity.

**Pattern**: `tf_<domain>_<granularity>` or `tf_<domain>_by_<dimension>`
**Purpose**: Pre-computed aggregates for faster queries on common patterns

**Examples**:

- `tf_sales_daily` - Daily sales rollup (same as `tf_sales` but at day granularity)
- `tf_sales_by_category` - Sales grouped by category
- `tf_events_monthly` - Monthly event aggregates
- `tf_api_requests_hourly` - Hourly API usage

**Structure** (identical to fact tables):

```sql
<!-- Code example in SQL -->
-- Pre-aggregated fact table at daily granularity
CREATE TABLE tf_sales_daily (
    id BIGSERIAL PRIMARY KEY,
    day DATE NOT NULL UNIQUE,  -- Granularity dimension
    -- Pre-aggregated measures
    revenue DECIMAL(10,2) NOT NULL,      -- SUM(revenue)
    quantity INT NOT NULL,               -- SUM(quantity)
    transaction_count INT NOT NULL,      -- COUNT(*)
    -- Dimensions (same JSONB pattern as tf_sales)
    dimensions JSONB NOT NULL,
    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```text
<!-- Code example in TEXT -->

**Note**: The legacy `ta_` prefix is deprecated. Use `tf_` for all fact tables regardless of granularity.

### Dimension Tables (td_)

**Prefix**: `td_` (table dimension)
**Pattern**: `td_<noun>`
**Purpose**: Reference data for ETL denormalization (NOT used at query time)

**Important**: FraiseQL does NOT join these tables. They are used by ETL to populate `dimensions` JSONB in fact tables.

**Examples**:

- `td_products` - Product catalog
- `td_customers` - Customer master data
- `td_locations` - Geographic hierarchy
- `td_categories` - Category taxonomy

**Structure** (regular table, not fact pattern):

```sql
<!-- Code example in SQL -->
CREATE TABLE td_products (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    category VARCHAR(100) NOT NULL,
    price DECIMAL(10,2) NOT NULL,
    -- No data JSONB column needed
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```text
<!-- Code example in TEXT -->

---

## Column Naming Conventions

### Measures (SQL Columns)

**Pattern**: Descriptive noun, numeric type
**Purpose**: Fast aggregation (10-100x faster than JSONB)

**Examples**:

- `revenue DECIMAL(10,2)` - Monetary value
- `quantity INT` - Count of items
- `duration_ms BIGINT` - Duration in milliseconds
- `error_count INT` - Number of errors
- `file_size_bytes BIGINT` - File size
- `response_time_ms INT` - Response time

**Naming Rules**:

- Use snake_case
- Include units if ambiguous (`_ms`, `_bytes`, `_pct`)
- Be specific (`revenue` not `amount`, `quantity` not `count`)

### Dimensions (JSONB Paths)

**Column Name**: `data` (default, configurable)
**Path Pattern**: Snake_case keys in JSONB

**Examples**:

```json
<!-- Code example in JSON -->
{
  "category": "Electronics",
  "region": "North America",
  "product_type": "Laptop",
  "customer_segment": "Enterprise",
  "payment_method": "Credit Card",
  "shipping_method": "Express"
}
```text
<!-- Code example in TEXT -->

**Nested Paths**:

```json
<!-- Code example in JSON -->
{
  "customer": {
    "segment": "Enterprise",
    "industry": "Technology"
  },
  "product": {
    "category": "Electronics",
    "subcategory": "Computers"
  }
}
```text
<!-- Code example in TEXT -->

**Access Pattern**:

```sql
<!-- Code example in SQL -->
-- Top-level
dimensions->>'category'

-- Nested
dimensions#>>'{customer,segment}'
```text
<!-- Code example in TEXT -->

### Denormalized Filters (Indexed SQL Columns)

**Pattern**: Foreign key or frequently-filtered attribute
**Purpose**: Fast WHERE filtering (avoid JSONB for high-selectivity filters)

**Examples**:

- `customer_id UUID` - Foreign key
- `product_id UUID` - Foreign key
- `occurred_at TIMESTAMPTZ` - Temporal filter
- `status VARCHAR(50)` - Status filter (e.g., 'completed', 'cancelled')
- `priority INT` - Priority level

**Why Denormalized?**:

```sql
<!-- Code example in SQL -->
-- ✅ FAST: Indexed SQL column
WHERE customer_id = 'uuid-123'  -- Uses B-tree index

-- ❌ SLOW: JSONB filter
WHERE dimensions->>'customer_id' = 'uuid-123'  -- GIN index slower for exact match
```text
<!-- Code example in TEXT -->

---

## ETL Responsibility

**Critical**: FraiseQL does NOT manage ETL. DBA/data team handles:

1. **Creating fact/aggregate tables** with proper structure
2. **Populating tables** with denormalized data
3. **Maintaining dimension tables** (`td_*`) for lookup
4. **Refreshing aggregate tables** via scheduled jobs

**Example ETL Flow**:

```sql
<!-- Code example in SQL -->
-- Step 1: Staging table receives raw data
INSERT INTO staging_sales (transaction_id, product_id, customer_id, revenue)
VALUES ('txn-001', 'prod-123', 'cust-456', 99.99);

-- Step 2: ETL enriches and denormalizes
INSERT INTO tf_sales (
    id, revenue, quantity, cost,
    dimensions,  -- ← Denormalized from td_products, td_customers
    customer_id, product_id, occurred_at
)
SELECT
    gen_random_uuid(),
    s.revenue, s.quantity, s.cost,
    jsonb_build_object(
        'product_category', p.category,
        'product_name', p.name,
        'customer_segment', c.segment,
        'customer_region', c.region
    ) AS dimensions,  -- ← Denormalization happens here
    s.customer_id, s.product_id, s.occurred_at
FROM staging_sales s
JOIN td_products p ON s.product_id = p.id  -- ← ETL time join
JOIN td_customers c ON s.customer_id = c.id;

-- Step 3: Clean staging
TRUNCATE staging_sales;
```text
<!-- Code example in TEXT -->

---

## Index Recommendations

### Fact Tables

```sql
<!-- Code example in SQL -->
-- Denormalized filter columns (B-tree)
CREATE INDEX idx_sales_customer ON tf_sales(customer_id);
CREATE INDEX idx_sales_product ON tf_sales(product_id);
CREATE INDEX idx_sales_occurred ON tf_sales(occurred_at);
CREATE INDEX idx_sales_status ON tf_sales(status);

-- JSONB dimensions (GIN, PostgreSQL only)
CREATE INDEX idx_sales_dimensions_gin ON tf_sales USING GIN(dimensions);

-- Specific JSONB path (faster than GIN for exact lookups)
CREATE INDEX idx_sales_category ON tf_sales ((dimensions->>'category'));

-- Composite indexes for common query patterns
CREATE INDEX idx_sales_customer_occurred
    ON tf_sales(customer_id, occurred_at DESC);
```text
<!-- Code example in TEXT -->

### Pre-Aggregated Fact Tables

```sql
<!-- Code example in SQL -->
-- Granularity dimension (unique)
CREATE UNIQUE INDEX idx_sales_daily_day ON tf_sales_daily(day);

-- JSONB dimensions (if still grouping within aggregates)
CREATE INDEX idx_sales_daily_dimensions_gin ON tf_sales_daily USING GIN(dimensions);
```text
<!-- Code example in TEXT -->

**Don't Over-Index**:

- Every index slows INSERT/UPDATE
- Monitor query patterns before adding indexes
- Use composite indexes for common filter combinations

---

## FraiseQL Schema Definition

### Fact Table Binding

```python
<!-- Code example in Python -->
from FraiseQL import schema, type, query, ID

@schema.type
class Sales:
    id: ID
    # Measures (SQL columns)
    revenue: float
    quantity: int
    cost: float
    # Dimensions (from dimensions JSONB)
    category: str            # dimensions->>'category'
    region: str              # dimensions->>'region'
    product_name: str        # dimensions->>'product_name'
    customer_segment: str    # dimensions->>'customer_segment'
    # Denormalized filters
    customer_id: UUID  # UUID v4 for GraphQL ID
    product_id: UUID  # UUID v4 for GraphQL ID
    occurred_at: str

@schema.query
def sales_aggregate(
    where: "SalesWhereInput" = None,
    groupBy: "SalesGroupByInput" = None,
    having: "SalesHavingInput" = None
) -> list["SalesAggregate"]:
    """Auto-generated aggregate query."""
    pass

# Mark as fact table
schema.bind("Sales", "view", "tf_sales", fact_table=True)
```text
<!-- Code example in TEXT -->

### Compiler Behavior

When `fact_table=True`:

1. **Introspect Table**: Detect measures, `data` column, filters
2. **Generate Aggregate Types**:
   - `SalesAggregate` - Result type with grouped dimensions + aggregated measures
   - `SalesGroupByInput` - Dimension paths + temporal buckets
   - `SalesHavingInput` - Aggregate filters
3. **Generate Query**: `sales_aggregate(where, groupBy, having)`

### Generated GraphQL (PostgreSQL)

```graphql
<!-- Code example in GraphQL -->
type SalesAggregate {
  # Grouped dimensions (from dimensions JSONB)
  category: String
  region: String
  product_name: String
  occurred_at_day: Date
  occurred_at_month: Date

  # Aggregated measures (from SQL columns)
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  quantity_sum: Int
  quantity_avg: Int
}

input SalesGroupByInput {
  category: Boolean
  region: Boolean
  product_name: Boolean
  customer_segment: Boolean
  occurred_at_day: Boolean
  occurred_at_week: Boolean
  occurred_at_month: Boolean
}

input SalesHavingInput {
  revenue_sum_gt: Float
  revenue_avg_gte: Float
  count_eq: Int
}
```text
<!-- Code example in TEXT -->

---

## Best Practices

### DO ✅

- Use `tf_` prefix for all fact tables (any granularity)
- Use `td_` prefix for dimension tables (ETL reference data)
- Store measures as SQL columns (fast aggregation)
- Store dimensions in `data` JSONB (flexibility)
- Index denormalized filter columns
- Create pre-aggregated fact tables for common query patterns
- Use temporal bucketing for time-series analysis
- Name pre-aggregated tables clearly: `tf_sales_daily`, `tf_events_monthly`

### DON'T ❌

- Don't store measures in JSONB (10-100x slower)
- Don't use fact tables for low-volume reference data
- Don't update fact records (they should be immutable)
- Don't join tables at query time (FraiseQL doesn't support joins)
- Don't over-index (slows writes)
- Don't mix OLTP and OLAP patterns in same table

---

## Related Specifications

- **Fact-Dimension Pattern** (`../architecture/analytics/fact-dimension-pattern.md`) - Detailed pattern explanation
- **Schema Conventions** (`schema-conventions.md`) - General schema patterns
- **Aggregation Operators** (`aggregation-operators.md`) - Available aggregate functions

---
