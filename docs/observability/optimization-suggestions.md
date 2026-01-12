# Understanding Optimization Suggestions

## Overview

This guide explains each type of optimization suggestion in detail, helping you understand:

- What each suggestion means
- Why it was suggested
- How speedup is calculated
- What risks/trade-offs exist
- When to apply vs skip

---

## Suggestion Priority Levels

Every suggestion has a priority based on **impact score**:

**Impact Score** = (Queries Per Day) × (Speedup Factor)

| Priority | Impact Score | Typical Characteristics |
|----------|--------------|-------------------------|
| **Critical** | > 50,000 | Extremely high traffic + major speedup |
| **High** | 10,000 - 50,000 | High traffic or large speedup |
| **Medium** | 1,000 - 10,000 | Moderate impact |
| **Low** | < 1,000 | Minor improvement |

**Example**:
```
Suggestion: Denormalize dimensions->>'region'
- 8,500 queries/day × 12.5x speedup = 106,250 impact score
- Priority: Critical
```

---

## Suggestion Type 1: Denormalize JSON → Direct Column

### What It Means

Move frequently-accessed data from JSON/JSONB column to a dedicated direct column.

**Before (PostgreSQL)**:
```sql
CREATE TABLE tf_sales (
    id BIGINT PRIMARY KEY,
    revenue NUMERIC,
    dimensions JSONB  -- Contains: {region: 'US', category: 'Electronics'}
);

-- Query (slow):
SELECT * FROM tf_sales
WHERE dimensions->>'region' = 'US';  -- Parses JSON on every row
```

**After**:
```sql
CREATE TABLE tf_sales (
    id BIGINT PRIMARY KEY,
    revenue NUMERIC,
    region_id TEXT,        -- Denormalized from JSON
    dimensions JSONB
);

CREATE INDEX idx_tf_sales_region ON tf_sales (region_id);

-- Query (fast):
SELECT * FROM tf_sales
WHERE region_id = 'US';  -- Direct column lookup with index
```

**Before (SQL Server)**:
```sql
CREATE TABLE tf_sales (
    id BIGINT PRIMARY KEY,
    revenue DECIMAL(18,2),
    dimensions NVARCHAR(MAX)  -- JSON text
);

-- Query (slow):
SELECT * FROM tf_sales
WHERE JSON_VALUE(dimensions, '$.region') = 'US';
```

**After**:
```sql
CREATE TABLE tf_sales (
    id BIGINT PRIMARY KEY,
    revenue DECIMAL(18,2),
    region_id AS JSON_VALUE(dimensions, '$.region') PERSISTED,  -- Computed column
    dimensions NVARCHAR(MAX)
);

CREATE NONCLUSTERED INDEX idx_tf_sales_region ON tf_sales (region_id);

-- Query (fast):
SELECT * FROM tf_sales
WHERE region_id = 'US';
```

---

### Why Suggested

Denormalization is suggested when:

1. **High Frequency**: Path accessed 1000+ times/day (configurable)
2. **Used in Filters**: `WHERE` clause with high selectivity
3. **Used in Sorting**: `ORDER BY` on JSON path (always expensive)
4. **Used in Aggregates**: `GROUP BY` on JSON path

### Example Suggestion

```
Denormalize JSONB → Direct Column

Table: tf_sales
Path:  dimensions->>'region' (PostgreSQL)
       JSON_VALUE(dimensions, '$.region') (SQL Server)
→ New column: region_id (TEXT / NVARCHAR(50))

Impact:
• 8,500 queries/day affected
• Estimated speedup: 12.5x
• Current p95: 1,250ms → Projected: 100ms
• Storage cost: +15 MB

Reason: Frequently filtered with high selectivity (8%)

Access patterns:
- Filter (WHERE):     6,500 queries/day
- Sort (ORDER BY):    1,200 queries/day
- Aggregate (GROUP BY): 800 queries/day
```

---

### How Speedup is Calculated

#### Filter Speedup (PostgreSQL JSONB)

```
JSONB Filter Cost:
- Full table scan: 1,000,000 rows
- JSONB parse per row: 0.05ms
- Total: 1,000,000 × 0.05ms = 50,000ms

Direct Column Cost (with index):
- B-tree index lookup: log₂(1,000,000) = ~20 comparisons
- Index lookup: 20 × 0.001ms = 0.02ms
- Scan matched rows (8% selectivity): 80,000 × 0.0001ms = 8ms
- Total: 8.02ms

Speedup: 50,000ms ÷ 8.02ms ≈ 6,234x (capped at 100x in practice)
```

#### Filter Speedup (SQL Server JSON)

```
JSON Filter Cost:
- Full table scan: 1,000,000 rows
- JSON parse per row: 0.1ms (text parsing slower than JSONB)
- Total: 1,000,000 × 0.1ms = 100,000ms

Direct Column Cost (with nonclustered index):
- Index seek: log₂(1,000,000) × 0.001ms = 0.02ms
- RID lookups: 80,000 × 0.0002ms = 16ms
- Total: 16.02ms

Speedup: 100,000ms ÷ 16.02ms ≈ 6,242x (capped at 100x)
```

**Note**: Actual speedup varies by:
- Table size (larger = higher speedup)
- Selectivity (lower = higher speedup)
- Hardware (SSD vs HDD)
- Database version and configuration

---

### Storage Cost Calculation

```
New Column Storage:
- Column size: 4 bytes (INTEGER) or ~20 bytes (TEXT average)
- Rows: 1,000,000
- Total: 1,000,000 × 20 bytes = 20 MB

Index Storage (B-tree):
- Index overhead: ~2.5x column size
- Total: 20 MB × 2.5 = 50 MB

Total Storage: 20 MB + 50 MB = 70 MB
```

**Is it worth it?**

```
Cost: 70 MB storage (~$0.01/month in cloud)
Benefit: 8,500 queries/day × 1,150ms saved = 9,775 seconds/day saved
```

**Answer: YES** - Trivial storage cost for massive performance gain.

---

### When to Apply

✅ **Apply When**:
- High frequency (> 1000 queries/day)
- Used in filters with high selectivity
- Used in `ORDER BY` or `GROUP BY`
- Storage cost is acceptable (< 1 GB)

⚠️ **Consider Carefully When**:
- Low selectivity filters (> 50% of rows match)
- Rarely accessed path (< 100 queries/day)
- Very large storage cost (> 10 GB)

❌ **Don't Apply When**:
- Path only used in `SELECT` (projection)
- Path values change very frequently (high write cost)
- Path is deeply nested and complex

---

### Risks and Trade-offs

#### Risk 1: Write Performance

**Impact**: INSERT/UPDATE operations need to update additional column + index.

**Mitigation**:
- Typically negligible (< 5% write overhead)
- For write-heavy tables, test in staging first

#### Risk 2: Data Inconsistency

**Problem**: JSON and denormalized column could get out of sync.

**Solution**: Use database triggers or computed columns (SQL Server):

**PostgreSQL Trigger**:
```sql
CREATE OR REPLACE FUNCTION sync_region_id()
RETURNS TRIGGER AS $$
BEGIN
    NEW.region_id := NEW.dimensions->>'region';
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_sync_region_id
BEFORE INSERT OR UPDATE ON tf_sales
FOR EACH ROW EXECUTE FUNCTION sync_region_id();
```

**SQL Server Computed Column** (automatic):
```sql
ALTER TABLE tf_sales
ADD region_id AS JSON_VALUE(dimensions, '$.region') PERSISTED;
```

#### Risk 3: Schema Evolution

**Problem**: Adding columns requires schema migration.

**Mitigation**:
- Use FraiseQL's schema compilation to manage changes
- Apply migrations during low-traffic windows

---

## Suggestion Type 2: Add Index

### What It Means

Create an index on an existing column that's frequently filtered or sorted.

**Before**:
```sql
CREATE TABLE users (
    id BIGINT PRIMARY KEY,
    name TEXT,
    created_at TIMESTAMP
);

-- Query (slow - sequential scan):
SELECT * FROM users
WHERE created_at > '2026-01-01'
ORDER BY created_at DESC;
```

**After (PostgreSQL)**:
```sql
CREATE INDEX idx_users_created_at ON users (created_at);

-- Query (fast - index scan):
SELECT * FROM users
WHERE created_at > '2026-01-01'
ORDER BY created_at DESC;
```

**After (SQL Server)**:
```sql
CREATE NONCLUSTERED INDEX idx_users_created_at
ON users (created_at DESC);

-- Query (fast - index seek):
SELECT * FROM users
WHERE created_at > '2026-01-01'
ORDER BY created_at DESC;
```

---

### Why Suggested

Index suggestions occur when:

1. **No index exists** on frequently filtered column
2. **Column used in ORDER BY** without index (expensive sort)
3. **Foreign key** without index (slow joins)
4. **High query frequency** (> 1000 queries/day)

### Example Suggestion

```
Add Index

Table: users
Column: created_at
Index Type: B-tree (PostgreSQL) / Nonclustered (SQL Server)

Impact:
• 3,200 queries/day affected
• Estimated speedup: 8x
• Current p95: 850ms → Projected: 106ms
• Storage cost: +5 MB

Reason: Sorted in 90% of queries, no index exists

Query patterns:
- ORDER BY created_at DESC: 2,880 queries/day
- WHERE created_at > '...': 320 queries/day
```

---

### When to Apply

✅ **Apply When**:
- Column frequently filtered or sorted
- Table has > 10,000 rows
- Read-heavy workload (reads >> writes)

⚠️ **Consider Carefully When**:
- Write-heavy table (index slows inserts)
- Low selectivity (column has few distinct values)
- Table is small (< 1,000 rows - index overhead not worth it)

❌ **Don't Apply When**:
- Column is already indexed
- Very high write rate (> 10,000 inserts/sec)
- Index would be larger than the table

---

## Suggestion Type 3: Drop Unused Index

### What It Means

Remove an index that's never or rarely used.

**Before**:
```sql
CREATE TABLE products (
    id BIGINT PRIMARY KEY,
    name TEXT,
    legacy_sku TEXT
);

CREATE INDEX idx_products_legacy_sku ON products (legacy_sku);
-- ↑ Never used (0 scans in 30 days)
```

**After**:
```sql
-- Drop unused index
DROP INDEX idx_products_legacy_sku;

-- Benefits:
-- - Faster writes (no index maintenance)
-- - Reduced storage (reclaim disk space)
```

---

### Why Suggested

Unused index suggestions occur when:

1. **Zero scans** in analysis window (e.g., 30 days)
2. **Very low usage** (< 10 scans in 30 days)
3. **High write cost** (index is large, table has high insert rate)

### Example Suggestion

```
Drop Unused Index

Table: products
Index: idx_products_legacy_sku
Columns: [legacy_sku]

Impact:
• 0 queries/day using this index
• Write speedup: 2% (faster inserts)
• Storage reclaimed: +12 MB

Reason: Index created 2 years ago, never used

Statistics:
- Total scans: 0
- Last used: Never
- Index size: 12 MB
- Table inserts/day: 15,000
```

---

### When to Apply

✅ **Apply When**:
- Zero usage in 30+ days
- Index is large (> 100 MB)
- High insert rate (write optimization)

⚠️ **Consider Carefully When**:
- Index used occasionally (< 10x/month)
- Part of a deployment rollback plan
- Used by ad-hoc analytics queries

❌ **Don't Apply When**:
- Index supports unique constraint
- Required by foreign key
- Used for rare but critical queries

---

## Suggestion Type 4: Materialized View (Future)

### What It Means

Pre-compute expensive aggregate queries and store results.

**Before**:
```sql
-- Expensive aggregate (runs every time):
SELECT
    region_id,
    DATE_TRUNC('month', created_at) AS month,
    SUM(revenue) AS total_revenue,
    COUNT(*) AS order_count
FROM orders
GROUP BY region_id, month;
```

**After (PostgreSQL)**:
```sql
CREATE MATERIALIZED VIEW mv_monthly_revenue AS
SELECT
    region_id,
    DATE_TRUNC('month', created_at) AS month,
    SUM(revenue) AS total_revenue,
    COUNT(*) AS order_count
FROM orders
GROUP BY region_id, month;

CREATE INDEX idx_mv_monthly_revenue_region_month
ON mv_monthly_revenue (region_id, month);

-- Refresh strategy (choose one):
-- 1. On-demand: REFRESH MATERIALIZED VIEW mv_monthly_revenue;
-- 2. Periodic: Cron job every hour
-- 3. Incremental: Trigger on base table updates
```

---

### Why Suggested

Materialized view suggestions occur when:

1. **Expensive aggregation** (> 500ms execution time)
2. **High frequency** (> 100 queries/day)
3. **Stable data** (results don't change frequently)
4. **Tolerable staleness** (5-60 minute lag acceptable)

### Example Suggestion

```
Materialize View

Query: salesByRegion
Base Tables: orders, order_items

Impact:
• 450 queries/day affected
• Estimated speedup: 25x
• Current p95: 2,500ms → Projected: 100ms
• Storage cost: +200 MB

Reason: Expensive aggregate with stable results

Aggregation:
- GROUP BY: region_id, month
- Aggregates: SUM(revenue), COUNT(*), AVG(order_value)
- Data staleness acceptable: 1 hour

Refresh Strategy:
- Recommended: Periodic (every 1 hour)
- Alternatives: On-demand, Incremental
```

---

## Understanding Metrics in Suggestions

### Current p95 vs Projected p95

**p95** = 95th percentile latency (95% of queries are faster than this).

**Example**:
```
Current p95: 1,250ms
- 95% of queries complete in ≤ 1,250ms
- 5% of queries are slower (1,250ms+)

Projected p95: 100ms (after optimization)
- 95% of queries will complete in ≤ 100ms
- 12.5x improvement
```

**Why p95 instead of average?**

Average can be misleading:
```
Query times: [50ms, 55ms, 60ms, 45ms, 5000ms]
- Average: 1,042ms (skewed by outlier)
- p95: 5,000ms (shows worst-case user experience)
```

---

### Selectivity Percentage

**Selectivity** = (Rows Matched) ÷ (Total Rows)

**Example**:
```
Table: users (100,000 rows)
Query: WHERE region = 'US'
Matches: 15,000 rows
Selectivity: 15,000 ÷ 100,000 = 0.15 (15%)
```

**Why it matters**:

| Selectivity | Index Benefit | Reason |
|-------------|---------------|--------|
| 1-10% (high) | Excellent | Index narrows search dramatically |
| 10-30% (medium) | Good | Index still helps significantly |
| 30-50% (low) | Marginal | Index helps slightly |
| 50%+ (very low) | None | Sequential scan faster (most rows match) |

---

### Queries Per Day

**Calculation**: Total executions in analysis window ÷ window days.

**Example**:
```
Analysis window: 7 days
Total executions: 59,500
Queries per day: 59,500 ÷ 7 = 8,500
```

**Why it matters**:

Higher frequency = higher impact:
```
Optimization 1: 10,000 queries/day × 5x speedup = 50,000 impact
Optimization 2: 100 queries/day × 50x speedup = 5,000 impact
→ Prioritize Optimization 1 (10x higher impact)
```

---

## Suggestion Confidence Levels

Each suggestion includes a **confidence score**:

| Confidence | Meaning | Typical Scenario |
|------------|---------|------------------|
| **High (90%+)** | Very likely to succeed | Direct column with index, high frequency |
| **Medium (70-90%)** | Likely to succeed | Moderate frequency, good selectivity |
| **Low (50-70%)** | May succeed | Low frequency, estimated selectivity |

**Example**:
```
Suggestion: Denormalize dimensions->>'category'

Confidence: Medium (75%)

Factors:
✅ High frequency (5,000 queries/day)
✅ Used in filters
⚠️  Selectivity estimated (no direct measurement)
⚠️  Moderate speedup (5x vs 10x+)
```

---

## Comparing Suggestions

### Scenario: Choose 1 of 3 optimizations

```
Suggestion A: Denormalize tf_sales.region_id
- 8,500 queries/day × 12.5x speedup = 106,250 impact
- Storage: 15 MB
- Risk: Low

Suggestion B: Add index on users.created_at
- 3,200 queries/day × 8x speedup = 25,600 impact
- Storage: 5 MB
- Risk: Low

Suggestion C: Materialize view mv_monthly_revenue
- 450 queries/day × 25x speedup = 11,250 impact
- Storage: 200 MB
- Risk: Medium (requires refresh strategy)
```

**Recommendation**: Apply in order A → B → C (by impact score).

---

## Decision Framework

### Step 1: Check Prerequisites

- [ ] Suggestion has high/critical priority
- [ ] Storage cost is acceptable
- [ ] Write performance impact is acceptable
- [ ] Application schema can be updated

### Step 2: Estimate Real-World Impact

- [ ] Calculate time saved per day
- [ ] Estimate user experience improvement
- [ ] Consider business value

### Step 3: Assess Risks

- [ ] Review write performance impact
- [ ] Consider data consistency risks
- [ ] Check for deployment complexity

### Step 4: Test in Staging

- [ ] Apply migration to staging
- [ ] Run benchmark queries
- [ ] Measure actual speedup
- [ ] Monitor for 24-48 hours

### Step 5: Apply to Production

- [ ] Schedule maintenance window (if needed)
- [ ] Apply migration
- [ ] Monitor query performance
- [ ] Verify expected speedup

---

## Common Questions

### Q: Why is estimated speedup sometimes capped at 100x?

**A**: Theoretical speedup can be 1000x+ for very large tables, but real-world factors (disk I/O, caching, parallelism) limit gains. We cap at 100x to avoid unrealistic expectations.

---

### Q: Should I apply all suggestions?

**A**: No. Focus on:
- High/critical priority suggestions
- Low-risk changes first (add index before denormalization)
- Test incrementally

---

### Q: What if actual speedup doesn't match estimate?

**A**: Estimates are based on theoretical models. Actual speedup depends on:
- Hardware (SSD vs HDD)
- Database configuration
- Cache hit rates
- Concurrent query load

Always test in staging first.

---

### Q: Can I customize the cost model?

**A**: Yes, via configuration:

```toml
[observability.cost_model]
jsonb_parse_cost_ms = 0.05  # PostgreSQL JSONB parsing
json_parse_cost_ms = 0.1    # SQL Server text JSON
index_lookup_cost_ms = 0.001
row_scan_cost_ms = 0.0001
```

---

## Next Steps

- **[Migration Workflow](migration-workflow.md)** - How to apply suggestions safely
- **[Analysis Guide](analysis-guide.md)** - Generate suggestions
- **[Examples](examples/basic-denormalization.md)** - Real-world case studies

---

*Last updated: 2026-01-12*
