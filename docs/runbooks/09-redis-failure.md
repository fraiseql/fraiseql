# Runbook: Redis Failure (PKCE Store / Observers Backend Down)

FraiseQL uses Redis for an **explicit, configured** set of subsystems: the PKCE login
state store (`[fraiseql.security.pkce] redis_url`, compiled into the schema), the
observers cache/dedup backend
(`FRAISEQL_REDIS_URL`), and the Redis-backed auth rate limiter (opt-in
`redis-rate-limiting` build feature). The GraphQL query result cache is **in-process,
not Redis**. A configured Redis backend is never silently downgraded to in-memory
(#898): a server restarted while its configured Redis is unreachable **refuses to
boot**, naming Redis in the error.

## Symptoms

- A restart fails fast with a boot error naming Redis / the PKCE store
- OAuth logins fail mid-flow (`state not found` on `/auth/callback`)
- Redis connection refused errors in logs
- `NOAUTH Authentication required` errors (Redis auth failure)
- Observer actions erroring on dedup/cache operations

## Impact

- **Medium** while the server keeps running; **High** if it restarts (it will refuse
  to boot until Redis is back or the Redis config is removed)
- In-flight OAuth logins fail
- Observer dedup/cache operations error loudly

## Investigation

### 1. Redis Connectivity

```bash
# Check Redis configuration
env | grep -i "redis"

# Parse Redis URL
echo "Redis URL: $REDIS_URL"
# Format: redis://[:password@]host:port[/db]

REDIS_HOST=$(echo "$REDIS_URL" | cut -d'@' -f2 | cut -d':' -f1)
REDIS_PORT=$(echo "$REDIS_URL" | cut -d':' -f3 | cut -d'/' -f1)
echo "Connecting to $REDIS_HOST:$REDIS_PORT"

# Test basic connectivity
redis-cli -h $REDIS_HOST -p $REDIS_PORT ping

# If connection fails, test with telnet
nc -zv $REDIS_HOST $REDIS_PORT

# Check DNS resolution
nslookup $REDIS_HOST || host $REDIS_HOST

# Check if port is open
sudo netstat -tulpn | grep $REDIS_PORT || sudo ss -tulpn | grep $REDIS_PORT
```

### 2. Redis Server Status

```bash
# If connected, check server status
redis-cli -u $REDIS_URL INFO server

# Get detailed info
redis-cli -u $REDIS_URL INFO all | head -30

# Check memory usage
redis-cli -u $REDIS_URL INFO memory

# Check connected clients
redis-cli -u $REDIS_URL CLIENT LIST | head -10

# Check key count
redis-cli -u $REDIS_URL DBSIZE

# Check if Redis is accepting connections
redis-cli -u $REDIS_URL PING
```

### 3. Redis Authentication

```bash
# If getting AUTH errors
redis-cli -h $REDIS_HOST -p $REDIS_PORT PING
# vs
redis-cli -u $REDIS_URL PING

# The URL includes password, direct command doesn't
# If failing, check password is correct

# Test with explicit password
REDIS_PASS=$(echo "$REDIS_URL" | grep -oP '(?<=:)[^@]*(?=@)' || echo "")
echo "Password present: $([ -n "$REDIS_PASS" ] && echo "yes" || echo "no")"

# Verify auth works
redis-cli -h $REDIS_HOST -p $REDIS_PORT -a "$REDIS_PASS" PING
```

### 4. FraiseQL Redis Configuration

```bash
# Which subsystems name Redis? (each is explicit — there is no global backend key)
jq '.security.pkce.redis_url' /etc/fraiseql/schema.compiled.json   # PKCE state store
env | grep -E "FRAISEQL_(REDIS|REQUIRE_REDIS)"                     # observers cache/dedup

# Redis error counters the server actually exports:
curl -s -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" http://localhost:8000/metrics \
  | grep -E "fraiseql_(apq|pkce|rate_limit)_redis_errors_total"
```

### 5. Redis Cluster Status (if applicable)

```bash
# If using Redis Cluster
redis-cli -u $REDIS_URL CLUSTER INFO

# Check cluster nodes
redis-cli -u $REDIS_URL CLUSTER NODES

# Check for failing nodes
redis-cli -u $REDIS_URL CLUSTER SLOTS

# If one node down, cluster may still work (depending on quorum)
# But performance/availability affected
```

### 6. FraiseQL Logs for Redis Errors

```bash
# Check recent Redis-related errors
docker logs fraiseql-server | grep -i "redis\|cache" | tail -50

# Search for specific error types
docker logs fraiseql-server | grep -E "connection refused|NOAUTH|WRONGTYPE" | head -10

# Check when errors started
docker logs fraiseql-server | grep -i "redis" | head -1
```

## Mitigation

### Immediate (< 2 minutes)

1. **Restart Redis**

   ```bash
   # Most straightforward fix
   docker restart redis

   # Wait for startup
   sleep 3

   # Verify it's responding
   redis-cli -u $REDIS_URL PING
   ```

2. **De-configure the Redis-backed subsystems temporarily** (if restart doesn't work
   quickly)

   There is no "fallback to memory" switch — a configured Redis backend that is
   unreachable refuses to boot. To run without Redis you must explicitly remove the
   configuration that names it:

   ```bash
   # PKCE state store: its config is COMPILED — remove `redis_url` from
   # [fraiseql.security.pkce] in the project fraiseql.toml, re-run
   # `fraiseql compile`, redeploy the compiled schema — and make sure the
   # enforcement flag is unset:
   unset FRAISEQL_REQUIRE_REDIS
   # ⚠ In-memory PKCE state is only safe on a SINGLE replica: behind a load
   # balancer, logins fail whenever /auth/start and /auth/callback hit
   # different replicas.

   # Observers cache/dedup:
   unset FRAISEQL_REDIS_URL

   docker restart fraiseql-server
   ```

3. **Check for persistent issues**

   ```bash
   # Is Redis really down?
   docker ps | grep redis

   # If container doesn't exist
   docker ps -a | grep redis

   # If it exited, check why
   docker logs redis | tail -30
   ```

### Short-term (5-30 minutes)

1. **Verify Redis password/auth** (if auth error)

   ```bash
   # Test with password
   REDIS_PASS=$(echo "$REDIS_URL" | grep -oP '(?<=:)[^@]*(?=@)')
   REDIS_HOST=$(echo "$REDIS_URL" | cut -d'@' -f2 | cut -d':' -f1)
   REDIS_PORT=$(echo "$REDIS_URL" | cut -d':' -f3 | cut -d'/' -f1)

   redis-cli -h $REDIS_HOST -p $REDIS_PORT -a "$REDIS_PASS" PING

   # If password wrong, update it
   # Option 1: Update FraiseQL environment
   export REDIS_URL="redis://:newpassword@$REDIS_HOST:$REDIS_PORT"

   # Option 2: Update Redis password
   redis-cli -h $REDIS_HOST -p $REDIS_PORT \
             CONFIG SET requirepass "newpassword"
   ```

2. **Check Redis memory/eviction**

   ```bash
   # If Redis full, it may be unresponsive
   redis-cli -u $REDIS_URL INFO memory

   # Check eviction policy
   redis-cli -u $REDIS_URL CONFIG GET maxmemory-policy

   # Manual eviction if needed (delete old data)
   redis-cli -u $REDIS_URL FLUSHDB  # WARNING: Deletes all cache!

   # Or target specific keys
   redis-cli -u $REDIS_URL KEYS "cache:*" | wc -l
   redis-cli -u $REDIS_URL EVAL 'return redis.call("del", unpack(redis.call("keys", ARGV[1])))' 0 'cache:*'
   ```

3. **Clear stale cache** (if cache causing issues)

   ```bash
   # Flush all cache keys (safe if using TTL)
   redis-cli -u $REDIS_URL EVAL \
     'return redis.call("del", unpack(redis.call("keys", ARGV[1])))' \
     0 'cache:*'

   # Or flush entire Redis
   redis-cli -u $REDIS_URL FLUSHALL  # Only if Redis ONLY used for FraiseQL cache
   ```

### Extended Outage (30+ minutes)

1. **Run without Redis, deliberately**

   Remove every piece of configuration that names Redis (see "De-configure the
   Redis-backed subsystems" above: the compiled `[fraiseql.security.pkce] redis_url`,
   `FRAISEQL_REDIS_URL`, `FRAISEQL_REQUIRE_REDIS`), then restart.

   ```bash
   docker restart fraiseql-server

   # Service works but:
   # - PKCE login state is per-instance: only safe with ONE replica
   # - Observer dedup/cache is unavailable
   # - Rate limiting was per-instance already (the request throttle is in-process);
   #   with N replicas the effective budget is N × the configured limit
   ```

2. **Provision new Redis instance**

   ```bash
   # If current Redis cannot be recovered quickly
   docker run -d \
     --name redis-new \
     --restart unless-stopped \
     -p 6380:6379 \
     -e REDIS_PASSWORD="newpass" \
     redis:7 redis-server --requirepass newpass

   # Update FraiseQL to use new instance
   export REDIS_URL="redis://:newpass@redis-new:6380"
   docker restart fraiseql-server

   # Verify
   curl -s http://localhost:8000/metrics | grep redis_connection
   ```

## Resolution

### Complete Redis Recovery Workflow

```bash
#!/bin/bash
set -e

echo "=== Redis Recovery ==="

REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
REDIS_HOST=$(echo "$REDIS_URL" | cut -d'@' -f2 | cut -d':' -f1 | cut -d'/' -f1)
REDIS_PORT=$(echo "$REDIS_URL" | cut -d':' -f3 | cut -d'/' -f1)

# 1. Test connectivity
echo "1. Testing connectivity to $REDIS_HOST:$REDIS_PORT..."
if redis-cli -u $REDIS_URL PING > /dev/null 2>&1; then
    echo "   ✓ Redis is responding"
else
    echo "   ✗ Redis is not responding"
    echo "   Checking if service is running..."
    docker ps | grep redis || echo "   Redis container not running"

    # Try to restart
    echo "   Attempting restart..."
    docker restart redis
    sleep 3

    # Check again
    if redis-cli -u $REDIS_URL PING > /dev/null 2>&1; then
        echo "   ✓ Redis recovered after restart"
    else
        echo "   ✗ Redis still not responding"
        exit 1
    fi
fi

# 2. Check server stats
echo ""
echo "2. Redis server status:"
redis-cli -u $REDIS_URL INFO server | head -10

# 3. Check memory usage
echo ""
echo "3. Redis memory usage:"
USED=$(redis-cli -u $REDIS_URL INFO memory | grep used_memory_human | cut -d':' -f2)
PEAK=$(redis-cli -u $REDIS_URL INFO memory | grep used_memory_peak_human | cut -d':' -f2)
echo "   Current: $USED"
echo "   Peak: $PEAK"

# 4. Check key count
echo ""
echo "4. Keys in Redis:"
KEY_COUNT=$(redis-cli -u $REDIS_URL DBSIZE | cut -d':' -f2)
echo "   Total keys: $KEY_COUNT"

# 5. Verify FraiseQL connectivity
echo ""
echo "5. Checking FraiseQL Redis connection:"
curl -s http://localhost:8000/metrics | grep "redis_connection" || echo "   (No redis_connection metrics)"

# 6. Check cache metrics
echo ""
echo "6. Cache metrics:"
curl -s http://localhost:8000/metrics | grep -E "cache_hits|cache_misses|cache_size" | head -5

# 7. Verify FraiseQL is using Redis
echo ""
echo "7. FraiseQL Redis usage:"
if docker logs fraiseql-server | grep -q "redis.*connected"; then
    echo "   ✓ FraiseQL connected to Redis"
else
    echo "   ? Cannot confirm FraiseQL connection to Redis"
fi

# 8. Summary
echo ""
echo "=== Recovery Summary ==="
if redis-cli -u $REDIS_URL PING | grep -q "PONG"; then
    echo "✓ Redis is operational"
    echo "  Monitor cache hit rate to ensure normal operation"
else
    echo "✗ Redis is still not responding"
    exit 1
fi
```

### Check for Corruption (if Redis keeps crashing)

```bash
# 1. Check if dump file is corrupted
ls -lah /var/lib/redis/dump.rdb
file /var/lib/redis/dump.rdb

# 2. Backup current dump
cp /var/lib/redis/dump.rdb /var/lib/redis/dump.rdb.backup

# 3. Start Redis without persistence (for testing)
docker run -d --name redis-test -p 6380:6379 redis:7 redis-server --save ""

# 4. If this works, dump.rdb was corrupted
# Remove it and restart
docker stop redis
rm /var/lib/redis/dump.rdb
docker restart redis

# 5. If test instance also crashes, Redis binary may be corrupted
# Upgrade Redis
docker pull redis:7
docker stop redis
docker rm redis
docker run -d --name redis --restart unless-stopped -p 6379:6379 redis:7
```

## Prevention

### Monitoring and Alerting

> **Illustrative alert rules** — verify metric names against what your build's
> `/metrics` endpoint actually exports (`curl -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" …/metrics`).
> Names below that FraiseQL does not export directly (e.g. request/auth failure
> counters) must be derived from your load balancer, access logs, or exporters.

```bash
# Prometheus alerts for Redis
cat > /etc/prometheus/rules/fraiseql-redis.yml << 'EOF'
groups:
  - name: fraiseql_redis
    rules:
      - alert: RedisDown
        expr: redis_up == 0
        for: 1m
        action: page

      - alert: RedisMemoryHigh
        expr: (redis_memory_used / redis_memory_max) > 0.85
        for: 5m
        action: notify

      - alert: RedisConnectionErrors
        expr: rate(redis_connection_errors_total[5m]) > 0
        for: 2m
        action: notify

      - alert: CacheMissRateHigh
        expr: |
          rate(cache_misses_total[5m]) /
          (rate(cache_hits_total[5m]) + rate(cache_misses_total[5m]))
          > 0.9
        for: 10m
        action: notify
EOF
```

### Best Practices

```bash
# 1. Configure Redis appropriately
# For caching: Less strict persistence, faster eviction
# For rate limiting: More durable, no eviction (bounded key set)

# 2. Use separate Redis instances if possible
# Cache instance: maxmemory-policy=allkeys-lru
# Rate limit instance: maxmemory-policy=noeviction

# 3. Set appropriate maxmemory
# Monitor memory usage and set limit 20% below available RAM
redis-cli -u $REDIS_URL CONFIG SET maxmemory "2gb"
redis-cli -u $REDIS_URL CONFIG SET maxmemory-policy "allkeys-lru"

# 4. Enable persistence
# RDB (snapshots): Good for cache (losable)
# AOF (append-only file): Better for critical data
redis-cli -u $REDIS_URL CONFIG SET appendonly "yes"

# 5. Monitor and maintain
# Periodic INFO dumps
# Monitor growth of key count
# Clean up expired keys
```

### Regular Maintenance

```bash
# Weekly: Check Redis memory and key count
redis-cli -u $REDIS_URL INFO memory | grep "used_memory_human"
redis-cli -u $REDIS_URL DBSIZE

# Weekly: Verify FraiseQL connectivity
curl -s http://localhost:8000/metrics | grep "redis_connection"

# Monthly: Analyze cache hit rate
curl -s http://localhost:8000/metrics | grep -E "cache_hits|cache_misses"

# Monthly: Check for stale keys
redis-cli -u $REDIS_URL RANDOMKEY  # Ensure keys still exist

# Quarterly: Upgrade Redis to latest patch version
docker pull redis:7
docker stop redis
docker rm redis
docker run -d --name redis --restart unless-stopped -p 6379:6379 redis:7
```

## Escalation

- **Redis service down**: Infrastructure / Database team
- **Network connectivity issues**: Network / Infrastructure team
- **Redis memory issues**: Application tuning or hardware upgrade
- **Authentication issues**: Platform / Security team
- **Cluster/Replication issues**: Redis administrator
- **Data corruption**: Redis administrator + incident response
