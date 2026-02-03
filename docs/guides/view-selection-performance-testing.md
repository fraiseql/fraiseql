# View Selection Performance Testing Methodology

**Purpose**: Scientific approach to measuring and validating view performance improvements

**Audience**: Performance engineers, DBAs, Developers

**Duration**: 30-60 minutes per test

**Related Guides in This Series**:

- [Quick Reference](./view-selection-quick-reference.md) — Use to decide if testing is needed
- [Migration Checklist](./view-selection-migration-checklist.md) — Use after testing to plan rollout

---

## Overview

Before migrating from logical views (v_*, va_*) to table-backed views (tv_*, ta_*), you must validate that:

1. The table-backed view is actually faster
2. The speedup justifies the storage and maintenance overhead
3. Performance is consistent under production load

This guide provides the testing methodology.

---

## Prerequisites

### Setup

- [ ] Development/staging environment available
- [ ] Production-representative dataset (or >80% of prod size)
- [ ] Access to explain plan output
- [ ] Ability to run queries multiple times
- [ ] Database statistics up to date

**Pre-test Commands**:
```bash
# Refresh statistics
psql -c "ANALYZE;" && echo "Statistics updated"

# Check table sizes
psql -c "SELECT schemaname, tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename))
         FROM pg_tables WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
         ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC LIMIT 10;"
```

### Environment Specifications

**Document before testing:**

```
Environment: staging | prod | dev
Database version: _________
CPU cores: _________
RAM: _________ GB
Storage type: SSD | HDD | NVMe
Other processes running: _________ (should be minimal)
```

---

## Phase 1: Baseline Measurement (Logical Views)

### Step 1: Prepare Query Set

Choose realistic queries that users actually run:

```sql
-- Query 1: Simple lookup
SELECT * FROM v_user WHERE id = $1;

-- Query 2: List with filter
SELECT * FROM v_user WHERE created_at > NOW() - INTERVAL '30 days' LIMIT 100;

-- Query 3: Complex nesting (for tv_* testing)
SELECT * FROM v_user_full WHERE id = $1;  -- Includes posts, comments, likes

-- Query 4: Aggregation (for ta_* testing)
SELECT DATE(created_at), COUNT(*), SUM(total)
FROM va_orders
WHERE created_at >= $1 AND created_at < $2
GROUP BY DATE(created_at);
```

**Document**:

- Query set: ________________________
- Expected use frequency: ________ per second
- Data: Production size? ☐ Yes ☐ Partial (______%)

### Step 2: Single Query Benchmark

Run each query individually with EXPLAIN ANALYZE:

```sql
-- Reset statistics
TRUNCATE pg_stat_statements;

-- Run query with timing
EXPLAIN (ANALYZE, BUFFERS, TIMING, VERBOSE)
SELECT * FROM v_user_full WHERE id = '550e8400-e29b-41d4-a716-446655440000';
```

**Record Metrics**:

| Metric | Query 1 | Query 2 | Query 3 | Avg |
|--------|---------|---------|---------|-----|
| Execution time (ms) | _____ | _____ | _____ | _____ |
| Buffers hit (%) | _____ | _____ | _____ | _____ |
| Rows returned | _____ | _____ | _____ | _____ |
| CPU time (ms) | _____ | _____ | _____ | _____ |
| I/O time (ms) | _____ | _____ | _____ | _____ |

**Key Indicators to Look For**:

- Sequential scans (slow): _____ count
- Nested loop joins (often slow): _____ count
- Sort operations (expensive): _____ count
- Hash joins (faster): _____ count

### Step 3: Repeated Query Benchmark (Cache Warmed)

Run the same query 10 times to measure cache performance:

```python
import psycopg2
import time

conn = psycopg2.connect("dbname=fraiseql_staging")
cur = conn.cursor()

query = """
EXPLAIN (ANALYZE, BUFFERS, TIMING)
SELECT * FROM v_user_full WHERE id = %s
"""

times = []
for i in range(10):
    start = time.time()
    cur.execute(query, ('550e8400-e29b-41d4-a716-446655440000',))
    result = cur.fetchall()
    elapsed = time.time() - start
    times.append(elapsed * 1000)  # Convert to ms

avg_time = sum(times) / len(times)
min_time = min(times)
max_time = max(times)
p95_time = sorted(times)[int(len(times) * 0.95)]

print(f"Average: {avg_time:.2f}ms, Min: {min_time:.2f}ms, Max: {max_time:.2f}ms, P95: {p95_time:.2f}ms")
```

**Record**:

- Average time (10 runs): ______ ms
- Min: ______ ms
- Max: ______ ms
- P95: ______ ms
- Variance: ______ % (acceptable: <20%)

### Step 4: Load Test Benchmark

Run multiple concurrent queries to simulate production load:

```python
import psycopg2
import threading
import time
from statistics import mean, stdev

def run_query(thread_id, user_ids, results):
    """Run repeated queries in a thread"""
    conn = psycopg2.connect("dbname=fraiseql_staging")
    cur = conn.cursor()

    thread_times = []
    query = "SELECT * FROM v_user_full WHERE id = %s"

    for user_id in user_ids:
        start = time.time()
        cur.execute(query, (user_id,))
        _ = cur.fetchall()
        thread_times.append((time.time() - start) * 1000)

    results[thread_id] = thread_times
    conn.close()

# Test with 10 concurrent connections
num_threads = 10
queries_per_thread = 100
user_ids = ['550e8400-e29b-41d4-a716-44665544000' + str(i % 10) for i in range(1000)]

results = {}
threads = []
start_time = time.time()

for i in range(num_threads):
    t = threading.Thread(target=run_query, args=(i, user_ids, results))
    threads.append(t)
    t.start()

for t in threads:
    t.join()

total_time = time.time() - start_time

# Aggregate results
all_times = []
for thread_times in results.values():
    all_times.extend(thread_times)

print(f"Concurrency: {num_threads} threads")
print(f"Total queries: {len(all_times)}")
print(f"Total time: {total_time:.2f}s")
print(f"Throughput: {len(all_times) / total_time:.0f} queries/sec")
print(f"Avg response: {mean(all_times):.2f}ms")
print(f"P95 response: {sorted(all_times)[int(len(all_times) * 0.95)]:.2f}ms")
print(f"P99 response: {sorted(all_times)[int(len(all_times) * 0.99)]:.2f}ms")
```

**Record** (Logical View Baseline):

| Metric | Result |
|--------|--------|
| Concurrent connections | 10 |
| Total queries | ______ |
| Total time | ______ s |
| Throughput | ______ q/s |
| Avg response | ______ ms |
| P95 response | ______ ms |
| P99 response | ______ ms |
| Error rate | ______ % |

---

## Phase 2: Create Table-Backed View

### Create and Populate

```bash
# Create the table-backed view
psql -c "$(cat migration.sql)"

# Populate with data
psql -c "SELECT refresh_tv_user_profile();"

# Verify
psql -c "SELECT COUNT(*) FROM tv_user_profile;"
```

- [ ] Table created: ☐ Yes
- [ ] Initial population: ______ rows
- [ ] Storage used: ______ MB

---

## Phase 3: Test Table-Backed View

### Step 5: Single Query Benchmark (Table-Backed)

```sql
-- Same query, different table
EXPLAIN (ANALYZE, BUFFERS, TIMING, VERBOSE)
SELECT * FROM tv_user_profile WHERE id = '550e8400-e29b-41d4-a716-446655440000';
```

**Record Metrics**:

| Metric | tv_* Result | vs v_* | Improvement |
|--------|------------|--------|------------|
| Execution time (ms) | _____ | vs _____ | _____ x faster |
| Buffers hit (%) | _____ | vs _____ | _____ % better |
| CPU time (ms) | _____ | vs _____ | _____ % less |
| I/O time (ms) | _____ | vs _____ | _____ % less |

**Decision Point**:

- Is tv_* faster? ☐ Yes | ☐ No
- Speedup >= 5x? ☐ Yes | ☐ No (if not, investigate)

### Step 6: Repeated Query Benchmark (Table-Backed)

Run the same 10-query test:

```python
# Same script as Step 3, but with tv_user_profile instead of v_user_full
times_tv = [...]  # Results from table-backed view

print(f"Table-backed avg: {mean(times_tv):.2f}ms vs Logical avg: {mean(times_v):.2f}ms")
print(f"Speedup: {mean(times_v) / mean(times_tv):.1f}x")
```

**Record**:

- Average time (10 runs): ______ ms
- vs Logical view: ______ x faster
- Consistency: ☐ Stable (<20% variance) | ☐ Inconsistent (investigate)

### Step 7: Load Test Benchmark (Table-Backed)

```python
# Same load test script as Step 4, but with tv_user_profile

print(f"Logical view P95: {logical_p95:.2f}ms")
print(f"Table-backed P95: {table_backed_p95:.2f}ms")
print(f"Improvement: {logical_p95 / table_backed_p95:.1f}x")
```

**Record** (Table-Backed View):

| Metric | Result | vs Logical | Improvement |
|--------|--------|-----------|------------|
| Throughput | ______ q/s | vs ______ | _____ % increase |
| Avg response | ______ ms | vs ______ | _____ x faster |
| P95 response | ______ ms | vs ______ | _____ x faster |
| P99 response | ______ ms | vs ______ | _____ x faster |
| Error rate | ______ % | vs ______ | No change ☐ |

---

## Phase 4: Staleness & Refresh Performance

### Step 8: Measure Refresh Latency

Test how quickly the table-backed view updates after source data changes:

```python
import time
import psycopg2

conn = psycopg2.connect("dbname=fraiseql_staging")
cur = conn.cursor()

# Insert new user
insert_query = """
INSERT INTO tb_user (email, name) VALUES (%s, %s) RETURNING id
"""
cur.execute(insert_query, ('perf-test@example.com', 'Performance Test'))
conn.commit()
new_user_id = cur.fetchone()[0]

# Measure time until it appears in table-backed view
start_time = time.time()
while True:
    cur.execute("SELECT id FROM tv_user_profile WHERE id = %s", (new_user_id,))
    if cur.fetchone() is not None:
        latency = (time.time() - start_time) * 1000
        break
    time.sleep(10)  # Check every 10ms
    if time.time() - start_time > 5:  # Timeout after 5 seconds
        latency = 5000
        break

print(f"Refresh latency: {latency:.0f}ms")
```

**Record**:

- Refresh strategy: ☐ Trigger | ☐ Scheduled
- Refresh latency: ______ ms (target: <100ms for trigger, <5min for scheduled)
- Acceptable? ☐ Yes | ☐ No

### Step 9: Measure Refresh Overhead

Benchmark the refresh function performance:

```sql
-- Measure refresh time
\timing on

SELECT * FROM refresh_tv_user_profile();

-- Expected: <100ms for small updates, <5s for full refresh
```

**Record**:

- Refresh time (single record): ______ ms
- Refresh time (full table): ______ s
- CPU impact: ☐ Low | ☐ Medium | ☐ High

---

## Phase 5: Validation & Decision

### Step 10: Data Accuracy Verification

Verify that logical and table-backed views return the same data:

```sql
-- Compare row counts
SELECT
    (SELECT COUNT(*) FROM v_user_full) logical_count,
    (SELECT COUNT(*) FROM tv_user_profile) table_backed_count;

-- Compare specific rows
SELECT
    (SELECT data FROM v_user_full WHERE id = $1) logical_data,
    (SELECT data FROM tv_user_profile WHERE id = $1) table_backed_data;

-- Compare aggregations
SELECT
    (SELECT COUNT(*) FROM v_user_full WHERE created_at > NOW() - INTERVAL '30 days') logical_30d,
    (SELECT COUNT(*) FROM tv_user_profile WHERE created_at > NOW() - INTERVAL '30 days') table_backed_30d;
```

**Verify**: ☐ Identical results | ☐ Discrepancies found (investigate)

### Step 11: Storage Overhead Calculation

```sql
-- Measure storage used
SELECT
    'Logical view' as view_type,
    pg_size_pretty(pg_total_relation_size('v_user_full')) as total_size
UNION ALL
SELECT
    'Table-backed view',
    pg_size_pretty(pg_total_relation_size('tv_user_profile'))
UNION ALL
SELECT
    'Source table',
    pg_size_pretty(pg_total_relation_size('tb_user'));
```

**Record**:

- Source table (tb_user): ______ MB
- Logical view (v_user_full): ______ MB (storage: 0)
- Table-backed view (tv_user_profile): ______ MB
- Overhead: ______ % of source table
- Acceptable? ☐ Yes (< 50%) | ☐ Marginal (50-100%) | ☐ Too high (> 100%)

### Step 12: Cost-Benefit Analysis

Fill out this matrix to decide if migration is justified:

| Factor | Measurement | Weight | Score |
|--------|-------------|--------|-------|
| Query speedup | _____ x | 40% | _____ |
| P95 latency improvement | _____ ms | 30% | _____ |
| Storage overhead | ______ % | 20% | _____ |
| Refresh latency | ______ ms | 10% | _____ |
| **Total Score** | | 100% | **_____** |

**Scoring**:

- Query speedup: 5-10x = 100, 10-50x = 95, >50x = 90, <5x = 0
- P95 latency: <100ms = 100, 100-300ms = 80, 300-1000ms = 50, >1s = 0
- Storage: <10% = 100, 10-30% = 90, 30-50% = 70, >50% = 0
- Refresh: <50ms = 100, 50-200ms = 80, 200-1000ms = 50, >1s = 0

### Step 13: Final Decision

**Proceed with migration if:**

- [ ] Query speedup >= 5x (absolute minimum)
- [ ] P95 latency improved by >= 50%
- [ ] Storage overhead <= 50% of source table
- [ ] Data accuracy verified (100% match)
- [ ] Refresh latency acceptable for use case
- [ ] Team consensus on maintenance cost

**Decision**:

```
☐ PROCEED with migration
  Justification: ________________________

☐ REVISIT optimization alternatives
  Reason: ________________________

☐ REJECT - not worth it
  Reason: ________________________
```

**Sign-off**: _________________ Date: _______

---

## Performance Testing Report Template

```
# Performance Testing Report: [View Name]

## Executive Summary

- Speedup: _____ x
- Storage overhead: ______ %
- Decision: ☐ Proceed | ☐ Revisit | ☐ Reject

## Test Environment

- Database: _________ version _________
- Dataset size: ______ GB (_____ % of production)
- Concurrent connections: _____
- Test date: _________

## Baseline (Logical View)

- Average response: ______ ms
- P95 response: ______ ms
- Throughput: ______ q/s
- CPU: ______ % (peak)

## Table-Backed View

- Average response: ______ ms (_____ x faster)
- P95 response: ______ ms (_____ x faster)
- Throughput: ______ q/s
- CPU: ______ % (peak)

## Additional Metrics

- Refresh latency: ______ ms
- Storage used: ______ MB (______ % overhead)
- Data accuracy: ☐ Verified (100% match)
- Error rate: ______ %

## Recommendation
[Your conclusion and reasoning]

## Approval

- Tester: _________________ Date: _______
- DBA: _________________ Date: _______
- Architect: _________________ Date: _______
```

---

## Advanced Testing Scenarios

### Scenario 1: Peak Load Testing

Test table-backed view under production-like peak load:

```python
# Simulate 10x normal load
num_threads = 50  # vs normal 10
queries_per_thread = 1000

# Run load test
# Compare against logical view baseline
```

**Document**: Peak load handling: ☐ Acceptable | ☐ Degraded

### Scenario 2: Refresh Under Load

Test refresh performance while queries are running:

```sql
-- Session 1: Run continuous queries
\watch SELECT COUNT(*) FROM tv_user_profile;

-- Session 2: Trigger refresh
SELECT refresh_tv_user_profile();

-- Measure impact on query latency
```

**Document**: Query latency during refresh: ______ ms (vs normal: ______ ms)

### Scenario 3: Cache Sensitivity

Test performance with cold cache:

```bash
# Drop cache
sync && echo 3 > /proc/sys/vm/drop_caches

# Run query (first time, cache cold)
EXPLAIN (ANALYZE) SELECT * FROM tv_user_profile WHERE id = ?;

# vs warm cache from previous test
```

**Document**: Cold cache vs warm cache: ______ x slower

### Scenario 4: Concurrent Refresh

Test multiple concurrent refresh operations:

```sql
-- In separate transactions, run multiple refreshes
SELECT refresh_tv_user_profile();  -- Transaction 1
SELECT refresh_tv_user_profile();  -- Transaction 2
SELECT refresh_tv_user_profile();  -- Transaction 3

-- Measure lock contention
SELECT * FROM pg_locks WHERE relation::regclass::text LIKE 'tv_%';
```

**Document**: Lock contention: ☐ None | ☐ Minor | ☐ Significant

---

## Troubleshooting Poor Results

### Problem: Table-Backed View Not Faster

**Possible Causes**:

1. Missing indexes on table-backed view
2. JSONB column not indexed (GIN)
3. Statistics not updated
4. Query not using the table (still hitting logical view)

**Debugging**:
```sql
-- Check indexes exist
SELECT * FROM pg_indexes WHERE tablename = 'tv_user_profile';

-- Check query plan
EXPLAIN (VERBOSE) SELECT * FROM tv_user_profile WHERE id = ?;
-- Should show "Index Scan" not "Seq Scan"

-- Verify you're querying the right table
SELECT COUNT(*) FROM tv_user_profile;
SELECT COUNT(*) FROM v_user_profile;  -- Different?
```

### Problem: High Storage Overhead

**Possible Causes**:

1. Unnecessary JSONB composition (large nested structures)
2. Indexes too large relative to data
3. Historical data not cleaned up

**Resolution**:
```sql
-- Analyze storage breakdown
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) as total,
    pg_size_pretty(pg_relation_size('public.'||tablename)) as table_size,
    pg_size_pretty(pg_indexes_size('public.'||tablename)) as indexes_size
FROM pg_tables
WHERE tablename = 'tv_user_profile';

-- Consider dropping unnecessary indexes
-- Or simplify JSONB composition
```

### Problem: Refresh Latency Unacceptable

**Possible Causes**:

1. Trigger overhead too high (too many writes)
2. Refresh function doing full table scan
3. Competing queries blocking refresh

**Resolution**:

- Switch from trigger-based to scheduled batch
- Optimize refresh function with WHERE clause
- Schedule during low-traffic window

---

## Resources

**Supplementary Guides** (part of this series):

- [Quick Reference](./view-selection-quick-reference.md) — Quick decision matrix and benchmarks
- [Migration Checklist](./view-selection-migration-checklist.md) — Step-by-step migration workflow

**Core Documentation**:

- [View Selection Guide](../architecture/database/view-selection-guide.md) — Full decision framework
- [tv_* Table Pattern](../architecture/database/tv-table-pattern.md) — JSON plane patterns
- [ta_* Table Pattern](../architecture/database/ta-table-pattern.md) — Arrow plane patterns
- [Schema Conventions](../specs/schema-conventions.md) — Database design conventions

**External References**:

- [PostgreSQL EXPLAIN Documentation](https://www.postgresql.org/docs/current/sql-explain.html)
- [PostgreSQL Query Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)

**Suggested Workflow**:

1. Use Quick Reference to decide if migration is needed
2. Use this guide to validate performance improvements
3. Use Migration Checklist to plan and execute rollout
4. Refer to specific pattern guides (tv_* or ta_*) for details
