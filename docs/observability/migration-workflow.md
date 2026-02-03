# Migration Workflow: Applying Optimizations Safely

## Overview

This guide covers the **complete workflow** for safely applying observability-driven schema optimizations to production, including:

- Pre-migration checklist
- Staging environment testing
- Zero-downtime strategies
- Rollback procedures
- Post-migration validation

---

## Migration Safety Principles

### 1. Never Skip Staging

**Always test migrations in staging first** - no exceptions.

### 2. Backup Before Migration

**Always backup before applying migrations** - especially for production.

### 3. Monitor After Changes

**Track query performance for 24-48 hours** after migration.

### 4. Have a Rollback Plan

**Know how to revert** before applying changes.

### 5. Communicate Changes

**Notify team** of maintenance windows and schema changes.

---

## Complete Migration Workflow

### Phase 1: Generate Migration SQL

```bash
# Step 1: Analyze metrics
fraiseql-cli analyze \
  --database postgres://metrics-db:5432/metrics \
  --format sql > migrations/optimize-$(date +%Y%m%d).sql

# Step 2: Review SQL
less migrations/optimize-20260112.sql
```

**What to check**:

- ✅ SQL syntax is correct
- ✅ Table/column names match your schema
- ✅ No destructive operations (DROP TABLE, TRUNCATE)
- ✅ Comments explain each migration
- ✅ Rollback statements included

---

### Phase 2: Pre-Migration Checklist

Before applying to **any environment**:

#### Database Checks

- [ ] **Verify database connection**

  ```bash
  psql $DATABASE_URL -c "SELECT version()"
  ```

- [ ] **Check database size and free space**

  ```bash
  # PostgreSQL
  psql $DATABASE_URL -c "
    SELECT pg_size_pretty(pg_database_size(current_database()))
  "

  # SQL Server
  sqlcmd -S localhost -Q "
    EXEC sp_spaceused
  "
  ```

- [ ] **Check current locks**

  ```bash
  # PostgreSQL
  psql $DATABASE_URL -c "
    SELECT * FROM pg_locks WHERE granted = false
  "
  ```

- [ ] **Estimate migration duration**

  ```bash
  # Test on copy of production data
  time psql staging < migrations/optimize-20260112.sql
  ```

#### Backup Checks

- [ ] **Create full database backup**

  ```bash
  # PostgreSQL
  pg_dump -Fc $DATABASE_URL > backup-$(date +%Y%m%d-%H%M%S).dump

  # SQL Server
  sqlcmd -S localhost -Q "
    BACKUP DATABASE mydb TO DISK = 'C:\Backups\mydb-20260112.bak'
  "
  ```

- [ ] **Verify backup integrity**

  ```bash
  # PostgreSQL: List backup contents
  pg_restore --list backup-20260112-143000.dump | head

  # SQL Server: Verify backup
  sqlcmd -Q "RESTORE VERIFYONLY FROM DISK = 'C:\Backups\mydb-20260112.bak'"
  ```

- [ ] **Test backup restore** (in separate environment)

  ```bash
  # PostgreSQL
  createdb test_restore
  pg_restore -d test_restore backup-20260112-143000.dump

  # SQL Server
  sqlcmd -Q "RESTORE DATABASE test_restore FROM DISK = '...'"
  ```

#### Application Checks

- [ ] **Review application schema references**

  ```bash
  # Check if new columns need schema updates
  grep -r "dimensions->>region" app/
  ```

- [ ] **Prepare schema.json updates**

  ```python
  # Before migration:
  @fraiseql.fact_table(
      table_name='tf_sales',
      dimension_column='dimensions'
  )

  # After migration:
  @fraiseql.fact_table(
      table_name='tf_sales',
      dimension_column='dimensions',
      denormalized_filters=['region_id']  # NEW
  )
  ```

- [ ] **Compile new schema**

  ```bash
  fraiseql-cli compile schema.json --check
  ```

#### Team Communication

- [ ] **Schedule maintenance window** (if downtime needed)
- [ ] **Notify team** via Slack/email
- [ ] **Update status page** (if public-facing)

---

### Phase 3: Test in Staging

#### Apply Migration

```bash
# Apply to staging database
psql $STAGING_DATABASE_URL < migrations/optimize-20260112.sql
```

**Watch for**:

- ✅ All statements execute successfully
- ⚠️ Long-running operations (> 5 minutes)
- ❌ Errors or failures

#### Verify Schema Changes

```bash
# PostgreSQL: Check new columns
psql $STAGING_DATABASE_URL -c "
  SELECT column_name, data_type
  FROM information_schema.columns
  WHERE table_name = 'tf_sales'
  AND column_name = 'region_id'
"

# SQL Server: Check new columns
sqlcmd -S staging -Q "
  SELECT COLUMN_NAME, DATA_TYPE
  FROM INFORMATION_SCHEMA.COLUMNS
  WHERE TABLE_NAME = 'tf_sales'
  AND COLUMN_NAME = 'region_id'
"
```

#### Verify Indexes

```bash
# PostgreSQL
psql $STAGING_DATABASE_URL -c "
  SELECT indexname, indexdef
  FROM pg_indexes
  WHERE tablename = 'tf_sales'
  AND indexname = 'idx_tf_sales_region_id'
"

# SQL Server
sqlcmd -Q "
  SELECT name, type_desc
  FROM sys.indexes
  WHERE object_id = OBJECT_ID('tf_sales')
  AND name = 'idx_tf_sales_region_id'
"
```

#### Run Benchmark Queries

**Before Migration** (baseline):

```bash
# Capture baseline performance
psql $STAGING_DATABASE_URL -c "
  EXPLAIN ANALYZE
  SELECT * FROM tf_sales
  WHERE dimensions->>'region' = 'US'
" > benchmark-before.txt
```

**After Migration**:

```bash
# Measure new performance
psql $STAGING_DATABASE_URL -c "
  EXPLAIN ANALYZE
  SELECT * FROM tf_sales
  WHERE region_id = 'US'
" > benchmark-after.txt
```

**Compare Results**:

```bash
# Extract execution times
grep "Execution Time" benchmark-before.txt
# Execution Time: 1,250.456 ms

grep "Execution Time" benchmark-after.txt
# Execution Time: 98.123 ms

# Actual speedup: 1,250 / 98 = 12.76x ✅
```

#### Update Application Schema

```bash
# 1. Update schema.json
vim schema.json

# 2. Recompile
fraiseql-cli compile schema.json

# 3. Deploy to staging
git add schema.compiled.json
git commit -m "chore: update schema with denormalized region_id"
git push origin staging
```

#### Run Application Tests

```bash
# Run full test suite
npm test

# Run integration tests
npm run test:integration

# Manual smoke tests
curl http://staging-api.example.com/graphql \
  -d '{"query": "{ sales(where: {region: \"US\"}) { revenue } }"}'
```

#### Monitor Staging for 24-48 Hours

**Metrics to track**:

- Query latency (p50, p95, p99)
- Error rates
- Database CPU/memory usage
- Disk I/O

**Tools**:

```bash
# Query performance
psql $STAGING_DATABASE_URL -c "
  SELECT
    query,
    calls,
    mean_exec_time,
    max_exec_time
  FROM pg_stat_statements
  WHERE query LIKE '%tf_sales%'
  ORDER BY mean_exec_time DESC
  LIMIT 10
"
```

---

### Phase 4: Apply to Production

#### Choose Deployment Strategy

##### Strategy A: Maintenance Window (Safe, Simple)

**Best for**: Migrations requiring table locks (< 1 minute downtime)

```bash
# 1. Enable maintenance mode
curl -X POST https://api.example.com/admin/maintenance

# 2. Wait for active queries to complete (30 seconds)
sleep 30

# 3. Apply migration
psql $PRODUCTION_DATABASE_URL < migrations/optimize-20260112.sql

# 4. Verify changes
psql $PRODUCTION_DATABASE_URL -c "\d tf_sales"

# 5. Disable maintenance mode
curl -X POST https://api.example.com/admin/maintenance/off
```

**Duration**: 1-5 minutes
**Risk**: Low (predictable)
**User Impact**: Brief downtime

---

##### Strategy B: Zero-Downtime (PostgreSQL)

**Best for**: Large tables where maintenance window isn't acceptable

**PostgreSQL: Use CONCURRENTLY**

```sql
-- Step 1: Add column (non-blocking)
ALTER TABLE tf_sales ADD COLUMN region_id TEXT;

-- Step 2: Backfill data in batches (avoids long locks)
DO $$
DECLARE
    batch_size INT := 10000;
    offset_val INT := 0;
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

        offset_val := offset_val + batch_size;
        RAISE NOTICE 'Updated % rows', offset_val;

        -- Sleep 100ms between batches to avoid overwhelming DB
        PERFORM pg_sleep(0.1);
    END LOOP;
END $$;

-- Step 3: Create index concurrently (non-blocking)
CREATE INDEX CONCURRENTLY idx_tf_sales_region_id
  ON tf_sales (region_id);

-- Step 4: Analyze table
ANALYZE tf_sales;
```

**Duration**: 10-60 minutes (depending on table size)
**Risk**: Low
**User Impact**: None (zero downtime)

---

##### Strategy C: Zero-Downtime (SQL Server)

**SQL Server: Use Online Index Rebuild**

```sql
-- Step 1: Add computed column (instant)
ALTER TABLE tf_sales
ADD region_id AS JSON_VALUE(dimensions, '$.region');
GO

-- Step 2: Persist computed column (requires table scan)
-- Use online operation to avoid blocking
ALTER TABLE tf_sales
ALTER COLUMN region_id ADD PERSISTED;
GO

-- Step 3: Create index with ONLINE option
CREATE NONCLUSTERED INDEX idx_tf_sales_region_id
  ON tf_sales (region_id)
  WITH (ONLINE = ON, MAXDOP = 4);
GO

-- Step 4: Update statistics
UPDATE STATISTICS tf_sales WITH FULLSCAN;
GO
```

**Duration**: 5-30 minutes
**Risk**: Low
**User Impact**: None (online operations)

---

#### Execute Production Migration

```bash
# 1. Final backup (immediately before migration)
pg_dump -Fc $PRODUCTION_DATABASE_URL > backup-pre-migration.dump

# 2. Apply migration
psql $PRODUCTION_DATABASE_URL < migrations/optimize-20260112.sql

# 3. Verify immediately
psql $PRODUCTION_DATABASE_URL -c "
  SELECT column_name FROM information_schema.columns
  WHERE table_name = 'tf_sales' AND column_name = 'region_id'
"
# Expected: region_id | TEXT

# 4. Run test query
psql $PRODUCTION_DATABASE_URL -c "
  SELECT COUNT(*) FROM tf_sales WHERE region_id = 'US'
"
# Expected: Non-zero count
```

---

### Phase 5: Update Application

```bash
# 1. Update schema.json with denormalized columns
vim schema.json

# 2. Recompile schema
fraiseql-cli compile schema.json

# 3. Create deployment commit
git add schema.json schema.compiled.json
git commit -m "feat: use denormalized region_id for better performance

Migrated dimensions->>'region' to direct region_id column.

Expected impact:

- 8,500 queries/day affected
- 12.5x speedup
- p95 latency: 1,250ms → 100ms

Migration applied: 2026-01-12 14:30 UTC
"

# 4. Deploy to production
git push origin main

# 5. Restart application servers (if needed)
kubectl rollout restart deployment/fraiseql-api
```

---

### Phase 6: Post-Migration Validation

#### Immediate Validation (First 5 Minutes)

```bash
# 1. Check error logs
kubectl logs -f deployment/fraiseql-api | grep -i error

# 2. Monitor query latency
curl http://localhost:9090/metrics | grep graphql_query_duration_seconds

# 3. Run smoke tests
./scripts/smoke-test.sh production

# 4. Check database query performance
psql $PRODUCTION_DATABASE_URL -c "
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
```

#### Short-Term Validation (First 24 Hours)

**Metrics Dashboard**:

- Query latency (p50, p95, p99)
- Error rate
- Throughput (queries per second)
- Database CPU/memory
- Index usage statistics

**Compare Before/After**:

```bash
# Query performance comparison
psql $PRODUCTION_DATABASE_URL -c "
  SELECT
    LEFT(query, 50) AS query_snippet,
    calls,
    mean_exec_time AS avg_ms,
    max_exec_time AS max_ms
  FROM pg_stat_statements
  WHERE query LIKE '%tf_sales%'
  ORDER BY calls DESC
"
```

**Expected Results**:

```
Before Migration:
query_snippet: SELECT * FROM tf_sales WHERE dimensions->>'region'
calls: 8,500
avg_ms: 845.2
max_ms: 2,150.0

After Migration:
query_snippet: SELECT * FROM tf_sales WHERE region_id = 'US'
calls: 8,500
avg_ms: 68.5  ✅ (12.3x improvement)
max_ms: 185.0 ✅
```

#### Long-Term Validation (7-30 Days)

- **Track p95/p99 trends** - Ensure sustained improvement
- **Monitor disk usage growth** - Verify storage cost is acceptable
- **Check for regressions** - No new slow queries introduced

---

## Rollback Procedures

### When to Rollback

Rollback if:

- ❌ Query errors increase significantly (> 5%)
- ❌ Latency increases instead of decreasing
- ❌ Database CPU/memory spikes
- ❌ Application errors related to schema

### Quick Rollback (PostgreSQL)

```sql
-- Rollback Script (generated by analyze command)

-- Step 1: Drop index
DROP INDEX IF EXISTS idx_tf_sales_region_id;

-- Step 2: Remove column
ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;

-- Step 3: Analyze table
ANALYZE tf_sales;
```

**Execution**:

```bash
psql $PRODUCTION_DATABASE_URL < migrations/rollback-20260112.sql
```

### Quick Rollback (SQL Server)

```sql
-- Rollback Script

-- Step 1: Drop index
DROP INDEX IF EXISTS idx_tf_sales_region_id ON tf_sales;
GO

-- Step 2: Remove column
ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;
GO

-- Step 3: Update statistics
UPDATE STATISTICS tf_sales WITH FULLSCAN;
GO
```

### Application Rollback

```bash
# 1. Revert schema changes
git revert HEAD

# 2. Recompile
fraiseql-cli compile schema.json

# 3. Redeploy
git push origin main
kubectl rollout restart deployment/fraiseql-api
```

### Full Database Restore (Last Resort)

**Only if**:

- Data corruption occurred
- Rollback SQL fails
- Critical production issue

```bash
# PostgreSQL
pg_restore -d mydb backup-pre-migration.dump

# SQL Server
RESTORE DATABASE mydb FROM DISK = 'C:\Backups\backup-pre-migration.bak'
WITH REPLACE;
```

---

## Common Migration Scenarios

### Scenario 1: Small Table (< 10,000 Rows)

**Strategy**: Simple ALTER TABLE (fast, low risk)

```bash
# Total time: < 1 second
psql $DATABASE_URL <<EOF
ALTER TABLE small_table ADD COLUMN region_id TEXT;
UPDATE small_table SET region_id = dimensions->>'region';
CREATE INDEX idx_small_table_region ON small_table (region_id);
ANALYZE small_table;
EOF
```

**Risk**: Negligible
**Downtime**: None (< 100ms lock)

---

### Scenario 2: Medium Table (10K - 1M Rows)

**Strategy**: Batched updates (safe, moderate time)

```sql
-- Step 1: Add column (instant)
ALTER TABLE medium_table ADD COLUMN region_id TEXT;

-- Step 2: Backfill in batches
DO $$
DECLARE
    batch_size INT := 10000;
BEGIN
    LOOP
        UPDATE medium_table
        SET region_id = dimensions->>'region'
        WHERE id IN (
            SELECT id FROM medium_table
            WHERE region_id IS NULL
            LIMIT batch_size
        );

        EXIT WHEN NOT FOUND;
        PERFORM pg_sleep(0.05);  -- 50ms between batches
    END LOOP;
END $$;

-- Step 3: Create index concurrently
CREATE INDEX CONCURRENTLY idx_medium_table_region
  ON medium_table (region_id);
```

**Duration**: 5-15 minutes
**Risk**: Low
**Downtime**: None (CONCURRENTLY)

---

### Scenario 3: Large Table (> 1M Rows)

**Strategy**: Online operations + extended migration window

**PostgreSQL**:

```sql
-- Use pg_repack for minimal blocking (requires extension)
CREATE EXTENSION IF NOT EXISTS pg_repack;

-- Add column + backfill + index (all online)
-- This may take hours for very large tables
```

**SQL Server**:

```sql
-- Use online index build with MAXDOP
CREATE NONCLUSTERED INDEX idx_large_table_region
  ON large_table (region_id)
  WITH (
    ONLINE = ON,
    MAXDOP = 4,              -- Use 4 CPU cores
    SORT_IN_TEMPDB = ON,     -- Use tempdb for sorting
    RESUMABLE = ON,          -- Can pause/resume if needed
    MAX_DURATION = 120       -- Auto-pause after 120 minutes
  );
```

**Duration**: 1-4 hours (depending on table size)
**Risk**: Medium (monitor closely)
**Downtime**: None (online operations)

---

## Migration Monitoring

### Real-Time Monitoring During Migration

**Terminal 1: Run Migration**

```bash
psql $DATABASE_URL < migrations/optimize-20260112.sql
```

**Terminal 2: Monitor Progress**

```bash
# PostgreSQL: Watch long-running queries
watch -n 1 "psql $DATABASE_URL -c \"
  SELECT
    pid,
    now() - query_start AS duration,
    state,
    LEFT(query, 50) AS query_snippet
  FROM pg_stat_activity
  WHERE state != 'idle'
  ORDER BY duration DESC
\""
```

**Terminal 3: Monitor Locks**

```bash
# PostgreSQL: Watch for blocking locks
watch -n 1 "psql $DATABASE_URL -c \"
  SELECT
    l.pid,
    l.mode,
    l.granted,
    a.query
  FROM pg_locks l
  JOIN pg_stat_activity a ON l.pid = a.pid
  WHERE NOT l.granted
\""
```

---

## Best Practices

### 1. Always Test in Staging First

Never apply migrations directly to production.

### 2. Use CONCURRENTLY for Index Creation

```sql
-- ✅ Good (non-blocking)
CREATE INDEX CONCURRENTLY idx_name ON table (column);

-- ❌ Bad (blocks writes)
CREATE INDEX idx_name ON table (column);
```

### 3. Batch Large Updates

```sql
-- ✅ Good (batched)
UPDATE table SET col = val WHERE id IN (SELECT id LIMIT 10000);

-- ❌ Bad (locks entire table)
UPDATE table SET col = val;
```

### 4. Monitor Replication Lag

For replicated databases:

```bash
# Check replication lag
psql $REPLICA_URL -c "
  SELECT
    client_addr,
    state,
    sent_lsn,
    write_lsn,
    replay_lsn,
    sync_state
  FROM pg_stat_replication
"
```

### 5. Schedule During Low Traffic

Apply migrations during:

- Nighttime (2-6 AM local time)
- Weekends
- After feature freeze

---

## Troubleshooting

### Issue: Migration Takes Too Long

**Symptoms**: Migration running for > 30 minutes

**Solutions**:

1. **Check for blocking locks**

   ```sql
   SELECT * FROM pg_locks WHERE NOT granted;
   ```

2. **Kill long-running queries**

   ```sql
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE state = 'active' AND query_start < NOW() - INTERVAL '1 hour';
   ```

3. **Increase resources temporarily**

   ```sql
   -- PostgreSQL
   SET maintenance_work_mem = '2GB';
   SET max_parallel_maintenance_workers = 4;
   ```

---

### Issue: Index Creation Fails

**Error**: `ERROR: could not create unique index "idx_name"`

**Cause**: Duplicate values in column

**Solution**:

```sql
-- Find duplicates
SELECT column_name, COUNT(*)
FROM table
GROUP BY column_name
HAVING COUNT(*) > 1;

-- Clean up duplicates before creating index
```

---

## Next Steps

- **[Troubleshooting Guide](troubleshooting.md)** - Common issues and solutions
- **[Examples](./examples/basic-denormalization.md)** - Real-world case studies
- **[Operations Configuration](../operations/configuration.md)** - Observability settings

---

*Last updated: 2026-01-12*
