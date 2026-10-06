# Troubleshooting Guide - FraiseQL Observer System

## Quick Diagnosis

Start with this checklist to identify the issue:

```

1. System not processing events?
   → Check: Runtime health status
   → Run: GET /api/observers/runtime/health

2. Events processed but wrong actions executed?
   → Check: Condition evaluation
   → Run: GET /api/observers/logs?event_id=<id>

3. Dead Letter Queue growing?
   → Check: Action failures
   → Run: GET /api/observers/dlq?limit=20

4. System slow?
   → Check: Cache hit rate, latency metrics
   → Run: scrape /metrics (fraiseql_observer_*)

5. Event loss on restart?
   → Check: Checkpoint configuration
   → Verify: Database connectivity
```

The `/api/observers/*` routes are the `fraiseql-server` admin API; they need a token with
the `fraiseql:admin` scope (see [Operating Observers](../../../docs/operations/observers.md)). The examples below assume:

```bash
alias obs='curl -s -H "Authorization: Bearer $ADMIN_TOKEN"'
API=http://localhost:8000/api/observers
```

---

## Common Issues & Solutions

### Issue 1: "No Listener Running" Error

**Symptoms**:

```
Error: No listener is currently running
Status: Unable to process events
```

**Root Causes**:

1. Listener process crashed
2. Database connection failed
3. PostgreSQL LISTEN/NOTIFY not available
4. Permission issues

**Diagnostic Steps**:

```bash
# 1. Check runtime status
obs $API/runtime/health

# 2. Check the fraiseql-server logs

# 3. Verify database connectivity
psql postgresql://user:pass@localhost/fraiseql -c "SELECT version();"

# 4. Check database has NOTIFY capability
psql postgresql://user:pass@localhost/fraiseql -c "LISTEN test_channel; NOTIFY test_channel, 'test';"
```

**Solutions**:

#### If database connection failed

```bash
# Verify credentials
echo $DATABASE_URL

# Test connection
psql $DATABASE_URL -c "SELECT 1;"

# Restart fraiseql-server with the correct URL
DATABASE_URL="postgresql://user:pass@host:5432/db" fraiseql-server
```

#### If permission denied

```bash
# Grant needed permissions
psql postgresql://postgres:postgres@localhost/fraiseql << EOF
GRANT LISTEN ON DATABASE fraiseql TO observer_user;
GRANT ALL ON TABLE observer_checkpoints TO observer_user;
GRANT ALL ON SEQUENCE observer_checkpoints_id_seq TO observer_user;
EOF
```

#### If PostgreSQL LISTEN/NOTIFY not working

```bash
# Check if extension is available
psql $DATABASE_URL -c "SELECT * FROM pg_available_extensions WHERE name = 'uuid-ossp';"

# If missing, install
psql $DATABASE_URL -c "CREATE EXTENSION IF NOT EXISTS uuid-ossp;"
```

---

### Issue 2: Event Processing Stuck

**Symptoms**:

```
Events arriving in database
But not being processed
No error messages
Listener appears running
```

**Root Causes**:

1. Condition always evaluates to false
2. Dead action (invalid configuration)
3. Listener stuck waiting for external service
4. Connection pool exhausted

**Diagnostic Steps**:

```bash
# 1. Check runtime state
obs $API/runtime/health

# 2. Inspect the execution logs for a specific event
obs "$API/logs?event_id=<event_id>"

# 3. Check condition evaluation: run fraiseql-server with
#    RUST_LOG=fraiseql_server=debug and look for
#    "Event <event_id> processed: N actions succeeded, M skipped"

# 4. Check metrics for hung actions
curl -s -H "Authorization: Bearer $METRICS_TOKEN" http://localhost:8000/metrics \
  | grep fraiseql_observer_action_duration_seconds
```

**Solutions**:

#### If condition always false

```bash
# With RUST_LOG=fraiseql_server=debug, a condition that filtered the event out shows as:
# Event <event_id> processed: 0 actions succeeded, 1 skipped
```

**Fix**: Review and correct the condition in your observer definition:

```rust
// Wrong: This filters for events AFTER they're created
ObserverDefinition {
    condition: "status == 'shipped'",  // New orders have status='new'
    // ...
}

// Correct: Check for status change TO 'shipped'
ObserverDefinition {
    condition: "status_changed_to('shipped')",
    // ...
}
```

#### If external service hanging

```bash
# Increase timeout in configuration
retry_strategy: BackoffStrategy::Exponential {
    initial: Duration::from_millis(500),  // Increased timeout
    max: Duration::from_secs(10),
},

# Or enable circuit breaker to fast-fail
circuit_breaker: CircuitBreakerConfig {
    failure_threshold: 0.3,  // Fail fast
    timeout: Duration::from_secs(30),
}
```

---

### Issue 3: High DLQ Accumulation

**Symptoms**:

```
GET /api/observers/dlq/stats
{"total_items": 1250, ...}   (growing)
```

**Root Causes**:

1. External service unavailable (network, credential, endpoint issue)
2. Configuration error (invalid webhook URL)
3. Data validation failing
4. Rate limiting from external service

**Diagnostic Steps**:

```bash
# 1. List recent failures of one action type
obs "$API/dlq?action=webhook&limit=20"

# 2. Show specific failure
obs $API/dlq/<id>

# Expected fields: id, event_id, entity_type, entity_id, event_type,
# action_type, error_message, attempts

# 3. Check which action type is failing
obs $API/dlq/stats   # see "by_action"
```

**Solutions**:

#### If external service unreachable

```bash
# Test connectivity
curl -v https://webhook.example.com/notify

# If firewall issue:
# - Add observer server IP to allowlist
# - Check outbound firewall rules
# - Verify VPN if needed

# If DNS issue:
nslookup webhook.example.com

# If credential issue:
# Verify in configuration:
# - API key correct
# - Token not expired
# - Headers format correct
```

#### If endpoint invalid

```bash
# Check DLQ item for details
obs $API/dlq/<id> | jq .error_message

# Verify webhook URL in observer definition
grep "url" observers.yaml  # or config

# Test manually
curl -X POST https://webhook.example.com/notify \
  -H "Content-Type: application/json" \
  -d '{"test": true}'
```

#### If rate limited

```bash
# Check error details
obs $API/dlq/<id> | jq .error_message

# Look for rate limit indicators
# Common signs: HTTP 429, "too many requests", "quota exceeded"

# Solutions:
# 1. Reduce request rate (increase cache TTL)
cache_ttl: Duration::from_secs(600),  // Increased from 60s

# 2. Add request batching
batch_size: 100,  // Process multiple at once

# 3. Contact external service for higher limits
```

**Manual Retry**:

```bash
# Retry one item
obs -X POST $API/dlq/<id>/retry

# Or retry every item (there is no per-observer filter or dry run)
obs -X POST $API/dlq/retry-all
# Returns: items_retried, items_failed

# Verify success
obs $API/dlq/stats
```

---

### Issue 4: Duplicate Events Being Processed

**Symptoms**:

```
Same webhook called twice
Same email sent twice
Slack message posted multiple times
```

**Root Causes**:

1. Deduplication not enabled
2. Deduplication window too short
3. Event hash collision (rare)
4. Database checkpoint failure

**Diagnostic Steps**:

```bash
# 1. Check if deduplication enabled
cargo build --features "dedup" 2>&1 | grep -i dedup
# If not in features: that's the issue

# 2. Check deduplication stats (embedders only: fraiseql-server runs no dedup)
#    ($METRICS_URL: the /metrics endpoint serving the fraiseql_observer_* registry)
curl -s "$METRICS_URL" | grep fraiseql_observer_dedup_detected_total
```

**Solutions**:

#### If deduplication not enabled

```toml
# Add to features in Cargo.toml
[features]
dedup = ["redis"]

# Then rebuild
cargo build --release --features "dedup"
```

#### If deduplication window too short

```toml
# Increase from 5 minutes to 30 minutes (valid range: 1..=3600)
[redis]
dedup_window_secs = 1800   # was 300
```

#### To verify deduplication working

```bash
# 1. Send test event
INSERT INTO fraiseql_events (entity_type, entity_id, ...)
VALUES ('Order', 'order-123', ...);

# 2. Observe first execution
# Check: Webhook called once, email sent once

# 3. Send identical event again (simulate retry)
INSERT INTO fraiseql_events (entity_type, entity_id, ...)
VALUES ('Order', 'order-123', ...);

# 4. Verify dedup worked
curl -s "$METRICS_URL" | grep fraiseql_observer_dedup_detected_total
# Should have increased by 1
```

---

### Issue 5: Performance Degradation Over Time

**Symptoms**:

```
Events processed in 50ms initially
Events processed in 500ms+ after hours
Memory usage growing
Cache hit rate declining
```

**Root Causes**:

1. Cache evictions (not enough memory)
2. Redis connection pool exhausted
3. Database connection leak
4. Checkpoint table growing too large (without cleanup)

**Diagnostic Steps**:

```bash
# 1. Check metrics
curl -s "$METRICS_URL" | grep -E "fraiseql_observer_(cache_|action_duration_seconds|job_queue_depth)"

# 2. Check Redis memory
redis-cli INFO memory
# Look for used_memory, used_memory_peak

# 3. Check database connections
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'fraiseql_observers';"

# 4. Check checkpoint table size
psql $DATABASE_URL -c "SELECT pg_size_pretty(pg_total_relation_size('observer_checkpoints'));"
```

**Solutions**:

#### If cache memory exhausted

```toml
# Option 1: Reduce cache TTL (entries expire faster)
[redis]
cache_ttl_secs = 60   # was 300 (valid range: 1..=3600)
```

```yaml
# Option 2: Bound the memory Redis may use
# In docker-compose.yml
redis:
  command: redis-server --maxmemory 2gb --maxmemory-policy allkeys-lru
```

#### If connection pool exhausted

```rust
// Increase the size of the sqlx pool you hand to the checkpoint store
let pool = sqlx::postgres::PgPoolOptions::new()
    .min_connections(5)
    .max_connections(50) // Was 20
    .connect("postgresql://localhost/observers")
    .await?;
let checkpoint_store = PostgresCheckpointStore::new(pool);
```

Redis connections are a separate setting: `[redis] pool_size`.

#### If checkpoint table too large

```bash
# Add retention policy (keep 30 days)
psql $DATABASE_URL << EOF
DELETE FROM observer_checkpoints
WHERE created_at < NOW() - INTERVAL '30 days'
AND listener_id NOT IN (SELECT DISTINCT listener_id FROM observer_listeners WHERE status = 'active');
EOF

# Or create scheduled cleanup (using pg_cron)
SELECT cron.schedule('cleanup_old_checkpoints', '0 2 * * *', $$
  DELETE FROM observer_checkpoints
  WHERE created_at < NOW() - INTERVAL '30 days'
$$);
```

---

### Issue 6: Failover Not Working

**Symptoms**:

```
Multi-listener configured with 3 listeners
Primary listener crashes
Other listeners not taking over
Events stop processing
```

**Root Causes**:

1. Health check interval too long (doesn't detect failure fast)
2. Failover threshold too short (false positives)
3. Listeners not sharing checkpoint store
4. Coordinator not running

**Diagnostic Steps**:

`MultiListenerCoordinator` and `FailoverManager` are process-local: they elect among the
listeners registered in one process and cannot fail over across processes (#872). For
several `fraiseql-server` replicas, see
[Operating Observers - Scaling](../../../docs/operations/observers.md#scaling).

**Solutions**:

#### If health check too slow

```rust
multi_listener_config: Some(MultiListenerConfig {
    num_listeners: 3,
    health_check_interval: Duration::from_secs(2),  // Was 5
    failover_threshold: Duration::from_secs(30),    // Was 60
}),
```

#### If checkpoints not shared

```rust
// Ensure ALL listeners use the same checkpoint database
// In each listener process:
let pool = sqlx::PgPool::connect("postgresql://user:pass@postgres:5432/fraiseql").await?;
let checkpoint_store = PostgresCheckpointStore::new(pool);

// Verify table has unique index on listener_id
psql $DATABASE_URL -c "\d observer_checkpoints"
# Should show: UNIQUE INDEX listener_id
```

---

### Issue 7: Circuit Breaker Opening Too Easily

**Symptoms**:

```
Brief network hiccup
Circuit opens and stays open for minutes
All requests fail until timeout
```

**Root Causes**:

1. Failure threshold too low (too sensitive)
2. Sample size too small (not enough data)
3. Timeout too long (stuck open too long)
4. External service genuinely unreliable

**Diagnostic Steps**:

```bash
# 1. Count actions refused by an open circuit
curl -s -H "Authorization: Bearer $METRICS_TOKEN" http://localhost:8000/metrics \
  | grep 'fraiseql_observer_action_errors_total{.*error_type="circuit_breaker_open"'

# 2. Check action failure rate per action type
curl -s -H "Authorization: Bearer $METRICS_TOKEN" http://localhost:8000/metrics \
  | grep -E "fraiseql_observer_action_(errors|executed)_total"
```

**Solutions**:

#### Adjust circuit breaker thresholds

```rust
CircuitBreakerConfig {
    failure_threshold: 0.7,      // Higher = more tolerant (was 0.5)
    success_threshold: 0.5,      // Lower = easier to close (was 0.8)
    timeout: Duration::from_secs(120),  // Longer probe timeout
    sample_size: 200,  // Larger sample (more stable)
}
```

#### If external service unreliable

```rust
// Instead of relying on circuit breaker, use timeout + retry
retry_strategy: BackoffStrategy::Fixed {
    delay: Duration::from_millis(500),  // Longer wait between retries
},
max_retry_attempts: 10,  // More attempts
```

---

## Monitoring Checklist

### Critical Metrics to Watch

```promql
# 1. Is anything processing?
rate(observer_events_processed_total[5m]) > 0

# 2. Are actions succeeding?
(rate(observer_actions_failed_total[5m]) /
 rate(observer_actions_executed_total[5m])) < 0.05  # < 5% failure rate

# 3. Is DLQ growing?
observer_dlq_items_total < 50  # Alert if exceeded

# 4. Is latency acceptable?
histogram_quantile(0.99, observer_action_duration_seconds) < 1

# 5. Are listeners healthy?
observer_listener_health == 1 for all listeners

# 6. Is cache working?
(observer_cache_hits_total /
 (observer_cache_hits_total + observer_cache_misses_total)) > 0.7

# 7. Is deduplication effective?
(observer_dedup_skips_total /
 observer_events_processed_total) > 0.1
```

### Recommended Alerts

```yaml
groups:
  - name: observer_critical
    rules:
      - alert: NoEventsProcessing
        expr: rate(observer_events_processed_total[5m]) == 0
        for: 5m
        annotations:
          summary: "No events processed in 5 minutes"

      - alert: HighActionFailureRate
        expr: (rate(observer_actions_failed_total[5m]) /
               rate(observer_actions_executed_total[5m])) > 0.1
        for: 5m
        annotations:
          summary: "Action failure rate > 10%"

      - alert: DLQBacklog
        expr: observer_dlq_items_total > 100
        for: 10m
        annotations:
          summary: "Dead letter queue has {{ $value }} items"

      - alert: ListenerUnhealthy
        expr: observer_listener_health == 0
        for: 1m
        annotations:
          summary: "Listener {{ $labels.listener_id }} is unhealthy"
```

---

## Support & Escalation

### When to Check Logs

```bash
# Full debug logs
RUST_LOG=fraiseql_server=debug,fraiseql_observers=debug fraiseql-server 2>&1 | tee observer.log

# Filter for errors
grep -i "error\|panic" observer.log

# Filter for specific component
grep "checkpoint" observer.log  # Checkpoint issues
grep "dedup" observer.log      # Dedup issues
grep "circuit" observer.log    # Circuit breaker issues
```

### Getting Help

When reporting issues, include:

1. **Runtime status**:

   ```bash
   obs $API/runtime/health > status.json
   ```

2. **Recent Metrics**:

   ```bash
   curl -s -H "Authorization: Bearer $METRICS_TOKEN" \
     http://localhost:8000/metrics | grep fraiseql_observer_ > metrics.txt
   ```

3. **DLQ Status**:

   ```bash
   obs $API/dlq/stats > dlq-stats.json
   ```

4. **Recent Logs** (last 100 lines of the `fraiseql-server` output):

   ```bash
   docker logs <fraiseql-server container> --tail 100 > recent-logs.txt
   ```

5. **Configuration (redacted)**:

   ```bash
   env | grep -E "DATABASE|REDIS|ELASTIC" > config.env
   ```

---

## Prevention: Best Practices

### 1. Automated Monitoring

- Set up Prometheus scraping
- Create dashboards for key metrics
- Configure alerts for thresholds
- Regular metric review (weekly)

### 2. Gradual Rollout

- Test configuration changes in staging
- Deploy with small listener pool first
- Monitor for 24 hours before scaling
- Gradual feature enablement

### 3. Backup Strategies

- Regular PostgreSQL backups
- Redis persistence enabled
- Elasticsearch snapshots
- Configuration version control

### 4. Load Testing

Simulate load before production with the load test script in
[Performance Tuning](performance-tuning.md#load-test-script).

### 5. Failover Testing

```bash
# Monthly failover drills
# 1. Kill primary listener
# 2. Verify automatic failover
# 3. Check event continuity
# 4. Verify no data loss
```

---

## Performance Troubleshooting

### Slow Event Processing

1. **Identify bottleneck**:

   ```bash
   curl -s "$METRICS_URL" | grep fraiseql_observer_action_duration_seconds
   # Check which action type is slowest
   ```

2. **Optimize identified bottleneck**:
   - Increase cache TTL
   - Add circuit breaker
   - Batch operations

3. **Verify improvement**:

   ```bash
   # Compare before/after metrics
   curl -s "$METRICS_URL" | grep fraiseql_observer_ > after.txt
   ```

### High Memory Usage

1. **Identify source**:

   ```bash
   docker stats | grep observer
   # Check if memory grows over time
   ```

2. **Solutions**:
   - Reduce cache size
   - Reduce queue size
   - Enable periodic cleanup
   - Reduce dedup window

3. **Monitor**:

   ```bash
   watch -n 5 'docker stats'
   ```

---

## References

- Architecture Guide: `../../../../docs/architecture/overview.md`
- Configuration Examples: `configuration-examples.md`
- Operating observers under `fraiseql-server`: `../../../docs/operations/observers.md`
- Performance Tuning: `performance-tuning.md`
