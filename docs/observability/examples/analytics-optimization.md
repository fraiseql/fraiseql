# Example: Analytics Dashboard Optimization

## Overview

This example demonstrates optimizing a **complex analytics dashboard** with multiple JSON dimensions, resulting in significant performance improvements through strategic denormalization.

**Scenario**: Multi-tenant SaaS analytics platform with slow aggregate queries

**Results**: 8-15x speedup across multiple query types

**Databases**: PostgreSQL and SQL Server

---

## Initial State

### Schema

```python
@fraiseql.fact_table(
    table_name='tf_sales',
    measures=['revenue', 'cost', 'profit', 'quantity'],
    dimension_column='dimensions'
)
class SalesMetrics:
    revenue: float
    cost: float
    profit: float
    quantity: int
    dimensions: dict  # {tenant_id, region, category, product_id, date, customer_tier}
```

### Problem Queries

Dashboard has 5 main views, all slow:

1. **Sales by Region** (1,250ms avg)
2. **Sales by Category** (980ms avg)
3. **Top Products** (1,450ms avg)
4. **Customer Tier Analysis** (1,100ms avg)
5. **Time Series Trends** (2,200ms avg)

---

## Analysis Results

```bash
fraiseql-cli analyze --database postgres://... --format text
```

**Output**:

```
🚀 High-Impact Optimizations (5):

1. Denormalize dimensions->>'region' (8,500 queries/day, 12.5x speedup)
2. Denormalize dimensions->>'category' (5,200 queries/day, 10.2x speedup)
3. Denormalize dimensions->>'customer_tier' (3,100 queries/day, 8.5x speedup)
4. Add index on recorded_at (used in time-series queries)
5. Denormalize dimensions->>'tenant_id' (multi-tenant isolation)
```

---

## Implementation

### Migration (PostgreSQL)

```sql
-- Denormalize all key dimensions
ALTER TABLE tf_sales ADD COLUMN region_id TEXT;
ALTER TABLE tf_sales ADD COLUMN category_id TEXT;
ALTER TABLE tf_sales ADD COLUMN customer_tier TEXT;
ALTER TABLE tf_sales ADD COLUMN tenant_id UUID;

-- Backfill (batched)
UPDATE tf_sales SET
    region_id = dimensions->>'region',
    category_id = dimensions->>'category',
    customer_tier = dimensions->>'customer_tier',
    tenant_id = (dimensions->>'tenant_id')::UUID
WHERE id IN (SELECT id FROM tf_sales LIMIT 10000);

-- Create indexes
CREATE INDEX CONCURRENTLY idx_tf_sales_region ON tf_sales (region_id);
CREATE INDEX CONCURRENTLY idx_tf_sales_category ON tf_sales (category_id);
CREATE INDEX CONCURRENTLY idx_tf_sales_tier ON tf_sales (customer_tier);
CREATE INDEX CONCURRENTLY idx_tf_sales_tenant_region ON tf_sales (tenant_id, region_id);

ANALYZE tf_sales;
```

---

## Results

### Performance Improvements

| Query Type | Before | After | Speedup |
|------------|--------|-------|---------|
| Sales by Region | 1,250ms | 98ms | **12.8x** |
| Sales by Category | 980ms | 85ms | **11.5x** |
| Top Products | 1,450ms | 180ms | **8.1x** |
| Customer Tier Analysis | 1,100ms | 105ms | **10.5x** |
| Time Series | 2,200ms | 150ms | **14.7x** |

### Storage Cost

- Original table: 520 MB
- After optimization: 585 MB (+65 MB, 12.5% increase)
- Cost: ~$0.05/month additional storage

---

## Key Takeaways

1. **Multiple dimensions** can be denormalized simultaneously
2. **Composite indexes** (tenant_id, region_id) improve multi-tenant queries
3. **Storage cost** is minimal compared to performance gains
4. **Dashboard load time** reduced from 8-10 seconds to < 1 second

---

*See [basic-denormalization.md](basic-denormalization.md) for detailed step-by-step workflow.*
