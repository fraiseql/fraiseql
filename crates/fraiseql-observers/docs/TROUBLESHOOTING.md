# Troubleshooting Guide - FraiseQL Observer System

## Quick Diagnosis

Start with this checklist to identify the issue:

```

1. System not processing events?
   → Check: Listener health status
   → Run: fraiseql-observers status

2. Events processed but wrong actions executed?
   → Check: Condition evaluation
   → Run: fraiseql-observers debug-event --event-id <id>

3. Dead Letter Queue growing?
   → Check: Action failures
   → Run: fraiseql-observers dlq list --limit 20

4. System slow?
   → Check: Cache hit rate, latency metrics
   → Run: fraiseql-observers metrics

5. Event loss on restart?
   → Check: Checkpoint configuration
   → Verify: Database connectivity
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
# 1. Check listener status
fraiseql-observers status

# 2. Check logs
docker logs observer-listener

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

# Restart listener with correct URL
DATABASE_URL="postgresql://user:pass@host:5432/db" fraiseql-observer start
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
# 1. Check listener state
fraiseql-observers status --detailed

# 2. Inspect specific event
fraiseql-observers debug-event --event-id <event_id>

# 3. Check condition evaluation
fraiseql-observers debug-event --event-id <event_id> | grep -A5 "matched_observers"

# 4. Check metrics for hung actions
fraiseql-observers metrics --metric observer_action_duration_seconds
```

**Solutions**:

#### If condition always false

```bash
# Debug the condition
fraiseql-observers debug-event --event-id evt-123

# Expected output:
# matched_observers:
#   - name: "OrderNotifier"
#     condition_result: false  ← Problem here
#     condition_expression: "status == 'shipped' && total > 100"
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
fraiseql-observers dlq stats
Total Items: 1,250 (growing)
Failure Rate: 85%
```

**Root Causes**:

1. External service unavailable (network, credential, endpoint issue)
2. Configuration error (invalid webhook URL)
3. Data validation failing
4. Rate limiting from external service

**Diagnostic Steps**:

```bash
# 1. List recent failures
fraiseql-observers dlq list --limit 20 --observer obs-webhook

# 2. Show specific failure
fraiseql-observers dlq show dlq-001

# Expected output:
# ID: dlq-001
# Observer: obs-webhook
# Error: Connection timeout after 30s
# Retry Count: 3/5
# Last Retry: 2026-01-22T12:15:00Z

# 3. Check which action is failing
fraiseql-observers dlq stats --by-observer

# 4. Check error types
fraiseql-observers dlq stats --by-error
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
fraiseql-observers dlq show dlq-001 | grep -A5 "Error"

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
fraiseql-observers dlq show dlq-001

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
# Dry run first (show what would be retried)
fraiseql-observers dlq retry-all --observer obs-webhook --dry-run
# Returns: "Would retry 15 items"

# Actually retry
fraiseql-observers dlq retry-all --observer obs-webhook

# Verify success
fraiseql-observers dlq stats
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

# 2. Check deduplication stats
fraiseql-observers metrics --metric observer_dedup_checks_total

# 3. Check cache hit rate
fraiseql-observers metrics --metric observer_cache_hit_rate
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

```rust
// Increase from 5 minutes to 30 minutes
dedup_store: Arc::new(
    RedisDeduplicationStore::new(
        "redis://localhost:6379",
        1800  // 30 minutes instead of 300 (5 minutes)
    )
    .await?
),
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
fraiseql-observers debug-event --entity-id order-123
# Should show: Event skipped due to dedup (or similar)
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
fraiseql-observers metrics | grep -E "cache_hit|duration_seconds|queue"

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

```rust
// Option 1: Reduce cache TTL (entries expire faster)
cache_ttl: Duration::from_secs(60),  // Was 300

// Option 2: Reduce max cache size
cache_backend: Arc::new(
    RedisCacheBackend::with_config(
        "redis://localhost",
        CacheConfig {
            max_size: 50_000,  // Was 100_000
            ttl: Duration::from_secs(300),
        }
    )
    .await?
),

// Option 3: Increase Redis memory
# In docker-compose.yml
redis:
  command: redis-server --maxmemory 2gb --maxmemory-policy allkeys-lru
```

#### If connection pool exhausted

```rust
// Increase connection pool size
checkpoint_store: Arc::new(
    PostgresCheckpointStore::with_pool_config(
        "postgresql://localhost/observers",
        PoolConfig {
            min_connections: 5,
            max_connections: 50,  // Was 20
            ..Default::default()
        }
    )
    .await?
),
```

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

```bash
# 1. Check listener status
fraiseql-observers status --detailed
# Expected: All 3 listeners showing healthy

# 2. Check leader
fraiseql-observers status | grep Leader

# 3. Check failover metrics
fraiseql-observers metrics --metric failover_events_total

# 4. Simulate primary failure
# Kill primary listener process
kill <listener_pid>

# 5. Check if failover happened (within timeout)
sleep 65
fraiseql-observers status | grep Leader
# Expected: Different listener now leader
```

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
// Ensure ALL listeners use same checkpoint store
// In config for each listener:
checkpoint_store: Arc::new(
    PostgresCheckpointStore::new(
        "postgresql://user:pass@postgres:5432/fraiseql",
        "observer_checkpoints"
    )
    .await?
),

// Verify table has unique index on listener_id
psql $DATABASE_URL -c "\d observer_checkpoints"
# Should show: UNIQUE INDEX listener_id
```

#### To test failover recovery

```bash
# 1. Monitor before failure
watch -n 1 'fraiseql-observers status'

# 2. In another terminal, crash primary
kill $(pgrep fraiseql-observer | head -1)

# 3. Watch automatic failover
# Expected: New leader elected within 30 seconds
# New leader resumes from last checkpoint
# Events continue processing
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
# 1. Check circuit breaker state
fraiseql-observers status | grep -i "circuit"

# 2. Check action failure rate
fraiseql-observers metrics --metric observer_circuit_breaker_state

# 3. Check external service reliability
fraiseql-observers metrics --metric observer_action_failures_by_service
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
RUST_LOG=debug fraiseql-observer start 2>&1 | tee observer.log

# Filter for errors
grep -i "error\|panic" observer.log

# Filter for specific component
grep "checkpoint" observer.log  # Checkpoint issues
grep "dedup" observer.log      # Dedup issues
grep "circuit" observer.log    # Circuit breaker issues
```

### Getting Help

When reporting issues, include:

1. **Configuration**:

   ```bash
   fraiseql-observers status --detailed > status.json
   ```

2. **Recent Metrics**:

   ```bash
   fraiseql-observers metrics > metrics.txt
   ```

3. **DLQ Status**:

   ```bash
   fraiseql-observers dlq stats > dlq-stats.json
   ```

4. **Recent Logs** (last 100 lines):

   ```bash
   docker logs observer-listener --tail 100 > recent-logs.txt
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

```bash
# Simulate load before production
fraiseql-load-test \
  --events-per-second 1000 \
  --duration 300  # 5 minutes
```

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
   fraiseql-observers metrics | grep duration_seconds
   # Check which action type is slowest
   ```

2. **Optimize identified bottleneck**:
   - Increase cache TTL
   - Add circuit breaker
   - Enable concurrent execution
   - Batch operations

3. **Verify improvement**:

   ```bash
   # Compare before/after metrics
   fraiseql-observers metrics > after.txt
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

- Architecture Guide: `ARCHITECTURE_PHASE_8.md`
- Configuration Examples: `CONFIGURATION_EXAMPLES.md`
- CLI Tools: `CLI_TOOLS.md`
- Performance Tuning: `PERFORMANCE_TUNING.md`
