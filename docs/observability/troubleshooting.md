<!-- Skip to main content -->
---

title: Troubleshooting Guide
description: This guide covers common issues with FraiseQL's observability system and their solutions, organized by:
keywords: []
tags: ["documentation", "reference"]
---

# Troubleshooting Guide

## Overview

This guide covers common issues with FraiseQL's observability system and their solutions, organized by:

- Metrics collection problems
- Analysis issues
- Migration errors
- Performance problems

---

## Metrics Collection Issues

### Issue 1: No Metrics Being Collected

**Symptoms**:

- Metrics tables are empty
- `FraiseQL-cli analyze` returns "No data found"

**Diagnosis**:

```bash
<!-- Code example in BASH -->
# Check if metrics tables exist
psql $METRICS_DATABASE_URL -c "
  SELECT table_name
  FROM information_schema.tables
  WHERE table_schema = 'fraiseql_metrics'
"

# Check row count
psql $METRICS_DATABASE_URL -c "
  SELECT COUNT(*) FROM fraiseql_metrics.query_executions
"
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Observability Not Enabled

```bash
<!-- Code example in BASH -->
# Check environment variable
echo $FRAISEQL_OBSERVABILITY_ENABLED
# Expected: true
# Actual: (empty or false)

# Solution: Enable observability
export FRAISEQL_OBSERVABILITY_ENABLED=true

# Or in FraiseQL.toml:
[observability]
enabled = true
```text
<!-- Code example in TEXT -->

#### Cause 2: Database Connection Failed

```bash
<!-- Code example in BASH -->
# Test connection
psql $FRAISEQL_METRICS_DATABASE_URL -c "SELECT 1"
# Error: connection refused

# Solution: Check connection string
export FRAISEQL_METRICS_DATABASE_URL=postgres://user:pass@correct-host:5432/db
```text
<!-- Code example in TEXT -->

#### Cause 3: Sample Rate Too Low

```bash
<!-- Code example in BASH -->
# Check sample rate
echo $FRAISEQL_OBSERVABILITY_SAMPLE_RATE
# Output: 0.001 (0.1% - very low!)

# Solution: Increase sample rate
export FRAISEQL_OBSERVABILITY_SAMPLE_RATE=0.1  # 10%
```text
<!-- Code example in TEXT -->

#### Cause 4: Schema Not Created

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Create metrics schema
CREATE SCHEMA IF NOT EXISTS fraiseql_metrics;

CREATE TABLE fraiseql_metrics.query_executions (
    id BIGSERIAL PRIMARY KEY,
    query_name TEXT NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    sql_generation_time_ms FLOAT NOT NULL,
    db_round_trip_time_ms FLOAT NOT NULL,
    projection_time_ms FLOAT NOT NULL,
    rows_returned INTEGER NOT NULL,
    cache_hit BOOLEAN NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_query_executions_name_time
    ON fraiseql_metrics.query_executions (query_name, executed_at DESC);

-- (repeat for other tables - see configuration.md)
```text
<!-- Code example in TEXT -->

---

### Issue 2: Metrics Collection Lag

**Symptoms**:

- Latest metrics are 5+ minutes old
- Buffer not flushing

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- Check last metric timestamp
SELECT MAX(executed_at) FROM fraiseql_metrics.query_executions;
-- Expected: < 1 minute ago
-- Actual: 10 minutes ago
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Database Write Timeout

```bash
<!-- Code example in BASH -->
# Check application logs
grep "metrics write timeout" app.log
# Output: ERROR: metrics write timed out after 30s

# Solution: Increase timeout
export FRAISEQL_METRICS_DB_TIMEOUT_SECS=60
```text
<!-- Code example in TEXT -->

#### Cause 2: Database Connection Pool Exhausted

```bash
<!-- Code example in BASH -->
# Check pool size
echo $FRAISEQL_METRICS_DB_POOL_SIZE
# Output: 5 (too small for high traffic)

# Solution: Increase pool size
export FRAISEQL_METRICS_DB_POOL_SIZE=20
```text
<!-- Code example in TEXT -->

#### Cause 3: Flush Interval Too Long

```bash
<!-- Code example in BASH -->
# Check flush interval
echo $FRAISEQL_METRICS_FLUSH_INTERVAL_SECS
# Output: 300 (5 minutes)

# Solution: Flush more frequently
export FRAISEQL_METRICS_FLUSH_INTERVAL_SECS=60
```text
<!-- Code example in TEXT -->

---

### Issue 3: High Memory Usage

**Symptoms**:

- Application memory grows over time
- OOM (Out of Memory) errors

**Diagnosis**:

```bash
<!-- Code example in BASH -->
# Check application memory
docker stats FraiseQL-api
# MEM USAGE: 4.2 GB / 4 GB (near limit!)
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Metrics Buffer Too Large

```bash
<!-- Code example in BASH -->
# Check buffer size
echo $FRAISEQL_METRICS_BUFFER_SIZE
# Output: 10000 (very large!)

# Solution: Reduce buffer size
export FRAISEQL_METRICS_BUFFER_SIZE=100
```text
<!-- Code example in TEXT -->

#### Cause 2: Not Flushing to Database

Check if metrics are being written:

```sql
<!-- Code example in SQL -->
-- Check write rate
SELECT
    DATE_TRUNC('minute', executed_at) AS minute,
    COUNT(*) AS metrics_written
FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '10 minutes'
GROUP BY minute
ORDER BY minute DESC;

-- If all rows have same minute → metrics being batched but not flushed
```text
<!-- Code example in TEXT -->

**Solution**: Force flush or restart application.

---

## Analysis Issues

### Issue 4: No Suggestions Generated

**Symptoms**:

- `FraiseQL-cli analyze` returns 0 suggestions
- "No optimization opportunities found"

**Diagnosis**:

```bash
<!-- Code example in BASH -->
# Check metrics data exists
psql $METRICS_DATABASE_URL -c "
  SELECT COUNT(*) FROM fraiseql_metrics.query_executions
  WHERE executed_at > NOW() - INTERVAL '7 days'
"
# Output: 0 (no data!)
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Insufficient Data Collection Time

```bash
<!-- Code example in BASH -->
# Check oldest metric
psql $METRICS_DATABASE_URL -c "
  SELECT MIN(executed_at) FROM fraiseql_metrics.query_executions
"
# Output: 2026-01-12 10:00:00 (only 2 hours ago)

# Solution: Wait for 24-48 hours of data collection
```text
<!-- Code example in TEXT -->

#### Cause 2: Thresholds Too High

```bash
<!-- Code example in BASH -->
# Try lowering thresholds
FraiseQL-cli analyze \
  --database postgres://... \
  --min-frequency 10 \      # Default: 1000
  --min-speedup 2.0 \       # Default: 5.0
  --format text
```text
<!-- Code example in TEXT -->

#### Cause 3: No JSON Usage Detected

```sql
<!-- Code example in SQL -->
-- Check if any JSON paths were tracked
SELECT COUNT(*) FROM fraiseql_metrics.jsonb_accesses;
-- Output: 0 (no JSON usage)
```text
<!-- Code example in TEXT -->

**Explanation**: Observability focuses on JSON/JSONB optimization. If your schema doesn't use JSON columns, suggestions will be limited to index recommendations.

#### Cause 4: All Paths Already Optimized

```sql
<!-- Code example in SQL -->
-- Check if suggested columns already exist
SELECT column_name
FROM information_schema.columns
WHERE table_name = 'tf_sales'
AND column_name LIKE '%_id';

-- Output: region_id, category_id (already denormalized!)
```text
<!-- Code example in TEXT -->

**Solution**: This is good! Re-run analysis after schema changes or new traffic patterns emerge.

---

### Issue 5: Analysis Takes Too Long

**Symptoms**:

- `FraiseQL-cli analyze` runs for > 5 minutes
- High CPU usage during analysis

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- Check metrics table size
SELECT
    pg_size_pretty(pg_total_relation_size('fraiseql_metrics.query_executions'))
    AS size;
-- Output: 45 GB (very large!)
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Missing Indexes on Metrics Tables

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Add missing indexes
CREATE INDEX IF NOT EXISTS idx_query_executions_query_name
    ON fraiseql_metrics.query_executions (query_name);

CREATE INDEX IF NOT EXISTS idx_query_executions_executed_at
    ON fraiseql_metrics.query_executions (executed_at DESC);

CREATE INDEX IF NOT EXISTS idx_jsonb_accesses_table_path
    ON fraiseql_metrics.jsonb_accesses (table_name, jsonb_column, path);

ANALYZE fraiseql_metrics.query_executions;
ANALYZE fraiseql_metrics.jsonb_accesses;
```text
<!-- Code example in TEXT -->

```sql
<!-- Code example in SQL -->
-- SQL Server: Add missing indexes
CREATE NONCLUSTERED INDEX idx_query_executions_query_name
    ON fraiseql_metrics.query_executions (query_name);

CREATE NONCLUSTERED INDEX idx_query_executions_executed_at
    ON fraiseql_metrics.query_executions (executed_at DESC);

CREATE NONCLUSTERED INDEX idx_json_accesses_table_path
    ON fraiseql_metrics.json_accesses (table_name, json_column, path);

UPDATE STATISTICS fraiseql_metrics.query_executions WITH FULLSCAN;
UPDATE STATISTICS fraiseql_metrics.json_accesses WITH FULLSCAN;
```text
<!-- Code example in TEXT -->

#### Cause 2: Analyzing Too Much Data

```bash
<!-- Code example in BASH -->
# Use shorter time window
FraiseQL-cli analyze \
  --database postgres://... \
  --window 1d  # Instead of 30d
```text
<!-- Code example in TEXT -->

#### Cause 3: Large Aggregations

```sql
<!-- Code example in SQL -->
-- Check query execution plan
EXPLAIN ANALYZE
SELECT
    query_name,
    COUNT(*) as count,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY execution_time_ms) as p95
FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '7 days'
GROUP BY query_name;

-- Look for "Seq Scan" (bad) instead of "Index Scan" (good)
```text
<!-- Code example in TEXT -->

**Solution**: Add indexes (see Cause 1).

---

### Issue 6: Unrealistic Speedup Estimates

**Symptoms**:

- "Estimated speedup: 1000x" (seems too high)
- Actual improvement doesn't match estimate

**Explanation**:

Speedup estimates are **theoretical** based on:

- Table size (larger = higher speedup)
- Selectivity (lower = higher speedup)
- Database-specific cost models

**Reality Check**:

| Optimization | Typical Speedup | Possible Range |
|--------------|-----------------|----------------|
| Filter on indexed column | 5-20x | 2-50x |
| Sort on indexed column | 10-50x | 5-100x |
| Aggregate on indexed column | 5-15x | 3-30x |

**What to Do**:

1. **Test in staging first** - Get real measurements
2. **Use relative priority** - A 100x estimate is still better than a 10x estimate
3. **Focus on impact score** - Frequency × Speedup matters more than speedup alone

---

## Migration Issues

### Issue 7: Migration Fails with Lock Timeout

**Symptoms**:

```text
<!-- Code example in TEXT -->
ERROR: canceling statement due to lock timeout
CONTEXT: while adding column to table "tf_sales"
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Long-Running Queries

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Find blocking queries
SELECT
    pid,
    now() - query_start AS duration,
    state,
    query
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY duration DESC;
```text
<!-- Code example in TEXT -->

**Solution**: Wait for queries to complete or terminate them:

```sql
<!-- Code example in SQL -->
-- Terminate blocking query
SELECT pg_terminate_backend(12345);  -- Replace with actual PID
```text
<!-- Code example in TEXT -->

#### Cause 2: Not Using CONCURRENTLY

```sql
<!-- Code example in SQL -->
-- ❌ Bad: Blocks writes
CREATE INDEX idx_name ON table (column);

-- ✅ Good: Non-blocking
CREATE INDEX CONCURRENTLY idx_name ON table (column);
```text
<!-- Code example in TEXT -->

---

### Issue 8: Index Creation Fails

**Error**:

```text
<!-- Code example in TEXT -->
ERROR: could not create unique index "idx_name"
DETAIL: Key (region_id)=(NULL) is duplicated.
```text
<!-- Code example in TEXT -->

**Cause**: NULL values or duplicates in column

**Solution**:

```sql
<!-- Code example in SQL -->
-- Check for NULLs
SELECT COUNT(*) FROM tf_sales WHERE region_id IS NULL;
-- Output: 15,000 (NULLs exist!)

-- Option 1: Fill NULLs before creating index
UPDATE tf_sales SET region_id = 'UNKNOWN' WHERE region_id IS NULL;

-- Option 2: Use partial index (exclude NULLs)
CREATE INDEX idx_tf_sales_region
    ON tf_sales (region_id)
    WHERE region_id IS NOT NULL;
```text
<!-- Code example in TEXT -->

---

### Issue 9: Backfill Takes Too Long

**Symptoms**:

- `UPDATE` statement runs for > 30 minutes
- Table locked during backfill

**Solution**: Batch the update

```sql
<!-- Code example in SQL -->
-- ❌ Bad: Single large UPDATE (locks table)
UPDATE tf_sales SET region_id = dimensions->>'region';

-- ✅ Good: Batched UPDATE
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

        -- Sleep 50ms between batches
        PERFORM pg_sleep(0.05);

        RAISE NOTICE 'Updated batch of % rows', rows_updated;
    END LOOP;
END $$;
```text
<!-- Code example in TEXT -->

---

## Performance Issues

### Issue 10: High Database CPU After Migration

**Symptoms**:

- Database CPU usage increases from 30% to 80%
- Queries slower than before migration

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Check query performance
SELECT
    query,
    calls,
    mean_exec_time,
    max_exec_time
FROM pg_stat_statements
WHERE query LIKE '%tf_sales%'
ORDER BY mean_exec_time DESC
LIMIT 10;
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Index Not Being Used

```sql
<!-- Code example in SQL -->
-- Check if index is being used
EXPLAIN ANALYZE
SELECT * FROM tf_sales WHERE region_id = 'US';

-- Look for:

-- ✅ "Index Scan using idx_tf_sales_region" (good)
-- ❌ "Seq Scan on tf_sales" (bad - index not used)
```text
<!-- Code example in TEXT -->

**Solution 1: Update statistics**

```sql
<!-- Code example in SQL -->
-- PostgreSQL
ANALYZE tf_sales;

-- SQL Server
UPDATE STATISTICS tf_sales WITH FULLSCAN;
```text
<!-- Code example in TEXT -->

**Solution 2: Force index usage** (temporary debugging)

```sql
<!-- Code example in SQL -->
-- PostgreSQL
SET enable_seqscan = off;
EXPLAIN ANALYZE SELECT * FROM tf_sales WHERE region_id = 'US';
SET enable_seqscan = on;
```text
<!-- Code example in TEXT -->

#### Cause 2: Wrong Index Type

**Problem**: Created B-tree index on array/JSON column

**Solution**: Use appropriate index type

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Use GIN index for JSONB
CREATE INDEX idx_dimensions_gin ON tf_sales USING GIN (dimensions);

-- PostgreSQL: Use GiST index for full-text search
CREATE INDEX idx_name_gist ON users USING GiST (name gist_trgm_ops);
```text
<!-- Code example in TEXT -->

---

### Issue 11: Increased Write Latency

**Symptoms**:

- INSERT/UPDATE operations slower after adding indexes
- Write throughput decreased

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- Count indexes on table
SELECT COUNT(*)
FROM pg_indexes
WHERE tablename = 'tf_sales';
-- Output: 15 indexes (too many!)
```text
<!-- Code example in TEXT -->

**Explanation**: Every index must be updated on write operations.

**Solution**: Remove unused indexes

```sql
<!-- Code example in SQL -->
-- Find unused indexes
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
AND idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;

-- Drop unused indexes
DROP INDEX IF EXISTS idx_unused;
```text
<!-- Code example in TEXT -->

---

### Issue 12: Replication Lag Increased

**Symptoms** (PostgreSQL streaming replication):

- Replica lag increased from < 1s to 30s+
- Replica queries returning stale data

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- Check replication lag
SELECT
    client_addr,
    state,
    sent_lsn,
    write_lsn,
    replay_lsn,
    EXTRACT(EPOCH FROM (NOW() - replay_timestamp)) AS lag_seconds
FROM pg_stat_replication;
-- Output: lag_seconds = 45 (too high!)
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Large Backfill Operation

**Explanation**: Backfilling 1M rows generates lots of WAL (Write-Ahead Log) data.

**Solution**: Backfill in smaller batches with pauses

```sql
<!-- Code example in SQL -->
DO $$
DECLARE
    batch_size INT := 1000;  -- Smaller batches
BEGIN
    LOOP
        UPDATE tf_sales
        SET region_id = dimensions->>'region'
        WHERE id IN (
            SELECT id FROM tf_sales
            WHERE region_id IS NULL
            LIMIT batch_size
        );

        EXIT WHEN NOT FOUND;

        -- Longer pause to allow replica to catch up
        PERFORM pg_sleep(0.5);  -- 500ms pause
    END LOOP;
END $$;
```text
<!-- Code example in TEXT -->

#### Cause 2: Index Creation Generating WAL

**Solution**: Create index on replica separately (after primary)

```bash
<!-- Code example in BASH -->
# 1. Create index on primary
psql primary -c "CREATE INDEX CONCURRENTLY idx_name ON table (column)"

# 2. Wait for replication to catch up
# 3. Manually create index on replica (optional optimization)
psql replica -c "CREATE INDEX CONCURRENTLY idx_name ON table (column)"
```text
<!-- Code example in TEXT -->

---

## Connection Issues

### Issue 13: Database Connection Refused

**Symptoms**:

```text
<!-- Code example in TEXT -->
Error: connection refused
```text
<!-- Code example in TEXT -->

**Diagnosis**:

```bash
<!-- Code example in BASH -->
# Test connection
psql $DATABASE_URL -c "SELECT 1"
# Error: could not connect to server
```text
<!-- Code example in TEXT -->

**Common Causes & Solutions**:

#### Cause 1: Wrong Host/Port

```bash
<!-- Code example in BASH -->
# Check connection string
echo $DATABASE_URL
# postgres://user:pass@localhost:5432/db

# Solution: Verify host/port
nslookup db-host.example.com
telnet db-host.example.com 5432
```text
<!-- Code example in TEXT -->

#### Cause 2: Firewall Blocking Connection

```bash
<!-- Code example in BASH -->
# Test connectivity
nc -zv db-host.example.com 5432
# Connection refused (firewall blocking)

# Solution: Open firewall port
# (depends on your infrastructure)
```text
<!-- Code example in TEXT -->

#### Cause 3: Database Not Running

```bash
<!-- Code example in BASH -->
# Check if PostgreSQL is running
sudo systemctl status postgresql
# Status: inactive (dead)

# Solution: Start database
sudo systemctl start postgresql
```text
<!-- Code example in TEXT -->

---

## Data Quality Issues

### Issue 14: Incorrect Selectivity Estimates

**Symptoms**:

- Analysis suggests optimizing low-selectivity filters
- "90% of rows match" but suggested for optimization

**Diagnosis**:

```sql
<!-- Code example in SQL -->
-- Manually check selectivity
SELECT
    COUNT(CASE WHEN dimensions->>'region' = 'US' THEN 1 END)::FLOAT /
    COUNT(*)::FLOAT AS selectivity
FROM tf_sales;
-- Output: 0.92 (92% selectivity - very low!)
```text
<!-- Code example in TEXT -->

**Cause**: Metrics estimated selectivity incorrectly.

**Solution**: Re-analyze with longer time window or more samples

```bash
<!-- Code example in BASH -->
# Increase sample rate temporarily
export FRAISEQL_OBSERVABILITY_SAMPLE_RATE=1.0  # 100% sampling

# Wait 24 hours, then re-analyze
FraiseQL-cli analyze --database postgres://... --window 1d
```text
<!-- Code example in TEXT -->

---

## Getting Help

### Debug Mode

Enable verbose logging:

```bash
<!-- Code example in BASH -->
# Enable debug logging
export RUST_LOG=FraiseQL=debug

# Run analysis with debug output
FraiseQL-cli analyze --database postgres://... 2>&1 | tee debug.log
```text
<!-- Code example in TEXT -->

### Health Check Script

```bash
<!-- Code example in BASH -->
#!/bin/bash
# health-check.sh

echo "=== FraiseQL Observability Health Check ==="

echo -e "\n1. Checking observability configuration..."
if [ "$FRAISEQL_OBSERVABILITY_ENABLED" = "true" ]; then
    echo "✅ Observability enabled"
else
    echo "❌ Observability not enabled"
fi

echo -e "\n2. Checking database connection..."
if psql $FRAISEQL_METRICS_DATABASE_URL -c "SELECT 1" > /dev/null 2>&1; then
    echo "✅ Database connection successful"
else
    echo "❌ Database connection failed"
fi

echo -e "\n3. Checking metrics collection..."
METRICS_COUNT=$(psql $FRAISEQL_METRICS_DATABASE_URL -t -c "
    SELECT COUNT(*) FROM fraiseql_metrics.query_executions
    WHERE executed_at > NOW() - INTERVAL '1 hour'
")
if [ "$METRICS_COUNT" -gt 0 ]; then
    echo "✅ Metrics being collected ($METRICS_COUNT in last hour)"
else
    echo "❌ No metrics collected in last hour"
fi

echo -e "\n4. Checking metrics freshness..."
LATEST=$(psql $FRAISEQL_METRICS_DATABASE_URL -t -c "
    SELECT EXTRACT(EPOCH FROM (NOW() - MAX(executed_at)))
    FROM fraiseql_metrics.query_executions
")
if [ "${LATEST%.*}" -lt 300 ]; then
    echo "✅ Metrics are fresh (${LATEST%.*} seconds old)"
else
    echo "⚠️  Metrics are stale (${LATEST%.*} seconds old)"
fi

echo -e "\n=== Health Check Complete ==="
```text
<!-- Code example in TEXT -->

---

## Support Resources

- **GitHub Issues**: [github.com/FraiseQL/FraiseQL/issues](https://github.com/FraiseQL/FraiseQL/issues)
- **Discord**: [discord.gg/FraiseQL](https://discord.gg/FraiseQL)
- **Documentation**: [docs.FraiseQL.com/observability](https://docs.FraiseQL.com/observability)
- **Email**: <support@FraiseQL.com>

---

## Next Steps

- **[Operations Configuration](../operations/configuration.md)** - Tune observability settings
- **[Analysis Guide](../operations/analysis-guide.md)** - Run analysis effectively
- **[Migration Workflow](./migration-workflow.md)** - Apply changes safely

---

*Last updated: 2026-01-12*
