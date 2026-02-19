# Runbook: Memory Pressure / Out of Memory (OOM)

## Symptoms

- `SIGKILL` or `OOMKiller` messages in `dmesg` or container logs
- FraiseQL container crashes with exit code 137 (OOM) or 139 (SEGV)
- Memory usage steadily increasing: `docker stats` shows memory > threshold
- System swapping heavily (high `swap_in` / `swap_out` in `vmstat`)
- Slow performance even with adequate CPU (swapping to disk)
- Application becomes unresponsive (OOM killer pausing processes)

## Impact

- **Critical**: Service becomes unavailable when OOM killer triggers
- Container restart needed to recover
- Data loss if in-flight requests are killed
- Memory pressure causes other processes to slow down
- Potential memory leak in application code

## Investigation

### 1. Current Memory Usage

```bash
# Check FraiseQL server memory
docker stats fraiseql-server --no-stream

# Expected output shows MEMORY usage and %MEM
# Example: fraiseql-server  1.2G / 4G (30%)

# Check memory trend
watch -n 2 'docker stats fraiseql-server --no-stream'

# Check system-wide memory
free -h
# Expected: Free memory should be > 20% available

# Check swap usage
swapon -s
vmstat 1 5  # Shows memory and swap in/out columns
```

### 2. Identify Memory Leak

```bash
# Check if memory is growing over time (leak indicator)
echo "Memory usage over last hour:"
for i in {1..60}; do
    MEM=$(docker stats fraiseql-server --no-stream --format "{{.MemUsage}}" | cut -d'/' -f1 | cut -dM -f1)
    echo "$(date): $MEM MB"
    sleep 60
done

# Analyze trends
docker stats fraiseql-server --no-stream --format "table {{.Container}}\t{{.MemUsage}}"

# Check for specific memory consumers
top -b -n 1 | head -15  # Shows top processes by memory
ps aux | sort -k4 -rn | head -10  # Sort by %MEM column
```

### 3. Application-Level Memory

```bash
# Check metrics for in-memory caches, buffers
curl -s http://localhost:8815/metrics | grep -E "memory|cache|buffer" | head -20

# Common memory sinks in FraiseQL:
# - Query result caches
# - Connection pool buffers
# - Parsed schema in memory
# - JWT token cache
# - Rate limit counters (if using in-memory instead of Redis)

# Check if caching is enabled and consuming memory
docker logs fraiseql-server | grep -i "cache" | tail -20

# Check connection pool size (each connection uses ~1-5 MB)
curl -s http://localhost:8815/metrics | grep "db_pool_connections" | sort
```

### 4. Docker Memory Limits

```bash
# Check current memory limits
docker inspect fraiseql-server | jq '.HostConfig | {Memory, MemorySwap, MemoryReservation}'

# Check if container hit limits
docker stats fraiseql-server --no-stream | grep fraiseql

# View memory limit events
docker events --filter "type=container" --filter "container=fraiseql-server" --format '{{.Action}} {{.Actor.Attributes.name}} {{.Type}}'
```

### 5. Database Connection Pool

```bash
# Each PostgreSQL connection holds memory (typically 5-10 MB)
# Check active connections

curl -s http://localhost:8815/metrics | grep "db_pool_connections_active"

# Check how many connections are in use
psql $DATABASE_URL << 'EOF'
SELECT datname, count(*) as connections,
       max(state_change) as last_change
FROM pg_stat_activity
GROUP BY datname;
EOF

# Identify idle connections
psql $DATABASE_URL << 'EOF'
SELECT pid, usename, state, state_change, query_start
FROM pg_stat_activity
WHERE state = 'idle'
AND state_change < NOW() - INTERVAL '5 minutes';
EOF
```

### 6. Check for Container Limits vs Actual Usage

```bash
# Get container memory limit
LIMIT=$(docker inspect fraiseql-server | jq -r '.HostConfig.Memory')
echo "Memory limit: $((LIMIT / 1024 / 1024)) MB"

# Get current usage
USAGE=$(docker stats fraiseql-server --no-stream --format "{{.MemUsage}}" | cut -d'/' -f1 | tr -d 'MiB')
echo "Current usage: $USAGE MB"

# Calculate percentage
PCT=$((USAGE * 100 / (LIMIT / 1024 / 1024)))
echo "Usage: $PCT%"

# If usage > 85%, risk of OOM is high
if [ $PCT -gt 85 ]; then
    echo "⚠ WARNING: Memory usage is critical (${PCT}%)"
fi
```

## Mitigation

### Immediate Actions (< 2 minutes)

1. **Increase memory limit**
   ```bash
   # Stop the container
   docker stop fraiseql-server

   # Remove it (we'll restart with new limits)
   docker rm fraiseql-server

   # Restart with higher memory limit
   docker run -d \
     --name fraiseql-server \
     --memory="4g" \
     --memory-reservation="3g" \
     --restart unless-stopped \
     -p 8815:8815 \
     -p 9090:9090 \
     -e DATABASE_URL="$DATABASE_URL" \
     -e REDIS_URL="$REDIS_URL" \
     -e RUST_LOG=info \
     fraiseql:latest

   # Wait for startup
   sleep 5
   curl http://localhost:8815/health
   ```

2. **Kill idle database connections**
   ```bash
   # Frees up memory held by connection buffers
   psql $DATABASE_URL << 'EOF'
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE state = 'idle'
   AND state_change < NOW() - INTERVAL '5 minutes';
   EOF
   ```

3. **Clear in-memory caches** (if application supports it)
   ```bash
   # Some applications have cache clear endpoint
   curl -X POST http://localhost:8815/admin/cache/clear

   # Or restart to clear caches
   docker restart fraiseql-server
   sleep 5
   curl http://localhost:8815/health
   ```

4. **Reduce connection pool size temporarily**
   ```bash
   # Smaller pool = less memory, but lower concurrency
   export FRAISEQL_DB_POOL_MAX=10  # from default 20
   docker restart fraiseql-server
   ```

### Short-term (5-30 minutes)

5. **Enable request rate limiting to reduce load**
   ```bash
   # Use Redis for rate limiting instead of in-memory
   export REDIS_URL="redis://redis:6379"
   export FRAISEQL_RATE_LIMIT_BACKEND="redis"
   docker restart fraiseql-server
   ```

6. **Disable or reduce query caching**
   ```bash
   # If caching is consuming memory
   export FRAISEQL_QUERY_CACHE_ENABLED="false"
   # or
   export FRAISEQL_QUERY_CACHE_SIZE="1000"  # reduce from default
   docker restart fraiseql-server
   ```

7. **Enable swap space** (temporary measure while investigating)
   ```bash
   # Add swap space if not present
   sudo dd if=/dev/zero of=/swapfile bs=1G count=4
   sudo chmod 600 /swapfile
   sudo mkswap /swapfile
   sudo swapon /swapfile

   # Make permanent in /etc/fstab
   echo "/swapfile none swap sw 0 0" | sudo tee -a /etc/fstab
   ```

## Resolution

### Step-by-Step Memory Investigation

```bash
#!/bin/bash
set -e

echo "=== Memory Pressure Investigation ==="

# 1. Check current state
echo "1. Current memory state:"
free -h
docker stats fraiseql-server --no-stream
echo ""

# 2. Check memory limits
echo "2. Container memory configuration:"
docker inspect fraiseql-server | jq '.HostConfig | {Memory, MemorySwap, MemoryReservation}'
echo ""

# 3. Find large memory consumers
echo "3. Top memory processes:"
ps aux --sort=-%mem | head -10
echo ""

# 4. Check application metrics
echo "4. FraiseQL memory metrics:"
curl -s http://localhost:8815/metrics | grep -i "memory\|cache" | head -10
echo ""

# 5. Database connection count
echo "5. Database connections in memory:"
curl -s http://localhost:8815/metrics | grep "db_pool_connections"
echo ""

# 6. Identify memory leak pattern
echo "6. Checking for memory leak (trend)..."
MEM_BEFORE=$(docker stats fraiseql-server --no-stream --format "{{.MemUsage}}" | cut -d'/' -f1 | tr -d 'MiB')
echo "Memory before: ${MEM_BEFORE} MB"
sleep 60
MEM_AFTER=$(docker stats fraiseql-server --no-stream --format "{{.MemUsage}}" | cut -d'/' -f1 | tr -d 'MiB')
echo "Memory after:  ${MEM_AFTER} MB"
DIFF=$((MEM_AFTER - MEM_BEFORE))
echo "Difference: ${DIFF} MB in 60 seconds"

if [ $DIFF -gt 50 ]; then
    echo "⚠ WARNING: Memory increasing by ${DIFF}MB/min - likely leak"
    exit 1
else
    echo "✓ Memory stable"
fi
```

### Identifying Memory Leak Source

```bash
# 1. Check connection pool growth
echo "Monitoring connection pool over 5 minutes:"
for i in {1..5}; do
    echo "Minute $i:"
    curl -s http://localhost:8815/metrics | grep "db_pool_connections"
    sleep 60
done

# If connections growing: Pool leak (query not closing connections)
# Fix: Check that all queries properly close connections

# 2. Check cache growth
echo "Monitoring cache metrics:"
curl -s http://localhost:8815/metrics | grep "cache"

# If cache hits growing without eviction: Cache leak (items never evicted)
# Fix: Enable cache eviction or reduce TTL

# 3. Check for zombie processes/connections
psql $DATABASE_URL << 'EOF'
-- Find connections in bad state
SELECT pid, usename, state, state_change, query
FROM pg_stat_activity
WHERE state NOT IN ('active', 'idle')
OR query LIKE 'CLOSE%'
OR query IS NULL;
EOF

# If many CLOSE statements pending: Connection not closing properly
# Fix: Check connection cleanup code
```

### Memory-Optimized Configuration

```bash
# 1. Right-size container memory
# Recommended: RAM = (max_connections * 10MB) + 512MB overhead + buffer
# Example for 20 max connections: 20 * 10 = 200MB + 512MB = ~1GB minimum

docker update fraiseql-server --memory="2g" --memory-reservation="1.5g"

# 2. Optimize connection pool
# In fraiseql config or environment:
export FRAISEQL_DB_POOL_MIN=5
export FRAISEQL_DB_POOL_MAX=15  # Reduce if memory constrained
export FRAISEQL_DB_POOL_IDLE_TIMEOUT=300  # Reap idle connections

# 3. Optimize caching
export FRAISEQL_QUERY_CACHE_SIZE=5000  # Reduce if needed
export FRAISEQL_QUERY_CACHE_TTL=300

# 4. Use Redis for distributed caching/rate limiting
export REDIS_URL="redis://redis:6379"
export FRAISEQL_RATE_LIMIT_BACKEND="redis"
export FRAISEQL_CACHE_BACKEND="redis"

# 5. Restart with new config
docker restart fraiseql-server
```

### Permanent Fix Checklist

- [ ] Implement memory leak fix in application code
- [ ] Increase container memory limit (if temporary measure was applied)
- [ ] Add memory alerting at 70%, 85%, 95% thresholds
- [ ] Document final memory configuration
- [ ] Review connection pool sizing based on workload
- [ ] Enable Redis for caching/rate limiting (if applicable)
- [ ] Schedule periodic connection cleanup
- [ ] Test with production-like load

## Prevention

### Monitoring Setup

```bash
# Prometheus alerts for memory pressure
cat > /etc/prometheus/rules/fraiseql-memory.yml << 'EOF'
groups:
  - name: fraiseql_memory
    rules:
      - alert: HighMemoryUsage
        expr: |
          (container_memory_usage_bytes{name="fraiseql-server"} /
           container_spec_memory_limit_bytes{name="fraiseql-server"}) > 0.85
        for: 5m
        action: page

      - alert: CriticalMemoryUsage
        expr: |
          (container_memory_usage_bytes{name="fraiseql-server"} /
           container_spec_memory_limit_bytes{name="fraiseql-server"}) > 0.95
        for: 1m
        action: page

      - alert: MemoryLeakSuspected
        expr: |
          rate(container_memory_usage_bytes{name="fraiseql-server"}[5m]) > 0
        for: 30m
        action: notify
EOF
```

### Configuration Best Practices

```bash
# Recommended production configuration
docker run -d \
  --name fraiseql-server \
  --memory="2g" \
  --memory-reservation="1.5g" \
  --memory-swap="2g" \
  --restart unless-stopped \
  -p 8815:8815 \
  -p 9090:9090 \
  -e DATABASE_URL="$DATABASE_URL" \
  -e FRAISEQL_DB_POOL_MIN=5 \
  -e FRAISEQL_DB_POOL_MAX=15 \
  -e FRAISEQL_QUERY_CACHE_SIZE=10000 \
  -e FRAISEQL_RATE_LIMIT_BACKEND="redis" \
  -e REDIS_URL="redis://redis:6379" \
  fraiseql:latest
```

### Regular Maintenance

```bash
# Weekly: Check memory trend
docker stats fraiseql-server --no-stream > /var/log/fraiseql/memory_$(date +%Y%m%d).txt

# Monthly: Review memory usage pattern
tail -4 /var/log/fraiseql/memory_*.txt | tail -20

# When deploying: Load test to identify memory issues early
# Run load test with expected peak traffic for 30+ minutes
```

## Escalation

- **Memory leak in FraiseQL**: Application team (profile and fix code)
- **Poor memory management**: Performance team (optimize algorithms)
- **Insufficient host memory**: Infrastructure team (add RAM or scale out)
- **Database connection leak**: Database / Application team
- **Cache not evicting**: Application team (check cache configuration)
