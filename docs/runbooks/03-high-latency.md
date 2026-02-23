# Runbook: High Latency / Slow Queries

## Symptoms

- GraphQL query response times exceed SLA (typical: p99 > 500ms, p95 > 200ms)
- Client timeout errors or application slowdown
- Metrics show `request_duration_seconds_bucket` high percentiles increasing
- Database query times show in logs as `elapsed_ms > 500`
- Slow query logs appearing in PostgreSQL logs
- High CPU or memory utilization on FraiseQL server

## Impact

- User-facing requests delayed, potential timeout
- Batch processing and background jobs slow down
- Subscriptions and real-time updates lag
- Database load increases (queries running longer = more resource consumption)
- Cascading timeout failures if services depend on FraiseQL responses

## Investigation

### 1. Current Latency Metrics

```bash
# Check current p99 and p95 latencies
curl -s http://localhost:8815/metrics | grep "request_duration_seconds_bucket"

# Example output:
# request_duration_seconds_bucket{le="0.1"} 100
# request_duration_seconds_bucket{le="0.5"} 450
# request_duration_seconds_bucket{le="1.0"} 480
# request_duration_seconds_bucket{le="5.0"} 500
# If most requests are in high buckets, latency is a problem

# Calculate approximate percentiles manually
curl -s http://localhost:8815/metrics | grep "request_duration_seconds" | grep -E "(sum|count)"
```

### 2. Identify Slow Queries

```bash
# Enable query logging if not already enabled (temporary)
# Set RUST_LOG=fraiseql=debug to log all queries with timing

docker exec fraiseql-server env RUST_LOG=fraiseql=debug curl http://localhost:8815/metrics

# Or check existing logs for slow operations
docker logs fraiseql-server | grep -E "duration|elapsed|slow" | tail -30

# Check PostgreSQL slow query log
if [ -f /var/lib/postgresql/pg_log/slow_queries.log ]; then
    tail -50 /var/lib/postgresql/pg_log/slow_queries.log | sort -t'=' -k3 -rn | head -10
fi

# Or query PostgreSQL for slowest queries
psql $DATABASE_URL << 'EOF'
SELECT query, mean_exec_time, max_exec_time, calls
FROM pg_stat_statements
WHERE mean_exec_time > 100 -- queries taking > 100ms average
ORDER BY mean_exec_time DESC
LIMIT 20;
EOF
```

### 3. Database Query Performance

```bash
# Check for full table scans (should be rare on large tables)
psql $DATABASE_URL << 'EOF'
SELECT schemaname, tablename, seq_scan, seq_tup_read, idx_scan, idx_tup_fetch
FROM pg_stat_user_tables
WHERE seq_scan > 1000  -- tables with many sequential scans
ORDER BY seq_scan DESC
LIMIT 10;
EOF

# Analyze current query plan for specific slow query
psql $DATABASE_URL << 'EOF'
EXPLAIN ANALYZE SELECT * FROM large_table WHERE condition;
EOF

# Check for missing indexes
psql $DATABASE_URL << 'EOF'
SELECT schemaname, tablename, attname
FROM pg_stat_user_tables t
JOIN pg_stat_user_indexes i ON t.relid = i.relid
WHERE idx_scan = 0  -- indexes never used
AND indexrelname NOT LIKE '%_pkey'
LIMIT 20;
EOF

# Check table bloat
psql $DATABASE_URL << 'EOF'
SELECT schemaname, tablename, round(100 * pg_relation_size('schema.table') /
       pg_total_relation_size('schema.table')) AS bloat_percentage
FROM pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema');
EOF
```

### 4. Resource Utilization

```bash
# Check FraiseQL server resource usage
docker stats fraiseql-server --no-stream

# Expected: CPU < 80%, Memory < 2GB (adjust based on config)

# Check system-wide resources
top -b -n 1 | head -20  # CPU and memory
iostat -x 1 5           # Disk I/O
vmstat 1 5              # Memory and swap

# Check for memory leaks in FraiseQL
curl -s http://localhost:8815/metrics | grep "memory" || echo "Memory metrics not available"

# Monitor database connections
psql $DATABASE_URL << 'EOF'
SELECT datname, count(*) as connections
FROM pg_stat_activity
GROUP BY datname;
EOF
```

### 5. Check for Blocking Queries

```bash
# Find what's blocking what in PostgreSQL
psql $DATABASE_URL << 'EOF'
SELECT blocked_locks.pid, blocked_locks.usename, blocking_locks.pid,
       blocking_locks.usename, blocked_statements.query
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_statements ON blocked_statements.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks ON blocking_locks.locktype = blocked_locks.locktype
  AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database
  AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation
  AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page
  AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple
  AND blocking_locks.virtualxid IS NOT DISTINCT FROM blocked_locks.virtualxid
  AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid
  AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid
  AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid
  AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid
  AND blocking_locks.pid != blocked_locks.pid
JOIN pg_catalog.pg_stat_activity blocking_statements ON blocking_statements.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;
EOF
```

### 6. Network Latency

```bash
# Check latency between FraiseQL and Database
ping -c 4 postgres-host

# Check TCP retransmissions
netstat -s | grep "retransmit" || ss -s | grep "retrans"

# Monitor active connections and their state
netstat -an | grep ESTABLISHED | wc -l

# Check connection state distribution
netstat -an | grep postgres | awk '{print $6}' | sort | uniq -c
```

## Mitigation

### Immediate (0-5 minutes)

1. **Identify the slowest queries being run**

   ```bash
   # Log all queries with timing for next few minutes
   export RUST_LOG=fraiseql=debug
   docker restart fraiseql-server
   # Let it run for 2-3 minutes
   docker logs fraiseql-server | grep "duration\|elapsed" | sort -t'=' -k2 -rn | head -20
   ```

2. **Kill any long-running transactions**

   ```bash
   # Find transactions older than 5 minutes
   psql $DATABASE_URL << 'EOF'
   SELECT pid, usename, query, xact_start
   FROM pg_stat_activity
   WHERE xact_start < NOW() - INTERVAL '5 minutes'
   AND state = 'active';
   EOF

   # Kill them
   psql $DATABASE_URL << 'EOF'
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE xact_start < NOW() - INTERVAL '5 minutes'
   AND state = 'active';
   EOF
   ```

3. **Enable query result caching** (if available)

   ```bash
   # Set environment variable for cache duration
   export FRAISEQL_QUERY_CACHE_TTL=300  # 5 minute cache
   docker restart fraiseql-server
   ```

4. **Reduce database connection pool timeout**

   ```bash
   # Fail fast instead of waiting for pool exhaustion
   export FRAISEQL_DB_POOL_TIMEOUT=5  # seconds
   docker restart fraiseql-server
   ```

### Short-term (5-30 minutes)

5. **Run VACUUM and ANALYZE**

   ```bash
   # Full maintenance (may take time on large databases)
   psql $DATABASE_URL << 'EOF'
   VACUUM ANALYZE;
   EOF

   # Or just analyze (faster)
   psql $DATABASE_URL << 'EOF'
   ANALYZE;
   EOF
   ```

6. **Reindex bloated tables**

   ```bash
   # Find and reindex high-bloat indexes
   psql $DATABASE_URL << 'EOF'
   REINDEX TABLE CONCURRENTLY table_name;
   EOF
   ```

7. **Increase work_mem for complex queries** (if self-hosted PostgreSQL)

   ```bash
   # Temporary: Increase memory for sorting/hash joins
   psql $DATABASE_URL << 'EOF'
   SET work_mem = '256MB';
   EOF

   # Permanent: Update postgresql.conf
   echo "work_mem = 256MB" >> /etc/postgresql/15/main/postgresql.conf
   systemctl restart postgresql
   ```

## Resolution

### Root Cause Analysis and Fix

```bash
#!/bin/bash
set -e

echo "=== Query Latency Investigation ==="

# 1. Capture slowest queries
echo "1. Identifying slowest queries..."
SLOW_QUERY_FILE="/tmp/slow_queries.sql"
psql $DATABASE_URL << 'EOF' > "$SLOW_QUERY_FILE"
SELECT query, mean_exec_time, max_exec_time, calls,
       pg_size_pretty(mean_exec_time::bigint) as avg_time
FROM pg_stat_statements
WHERE mean_exec_time > 100
ORDER BY mean_exec_time DESC
LIMIT 10;
EOF
cat "$SLOW_QUERY_FILE"

# 2. Analyze execution plan
echo ""
echo "2. Analyzing query plans..."
# For each slow query, get execution plan
HEAD_QUERY=$(head -1 "$SLOW_QUERY_FILE" | cut -d'|' -f1)
echo "Explaining: $HEAD_QUERY"
psql $DATABASE_URL << EOF
EXPLAIN ANALYZE FORMAT JSON
$HEAD_QUERY;
EOF

# 3. Check for missing indexes
echo ""
echo "3. Checking for missing indexes..."
psql $DATABASE_URL << 'EOF'
-- Unused indexes
SELECT indexname FROM pg_stat_user_indexes WHERE idx_scan = 0 LIMIT 10;

-- Tables with full table scans
SELECT tablename FROM pg_stat_user_tables
WHERE seq_scan > idx_scan AND seq_scan > 100
ORDER BY seq_scan DESC LIMIT 10;
EOF

# 4. Check statistics are up to date
echo ""
echo "4. Checking table statistics..."
psql $DATABASE_URL << 'EOF'
SELECT schemaname, tablename, last_vacuum, last_analyze
FROM pg_stat_user_tables
WHERE last_analyze < NOW() - INTERVAL '1 day'
ORDER BY last_analyze;
EOF

# 5. Recommend fixes
echo ""
echo "=== Recommended Fixes ==="
echo "1. Add indexes for frequently queried columns"
echo "2. Run ANALYZE on tables with stale statistics"
echo "3. Partition very large tables"
echo "4. Review GraphQL query complexity (N+1 queries)"
echo "5. Enable query caching for repeated queries"
```

### Adding Indexes (if root cause is identified)

```bash
# 1. Identify columns in WHERE, JOIN, ORDER BY clauses of slow query
# For example: WHERE user_id = ? AND created_at > ?

# 2. Create index (CONCURRENT to avoid blocking)
psql $DATABASE_URL << 'EOF'
CREATE INDEX CONCURRENTLY idx_users_id_created
ON users(user_id, created_at)
WHERE deleted_at IS NULL;
EOF

# 3. Analyze to update statistics
psql $DATABASE_URL -c "ANALYZE users;"

# 4. Compare before/after query time
psql $DATABASE_URL << 'EOF'
EXPLAIN ANALYZE SELECT * FROM users WHERE user_id = 123 AND created_at > NOW() - INTERVAL '30 days';
EOF

# 5. Monitor index usage
psql $DATABASE_URL << 'EOF'
SELECT indexname, idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
WHERE tablename = 'users'
ORDER BY idx_scan DESC;
EOF
```

### Query Optimization

```bash
# 1. Look for N+1 query patterns in FraiseQL logs
# Each GraphQL field should not trigger separate queries

# 2. Use query batching / DataLoader pattern if available

# 3. Optimize GraphQL queries sent by clients
# Include only needed fields, avoid deep nesting

# 4. Review compiled schema for inefficient field resolvers
jq '.types[] | select(.name == "User") | .fields[]' /etc/fraiseql/schema.compiled.json
```

## Prevention

### Monitoring Setup

```bash
# Prometheus alerts for high latency
cat > /etc/prometheus/rules/fraiseql-latency.yml << 'EOF'
groups:
  - name: fraiseql_latency
    rules:
      - alert: HighP99Latency
        expr: histogram_quantile(0.99, request_duration_seconds) > 0.5
        for: 5m
        action: page

      - alert: HighP95Latency
        expr: histogram_quantile(0.95, request_duration_seconds) > 0.2
        for: 10m
        action: page

      - alert: SlowDatabaseQueries
        expr: pg_stat_statements_mean_exec_time > 100
        for: 5m
        action: notify
EOF

# Grafana dashboard for latency monitoring
# Create dashboard with:
# - Request latency percentiles (p50, p95, p99)
# - Database query time histogram
# - Connection pool usage
# - Query count and error rate
```

### Best Practices

- **Query profiling**: Use `EXPLAIN ANALYZE` before deploying new queries
- **Index strategy**: Create indexes for columns in WHERE, JOIN, ORDER BY
- **Caching**: Enable query caching for repeated queries (if available)
- **Connection pooling**: Size pool appropriately (default 5-20 connections)
- **Pagination**: Always paginate large result sets
- **Field selection**: Only request fields actually needed in GraphQL
- **Batch queries**: Use DataLoader pattern for N+1 prevention
- **Load testing**: Perform load tests before major deployments

### Regular Maintenance

```bash
# Weekly: Check for missing indexes and run ANALYZE
0 2 * * 0 psql $DATABASE_URL -c "ANALYZE;" &>/dev/null

# Monthly: Vacuum and reindex
0 3 1 * * psql $DATABASE_URL -c "VACUUM ANALYZE;" &>/dev/null

# Quarterly: Full maintenance during low-traffic window
0 2 1 */3 * psql $DATABASE_URL -c "REINDEX DATABASE $DATABASE_NAME;" &>/dev/null
```

## Escalation

- **Database queries slow**: Database team (add indexes, tune PostgreSQL)
- **High memory usage**: Application team (possible memory leak)
- **High CPU usage**: Performance team (optimize algorithms)
- **Network latency**: Infrastructure / Network team
- **Client-side slowdown**: Client application team (optimize GraphQL queries)
