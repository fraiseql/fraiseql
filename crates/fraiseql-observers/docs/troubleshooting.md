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

1. Delivery is at-least-once: a batch whose actions ran but was not yet recorded in the
   dispatch ledger runs again after a crash, a restart or a lost poll lease
2. Two drivers poll the same change log under different listener ids
3. A NATS message was not acknowledged within `ack_wait_secs` and was redelivered

**Diagnostic Steps**:

```bash
# 1. Inspect the executions recorded for the event
obs "$API/logs?event_id=<uuid>"

# 2. Check which replica polls: its log says "Holding the change-log poll lease"
docker logs <fraiseql-server container> 2>&1 | grep "poll lease"
```

**Solutions**:

- Make actions idempotent where a repeat matters. See
  [Observer Idempotency](../../../docs/operations/observer-idempotency.md).
- Run a single driver per change log. Replicas of `fraiseql-server` share one listener id
  and one poller.
- For NATS, raise `nats.jetstream.ack_wait_secs` above the slowest action.

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
curl -s "$METRICS_URL" | grep -E "fraiseql_observer_(cache_|action_duration_seconds|backlog_size)"

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

### Issue 6: Standby Replica Not Taking Over

**Symptoms**:

```
Several fraiseql-server replicas run with observers enabled
The polling replica stops
No other replica resumes polling
Events stop processing
```

**Root Causes**:

1. The stopped replica's PostgreSQL session is still open, so it still holds the advisory lock
2. The replicas use different databases, so they do not share a checkpoint

**Diagnostic Steps**:

Exactly one replica polls: the one holding a PostgreSQL advisory lock keyed on the listener
id. The others retry the lock every second. See
[Operating Observers - Scaling](../../../docs/operations/observers.md#scaling).

```bash
# Which replica holds the lease? Its log says:
#   "Holding the change-log poll lease"

# Is the old session still holding an advisory lock?
psql $DATABASE_URL -c "SELECT pid, granted FROM pg_locks WHERE locktype = 'advisory';"
```

**Solutions**:

- End a stuck session with `pg_terminate_backend(<pid>)`. A standby takes the lock at its
  next retry and resumes from the stored checkpoint.
- Point every replica at the same database.

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

`fraiseql-server` records four observer series on `/metrics` (see
[docs/operations/observers.md#metrics](../../../docs/operations/observers.md#metrics)).
`fraiseql_observer_action_executed_total` and the duration histogram count successful
actions only.

```promql
# 1. Is anything processing?
rate(fraiseql_observer_events_processed_total[5m]) > 0

# 2. Are actions succeeding? (< 5% failure rate)
sum(rate(fraiseql_observer_action_errors_total[5m]))
  / (sum(rate(fraiseql_observer_action_executed_total[5m]))
     + sum(rate(fraiseql_observer_action_errors_total[5m]))) < 0.05

# 3. Is latency acceptable?
histogram_quantile(0.99, sum by (le) (rate(fraiseql_observer_action_duration_seconds_bucket[5m]))) < 1
```

The dead letter queue and the runtime's health are not metrics. Read them from the admin
API: `GET /api/observers/dlq/stats` and `GET /api/observers/runtime/health`.

### Recommended Alerts

```yaml
groups:
  - name: observer_critical
    rules:
      - alert: NoEventsProcessing
        expr: rate(fraiseql_observer_events_processed_total[5m]) == 0
        for: 5m
        annotations:
          summary: "No events processed in 5 minutes"

      - alert: HighActionFailureRate
        expr: |
          sum(rate(fraiseql_observer_action_errors_total[5m]))
            / (sum(rate(fraiseql_observer_action_executed_total[5m]))
               + sum(rate(fraiseql_observer_action_errors_total[5m]))) > 0.1
        for: 5m
        annotations:
          summary: "Action failure rate > 10%"
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
grep "poll lease" observer.log  # Poll lease issues
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
   env | grep -E "DATABASE|REDIS|NATS" > config.env
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
- Configuration version control

### 4. Load Testing

Simulate load before production with the load test script in
[Performance Tuning](performance-tuning.md#load-test-script).

### 5. Takeover Testing

```bash
# Regular takeover drills
# 1. Stop the replica holding the poll lease
# 2. Verify a standby logs "Holding the change-log poll lease"
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
   - Enable periodic cleanup

3. **Monitor**:

   ```bash
   watch -n 5 'docker stats'
   ```

---

## References

- Architecture Guide: `../../../../docs/architecture/overview.md`
- Operating observers under `fraiseql-server`: `../../../docs/operations/observers.md`
- Performance Tuning: `performance-tuning.md`
