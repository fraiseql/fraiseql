# Example: Basic JSONB Denormalization

## Overview

This example demonstrates a **complete end-to-end workflow** for denormalizing a frequently-accessed JSONB path to a direct column, resulting in a **12.5x query speedup**.

**Scenario**: E-commerce analytics application with slow region-based filtering

**Duration**: ~2 hours (including 24h monitoring)

**Databases covered**: PostgreSQL and SQL Server

---

## Initial Setup

### Application Architecture

```
┌─────────────────────────────────────────┐
│  Frontend Dashboard                      │
│  - Shows sales by region                │
│  - Refreshes every 30 seconds           │
└─────────────────┬───────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────┐
│  FraiseQL GraphQL API                   │
│  - Handles 1000 qps                     │
│  - Most queries filter by region        │
└─────────────────┬───────────────────────┘
                  │
                  ↓
┌─────────────────────────────────────────┐
│  PostgreSQL Database                     │
│  - tf_sales table (1.5M rows)           │
│  - dimensions JSONB column                    │
└─────────────────────────────────────────┘
```

---

## Step 1: Initial Schema

### Database Schema (PostgreSQL)

```sql
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,
    revenue NUMERIC(12, 2) NOT NULL,
    quantity INTEGER NOT NULL,
    dimensions JSONB NOT NULL,  -- {region: 'US', category: 'Electronics', date: '2026-01-01'}
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tf_sales_recorded_at ON tf_sales (recorded_at);
-- Note: No index on dimensions->>'region' (slow queries)
```

### Database Schema (SQL Server)

```sql
CREATE TABLE tf_sales (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    revenue DECIMAL(12, 2) NOT NULL,
    quantity INT NOT NULL,
    dimensions NVARCHAR(MAX) NOT NULL,  -- JSON text: {"region": "US", "category": "Electronics"}
    recorded_at DATETIME2 NOT NULL DEFAULT GETDATE()
);

CREATE NONCLUSTERED INDEX idx_tf_sales_recorded_at
    ON tf_sales (recorded_at);
```

---

### Application Schema (Python)

```python
# schema.py
import fraiseql

@fraiseql.fact_table(
    table_name='tf_sales',
    measures=['revenue', 'quantity'],
    dimension_column='dimensions'  # JSONB column
)
class SalesMetrics:
    id: int
    revenue: float
    quantity: int
    dimensions: dict  # {region: str, category: str, date: str}
    recorded_at: datetime

@fraiseql.aggregate_query(fact_table='tf_sales')
def sales_by_region(region: str = None):
    """Get sales aggregated by region"""
    pass
```

---

### Sample Queries

**GraphQL Query**:

```graphql
query {
  salesByRegion(where: { region: "US" }) {
    totalRevenue
    totalQuantity
    averageOrderValue
  }
}
```

**Generated SQL (PostgreSQL)**:

```sql
SELECT
    SUM(revenue) AS totalRevenue,
    SUM(quantity) AS totalQuantity,
    AVG(revenue) AS averageOrderValue
FROM tf_sales
WHERE dimensions->>'region' = 'US';  -- ⚠️ Slow: JSONB extraction on every row
```

**Generated SQL (SQL Server)**:

```sql
SELECT
    SUM(revenue) AS totalRevenue,
    SUM(quantity) AS totalQuantity,
    AVG(revenue) AS averageOrderValue
FROM tf_sales
WHERE JSON_VALUE(dimensions, '$.region') = 'US';  -- ⚠️ Slow: JSON parsing on every row
```

---

## Step 2: Problem Detection

### Performance Issues

**User Complaints**:

- Dashboard takes 5-10 seconds to load
- Timeout errors during peak traffic
- Poor user experience

**Initial Metrics** (before optimization):

| Metric | Value |
|--------|-------|
| Query frequency | 8,500 queries/day |
| Average latency | 850ms |
| p95 latency | **1,250ms** |
| p99 latency | 2,100ms |
| Affected users | ~500/day |

---

## Step 3: Enable Observability

### Configuration

```bash
# Enable observability
export FRAISEQL_OBSERVABILITY_ENABLED=true
export FRAISEQL_OBSERVABILITY_SAMPLE_RATE=0.1  # 10% sampling
export FRAISEQL_METRICS_DATABASE_URL=postgres://metrics:pass@localhost:5432/metrics
```

**Or in `fraiseql.toml`**:

```toml
[observability]
enabled = true
sample_rate = 0.1

[observability.database]
url = "postgres://metrics:pass@localhost:5432/metrics"
```

### Restart Application

```bash
# Restart to load new config
kubectl rollout restart deployment/fraiseql-api

# Verify observability is active
kubectl logs -f deployment/fraiseql-api | grep observability
# Output: INFO observability enabled, sample_rate=0.1
```

---

## Step 4: Collect Metrics (24-48 Hours)

**Wait Period**: 24-48 hours for statistically significant data

**During this time**:

- Metrics automatically collected in background
- No performance impact (< 5% overhead with 10% sampling)
- No application changes needed

**Monitoring Collection**:

```sql
-- Check metrics are being collected
SELECT COUNT(*) FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '1 hour';
-- Expected: ~850 rows/hour (at 10% sampling, 8,500 queries/day)

-- Check JSON path tracking
SELECT
    table_name,
    jsonb_column,
    path,
    COUNT(*) AS accesses
FROM fraiseql_metrics.jsonb_accesses
WHERE recorded_at > NOW() - INTERVAL '1 day'
GROUP BY table_name, jsonb_column, path
ORDER BY accesses DESC;
-- Expected: tf_sales | dimensions | region | ~850
```

---

## Step 5: Run Analysis

### Analyze Metrics

```bash
fraiseql-cli analyze \
  --database postgres://metrics:pass@localhost:5432/metrics \
  --format text
```

### Analysis Output

```
📊 Observability Analysis Report

Database: PostgreSQL
Window: Last 7 days (2026-01-05 to 2026-01-12)
Analyzed: 59,500 query executions
JSON accesses: 8,500 (dimensions->>'region')

🚀 High-Impact Optimization (1):

  1. Denormalize JSONB → Direct Column

     Table: tf_sales
     Path:  dimensions->>'region'
     → New column: region_id (TEXT)

     Impact:
     • 8,500 queries/day affected
     • Estimated speedup: 12.5x
     • Current p95: 1,250ms → Projected: 100ms
     • Storage cost: +15 MB

     Reason: Frequently filtered with high selectivity (8%)

     Access Patterns:
     - Filter (WHERE):  6,500 queries/day
     - Aggregate (GROUP BY): 2,000 queries/day

     Query Examples:
     - salesByRegion: 8,500 calls/day, avg 850ms
     - regionalTrends: 2,000 calls/day, avg 920ms

---

💡 Next Steps:
   1. Generate SQL: fraiseql-cli analyze --format sql > optimize.sql
   2. Review: less optimize.sql
   3. Test in staging
   4. Apply to production
```

---

## Step 6: Generate Migration SQL

```bash
fraiseql-cli analyze \
  --database postgres://metrics:pass@localhost:5432/metrics \
  --format sql > migrations/denormalize-region-20260112.sql
```

### Generated SQL (PostgreSQL)

```sql
-- ============================================================
-- FraiseQL Observability-Driven Schema Optimization
-- Generated: 2026-01-12 14:30:00 UTC
-- Database: PostgreSQL
-- ============================================================

-- ------------------------------------------------------------
-- Migration: Denormalize dimensions->>'region'
-- Table: tf_sales
-- Impact: 8,500 queries/day, 12.5x speedup
-- Storage: +15 MB
-- ------------------------------------------------------------

-- Step 1: Add new column
ALTER TABLE tf_sales ADD COLUMN region_id TEXT;

-- Step 2: Backfill data from JSONB
-- Batched update to avoid long locks
DO $$
DECLARE
    batch_size INT := 10000;
    rows_updated INT;
BEGIN
    LOOP
        UPDATE tf_sales
        SET region_id = dimensions->>'region'
        WHERE id IN (
            SELECT id FROM tf_sales
            WHERE region_id IS NULL
            ORDER BY id
            LIMIT batch_size
        );

        GET DIAGNOSTICS rows_updated = ROW_COUNT;
        EXIT WHEN rows_updated = 0;

        RAISE NOTICE 'Updated % rows', rows_updated;
        PERFORM pg_sleep(0.05);  -- 50ms pause between batches
    END LOOP;
END $$;

-- Step 3: Create index (CONCURRENTLY to avoid blocking writes)
CREATE INDEX CONCURRENTLY idx_tf_sales_region_id
  ON tf_sales (region_id);

-- Step 4: Analyze for statistics
ANALYZE tf_sales;

-- ------------------------------------------------------------
-- Rollback (if needed):
-- ------------------------------------------------------------
-- DROP INDEX IF EXISTS idx_tf_sales_region_id;
-- ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;
```

### Generated SQL (SQL Server)

```sql
-- ============================================================
-- FraiseQL Observability-Driven Schema Optimization
-- Generated: 2026-01-12 14:30:00 UTC
-- Database: SQL Server
-- ============================================================

-- ------------------------------------------------------------
-- Migration: Denormalize JSON_VALUE(dimensions, '$.region')
-- Table: tf_sales
-- Impact: 8,500 queries/day, 12.5x speedup
-- Storage: +15 MB
-- ------------------------------------------------------------

-- Step 1: Add computed column
ALTER TABLE tf_sales
ADD region_id AS JSON_VALUE(dimensions, '$.region');
GO

-- Step 2: Persist computed column (materializes value)
ALTER TABLE tf_sales
ALTER COLUMN region_id ADD PERSISTED;
GO

-- Step 3: Create nonclustered index (ONLINE to avoid blocking)
CREATE NONCLUSTERED INDEX idx_tf_sales_region_id
  ON tf_sales (region_id)
  WITH (ONLINE = ON, MAXDOP = 4);
GO

-- Step 4: Update statistics
UPDATE STATISTICS tf_sales WITH FULLSCAN;
GO

-- ------------------------------------------------------------
-- Rollback (if needed):
-- ------------------------------------------------------------
-- DROP INDEX IF EXISTS idx_tf_sales_region_id ON tf_sales;
-- GO
-- ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;
-- GO
```

---

## Step 7: Test in Staging

### Apply Migration to Staging

```bash
# Backup staging database
pg_dump staging > backup-staging-$(date +%Y%m%d).dump

# Apply migration
psql staging < migrations/denormalize-region-20260112.sql

# Expected output:
# ALTER TABLE
# NOTICE: Updated 10000 rows
# NOTICE: Updated 10000 rows
# ...
# NOTICE: Updated 5000 rows
# CREATE INDEX
# ANALYZE
```

### Verify Schema Changes

```bash
# Check new column exists
psql staging -c "\d tf_sales"

# Output:
#  Column     |           Type           | Nullable | Default
# ------------+--------------------------+----------+---------
#  id         | bigint                   | not null | ...
#  revenue    | numeric(12,2)            | not null |
#  quantity   | integer                  | not null |
#  dimensions | jsonb                    | not null |
#  region_id  | text                     |          |  ← NEW
#  recorded_at| timestamp with time zone | not null | now()
```

### Run Benchmark Queries

**Before Optimization**:

```bash
psql staging -c "
EXPLAIN ANALYZE
SELECT SUM(revenue) FROM tf_sales
WHERE dimensions->>'region' = 'US'
" > benchmark-before.txt

cat benchmark-before.txt
# Seq Scan on tf_sales (cost=0.00..45678.90 rows=120000 width=8)
#   Filter: ((dimensions ->> 'region'::text) = 'US'::text)
# Planning Time: 0.234 ms
# Execution Time: 1,247.823 ms  ← SLOW
```

**After Optimization**:

```bash
psql staging -c "
EXPLAIN ANALYZE
SELECT SUM(revenue) FROM tf_sales
WHERE region_id = 'US'
" > benchmark-after.txt

cat benchmark-after.txt
# Index Scan using idx_tf_sales_region_id (cost=0.42..4523.89 rows=120000 width=8)
#   Index Cond: (region_id = 'US'::text)
# Planning Time: 0.156 ms
# Execution Time: 98.234 ms  ← FAST! (12.7x improvement)
```

**Actual Speedup**: 1,247ms / 98ms = **12.7x** ✅

---

## Step 8: Update Application Schema

### Update Python Schema

```python
# schema.py (UPDATED)
import fraiseql

@fraiseql.fact_table(
    table_name='tf_sales',
    measures=['revenue', 'quantity'],
    dimension_column='dimensions',
    denormalized_filters=['region_id']  # ← NEW: Use direct column for filtering
)
class SalesMetrics:
    id: int
    revenue: float
    quantity: int
    region_id: str  # ← NEW: Direct column
    dimensions: dict
    recorded_at: datetime

@fraiseql.aggregate_query(fact_table='tf_sales')
def sales_by_region(region: str = None):
    """Get sales aggregated by region"""
    pass
```

### Recompile Schema

```bash
fraiseql-cli compile schema.json

# Output:
# ✓ Schema compiled successfully
#   Input:  schema.json
#   Output: schema.compiled.json
#   Types: 5
#   Queries: 12
```

### New Generated SQL

**New PostgreSQL Query** (after schema update):

```sql
SELECT
    SUM(revenue) AS totalRevenue,
    SUM(quantity) AS totalQuantity,
    AVG(revenue) AS averageOrderValue
FROM tf_sales
WHERE region_id = 'US';  -- ✅ Fast: Direct column with index
```

---

## Step 9: Deploy to Staging

```bash
git add schema.json schema.compiled.json
git commit -m "feat: denormalize region_id for 12.5x speedup

Migrated dimensions->>'region' to direct region_id column.

Impact:
- 8,500 queries/day affected
- p95 latency: 1,250ms → 100ms (12.7x improvement)
- Storage cost: +15 MB

Migration: denormalize-region-20260112.sql
"

git push origin staging

# Deploy to staging
kubectl rollout restart deployment/fraiseql-api --namespace=staging
```

### Monitor Staging

```bash
# Monitor query latency
kubectl logs -f deployment/fraiseql-api --namespace=staging | grep salesByRegion

# Output:
# INFO query=salesByRegion duration=102ms  ← Fast!
# INFO query=salesByRegion duration=95ms
# INFO query=salesByRegion duration=98ms
```

---

## Step 10: Apply to Production

### Pre-Production Checklist

- [x] Tested in staging for 24 hours
- [x] Verified 12.7x speedup
- [x] No errors in staging logs
- [x] Schema recompiled
- [x] Team notified
- [x] Rollback plan ready

### Backup Production

```bash
pg_dump production > backup-prod-$(date +%Y%m%d-%H%M%S).dump
```

### Apply Migration

```bash
# Apply migration to production
psql production < migrations/denormalize-region-20260112.sql

# Monitor progress
tail -f /var/log/postgresql/postgresql.log
```

### Deploy Updated Schema

```bash
git checkout main
git merge staging
git push origin main

# Deploy to production
kubectl rollout restart deployment/fraiseql-api --namespace=production
```

---

## Step 11: Post-Migration Validation

### Immediate Validation

```bash
# Check query performance
psql production -c "
SELECT
    query,
    calls,
    mean_exec_time,
    max_exec_time
FROM pg_stat_statements
WHERE query LIKE '%region_id%'
ORDER BY calls DESC
LIMIT 5
"

# Expected:
# query: SELECT ... WHERE region_id = $1
# calls: 8500
# mean_exec_time: 95.2  ← Fast!
# max_exec_time: 185.3
```

### 24-Hour Monitoring

**Metrics Dashboard**:

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Average latency | 850ms | 68ms | **12.5x** ✅ |
| p95 latency | 1,250ms | 100ms | **12.5x** ✅ |
| p99 latency | 2,100ms | 185ms | **11.4x** ✅ |
| Error rate | 0.5% | 0.1% | **5x better** ✅ |
| Queries/day | 8,500 | 8,500 | (same) |
| Storage | 520 MB | 535 MB | +15 MB |

**User Feedback**:

- Dashboard load time: 5-10s → **< 1s** ✅
- No timeout errors ✅
- Positive user feedback ✅

---

## Results Summary

### Performance Improvements

✅ **12.5x query speedup** (1,250ms → 100ms p95)
✅ **95% latency reduction**
✅ **80% reduction in timeout errors**
✅ **Improved user experience**

### Costs

- **Development time**: 2 hours
- **Migration downtime**: 0 seconds (zero-downtime migration)
- **Storage cost**: +15 MB (~$0.01/month)
- **Write overhead**: < 2% (negligible)

### ROI Calculation

```
Time Saved Per Day:
  8,500 queries × 1,150ms saved = 9,775 seconds = 2.7 hours

User Impact:
  500 active users × 17 queries/user × 1.15s saved = 9,775 seconds saved daily
  = Better user experience, reduced churn

Cost:
  Development: 2 hours
  Storage: $0.01/month

ROI: Massive positive impact for minimal cost
```

---

## Key Takeaways

### What Worked Well

1. ✅ **Observability-driven approach** - Data-backed decision making
2. ✅ **Zero-downtime migration** - CONCURRENTLY index creation
3. ✅ **Staging testing** - Caught issues before production
4. ✅ **Gradual rollout** - Staging → Production

### Lessons Learned

1. **Wait for sufficient data** - 24-48 hours minimum for analysis
2. **Test in staging first** - Always verify speedup before production
3. **Monitor after deployment** - Watch for unexpected issues
4. **Keep rollback ready** - Have a plan B

### Next Optimizations

Based on continuing analysis:

```bash
fraiseql-cli analyze --database postgres://... --format text

# Output:
# 🚀 Additional Optimizations (2):
#   2. Denormalize dimensions->>'category' (5,200 queries/day, 8x speedup)
#   3. Add index on recorded_at (3,100 queries/day, 6x speedup)
```

---

## Additional Resources

- **[Observability Architecture](../../operations/observability-architecture.md)** - System design details
- **[Analysis Guide](../../operations/analysis-guide.md)** - Running analysis
- **[Migration Workflow](../migration-workflow.md)** - Safe deployment practices

---

*Last updated: 2026-01-12*
