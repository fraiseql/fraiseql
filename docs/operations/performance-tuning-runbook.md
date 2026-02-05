<!-- Skip to main content -->
---
title: Performance Tuning Runbook
description: Operational procedures for diagnosing and optimizing FraiseQL query performance in production.
keywords: ["deployment", "scaling", "performance", "monitoring", "troubleshooting"]
tags: ["documentation", "reference"]
---

# Performance Tuning Runbook

**Status:** ✅ Production Ready
**Audience:** DevOps, Database Administrators, Performance Engineers
**Reading Time:** 30-40 minutes
**Last Updated:** 2026-02-05

Operational procedures for diagnosing and optimizing FraiseQL query performance in production.

---

## Overview

This runbook provides **diagnosis workflows** and **remediation steps** for common performance issues. Each section includes:

- **Symptoms** (what users see)
- **Diagnosis** (how to identify root cause)
- **Solutions** (how to fix it)
- **Prevention** (how to avoid in future)

---

## Quick Diagnosis Tree

```text
<!-- Code example in TEXT -->
Is performance issue...

1. NEW: Slow since deployment?
   → Go to: AFTER SCHEMA CHANGE (below)

2. GRADUAL: Getting slower over time?
   → Go to: INDEX FRAGMENTATION or STATISTICS STALE

3. INTERMITTENT: Only sometimes slow?
   → Go to: CONNECTION POOL EXHAUSTION or DATABASE UNDER LOAD

4. SPECIFIC QUERY: One query is slow?
   → Go to: QUERY ANALYSIS

5. BROAD: Many queries slow?
   → Go to: DATABASE TUNING or NETWORK LATENCY
```text
<!-- Code example in TEXT -->

---

## 1. Query Performance Analysis

### Symptom: Single Query Takes > 1 Second

### Diagnosis Step 1: Enable Query Logging

```bash
<!-- Code example in BASH -->
# Set environment variable
export RUST_LOG=FraiseQL=debug

# Restart server
systemctl restart FraiseQL

# Watch logs
tail -f /var/log/FraiseQL.log | grep "QUERY"
```text
<!-- Code example in TEXT -->

**Look for:**

- Query execution time
- SQL generation time
- Database roundtrip time
- Result transformation time

### Diagnosis Step 2: Get Query Plan from Database

```sql
<!-- Code example in SQL -->
-- PostgreSQL
EXPLAIN ANALYZE
SELECT ... FROM ... WHERE ...;

-- MySQL
EXPLAIN FORMAT=JSON
SELECT ... FROM ... WHERE ...;

-- SQL Server
SET STATISTICS IO ON;
SET STATISTICS TIME ON;
SELECT ... FROM ... WHERE ...;
```text
<!-- Code example in TEXT -->

**Interpret output:**

- **Seq Scan** = Sequential scan (bad, table is too large)
- **Index Scan** = Using index (good)
- **Nested Loop** = Joining rows inefficiently (check indexes)
- **Hash Join** = Hash-based join (acceptable)

### Diagnosis Step 3: Check for Missing Indexes

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Find tables without indexes
SELECT schemaname, tablename
FROM pg_tables
WHERE schemaname = 'public'
EXCEPT
SELECT schemaname, tablename
FROM pg_indexes
WHERE schemaname = 'public'
ORDER BY tablename;

-- Check most common WHERE columns in slow queries
SELECT query FROM pg_stat_statements
WHERE mean_time > 100
ORDER BY mean_time DESC LIMIT 5;

-- Example output: "SELECT * FROM users WHERE created_at >= ..."
-- → Need index on created_at
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Add Missing Index**

```sql
<!-- Code example in SQL -->
-- Identify filter columns from EXPLAIN output
CREATE INDEX idx_user_created_at ON users(created_at);

-- Verify index is used
EXPLAIN SELECT * FROM users WHERE created_at >= '2026-01-01';
-- Should show "Index Scan" not "Seq Scan"
```text
<!-- Code example in TEXT -->

**Syntax per database:**

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Concurrent index creation (doesn't lock table)
CREATE INDEX CONCURRENTLY idx_user_created_at ON users(created_at);

-- MySQL: Online index creation (5.7+)
ALTER TABLE users ADD INDEX idx_created_at (created_at), ALGORITHM=INPLACE;

-- SQL Server: Online index creation
CREATE INDEX idx_created_at ON users(created_at) WITH (ONLINE=ON);
```text
<!-- Code example in TEXT -->

**Solution 2: Composite Indexes for Common Filter Combinations**

```sql
<!-- Code example in SQL -->
-- If queries often filter by both tenant and status:
-- CREATE INDEX idx_user_tenant_status ON users(tenant_id, status);
-- Covers WHERE tenant_id = X AND status = 'active'

-- If queries filter by range, put range column last:
-- CREATE INDEX idx_posts_user_date ON posts(user_id, created_at);
-- Covers WHERE user_id = X AND created_at >= Y
```text
<!-- Code example in TEXT -->

**Solution 3: Switch to Materialized View (tv_*)**

If index doesn't help (aggregation or complex join):

```python
<!-- Code example in Python -->
# Before: Logical view (computed per query)
@type
class UserStats:
    post_count: int = field(computed="COUNT(SELECT ...)")

# After: Table-backed view (materialized daily)
@type
class UserStats:
    post_count: int  # Pre-computed, indexed
```text
<!-- Code example in TEXT -->

**Solution 4: Reduce Query Scope**

```graphql
<!-- Code example in GraphQL -->
# Before: Fetching too much
query {
  users {  # Gets all 10M users!
    id
    name
    posts { id title }
  }
}

# After: Add filters
query {
  users(where: { created_at: { gte: "2026-01-01" } }) {
    id
    name
    posts { id title }
  }
}
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor slow queries: `max_query_time > 500ms`
- [ ] Weekly index review: Check for missing indexes on filtered columns
- [ ] Query profiling in staging: Profile all new queries before deploying
- [ ] Document expected performance: "Query X should run in < 100ms"

---

## 2. Database Connection Pool Issues

### Symptom: "Too Many Connections" or "Connection Timeout"

### Diagnosis

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Check active connections
SELECT COUNT(*) FROM pg_stat_activity;
SELECT max_conn FROM pg_settings WHERE name = 'max_connections';

-- Example: 100 max connections, 95 active → Almost exhausted

-- Find slow connections
SELECT pid, usename, state, query_start, query
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY query_start;

-- MySQL: Check connection count
SHOW PROCESSLIST;
SHOW VARIABLES LIKE 'max_connections';
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Increase Pool Size**

```toml
<!-- Code example in TOML -->
# FraiseQL.toml
[database]
pool_size = 50  # Was 10, increase to 50
```text
<!-- Code example in TEXT -->

**Maximum safe values:**

- PostgreSQL max_connections: Usually 200-1000 (depends on server)
- MySQL max_connections: Usually 500-10000
- SQLite: Not applicable (single connection)

**Solution 2: Enable Connection Pooling at Database Level**

```bash
<!-- Code example in BASH -->
# PostgreSQL: Use PgBouncer
sudo apt install pgbouncer

# Configure /etc/pgbouncer/pgbouncer.ini
[databases]
mydb = host=localhost port=5432 dbname=mydb

[pgbouncer]
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25
```text
<!-- Code example in TEXT -->

**Solution 3: Kill Slow/Idle Connections**

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Kill connections idle > 5 minutes
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE state = 'idle'
AND query_start < now() - interval '5 minutes';

-- MySQL: Kill long-running connections
KILL QUERY process_id;
```text
<!-- Code example in TEXT -->

**Solution 4: Set Connection Timeout**

```toml
<!-- Code example in TOML -->
[database]
connection_timeout_seconds = 10  # Wait max 10s for connection
query_timeout_seconds = 30       # Abort query after 30s
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor pool usage: Alert at 80% capacity
- [ ] Set max_client_conn based on peak load
- [ ] Regular connection review (weekly)
- [ ] Implement query timeouts
- [ ] Close subscriptions on disconnect

---

## 3. Index Fragmentation

### Symptom: Query Was Fast, Now Slow (Same Data Size)

### Diagnosis

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Check index bloat
SELECT schemaname, tablename, indexname, idx_scan
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY pg_relation_size(indexrelid) DESC;

-- MySQL: Check index fragmentation
SELECT object_schema, object_name, count_read, count_write
FROM performance_schema.table_io_waits_summary_by_index_usage
WHERE count_read > 0
ORDER BY count_read DESC;
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Reindex (PostgreSQL)**

```sql
<!-- Code example in SQL -->
-- Reindex single index (requires lock)
REINDEX INDEX idx_user_created_at;

-- Reindex entire table (rebuilds all indexes)
REINDEX TABLE users;

-- Concurrent reindex (no lock, v12+)
REINDEX INDEX CONCURRENTLY idx_user_created_at;
```text
<!-- Code example in TEXT -->

**Solution 2: Optimize Table (MySQL)**

```sql
<!-- Code example in SQL -->
-- Defragment and rebuild indexes
OPTIMIZE TABLE users;

-- Analyze statistics
ANALYZE TABLE users;
```text
<!-- Code example in TEXT -->

**Solution 3: Regular Maintenance Schedule**

```bash
<!-- Code example in BASH -->
# Weekly index maintenance
0 2 * * 0 FraiseQL-maintenance reindex --tables all

# Monthly full optimization
0 2 1 * * FraiseQL-maintenance optimize --full
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Schedule weekly index maintenance
- [ ] Monitor index bloat: Alert if > 20% bloat
- [ ] Use concurrent indexing operations
- [ ] Regular ANALYZE to update statistics

---

## 4. Stale Database Statistics

### Symptom: Query Planner Chooses Wrong Index or Seq Scan

### Diagnosis

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Check when statistics were last updated
SELECT schemaname, tablename, last_vacuum, last_analyze
FROM pg_stat_user_tables
ORDER BY last_analyze;

-- If last_analyze is very old → Update statistics

-- MySQL: Check table statistics
SELECT object_schema, object_name, count_insert, count_update, count_delete
FROM performance_schema.table_io_waits_summary_by_table
WHERE count_insert > 10000 OR count_update > 10000
ORDER BY count_insert DESC;
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Update Statistics (ANALYZE)**

```sql
<!-- Code example in SQL -->
-- PostgreSQL
ANALYZE users;
ANALYZE;  -- All tables

-- MySQL
ANALYZE TABLE users;

-- SQL Server
UPDATE STATISTICS users;
```text
<!-- Code example in TEXT -->

**Solution 2: Auto-Vacuum Configuration (PostgreSQL)**

```sql
<!-- Code example in SQL -->
-- Check autovacuum settings
SELECT name, setting FROM pg_settings WHERE name LIKE 'autovacuum%';

-- Increase frequency if needed
ALTER DATABASE mydb SET autovacuum_naptime = '30s';  -- Default 60s
```text
<!-- Code example in TEXT -->

**Solution 3: Schedule Regular ANALYZE**

```bash
<!-- Code example in BASH -->
# Hourly analysis of heavily modified tables
0 * * * * psql -d $DATABASE -c "ANALYZE users; ANALYZE posts;"

# Daily full database analysis
0 2 * * * psql -d $DATABASE -c "ANALYZE;"
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Enable autovacuum (PostgreSQL)
- [ ] Schedule regular ANALYZE: Daily for OLTP, Hourly for heavily modified tables
- [ ] Monitor last_analyze timestamp
- [ ] Alert if statistics > 24 hours old

---

## 5. Slow Aggregation Queries

### Symptom: GROUP BY or COUNT(DISTINCT) Queries Taking > 10 Seconds

### Diagnosis

```sql
<!-- Code example in SQL -->
-- Identify aggregation queries
SELECT query FROM pg_stat_statements
WHERE query LIKE '%COUNT%' OR query LIKE '%GROUP BY%'
ORDER BY mean_time DESC LIMIT 5;

-- Check if they use indexes
EXPLAIN SELECT COUNT(DISTINCT user_id) FROM posts;
-- Look for "Seq Scan" (bad) vs "Index Only Scan" (good)
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Add Index for Aggregation Column**

```sql
<!-- Code example in SQL -->
-- For: COUNT(DISTINCT user_id)
CREATE INDEX idx_posts_user_id ON posts(user_id);

-- For: GROUP BY status
CREATE INDEX idx_orders_status ON orders(status);

-- For: Multiple columns in GROUP BY
CREATE INDEX idx_user_org_status ON users(organization_id, status);
```text
<!-- Code example in TEXT -->

**Solution 2: Pre-Compute with Materialized View**

```python
<!-- Code example in Python -->
# Before: Slow query in every request
@FraiseQL.query
def user_stats():
    # SELECT user_id, COUNT(*) as post_count FROM posts GROUP BY user_id
    # Takes 10+ seconds on 100M rows!

# After: Use table-backed view
@type
class UserStats:
    user_id: ID
    post_count: int  # Pre-computed, updated hourly
    updated_at: DateTime
```text
<!-- Code example in TEXT -->

**Solution 3: Approximate Aggregations**

For very large datasets where approximate values acceptable:

```python
<!-- Code example in Python -->
# Use HyperLogLog instead of COUNT(DISTINCT)
@type
class PostStats:
    unique_users_approx: int  # HyperLogLog count
    # 5% error but 100x faster
```text
<!-- Code example in TEXT -->

**Solution 4: Partition Large Tables**

```sql
<!-- Code example in SQL -->
-- PostgreSQL: Partition posts table by date
CREATE TABLE posts_2026_01 PARTITION OF posts
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');

-- Aggregation on monthly partition is much faster
SELECT COUNT(*) FROM posts_2026_01;
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Profile GROUP BY queries before deploying
- [ ] Create indexes on aggregation columns
- [ ] Use materialized views for complex aggregations
- [ ] Monitor query time: Alert if > 5 seconds

---

## 6. N+1 Query Problem

### Symptom: Many Small Queries Instead of One Large Query

### Diagnosis

```bash
<!-- Code example in BASH -->
# Enable FraiseQL query logging
export RUST_LOG=fraiseql_core=debug

# Run problematic query
# Look for: "Executing SELECT..." repeated for each parent record

# Example: Query 100 users, then 100 queries for posts = 101 queries!
```text
<!-- Code example in TEXT -->

**Count queries:**

```bash
<!-- Code example in BASH -->
grep -c "Executing SELECT" logs.txt
# 101 queries → N+1 problem!
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: FraiseQL Auto-Batching**

```graphql
<!-- Code example in GraphQL -->
# FraiseQL automatically batches nested queries:
query {
  users(first: 100) {
    id
    posts(first: 50) {  # Batched automatically!
      id
      title
    }
  }
}
```text
<!-- Code example in TEXT -->

**Result:** ~2 queries total (users + batched posts)

**Solution 2: Use Table-Backed View with Pre-Fetched Data**

```python
<!-- Code example in Python -->
@type
class UserWithPosts:
    """Denormalized view with posts pre-fetched."""
    id: ID
    name: str
    posts_json: List[Post]  # Fetched in view definition
```text
<!-- Code example in TEXT -->

**Solution 3: Flatten Query Structure**

Instead of:

```graphql
<!-- Code example in GraphQL -->
query {
  users { id posts { id comments { id } } }
}
```text
<!-- Code example in TEXT -->

Do separate queries:

```graphql
<!-- Code example in GraphQL -->
query { users { id } }
query { posts { id userId } }
query { comments { id postId } }
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor query count per request: Alert if > 10 queries per request
- [ ] Load test with large datasets (1000+ records)
- [ ] Profile queries in staging: Identify N+1 before deploying
- [ ] Test queries with `EXPLAIN` to see execution plan

---

## 7. Network Latency Issues

### Symptom: Queries Slow Even Though Database is Fast

### Diagnosis

```bash
<!-- Code example in BASH -->
# Measure latency to database
ping -c 10 database-host
# Normal: 1-10ms
# High: > 50ms indicates network issue

# Measure database response time
time psql -h database-host -d mydb -c "SELECT COUNT(*) FROM users;"
# Split time into:
# - Connection time (first line of output)
# - Query execution time

# Check network path
traceroute database-host
# Look for high latency at any hop
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Reduce Network Roundtrips**

```python
<!-- Code example in Python -->
# Before: Multiple separate queries
user = await db.query("SELECT * FROM users WHERE id = ?", [user_id])
posts = await db.query("SELECT * FROM posts WHERE user_id = ?", [user_id])

# After: Single joined query
result = await db.query("""
    SELECT u.*, p.*
    FROM users u
    LEFT JOIN posts p ON u.id = p.user_id
    WHERE u.id = ?
""", [user_id])
```text
<!-- Code example in TEXT -->

**Solution 2: Use Connection Pooling Closer to App**

```bash
<!-- Code example in BASH -->
# Deploy PgBouncer/ProxySQL on same host as FraiseQL
# Reduces network roundtrips from 100ms to 1ms
```text
<!-- Code example in TEXT -->

**Solution 3: Cache Frequently Accessed Data**

```toml
<!-- Code example in TOML -->
[FraiseQL.caching]
enabled = true
ttl_seconds = 300  # Cache query results 5 minutes
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor network latency: Alert if > 50ms
- [ ] Deploy database close to application (same AZ)
- [ ] Use connection pooling
- [ ] Batch queries to reduce roundtrips

---

## 8. Memory Leaks or Growing Memory Usage

### Symptom: Memory Usage Increases Over Time, Never Returns

### Diagnosis

```bash
<!-- Code example in BASH -->
# Monitor memory usage
top -p <fraiseql_pid>
# Look at RES (resident set size)
# Should be stable, not growing

# Check for open file handles
lsof -p <fraiseql_pid> | wc -l
# If growing → File handle leak

# Check for unclosed database connections
SELECT count(*) FROM pg_stat_activity WHERE usename = 'fraiseql_user';
# Should not grow over time

# Check subscription connections
SELECT count(*) FROM websocket_connections;
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Check for Unclosed Resources**

```python
<!-- Code example in Python -->
# Ensure subscriptions are closed
try:
    async for event in subscription:
        process_event(event)
finally:
    subscription.close()  # Always close

# Ensure database connections returned to pool
async with pool.acquire() as conn:
    # Connection automatically returned when block exits
```text
<!-- Code example in TEXT -->

**Solution 2: Set Resource Limits**

```toml
<!-- Code example in TOML -->
[FraiseQL.limits]
max_concurrent_queries = 1000
max_subscription_connections = 5000
max_result_size_bytes = 10485760  # 10MB
```text
<!-- Code example in TEXT -->

**Solution 3: Regular Memory Profiling**

```bash
<!-- Code example in BASH -->
# Use memory profiler (if supported)
cargo profiling memory --duration 60s

# Restart after 24 hours if needed
systemctl restart FraiseQL
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor memory: Alert if growth > 10%/day
- [ ] Regular service restarts: Daily or weekly
- [ ] Subscribe to all resource cleanup
- [ ] Set timeouts on all connections

---

## 9. Query Caching Effectiveness

### Symptom: Query Results Seem Stale or Caching Not Working

### Diagnosis

```bash
<!-- Code example in BASH -->
# Check cache configuration
curl http://localhost:5000/health/cache
# Returns: cache hits, misses, size

# Enable cache logging
export RUST_LOG=fraiseql_cache=debug

# Run same query twice
curl -X POST http://localhost:5000/graphql \
  -d '{"query": "{ users { id } }"}'
curl -X POST http://localhost:5000/graphql \
  -d '{"query": "{ users { id } }"}'

# Check logs for "cache hit" vs "cache miss"
```text
<!-- Code example in TEXT -->

### Solutions

**Solution 1: Enable Query Caching**

```toml
<!-- Code example in TOML -->
[FraiseQL.caching]
enabled = true
default_ttl_seconds = 300  # Cache 5 minutes
```text
<!-- Code example in TEXT -->

**Solution 2: Invalidate Cache on Mutation**

```graphql
<!-- Code example in GraphQL -->
mutation {
  createUser(name: "Alice") @cache(invalidate: ["users"]) {
    id
    name
  }
}
```text
<!-- Code example in TEXT -->

**Solution 3: Adjust TTL Based on Data Freshness**

```python
<!-- Code example in Python -->
@FraiseQL.type
class User:
    # Cache for 5 minutes (user data doesn't change often)
    id: ID
    name: str

@FraiseQL.type
class InventoryLevel:
    # Don't cache (inventory changes constantly)
    quantity: int = field(cache=False)
```text
<!-- Code example in TEXT -->

### Prevention

- [ ] Monitor cache effectiveness: Alert if hit rate < 30%
- [ ] Set appropriate TTL for each data type
- [ ] Implement cache invalidation on mutations
- [ ] Profile cache performance: Expected hit rate?

---

## 10. Production Response Checklist

**When performance issue reported:**

1. **Immediately:**
   - [ ] Check error logs for exceptions
   - [ ] Verify database connectivity
   - [ ] Check if it's a known issue

2. **Within 5 minutes:**
   - [ ] Identify affected queries
   - [ ] Check query count: Normal load?
   - [ ] Run EXPLAIN on slow query
   - [ ] Check for missing indexes

3. **Within 15 minutes:**
   - [ ] Apply temporary mitigation (cache, timeout, index)
   - [ ] Monitor for improvement
   - [ ] Communicate status to team

4. **Later:**
   - [ ] Root cause analysis
   - [ ] Implement permanent fix
   - [ ] Deploy to staging first
   - [ ] Gradual rollout to production
   - [ ] Document in runbook

---

## See Also

**Related Guides:**

- **[Schema Design Best Practices](../guides/schema-design-best-practices.md)** — Designing for performance
- **[Common Gotchas](../guides/common-gotchas.md)** — Avoid performance pitfalls
- **[Monitoring & Observability](../guides/monitoring.md)** — Setting up performance metrics
- **[View Selection Guide](../guides/view-selection-performance-testing.md)** — Testing view performance

**Architecture & Database:**

- **[Database Targeting](../architecture/database/database-targeting.md)** — Database-specific optimization
- **[Arrow Plane Architecture](../architecture/database/arrow-plane.md)** — Columnar query optimization
- **[Observability Architecture](./observability-architecture.md)** — Runtime performance monitoring

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
