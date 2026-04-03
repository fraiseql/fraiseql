# Troubleshooting Guide

Common operational issues and their diagnostic steps.

## Connection Refused on Startup

**Symptom**: Server exits with `Database connection failed` or `ConnectionRefused`.

**Causes**:

1. Database not running
2. Wrong `database_url` in config
3. Firewall/network rules blocking the port

**Steps**:
```bash
# 1. Verify the database is reachable
pg_isready -h localhost -p 5432

# 2. Check the configured URL
echo $FRAISEQL_DATABASE_URL
# or inspect fraiseql.toml: database_url = "..."

# 3. Test connectivity manually
psql "$FRAISEQL_DATABASE_URL" -c "SELECT 1"
```

## Auth Token Expired / JWT Validation Failures

**Symptom**: All requests return 401 after a period of working correctly.

**Causes**:

1. OIDC provider rotated signing keys (JWKS)
2. Clock skew between server and OIDC provider
3. Token audience mismatch after config change

**Steps**:
```bash
# 1. Check server logs for specific JWT error
journalctl -u fraiseql | grep -i "jwt\|token\|auth" | tail -20

# 2. Verify JWKS endpoint is reachable
curl -s https://your-provider/.well-known/openid-configuration | jq .jwks_uri

# 3. Check clock skew
date -u  # Compare with provider's server time

# 4. Force JWKS cache refresh (if using admin API)
curl -X POST http://localhost:8000/admin/reload-schema \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

## High Cache Miss Rate

**Symptom**: `fraiseql_cache_hit_ratio` metric below 0.5.

**Causes**:

1. Cache TTL too short for the query pattern
2. Frequent mutations invalidating cache entries
3. High cardinality in WHERE clauses (each unique filter = unique cache key)

**Steps**:
```bash
# 1. Check cache stats via admin API
curl http://localhost:8000/admin/cache/stats \
  -H "Authorization: Bearer $ADMIN_TOKEN" | jq

# 2. Look at per-view TTL overrides in fraiseql.toml
grep -A5 "cache" fraiseql.toml

# 3. Check mutation frequency
journalctl -u fraiseql | grep "mutation.*invalidat" | wc -l
```

**Fixes**:

- Increase `cache_ttl_secs` for rarely-mutated views
- Use entity-aware invalidation (mutations with `entity_id` only evict matching entries)
- Consider `invalidates_views` on mutation definitions to limit blast radius

## Observer Backpressure / Dropped Events

**Symptom**: `fraiseql_observer_backpressure_total` metric increasing.

**Causes**:

1. Observer action (webhook, notification) is slow or timing out
2. Too many events for the configured channel buffer size
3. Database LISTEN connection dropped

**Steps**:
```bash
# 1. Check observer error counts
curl http://localhost:8000/metrics | grep observer_error

# 2. Look for OB-codes in logs
journalctl -u fraiseql | grep "OB0" | tail -20
# OB007 = channel backpressure
# OB009 = retry exhaustion
# OB011 = DLQ overflow

# 3. Verify LISTEN connection is alive
psql "$FRAISEQL_DATABASE_URL" -c "SELECT * FROM pg_stat_activity WHERE query LIKE 'LISTEN%'"
```

**Fixes**:

- Increase `observer_buffer_size` in config
- Add retry configuration with exponential backoff
- Check network connectivity to webhook endpoints

## Pool Exhaustion

**Symptom**: Requests timing out with `ConnectionPool` errors.

**Steps**:
```bash
# 1. Check current pool metrics
curl http://localhost:8000/metrics | grep pool

# 2. Look at active connections in PostgreSQL
psql "$FRAISEQL_DATABASE_URL" -c \
  "SELECT count(*), state FROM pg_stat_activity WHERE datname = current_database() GROUP BY state"

# 3. Check for long-running queries
psql "$FRAISEQL_DATABASE_URL" -c \
  "SELECT pid, now() - query_start AS duration, query FROM pg_stat_activity WHERE state = 'active' ORDER BY duration DESC LIMIT 5"
```

**Fixes**:

- Increase `pool_max_size` (and restart)
- Enable `pool_tuning` for monitoring recommendations
- Set `request_timeout_secs` to kill runaway queries
- Check for N+1 query patterns in your schema

## Rate Limiting Unexpectedly Triggered

**Symptom**: Legitimate requests receiving 429 Too Many Requests.

**Steps**:
```bash
# 1. Check rate limit config
grep -A10 "rate_limiting" fraiseql.toml

# 2. Look at per-IP counters in logs
journalctl -u fraiseql | grep "rate.limit" | tail -10

# 3. Check if behind a reverse proxy (all requests may appear from one IP)
# Ensure X-Forwarded-For is configured and trusted
```

**Fixes**:

- Increase `rps_per_ip` or `rps_per_user`
- Configure trusted proxy headers so real client IPs are used
- Use per-user limits (requires auth) instead of per-IP
