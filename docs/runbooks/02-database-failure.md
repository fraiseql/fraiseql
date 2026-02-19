# Runbook: Database Failure (PostgreSQL Down/Degraded)

## Symptoms

- `database connection refused` errors in FraiseQL logs
- All GraphQL queries fail with database error messages
- High error rate (>50% of requests failing)
- Timeout errors connecting to PostgreSQL
- Connection pool is exhausted or shows no idle connections
- Slow queries reported in `slow_query_log` metrics
- Replication lag (if using streaming replication)

## Impact

- **Critical**: All GraphQL queries fail (service unavailable)
- Compiled schema can be served (no DB required)
- Authentication and rate limiting may still work if using Redis
- Webhooks cannot fire (data not available)
- Real-time subscriptions hang

## Investigation

### 1. PostgreSQL Connectivity

```bash
# Test basic PostgreSQL connectivity
psql $DATABASE_URL -c "SELECT now();" 2>&1 | head -20

# Get connection details from DATABASE_URL
# Format: postgresql://user:password@host:port/database
echo "Parsed connection string:"
echo "$DATABASE_URL" | grep -oE "://(.*?)@" | head -1

# Check if host is reachable
HOST=$(echo "$DATABASE_URL" | grep -oE "@[^/:]*" | cut -c2-)
PORT=$(echo "$DATABASE_URL" | grep -oE ":[0-9]+/" | grep -oE "[0-9]+")
echo "Testing connectivity to $HOST:$PORT..."
nc -zv $HOST $PORT 2>&1 || telnet $HOST $PORT

# Check DNS resolution
nslookup $HOST || host $HOST

# Check routing
traceroute $HOST
```

### 2. FraiseQL Connection Pool Status

```bash
# Check metrics for connection pool state
curl -s http://localhost:8815/metrics | grep -A 5 "db_pool"

# Expected output shows:
# db_pool_connections_active{} N
# db_pool_connections_idle{} M
# db_pool_connections_max{} K

# If active+idle < max, connections are being held
# If active == max, pool is exhausted (see runbook 07)

# Check recent error logs
docker logs fraiseql-server | grep -i "connection\|database\|pool" | tail -20
```

### 3. PostgreSQL Server Status

```bash
# If you have access to PostgreSQL host:
ssh postgres-host

# Check if PostgreSQL is running
systemctl status postgresql

# Check PostgreSQL logs
tail -50 /var/log/postgresql/postgresql.log

# Check if accepting connections
pg_isready -h $HOST -p $PORT -U $USER

# If running in Docker:
docker ps | grep postgres
docker logs postgres-container --tail 50
```

### 4. Database Integrity

```bash
# If able to connect, check database status:
psql $DATABASE_URL << 'EOF'
-- Check replication lag (if applicable)
SELECT EXTRACT(EPOCH FROM (NOW() - pg_last_wal_receive_lsn())) AS replication_lag_seconds;

-- Check for long-running transactions
SELECT pid, xact_start, query FROM pg_stat_activity
WHERE state = 'active' AND query_start < NOW() - INTERVAL '5 minutes'
ORDER BY xact_start;

-- Check for table bloat
SELECT schemaname, tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
FROM pg_tables
WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 10;

-- Check disk space
SELECT pg_size_pretty(pg_database_size(current_database())) as db_size;

-- Check active connections
SELECT datname, count(*) FROM pg_stat_activity GROUP BY datname;
EOF
```

### 5. Network and Firewall

```bash
# Check network interfaces
ip addr show

# Check firewall rules (if applicable)
sudo ufw status
sudo iptables -L -n | grep -E "5432|port"

# Check if port 5432 is listening
sudo netstat -tulpn | grep 5432 || sudo ss -tulpn | grep 5432

# Check DNS resolution for database host
getent hosts postgres.example.com

# Check routes
ip route
```

## Mitigation

### Immediate Actions (< 2 minutes)

1. **Attempt FraiseQL restart** (often recovers from transient issues)
   ```bash
   docker restart fraiseql-server
   sleep 5
   curl http://localhost:8815/health
   ```

2. **Check database is actually down**
   ```bash
   # Try connecting directly from another container/host
   docker run --rm postgres:15 psql "$DATABASE_URL" -c "SELECT 1"
   ```

3. **Enable read-only mode or graceful degradation** (if supported)
   ```bash
   # Set environment variable to enable cached responses only
   export FRAISEQL_FALLBACK_MODE=cache_only
   docker restart fraiseql-server
   ```

4. **Isolate the problem** - Is it PostgreSQL or just FraiseQL?
   ```bash
   # From a different host/container
   psql postgresql://user:pass@other-host:5432/other_db -c "SELECT 1"
   ```

### For PostgreSQL Connection Refused

```bash
# 1. Check if PostgreSQL is actually running
docker ps | grep postgres

# 2. If container exists, restart it
docker restart postgres
docker logs postgres --tail 20

# 3. If container doesn't exist, check backup
docker ps -a | grep postgres

# 4. Check PostgreSQL port is being exposed
docker port postgres | grep 5432

# 5. If using cloud PostgreSQL (AWS RDS, Azure, GCP):
#    - Check security group rules allow FraiseQL server's IP
#    - Check database user exists and has correct password
#    - Check connection limit not exceeded (RDS has max connections)
```

### For Slow Queries

```bash
# 1. Identify slow queries
psql $DATABASE_URL << 'EOF'
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;
EOF

# 2. Check for missing indexes
psql $DATABASE_URL << 'EOF'
-- Find sequential scans on large tables
SELECT schemaname, tablename, seq_scan, idx_scan
FROM pg_stat_user_tables
WHERE seq_scan > idx_scan AND seq_scan > 1000
ORDER BY seq_scan DESC;
EOF

# 3. Kill long-running queries causing blocking
psql $DATABASE_URL << 'EOF'
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE pid <> pg_backend_pid()
AND query_start < NOW() - INTERVAL '30 minutes';
EOF

# 4. Vacuum and analyze if database was under write load
psql $DATABASE_URL << 'EOF'
VACUUM ANALYZE;
EOF
```

### For Replication Lag (if applicable)

```bash
# Check replication status on primary
psql primary_database << 'EOF'
SELECT client_addr, state, write_lag, flush_lag, replay_lag
FROM pg_stat_replication;
EOF

# If lag is significant, check network between primary and replica
# Increase TCP buffer sizes if needed
sysctl -w net.core.rmem_max=134217728
sysctl -w net.core.wmem_max=134217728
```

## Resolution

### Step-by-Step Recovery

```bash
#!/bin/bash
set -e

echo "=== PostgreSQL Failure Recovery ==="

# 1. Confirm the database is down
echo "1. Testing database connectivity..."
if ! psql "$DATABASE_URL" -c "SELECT 1" 2>/dev/null; then
    echo "   ✗ Database is down or unreachable"
else
    echo "   ✓ Database is reachable - this may be a connection pool issue"
    echo "   See runbook 07 (Connection Pool Exhaustion)"
    exit 0
fi

# 2. Check if PostgreSQL service is running (if self-hosted)
echo "2. Checking PostgreSQL service..."
if systemctl is-active --quiet postgresql; then
    echo "   ✓ PostgreSQL service is running"
else
    echo "   ✗ PostgreSQL service is not running - starting..."
    systemctl start postgresql
    sleep 10
fi

# 3. Check PostgreSQL logs for errors
echo "3. Checking PostgreSQL logs..."
tail -20 /var/log/postgresql/postgresql.log | grep -i "error\|fatal" | head -5

# 4. Test connectivity again
echo "4. Testing connectivity..."
if psql "$DATABASE_URL" -c "SELECT now()" > /dev/null; then
    echo "   ✓ Database is now accessible"
else
    echo "   ✗ Database still unreachable - escalating"
    exit 1
fi

# 5. Check replication lag (if applicable)
echo "5. Checking replication status..."
REPLICATION_LAG=$(psql "$DATABASE_URL" -t -c "SELECT EXTRACT(EPOCH FROM (NOW() - pg_last_wal_receive_lsn()))::int")
if [ "$REPLICATION_LAG" -gt 300 ]; then
    echo "   ⚠ WARNING: Replication lag is ${REPLICATION_LAG}s (>5 min)"
else
    echo "   ✓ Replication lag is acceptable (${REPLICATION_LAG}s)"
fi

# 6. Restart FraiseQL to reconnect
echo "6. Restarting FraiseQL..."
docker restart fraiseql-server
sleep 5

# 7. Verify FraiseQL recovery
echo "7. Verifying FraiseQL..."
if curl -s http://localhost:8815/health | jq -e '.status == "healthy"' > /dev/null; then
    echo "   ✓ FraiseQL recovered successfully"
    exit 0
else
    echo "   ✗ FraiseQL health check failed"
    exit 1
fi
```

### Full Database Failover (if using replication)

```bash
# 1. Confirm primary is down
psql primary_connection_string -c "SELECT 1" || echo "Primary confirmed down"

# 2. Promote replica to primary
ssh replica_host
sudo -u postgres /usr/lib/postgresql/15/bin/pg_ctl promote -D /var/lib/postgresql/15/main

# 3. Wait for promotion to complete
sleep 10
pg_isready -h localhost

# 4. Update FraiseQL connection string to replica
# Edit fraiseql-server environment or config
export DATABASE_URL="postgresql://user:pass@replica_host:5432/database"
docker restart fraiseql-server

# 5. Verify
curl http://localhost:8815/health
```

## Prevention

### Database Reliability Measures

- **Monitoring**: Set up alerts for connection failures, high latency, replication lag
  ```bash
  # Prometheus alert rule example
  - alert: PostgreSQLDown
    expr: postgres_up == 0
    for: 1m
    action: page
  ```

- **Connection pool tuning**:
  ```bash
  # Review in fraiseql config
  db_pool_min_connections=5
  db_pool_max_connections=20
  db_pool_timeout_seconds=30
  ```

- **Regular backups**:
  ```bash
  # Daily automated backup
  pg_dump "$DATABASE_URL" > /backups/database_$(date +%Y%m%d).sql
  ```

- **Replication setup** (for HA):
  ```bash
  # Configure streaming replication with hot standby
  # See PostgreSQL documentation
  ```

- **Connection string redundancy**:
  ```bash
  # Use multiple hosts in connection string
  DATABASE_URL="postgresql://user:pass@primary:5432,replica:5432/database?load_balance_hosts=true"
  ```

### Preventive Checks

```bash
# Weekly: Check database size growth
psql $DATABASE_URL -c "SELECT pg_database.datname, pg_size_pretty(pg_database_size(pg_database.datname)) FROM pg_database WHERE datname = current_database();"

# Weekly: Reindex and vacuum
psql $DATABASE_URL << 'EOF'
REINDEX DATABASE current_database();
VACUUM ANALYZE;
EOF

# Monthly: Check for table bloat
psql $DATABASE_URL << 'EOF'
SELECT schemaname, tablename, round(100*live_tuples/(live_tuples+dead_tuples)) AS ratio
FROM pg_stat_user_tables
WHERE (live_tuples+dead_tuples) > 0
AND round(100*dead_tuples/(live_tuples+dead_tuples)) > 20
ORDER BY dead_tuples DESC;
EOF
```

## Escalation

- **PostgreSQL service down** → Database team / SRE
- **Network connectivity issues** → Infrastructure / Network team
- **Cloud provider issues (RDS, Azure, GCP)** → Cloud platform team
- **Replication lag** → Database admin
- **Disk space issues** → Storage / Infrastructure team
- **Unrecoverable data loss** → Incident commander + Backup recovery team
