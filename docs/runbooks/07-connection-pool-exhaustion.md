# Runbook: Connection Pool Exhaustion

## Symptoms

- GraphQL queries timeout with `connection pool exhausted` error
- Metrics show `db_pool_connections_active` == `db_pool_connections_max`
- No idle connections available (db_pool_connections_idle == 0)
- Requests queue up waiting for pool connections to become available
- Response times spike dramatically as requests wait for pooled connections
- PostgreSQL shows high `active_connections` in pg_stat_activity
- Application logs show: `timeout waiting for pooled connection`

## Impact

- **High**: Requests unable to acquire database connections
- All GraphQL queries fail once pool is exhausted
- New requests queue and eventually timeout
- Server becomes unusable until connections are released

## Investigation

### 1. Connection Pool Status

```bash
# Check current pool state
curl -s http://localhost:8815/metrics | grep "db_pool_connections"

# Expected output:
# db_pool_connections_active{} 20
# db_pool_connections_idle{} 0
# db_pool_connections_max{} 20
# If active == max and idle == 0, pool is exhausted

# Get more details
curl -s http://localhost:8815/metrics | grep -E "db_pool|connection" | sort

# Check pool connection duration (how long connections held)
curl -s http://localhost:8815/metrics | grep "connection_duration"

# Monitor pool state in real-time
watch -n 1 'curl -s http://localhost:8815/metrics | grep db_pool_connections'
```

### 2. What's Holding Connections

```bash
# Query PostgreSQL for connections from FraiseQL
psql $DATABASE_URL << 'EOF'
SELECT pid, usename, state, state_change, query_start, query
FROM pg_stat_activity
WHERE datname = current_database()
AND state != 'idle in transaction'
ORDER BY query_start;
EOF

# Find long-running queries holding connections
psql $DATABASE_URL << 'EOF'
SELECT pid, usename, state, query_start,
       EXTRACT(EPOCH FROM (NOW() - query_start)) as duration_secs, query
FROM pg_stat_activity
WHERE datname = current_database()
AND query_start < NOW() - INTERVAL '10 seconds'
ORDER BY duration_secs DESC;
EOF

# Check for idle-in-transaction (locks held, connection never releases)
psql $DATABASE_URL << 'EOF'
SELECT pid, usename, state, state_change, query
FROM pg_stat_activity
WHERE state = 'idle in transaction'
ORDER BY state_change;
EOF
```

### 3. FraiseQL Connection Pool Configuration

```bash
# Check current pool configuration
env | grep -E "DB_POOL|FRAISEQL.*POOL"

# View configuration in compiled schema (if present)
jq '.database.connection_pool' /etc/fraiseql/schema.compiled.json

# Check pool timeout settings
env | grep -E "POOL_TIMEOUT|POOL_IDLE"

# Typical defaults:
# DB_POOL_MIN_CONNECTIONS=5
# DB_POOL_MAX_CONNECTIONS=20
# DB_POOL_IDLE_TIMEOUT=300 seconds
# DB_POOL_CONNECTION_TIMEOUT=30 seconds
```

### 4. Request Queue Depth

```bash
# Check how many requests are waiting for connections
curl -s http://localhost:8815/metrics | grep "connection_pool_queue\|waiting"

# Check total active requests
curl -s http://localhost:8815/metrics | grep "requests_total"

# Calculate queue depth
# If requests_in_flight > connections_active, requests are queued

ACTIVE_CONNS=$(curl -s http://localhost:8815/metrics | grep "db_pool_connections_active" | awk '{print $NF}')
REQUEST_TOTAL=$(curl -s http://localhost:8815/metrics | grep "requests_total\[^a-z\]" | head -1 | awk '{print $NF}')
echo "Active connections: $ACTIVE_CONNS"
echo "Requests in last period: $REQUEST_TOTAL"
```

### 5. Query Execution Time Distribution

```bash
# Check if queries are fast but there are too many
curl -s http://localhost:8815/metrics | grep "request_duration_seconds" | grep -E "p99|p95|p50"

# If p50 is very fast (< 100ms) but pool still exhausted:
# Problem is query volume, not query slowness

# Check queries per second
curl -s http://localhost:8815/metrics | grep "requests_total"
# And compare to pool size

# Formula: Max RPS = (pool_size / avg_query_duration_secs)
# Example: 20 connections * 10 req/sec per connection = 200 max RPS
```

### 6. Check for Connection Leaks

```bash
# Monitor connection count over time
echo "Tracking connection count..."
for i in {1..10}; do
    ACTIVE=$(curl -s http://localhost:8815/metrics | grep "db_pool_connections_active" | awk '{print $NF}')
    IDLE=$(curl -s http://localhost:8815/metrics | grep "db_pool_connections_idle" | awk '{print $NF}')
    echo "$(date '+%H:%M:%S') active=$ACTIVE idle=$IDLE"
    sleep 10
done

# If connections keep growing and not being released: LEAK
# If activity matches request volume: Normal behavior under load
```

## Mitigation

### Immediate (< 2 minutes)

1. **Kill long-running queries holding connections**
   ```bash
   # Find and kill queries that have run > 5 minutes
   psql $DATABASE_URL << 'EOF'
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE query_start < NOW() - INTERVAL '5 minutes'
   AND pid <> pg_backend_pid();
   EOF

   # Verify connections released
   psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity;"
   ```

2. **Clear idle-in-transaction connections** (highest priority!)
   ```bash
   # These are transaction zombies holding locks
   psql $DATABASE_URL << 'EOF'
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE state = 'idle in transaction';
   EOF

   # Verify they're gone
   curl -s http://localhost:8815/metrics | grep "db_pool_connections"
   ```

3. **Restart FraiseQL to reset connection pool**
   ```bash
   # Force new connections (clears any stuck ones)
   docker restart fraiseql-server

   # Wait for startup
   sleep 5

   # Verify pool reset
   curl -s http://localhost:8815/metrics | grep "db_pool_connections"
   # Should show: active=0 or very low, idle=pool_size
   ```

### Short-term (5-30 minutes)

4. **Increase pool size temporarily**
   ```bash
   # Scale up pool to handle load
   export DB_POOL_MAX_CONNECTIONS=50  # from default 20
   export DB_POOL_MIN_CONNECTIONS=10

   docker restart fraiseql-server
   sleep 3

   # Verify new pool size
   curl -s http://localhost:8815/metrics | grep "db_pool_connections_max"
   ```

5. **Reduce connection idle timeout** (faster reaping)
   ```bash
   # Return connections to pool more aggressively
   export DB_POOL_IDLE_TIMEOUT=60  # from default 300 (5 min)

   docker restart fraiseql-server
   ```

6. **Enable query timeout**
   ```bash
   # Kill any query that takes > 30 seconds
   export FRAISEQL_QUERY_TIMEOUT=30000  # milliseconds

   docker restart fraiseql-server
   ```

7. **Implement connection pooler proxy** (if needed)
   ```bash
   # Use PgBouncer to multiplex connections
   # PgBouncer allows N client connections to share M database connections

   docker run -d \
     --name pgbouncer \
     --restart unless-stopped \
     -p 5433:5433 \
     -e DATABASES_HOST="postgres" \
     -e DATABASES_PORT="5432" \
     -e DATABASES_USER="$DB_USER" \
     -e DATABASES_PASSWORD="$DB_PASSWORD" \
     -e DATABASES_DBNAME="$DB_NAME" \
     edoburu/pgbouncer:latest

   # Update FraiseQL to connect through pgbouncer
   export DATABASE_URL="postgresql://user:pass@pgbouncer:5433/dbname"
   docker restart fraiseql-server
   ```

## Resolution

### Diagnose Root Cause

```bash
#!/bin/bash
set -e

echo "=== Connection Pool Exhaustion Analysis ==="

# 1. Check pool state
echo "1. Current pool state:"
curl -s http://localhost:8815/metrics | grep "db_pool_connections"
echo ""

# 2. Find what's using connections
echo "2. Active queries:"
psql $DATABASE_URL << 'EOF'
SELECT pid, usename, state, EXTRACT(EPOCH FROM (NOW() - query_start)) as duration_secs, query
FROM pg_stat_activity
WHERE datname = current_database()
ORDER BY query_start
LIMIT 10;
EOF
echo ""

# 3. Check for idle-in-transaction
echo "3. Idle-in-transaction connections (highest priority):"
psql $DATABASE_URL << 'EOF'
SELECT count(*), min(state_change) FROM pg_stat_activity
WHERE state = 'idle in transaction';
EOF
echo ""

# 4. Analyze query duration distribution
echo "4. Query duration histogram:"
curl -s http://localhost:8815/metrics | grep "request_duration_seconds_bucket" | head -10
echo ""

# 5. Calculate capacity
MAX_CONN=$(curl -s http://localhost:8815/metrics | grep "db_pool_connections_max" | awk '{print $NF}')
AVG_DURATION=$(curl -s http://localhost:8815/metrics | grep "request_duration_seconds_sum\[^a-z\]" | head -1 | awk '{print $NF}' | cut -d'.' -f1)
echo "5. Pool capacity analysis:"
echo "   Max connections: $MAX_CONN"
echo "   Avg query duration: ${AVG_DURATION}s"
if [ -z "$AVG_DURATION" ] || [ "$AVG_DURATION" = "0" ]; then
    echo "   Cannot calculate - insufficient metrics data"
else
    MAX_RPS=$(echo "scale=2; $MAX_CONN / $AVG_DURATION" | bc)
    echo "   Max sustainable RPS: $MAX_RPS"
fi
```

### Fix 1: Too Many Long-Running Queries

```bash
# Problem: Queries take too long, connections held for extended time

# 1. Identify slow queries
psql $DATABASE_URL << 'EOF'
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;
EOF

# 2. Optimize slow queries (add indexes, rewrite SQL)
# See runbook 03 (High Latency) for detailed optimization

# 3. Or increase pool size to accommodate slower queries
export DB_POOL_MAX_CONNECTIONS=50
docker restart fraiseql-server
```

### Fix 2: Connection Leak (Not Releasing)

```bash
# Problem: Connections held indefinitely (leak)

# 1. Monitor connection growth
echo "Monitoring for leak (run with active traffic):"
for i in {1..20}; do
    ACTIVE=$(psql $DATABASE_URL -t -c "SELECT count(*) FROM pg_stat_activity;")
    echo "$(date '+%H:%M:%S') Connections: $ACTIVE"
    sleep 10
done

# 2. If connections always growing: LEAK
# Check application code:
# - Are connections being released in error paths?
# - Are transactions being rolled back?
# - Are connection close being deferred?

# 3. Temporary: Reduce connection timeout
# So stuck connections are killed
export DB_POOL_IDLE_TIMEOUT=30
docker restart fraiseql-server

# 4. Or restart periodically
# Use cron to restart FraiseQL hourly during this debugging
```

### Fix 3: Too Much Concurrency (High Request Volume)

```bash
# Problem: Load is legitimate but pool too small

# 1. Increase pool size
export DB_POOL_MAX_CONNECTIONS=100  # Significant increase

docker restart fraiseql-server

# 2. Or implement caching to reduce database load
export FRAISEQL_QUERY_CACHE_ENABLED=true
export FRAISEQL_QUERY_CACHE_TTL=300

docker restart fraiseql-server

# 3. Or implement request rate limiting
export FRAISEQL_RATE_LIMIT_ENABLED=true
export FRAISEQL_RATE_LIMIT_AUTH_MAX_REQUESTS=1000
export FRAISEQL_RATE_LIMIT_WINDOW_SECS=60

docker restart fraiseql-server

# 4. Or scale horizontally (multiple FraiseQL instances)
# Each instance gets its own connection pool
# Load balance across instances
```

## Prevention

### Monitoring and Alerting

```bash
# Prometheus alerts for connection pool
cat > /etc/prometheus/rules/fraiseql-pool.yml << 'EOF'
groups:
  - name: fraiseql_pool
    rules:
      - alert: ConnectionPoolExhausted
        expr: db_pool_connections_active == db_pool_connections_max
        for: 2m
        action: page

      - alert: HighPoolUtilization
        expr: |
          (db_pool_connections_active / db_pool_connections_max) > 0.8
        for: 5m
        action: notify

      - alert: IdleInTransaction
        expr: pg_stat_activity{state="idle in transaction"} > 0
        for: 5m
        action: notify
EOF

# Grafana dashboard for pool monitoring:
# - Active connections (gauge)
# - Idle connections (gauge)
# - Pool utilization % (gauge)
# - Connection wait time (histogram)
```

### Best Practices

```bash
# 1. Right-size pool based on load
# Pool size = (peak_rps * avg_query_duration) * 1.2 safety factor
# Example: (100 rps * 0.2s) * 1.2 = 24 connections

# 2. Kill idle-in-transaction connections
# Add to application code:
# - Always close transactions explicitly
# - Use connection context managers
# - Never hold connections across awaits/yields

# 3. Set reasonable query timeouts
export FRAISEQL_QUERY_TIMEOUT=30000  # 30 seconds max

# 4. Monitor queue depth
# Prometheus: rate(connection_wait_total[5m])

# 5. Use read replicas for read-heavy workloads
# Distribute load across multiple database instances

# 6. Cache frequently accessed data
# Reduce database load and connection requirements
```

### Configuration Tuning

```bash
# Development
DB_POOL_MIN=2
DB_POOL_MAX=10
DB_POOL_IDLE_TIMEOUT=60

# Staging (similar to production)
DB_POOL_MIN=5
DB_POOL_MAX=30
DB_POOL_IDLE_TIMEOUT=300

# Production (high volume)
DB_POOL_MIN=10
DB_POOL_MAX=100
DB_POOL_IDLE_TIMEOUT=300
DB_POOL_TIMEOUT=30

# Very high volume (with caching)
DB_POOL_MIN=20
DB_POOL_MAX=200
DB_POOL_IDLE_TIMEOUT=120
# + query caching enabled
# + Redis for distributed caching
```

### Regular Maintenance

```bash
# Weekly: Check for idle-in-transaction connections
psql $DATABASE_URL << 'EOF'
SELECT count(*) FROM pg_stat_activity WHERE state = 'idle in transaction';
EOF

# Weekly: Review connection pool metrics
curl -s http://localhost:8815/metrics | grep "db_pool"

# Monthly: Analyze connection usage patterns
# Resize pool if consistently hitting >80% utilization
```

## Escalation

- **Slow queries causing pool exhaustion**: Database team (optimize queries)
- **Connection leak in application**: Application team (fix leak)
- **Database server issue**: Database team / SRE
- **Load too high for current pool size**: Infrastructure team (scale)
- **PostgreSQL configuration issue**: Database admin
