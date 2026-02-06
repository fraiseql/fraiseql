<!-- Skip to main content -->
---

title: View Selection Migration Checklist
description: - SQL query fundamentals and JOIN optimization
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# View Selection Migration Checklist

**Status:** ✅ Production Ready
**Audience:** DBAs, Architects, Developers
**Reading Time:** 10-15 minutes
**Last Updated:** 2026-02-05

**Duration**: 1-3 hours depending on complexity

---

## Prerequisites

### Required Knowledge

- SQL query fundamentals and JOIN optimization
- Database views (logical vs materialized)
- Query performance analysis (EXPLAIN ANALYZE)
- Index design and usage
- View naming conventions in FraiseQL (v_*, va_*, tv_*, ta_*)
- Schema compilation and deployment workflows
- Backup and recovery procedures
- Schema version control and migration tracking

### Required Software

- FraiseQL v2.0.0-alpha.1 or later
- SQL client for your database (psql, mysql, sqlcmd, sqlite3)
- Schema migration tool (Flyway, Liquibase, or custom scripts)
- Git for version control
- Performance profiling tool (pg_stat_statements, EXPLAIN)
- DDL generation tool (FraiseQL-cli or SDK)

### Required Infrastructure

- Production and staging database environments
- Backup system for database snapshots
- Query performance monitoring
- Capacity planning (disk space for table-backed views)
- Test database for schema validation
- Network connectivity to all database environments
- Deployment pipeline or manual change management process

#### Optional but Recommended

- Query performance baseline metrics
- Automated performance regression testing
- Blue-green or canary deployment strategy
- Database replication for zero-downtime migration
- Load testing tools for post-migration validation
- Monitoring and alerting on query latency

**Time Estimate:** 1-3 hours for complete migration (evaluation + planning + execution + testing)

## Pre-Migration: Evaluation Phase

### 1. Identify the Problem

- [ ] Query is slow (>1 second for JSON, >1 second for Arrow)
- [ ] Production metrics show high database CPU/memory
- [ ] Dashboards timing out
- [ ] GraphQL subscriptions have variable latency
- [ ] Documented specific bottleneck with EXPLAIN output

**Action**: Run EXPLAIN ANALYZE on current query

```sql
<!-- Code example in SQL -->
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_full WHERE id = '550e8400...'
-- OR
SELECT * FROM va_orders WHERE created_at >= NOW() - INTERVAL '7 days'
```text
<!-- Code example in TEXT -->

**Document**:

- Current execution time: ______ ms
- Current memory usage: ______ MB
- Current CPU: ______ %
- Affected users/dashboards: ___________________

### 2. Confirm Table-Backed is Right Solution

- [ ] Is it really a JOIN problem? (EXPLAIN shows sequential scans, nested loops)
- [ ] Not a query planner issue (checked statistics, analyzed tables)
- [ ] Not an indexing problem (tried indexes, still slow)
- [ ] Not a WHERE clause issue (predicates are reasonable)
- [ ] Not N+1 problem on client side

**Check**: Run query with different filters to confirm consistency

```sql
<!-- Code example in SQL -->
-- Baseline
EXPLAIN (ANALYZE) SELECT * FROM v_user_full WHERE id = ?;
-- Multiple filters
EXPLAIN (ANALYZE) SELECT * FROM v_user_full WHERE created_at > ? AND status = ?;
```text
<!-- Code example in TEXT -->

**Document**: Average slow query time across different conditions: ______ ms

### 3. Calculate Business Justification

- [ ] Estimated speedup: 10-50x faster (tv_*) or 50-100x (ta_*)
- [ ] Storage overhead acceptable: ____% additional storage
- [ ] Write volume supports refresh strategy: _____ writes/min
- [ ] Read volume justifies storage: _____ reads/sec
- [ ] Team consensus on adding table maintenance cost

**Decision**: ☐ Proceed with migration | ☐ Try other optimizations first

---

## Design Phase

### 4. Design Composition View (if applicable)

#### For tv_* (JSON)

- [ ] Identified all related entities (User → Posts → Comments → Likes)
- [ ] Documented JSONB structure needed
- [ ] Created intermediate composition views as helpers
  - [ ] v_comments_by_post
  - [ ] v_posts_with_comments
  - [ ] (other composition views)

**Template**:

```sql
<!-- Code example in SQL -->
-- Helper view: Aggregate comments per post
CREATE OR REPLACE VIEW v_comments_by_post AS
SELECT
    fk_post,
    jsonb_agg(
        jsonb_build_object(
            'id', id,
            'text', text,
            'createdAt', created_at
        )
        ORDER BY created_at DESC
    ) AS comments_data
FROM tb_comment
WHERE deleted_at IS NULL
GROUP BY fk_post;
```text
<!-- Code example in TEXT -->

**Document**:

- Entity hierarchy: ________________________
- Composition depth: _____ levels
- Intermediate views created: ___________________

### For ta_* (Arrow)

- [ ] Identified columns to extract from complex joins
- [ ] Decided on denormalization strategy (time-series columns, aggregates)
- [ ] Planned indexing strategy (BRIN for timestamps, B-tree for FK)

**Template**:

```sql
<!-- Code example in SQL -->
CREATE TABLE ta_orders (
    id TEXT PRIMARY KEY,
    total NUMERIC,
    created_at TIMESTAMPTZ,
    -- Denormalized filters
    customer_id UUID,
    status TEXT,
    -- Aggregates
    item_count INTEGER,

    FOREIGN KEY (id) REFERENCES tb_order(id)
);
```text
<!-- Code example in TEXT -->

**Document**:

- Columns extracted: ________________________
- Denormalized filters: ____________________
- Indexes planned: _________________________

### 5. Choose Refresh Strategy

#### For tv_* (JSON)

- [ ] Trigger-based (real-time, <100ms)
  - [ ] Write volume < 1K/min
  - [ ] Latency requirements < 1 second

- [ ] Scheduled batch (low overhead, staleness acceptable)
  - [ ] Write volume > 1K/min
  - [ ] Can tolerate 5-15 minute staleness

- [ ] Manual/command-based (development/testing)
  - [ ] Low volume or ad-hoc usage

**Document**: Chosen strategy: ☐ Trigger | ☐ Scheduled | ☐ Manual

### For ta_* (Arrow)

- [ ] Trigger-based + BRIN (real-time for time-series)
  - [ ] BRIN index on timestamp columns
  - [ ] < 10M row inserts/day

- [ ] Scheduled batch (nightly/hourly)
  - [ ] > 10M rows
  - [ ] Analytics use case (staleness OK)

**Document**: Chosen strategy: ☐ Trigger | ☐ Scheduled

### 6. Plan Monitoring

- [ ] Staleness tracking (updated_at column)
- [ ] Refresh performance metrics
- [ ] Data accuracy verification queries
- [ ] Alert thresholds for stale data
- [ ] Query performance regression detection

**Template Monitoring Queries**:

```sql
<!-- Code example in SQL -->
-- Staleness
SELECT MAX(updated_at) - NOW() as staleness FROM tv_user_profile;

-- Refresh performance
EXPLAIN (ANALYZE) SELECT refresh_tv_user_profile();

-- Accuracy
SELECT
    (SELECT COUNT(*) FROM tv_user_profile) tv_count,
    (SELECT COUNT(*) FROM v_user) v_count;
```text
<!-- Code example in TEXT -->

**Document**: Monitoring plan attached: ☐ Yes | ☐ No

---

## Implementation Phase

### 7. Create Table-Backed View (Non-Production)

**Environment**: Development/Staging (NOT production yet)

- [ ] Created physical table with correct schema
- [ ] Added indexes (GIN for JSONB, BRIN for timestamps)
- [ ] Tested initial population query
- [ ] Refresh function executes successfully
- [ ] No SQL errors in trigger definitions

**Execution**:

```bash
<!-- Code example in BASH -->
# In dev/staging environment
psql -h staging-db -U postgres fraiseql_staging < migration.sql

# Verify
psql -h staging-db -U postgres fraiseql_staging \
  -c "SELECT COUNT(*) FROM tv_user_profile;"
```text
<!-- Code example in TEXT -->

**Document**:

- Table created: ☐ Yes | Date: _______
- Indexes created: ☐ Yes | Count: _______
- Initial population: ______ rows inserted
- Refresh time: ______ ms

### 8. Initial Population

- [ ] Population query completes without errors
- [ ] Row count matches source view
- [ ] JSONB structure is valid (no nulls in data)
- [ ] Storage usage within estimates

**Verification**:

```sql
<!-- Code example in SQL -->
-- Row count match
SELECT
    (SELECT COUNT(*) FROM tv_user_profile) as tv_count,
    (SELECT COUNT(*) FROM v_user) as v_count,
    (SELECT COUNT(*) FROM tb_user WHERE deleted_at IS NULL) as source_count;

-- JSONB validity
SELECT COUNT(*) FROM tv_user_profile WHERE data IS NULL OR data = 'null'::JSONB;

-- Storage
SELECT pg_size_pretty(pg_total_relation_size('tv_user_profile'));
```text
<!-- Code example in TEXT -->

**Document**:

- Rows inserted: ______
- JSONB null count: ______
- Storage used: ______
- Population time: ______ seconds

### 9. Set Up Refresh Mechanism

#### For trigger-based

- [ ] Refresh trigger function created
- [ ] Trigger attached to source table(s)
- [ ] Test write to source table
- [ ] Verify table-backed view auto-updated

**Execution**:

```bash
<!-- Code example in BASH -->
# Create trigger function and attach
psql -h staging-db -U postgres fraiseql_staging < trigger_setup.sql

# Test
psql -h staging-db -U postgres fraiseql_staging \
  -c "INSERT INTO tb_user (email, name) VALUES ('test@example.com', 'Test');"

# Check auto-refresh
psql -h staging-db -U postgres fraiseql_staging \
  -c "SELECT COUNT(*) FROM tv_user_profile WHERE updated_at > NOW() - INTERVAL '10s';"
```text
<!-- Code example in TEXT -->

**Document**:

- Triggers created: ☐ Yes | Count: _______
- Test write executed: ☐ Yes
- Auto-refresh verified: ☐ Yes | Time: ______ ms

### For scheduled batch

- [ ] Refresh function created
- [ ] pg_cron schedule added
- [ ] Test refresh execution
- [ ] Verify schedule timing

**Execution**:

```bash
<!-- Code example in BASH -->
psql -h staging-db -U postgres fraiseql_staging \
  -c "SELECT cron.schedule('refresh-tv-profile', '*/5 * * * *', 'SELECT refresh_tv_user_profile();');"

# Test manual execution
psql -h staging-db -U postgres fraiseql_staging \
  -c "SELECT * FROM refresh_tv_user_profile();"
```text
<!-- Code example in TEXT -->

**Document**:

- Schedule created: ☐ Yes | Interval: _______
- Manual test result: ________________

### 10. Performance Testing (Staging)

- [ ] Baseline query time recorded for old view
- [ ] New table-backed query time measured
- [ ] Speedup calculated and documented
- [ ] Queries return identical results
- [ ] Data accuracy verified with test queries

**Benchmark Script**:

```sql
<!-- Code example in SQL -->
-- Old view (baseline)
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user_full WHERE id = '550e8400-e29b-41d4-a716-446655440000';
-- Expected: 2-5s

-- New view
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM tv_user_profile WHERE id = '550e8400-e29b-41d4-a716-446655440000';
-- Expected: 100-200ms

-- Verify identical results
SELECT
    (SELECT COUNT(*) FROM v_user_full WHERE id = ?) old_count,
    (SELECT COUNT(*) FROM tv_user_profile WHERE id = ?) new_count;
```text
<!-- Code example in TEXT -->

**Document**:

- Old view time: ______ ms
- New view time: ______ ms
- Speedup: ______ x
- Results identical: ☐ Yes | ☐ No
- Outliers identified: ________________________

### 11. Schema Binding Update

- [ ] Updated authoring layer (Python/TypeScript)
- [ ] Changed type binding to use new view

**Example Change**:

```python
<!-- Code example in Python -->
# Before
@FraiseQL.type()
class User:
    pass

# After
@FraiseQL.type(view="tv_user_profile")
class UserProfile:
    pass
```text
<!-- Code example in TEXT -->

**Document**:

- Binding updated: ☐ Yes
- File(s) modified: ________________________
- Compilation tested: ☐ Yes

### 12. Integration Testing (Staging)

- [ ] Ran existing test suite against new view
- [ ] No test regressions
- [ ] End-to-end GraphQL/Arrow Flight queries work
- [ ] Subscription latency improved (if applicable)
- [ ] Concurrent queries don't cause issues

**Test Commands**:

```bash
<!-- Code example in BASH -->
# GraphQL queries
curl -X POST http://staging-server/graphql \
  -d '{"query": "{ user(id: \"...\") { id name posts { id title } } }"}'

# Arrow Flight queries
python -c "import pyarrow.flight as flight; ..."

# Run test suite
pytest tests/ -v -k "user_profile"
```text
<!-- Code example in TEXT -->

**Document**:

- Test suite run: ☐ Passed | ☐ Failed
- Regressions found: ________________________
- Subscription latency improvement: ______ ms

---

## Production Deployment

### 13. Pre-Deployment Checklist

- [ ] Change approved by senior architect/DBA
- [ ] Rollback plan documented
- [ ] On-call team notified
- [ ] Deployment window scheduled
- [ ] Backups verified

**Approvals**:

- Architect: _________________ Date: _______
- DBA: _________________ Date: _______
- Product: _________________ Date: _______

### 14. Production Deployment (Careful!)

#### Step 1: Create Table

```bash
<!-- Code example in BASH -->
# During low-traffic window
psql -h prod-db -U postgres fraiseql_prod < migration.sql
```text
<!-- Code example in TEXT -->

#### Step 2: Verify Creation

```sql
<!-- Code example in SQL -->
SELECT
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename))
FROM pg_tables
WHERE tablename = 'tv_user_profile';
```text
<!-- Code example in TEXT -->

- [ ] Table created successfully
- [ ] Size reasonable: ______ MB
- [ ] Document: Date/time deployed: _______

#### Step 3: Initial Population

```sql
<!-- Code example in SQL -->
SELECT * FROM refresh_tv_user_profile();
```text
<!-- Code example in TEXT -->

- [ ] Rows inserted: ______
- [ ] Completed without errors: ☐ Yes

#### Step 4: Enable Triggers/Schedules

```sql
<!-- Code example in SQL -->
-- Attach triggers or enable schedule
ALTER TABLE tv_user_profile ENABLE TRIGGER ALL;

-- Or verify schedule
SELECT * FROM cron.job WHERE jobname LIKE 'refresh%';
```text
<!-- Code example in TEXT -->

- [ ] Triggers/schedules active: ☐ Yes
- [ ] Document: Time enabled: _______

#### Step 5: Deploy Code

- [ ] GraphQL schema redeployed with type binding
- [ ] No errors in deployment logs
- [ ] Service restarted cleanly

**Document**:

- Deployment time: _______
- Downtime: _______ seconds (if any)
- Errors: ________________________

### 15. Post-Deployment Monitoring (24 hours)

#### Every 15 minutes

- [ ] Query error rates normal
- [ ] Staleness within acceptable range
- [ ] No spike in database load
- [ ] Application response times improved

### Hourly

- [ ] Check monitoring dashboard for anomalies
- [ ] Verify refresh function completing
- [ ] Query performance as expected
- [ ] No customer complaints

**Query Health**:

```sql
<!-- Code example in SQL -->
-- Check staleness
SELECT MAX(updated_at) - NOW() as staleness FROM tv_user_profile;

-- Check refresh stats
SELECT COUNT(*) as row_count FROM tv_user_profile;

-- Check index usage
SELECT schemaname, tablename, indexname, idx_scan
FROM pg_stat_user_indexes
WHERE tablename = 'tv_user_profile';
```text
<!-- Code example in TEXT -->

**Document**:

- Monitoring period: 24 hours ☐ Completed
- Issues found: ________________________
- Performance met expectations: ☐ Yes | ☐ No

### 16. Rollback Decision

#### If performance doesn't improve or issues occur

- [ ] Revert type binding to old view
- [ ] Drop table-backed view (or keep for later use)
- [ ] Redeploy service
- [ ] Monitor application recovery

**Rollback Command**:

```python
<!-- Code example in Python -->
# Revert to old view
@FraiseQL.type()  # Back to v_user_profile
class UserProfile:
    pass
```text
<!-- Code example in TEXT -->

**Document**:

- Rollback initiated: ☐ Yes | Date: _______
- Reason: ________________________
- Recovery time: ______ minutes

---

## Post-Migration

### 17. Documentation Updates

- [ ] Added view to system documentation
- [ ] Documented refresh strategy used
- [ ] Recorded performance improvement
- [ ] Added to operations runbook
- [ ] Updated SLOs/alerts

**Files Updated**:

- [ ] Confluence/Wiki: ________________________
- [ ] Runbook: ________________________
- [ ] SLO document: ________________________

### 18. Team Communication

- [ ] Team notified of deployment
- [ ] Performance improvement metrics shared
- [ ] Troubleshooting guide provided
- [ ] On-call runbook updated
- [ ] Training completed (if needed)

**Communications**:

- [ ] Slack announcement: ☐ Sent | Time: _______
- [ ] Team meeting: ☐ Held | Date: _______
- [ ] Documentation link shared: ☐ Yes

### 19. Ongoing Maintenance

- [ ] Added to deployment checklist
- [ ] Backup strategy includes table-backed view
- [ ] Schema migrations plan for table-backed view
- [ ] Monitoring alerts configured
- [ ] Quarterly review scheduled

**Maintenance Schedule**:

- Weekly staleness check: _______ (assigned to)
- Monthly performance review: _______ (assigned to)
- Quarterly refresh strategy review: _______ (assigned to)

### 20. Success Metrics

#### Document final results

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| Query time improvement | 10-50x | ______ x | ☐ Pass |
| P95 latency | <300ms | ______ ms | ☐ Pass |
| Staleness | <5min | ______ min | ☐ Pass |
| Storage overhead | <30% | ______ % | ☐ Pass |
| Error rate | 0 | ______ % | ☐ Pass |
| User satisfaction | +20% | ______ % | ☐ Pass |

**Overall Migration Status**: ☐ SUCCESS | ☐ PARTIAL | ☐ ROLLED BACK

**Sign-off**: _________________ Date: _______

---

## Appendix: Common Issues & Resolutions

### Issue: Triggers not firing

**Symptoms**: Table-backed view not updating after inserts

**Debug**:

```sql
<!-- Code example in SQL -->
-- Check trigger exists
SELECT * FROM information_schema.triggers
WHERE event_object_table = 'tb_user' AND trigger_name LIKE 'trg_refresh%';

-- Check if enabled
SELECT * FROM pg_trigger WHERE tgrelid = 'tb_user'::regclass;

-- Test manually
SELECT refresh_tv_user_profile_for_user('test-id'::UUID);
```text
<!-- Code example in TEXT -->

**Resolution**: Re-create trigger or enable if disabled

### Issue: Refresh function too slow

**Symptoms**: EXPLAIN ANALYZE on refresh shows >1s

**Debug**:

```sql
<!-- Code example in SQL -->
EXPLAIN (ANALYZE) SELECT refresh_tv_user_profile();
-- Look for sequential scans, missing indexes
```text
<!-- Code example in TEXT -->

**Resolution**: Add indexes or switch to batch refresh

### Issue: Schema mismatch after code deploy

**Symptoms**: Queries fail with "field not found in JSONB"

**Debug**:

```sql
<!-- Code example in SQL -->
-- Check JSONB structure
SELECT data FROM tv_user_profile LIMIT 1;
-- Look for missing/changed fields
```text
<!-- Code example in TEXT -->

**Resolution**: Manually refresh or re-run initial population

### Issue: Storage disk full

**Symptoms**: Insert errors due to disk space

**Debug**:

```sql
<!-- Code example in SQL -->
SELECT
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename))
FROM pg_tables
WHERE tablename LIKE 'tv_%' OR tablename LIKE 'ta_%'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```text
<!-- Code example in TEXT -->

**Resolution**: Archive old data or drop unused views

---

## Troubleshooting

### "Migration performance test shows no improvement (same latency)"

**Cause:** Table-backed view might not solve your bottleneck or indexes missing.

#### Diagnosis

1. Re-run performance test: Compare v_*vs tv_* latencies
2. Check table was actually created: `SELECT * FROM information_schema.tables WHERE table_name = 'tv_name';`
3. Run EXPLAIN: Compare execution plans for both views

#### Solutions

- Verify table has correct indexes: `SELECT * FROM pg_indexes WHERE tablename = 'tv_name';`
- Check that query is actually using table (not original view)
- Bottleneck might be in subquery, not view complexity
- Consider partitioning if table has millions of rows
- For Arrow plane (ta_*): May need ClickHouse integration for benefits

### "Cannot rollback migration - original view needed"

**Cause:** Table-backed view doesn't cover all access patterns of original view.

#### Diagnosis

1. Check which queries use tv_*vs original v_*
2. Find queries failing on tv_*: Review error logs
3. Compare schemas: Does tv_*have all columns of v_*?

#### Solutions

- Keep both views temporarily: v_*for queries, tv_* for new code
- Migrate gradually: Update application references one at a time
- Add missing columns to table-backed view
- Fix queries to work with table schema

### "View refresh is slow or blocking queries"

**Cause:** Materialized view refresh locks table.

#### Diagnosis

1. Check refresh time: How long does REFRESH MATERIALIZED VIEW take?
2. Check if indexes exist: Needed for refresh to be fast
3. Monitor table size: Large tables = slower refresh

#### Solutions

- Add indexes to base tables before refresh
- For high-frequency tables: Consider different strategy
- Use REFRESH MATERIALIZED VIEW CONCURRENTLY (PostgreSQL 9.5+)
- Schedule refreshes during off-peak hours
- Consider real-time CDC instead of materialized view

### "Schema mismatch between schema.json and table structure"

**Cause:** Table was created manually and doesn't match schema definition.

#### Diagnosis

1. Compare schemas: `SELECT * FROM schema.json WHERE name = 'X'`
2. Check table columns: `SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'tv_name';`
3. Look for type mismatches: string vs int, timestamp vs date

#### Solutions

- Re-generate table from schema.json: Drop and recreate
- Use DDL generation tool to ensure consistency
- Add missing columns to table
- Update schema.json if intentional difference

### "Deployment rollback failed - stuck in middle state"

**Cause:** Migration script interrupted or deployment killed.

#### Diagnosis

1. Check if table exists partially: `SELECT COUNT(*) FROM tv_name;`
2. Check migration script status: Look for error logs
3. Verify data consistency: Compare row counts with source view

#### Solutions

- Complete the migration manually or rollback
- Drop table and restart migration
- Restore from backup if data corrupted
- Implement migration idempotency: migrations should be safe to retry

### "Query performance degraded for users on old view"

**Cause:** Migration happened but some queries still use original view.

#### Diagnosis

1. Check query metrics: Which endpoints are slow?
2. Verify which queries execute: Enable query logging
3. Check application code: Are some components not updated?

#### Solutions

- Identify which component/query uses old view
- Update application to use table-backed view
- Monitor for old view usage: Add alerts
- Deprecate old view once everything migrated

### "Table-backed view takes too much disk space"

**Cause:** Materialized views with large datasets can be very large.

#### Diagnosis

1. Check table size: `SELECT pg_size_pretty(pg_total_relation_size('tv_name'));`
2. Compare to source view: How much larger?
3. Check if table can be partitioned

#### Solutions

- Consider partitioning by date: Store only recent data in table
- Use archive strategy: Move old data to separate table
- Reduce columns: Include only what's needed
- If not worth disk cost: Keep logical view instead

### "ClickHouse integration not improving Arrow plane performance"

**Cause:** Data not being ingested to ClickHouse or query going to PostgreSQL instead.

#### Diagnosis

1. Check if ClickHouse table exists: `SELECT name FROM system.tables WHERE database = 'default';`
2. Verify ingestion: `SELECT COUNT(*) FROM clickhouse_table;`
3. Check routing: Which backend does Arrow query use?

#### Solutions

- Verify CDC is configured to send to ClickHouse
- Check network connectivity to ClickHouse
- Ensure ClickHouse table schema matches
- Force query to ClickHouse: Check FraiseQL.toml routing config
- Monitor ClickHouse logs for insert errors

---

## Template Customization

### Copy this section for your specific migration

### Migration Details

| Item | Value |
|------|-------|
| Source view | v_user_full |
| Target table | tv_user_profile |
| Environment | production |
| Approval date | _________ |
| Deployment date | _________ |
| Expected speedup | _________ x |
| Actual speedup | _________ x |
| Status | ☐ Success |

---

## Supplementary Resources

**Related Guides** (also available in this series):

- [Quick Reference](./view-selection-quick-reference.md) — Use before starting migration (decision matrix, benchmarks)
- [Performance Testing](./view-selection-performance-testing.md) — Use for validation in Phase 2 (benchmarking methodology)

**Core Documentation**:

- [View Selection Guide](../architecture/database/view-selection-guide.md) — Comprehensive decision framework
- [tv_* Table Pattern](../architecture/database/tv-table-pattern.md) — JSON plane patterns and examples
- [ta_* Table Pattern](../architecture/database/ta-table-pattern.md) — Arrow plane patterns and examples
- [Schema Conventions](../specs/schema-conventions.md) — Database naming and structure

**Suggested Reading Order**:

1. Quick Reference (this guide)
2. View Selection Guide (full decision framework)
3. This Checklist (migration workflow)
4. Performance Testing (validation methodology)
5. Specific pattern guides (tv_*or ta_*) as reference
