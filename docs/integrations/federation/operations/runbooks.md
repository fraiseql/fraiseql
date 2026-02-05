# Federation Observability Operational Runbooks

**Last Updated**: 2026-01-28
**Version**: 1.0
**Status**: Production Ready

---

## Overview

This document provides step-by-step operational procedures for diagnosing and resolving federation observability issues. All runbooks follow the principle: **When in doubt, correlate traces, metrics, and logs**.

---

## Table of Contents

1. [Slow Query Investigation (Trace-Driven)](#slow-query-investigation)
2. [High Error Rate Response](#high-error-rate-response)
3. [Cache Hit Rate Degradation](#cache-hit-rate-degradation)
4. [Subgraph Latency Issues](#subgraph-latency-issues)
5. [Complete Observability Pipeline Failure](#complete-observability-pipeline-failure)
6. [Performance Baseline Analysis](#performance-baseline-analysis)
7. [Alert Threshold Tuning](#alert-threshold-tuning)

---

## Slow Query Investigation

**Alert**: `EntityResolutionLatencySLOBreach` or `SubgraphRequestLatencySLOBreach`

**Objective**: Diagnose why a query exceeded its SLO latency threshold.

### Step 1: Collect Initial Data

```bash
# Find the affected query in Jaeger
# Search by:
# - Duration: > 100ms (entity resolution) or > 500ms (subgraph)
# - Time range: Last 1 hour
# - Operation: "federation.query.execute"

# In Jaeger UI:
# 1. Go to Search tab
# 2. Service: "fraiseql-core"
# 3. Operation: "federation.query.execute"
# 4. Min Duration: "100ms" (adjust based on alert)
# 5. Sort by Duration, newest first
```text

### Step 2: Examine Span Breakdown

```text
Root Span: federation.query.execute (125ms total)
  ├─ Parse & Validate (2ms)
  ├─ federation.entity_resolution (78ms) ← Bottleneck
  │  ├─ Database Query (45ms) ← Database is slow
  │  ├─ Deduplication (2ms)
  │  └─ Projection (31ms)
  ├─ federation.subgraph_request[users] (32ms)
  │  └─ HTTP roundtrip (28ms)
  └─ Result Assembly (13ms)
```text

### Step 3: Identify Bottleneck Category

**If entity_resolution > 50ms:**

```sql
-- Check database query performance
SELECT query, avg(duration_ms), max(duration_ms), count(*)
FROM slow_queries
WHERE operation = '_entities' AND timestamp > now() - interval '1 hour'
GROUP BY query
ORDER BY avg(duration_ms) DESC
LIMIT 5;
```text

**Action Items**:

- Check database CPU/Memory utilization
- Look for missing indexes on key columns
- Review table statistics (analyze table)
- Check connection pool saturation

**If subgraph_request > 500ms:**

```bash
# Check subgraph health
curl -s https://subgraph-users.internal/health | jq .
curl -s https://subgraph-posts.internal/health | jq .

# Check network latency
ping subgraph-users.internal
ping subgraph-posts.internal

# Check if subgraph is under load
curl -s https://subgraph-users.internal/metrics | grep http_requests_total
```text

**Action Items**:

- Contact subgraph team with timing data
- Check network routing (geo proximity)
- Consider caching strategy for frequently-accessed entities
- Verify subgraph is not being rate-limited

### Step 4: Check for Pattern

```bash
# From logs, correlate with query type
TRACE_ID="4bf92f3577b34da6a3ce929d0e0e4736"

# Search logs
kubectl logs -l app=fraiseql --all-containers --tail=1000 | \
  grep $TRACE_ID | jq '.context.typename, .context.entity_count'

# Expected output:
# "User"
# "3"
# "Post"
# "2"

# Pattern: High cardinality (many unique entities) → dedup inefficiency
# Pattern: Specific typename slow → specific table issue
# Pattern: All queries slow → system-wide issue (load, GC)
```text

### Step 5: Document Finding

```json
{
  "incident_id": "INC-2026-01-28-001",
  "date": "2026-01-28T14:32:00Z",
  "affected_queries": ["federation.query.execute"],
  "root_cause": "Users table missing index on key column",
  "latency_spike": {
    "baseline_p99": "32ms",
    "spike_p99": "125ms",
    "increase": "291%"
  },
  "resolution": "CREATE INDEX idx_users_id ON users(id);",
  "time_to_resolve": "5 minutes",
  "prevention": "Add index monitoring to CI/CD checks"
}
```text

### Step 6: Preventive Measures

- [ ] Set up slow query log monitoring
- [ ] Establish baseline latency per query type
- [ ] Configure index coverage for all key columns
- [ ] Set up database query plan regression tests
- [ ] Document entity resolution latency SLA per typename

---

## High Error Rate Response

**Alerts**:

- `EntityResolutionErrorRateHigh` (>1%)
- `SubgraphRequestErrorRateHigh` (>5%)
- `MutationErrorRateHigh` (>1%)

**Objective**: Stop data loss and restore normal operation.

### Step 1: IMMEDIATE - Assess Severity

```bash
# Check current error rate in real-time
prometheus_query='rate(federation_errors_total[5m])'

# If errors/sec < 1: Proceed to Step 2 (investigation)
# If errors/sec > 10: Proceed to Step 3 (mitigation)
```text

### Step 2: Identify Error Source

```bash
# Get error distribution by type
kubectl logs -l app=fraiseql --tail=1000 | \
  jq 'select(.error_message != null) | {
    error_type: .context.operation_type,
    error_message,
    count: 1
  }' | sort | uniq -c | sort -rn
```text

**Common errors**:

| Error | Cause | Action |
|-------|-------|--------|
| `subgraph returned 5xx` | Subgraph down/broken | Contact subgraph team |
| `database connection timeout` | Connection pool exhausted | Increase pool size |
| `validation error: unknown typename` | Schema mismatch | Redeploy federation service |
| `mutation validation failed` | Input validation | Check client input format |

### Step 3: Mitigation (If >10 errors/sec)

```bash
# Option A: Temporarily disable problematic subgraph
kubectl set env deployment/fraiseql \
  FEDERATION_DISABLED_SUBGRAPHS=users_subgraph

# Option B: Reduce mutation rate
kubectl set env deployment/fraiseql \
  MUTATION_RATE_LIMIT_PER_SEC=10

# Option C: Drain and restart federation pods
kubectl rollout restart deployment/fraiseql
```text

### Step 4: Root Cause Analysis

**For Database Errors**:

```sql
-- Check if connection pool is exhausted
SELECT count(*) as active_connections FROM pg_stat_activity;

-- Check for hung queries
SELECT pid, query, state, query_start
FROM pg_stat_activity
WHERE state != 'idle'
  AND query_start < now() - interval '5 minutes';

-- Check for locks
SELECT * FROM pg_locks WHERE NOT granted;
```text

**For Subgraph Errors**:

```bash
# Check subgraph logs
kubectl logs -l app=users-subgraph --tail=100 | grep -i error

# Check subgraph metrics
curl -s https://subgraph-users.internal/metrics | \
  grep 'federation_entity_resolutions_errors'
```text

### Step 5: Resolution

Once root cause identified:

```bash
# Fix the issue (specific to cause)
# For database: ANALYZE table, add index, increase connections
# For subgraph: Redeploy, increase replicas, clear cache

# Monitor recovery
watch -n 5 'curl -s http://prometheus:9090/api/v1/query?query=rate(federation_errors_total%5B5m%5D) | jq .data.result[0].value[1]'

# Expect error rate to drop within 2 minutes of fix
```text

### Step 6: Post-Incident Review

- [ ] Document error message and frequency
- [ ] Add specific error type to alert rules if new
- [ ] Update runbook with new error pattern
- [ ] Schedule deep dive on root cause
- [ ] Create issue for permanent fix
- [ ] Notify affected clients if applicable

---

## Cache Hit Rate Degradation

**Alert**: `EntityCacheHitRateLow` (<70%)

**Objective**: Restore cache effectiveness.

### Diagnosis Workflow

```text
┌─────────────────────────┐
│ Cache Hit Rate Drops    │
│ (e.g., 85% → 55%)       │
└────────────┬────────────┘
             │
    ┌────────┴────────┐
    │                 │
    ▼                 ▼
Changed Query?    Cache Invalidation
Pattern?          Bug?
    │                 │
    ├─→ A             ├─→ B
```text

### Path A: Query Pattern Changed

```bash
# Analyze recent queries
kubectl logs -l app=fraiseql --since=2h | \
  jq 'select(.context.operation_type == "entity_resolution") |
      {typename: .context.typename, entity_count: .context.entity_count}' | \
  sort | uniq -c | sort -rn

# Compare to baseline
# If high cardinality (many unique entities) increased:
#   Conclusion: More cache misses are expected
#   Action: Update baseline in alert thresholds
```text

### Path B: Cache Invalidation Bug

```bash
# Check cache metrics directly
prometheus_query='federation_entity_cache_misses / (federation_entity_cache_hits + federation_entity_cache_misses)'

# If misses spike suddenly (not gradual):
#   Likely cause: Invalidation too aggressive
#   Check recent deployments/changes

# Review cache invalidation logic
grep -r "cache.*invalidate\|cache.*clear" src/federation/ | \
  head -20

# Verify cache timeout settings
grep -r "CACHE.*TTL\|CACHE.*TIMEOUT" src/
```text

### Resolution

**For Query Pattern Change**:

- Update alert threshold based on new baseline
- Document new query patterns
- Consider caching strategy adjustments

**For Invalidation Bug**:

```bash
# If too-aggressive invalidation detected:
# 1. Revert recent cache-related changes
git log --oneline src/federation/ | grep -i cache | head -3
git revert COMMIT_HASH

# 2. Or adjust invalidation policy
# Change from: invalidate on ANY write
# To: invalidate on SPECIFIC field updates
```text

### Prevention

- [ ] Establish cache hit rate baselines per typename
- [ ] Monitor query cardinality trends
- [ ] Add cache metrics to dashboards
- [ ] Alert on sudden invalidation spikes
- [ ] Document cache invalidation strategy in code

---

## Subgraph Latency Issues

**Alert**: `SubgraphRequestLatencySLOBreach` (>500ms p99)

**Objective**: Restore subgraph response times.

### Quick Triage

```bash
# Step 1: Identify which subgraph is slow
prometheus_query='histogram_quantile(0.99, rate(federation_subgraph_request_duration_us{subgraph="users"}[5m])) / 1000'
# Result: 750ms (users subgraph slow)

# Step 2: Isolate subgraph health
curl -s https://subgraph-users.internal/health | jq '.'
# Check: status, version, database connectivity, cache size

# Step 3: Check federation service latency to that subgraph
prometheus_query='histogram_quantile(0.99, federation_subgraph_request_duration_us{subgraph="users"}) / 1000'
# vs
curl -X POST https://subgraph-users.internal/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ _service { sdl } }"}' \
  --output /dev/null -w "%{time_total}s"
# If direct call is fast but federated call is slow:
#   Issue: Network/routing or large response payload
```text

### Network Diagnosis

```bash
# Check network latency
ping subgraph-users.internal
traceroute subgraph-users.internal

# Check if using correct DNS
nslookup subgraph-users.internal

# Check subgraph is not rate-limiting
curl -I -H "Authorization: Bearer $TOKEN" \
  https://subgraph-users.internal/health
# Check for 429 (Too Many Requests) or X-RateLimit headers
```text

### Subgraph-Side Investigation

```bash
# SSH into subgraph and check:

# 1. Database connectivity
SELECT NOW(); -- Should be instant

# 2. Slow query log
tail -100 /var/log/postgres/slow.log | head -5

# 3. Connection pool
SELECT count(*) FROM pg_stat_activity;

# 4. Query cache (if using)
redis-cli INFO stats | grep keyspace_hits

# 5. Replication lag (if replicated)
SELECT EXTRACT(EPOCH FROM (NOW() - pg_last_xact_replay_timestamp())) as replication_lag_seconds;
```text

### Resolution Steps

1. **If subgraph database is slow**:
   - Add indexes (coordinate with subgraph team)
   - Optimize GraphQL queries
   - Increase connection pool

2. **If network is slow**:
   - Check for network congestion
   - Consider CDN for federated queries
   - Verify routing (geo-locality)

3. **If response payload is huge**:
   - Analyze query fields returned
   - Consider pagination/pagination limits
   - Compress responses

### Escalation

```bash
# If latency remains high after basic checks:
# 1. Contact subgraph team with:
#    - Slow query IDs (trace_id)
#    - Timing breakdown (from Jaeger spans)
#    - Reproducible query

# 2. Provide metrics data:
prometheus_query='federation_subgraph_request_duration_us{subgraph="users"}'
# Export as CSV, send to subgraph team

# 3. Request subgraph SLO review
# Ensure both teams agree on latency target
```text

---

## Complete Observability Pipeline Failure

**Scenario**: Queries return results but no traces/metrics/logs visible

**Objective**: Restore observability without breaking queries.

### Diagnosis

```bash
# Step 1: Verify federation is working
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user(id: \"1\") { id name } }"}' | jq .

# If result is valid JSON with data: Federation is working ✓

# Step 2: Check each observability component independently

# Is Jaeger receiving traces?
curl -s http://jaeger:16686/api/services | jq '.data | length'
# Should return > 0

# Is Prometheus receiving metrics?
curl -s http://prometheus:9090/api/v1/query?query=federation_entity_resolutions_total | jq '.data.result[0].value[1]'
# Should return a number > 0

# Are logs being written?
tail -100 /var/log/fraiseql/app.log | jq '.trace_id' | head -5
# Should show trace IDs

# Step 3: Identify failed component
# Trace data: Check Step 2 #1
# Metrics data: Check Step 2 #2
# Log data: Check Step 2 #3
```text

### Recover Each Component

**If Traces Missing**:

```bash
# Check Jaeger connectivity
curl -s http://jaeger:14268/api/traces?service=fraiseql-core | jq '.traceID' | head -3

# Restart Jaeger agent
docker restart jaeger-agent

# Verify traces flow
kubectl logs -l app=fraiseql --tail=50 | grep -i "trace"
```text

**If Metrics Missing**:

```bash
# Check Prometheus targets
curl -s http://prometheus:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job=="fraiseql")'

# Ensure federation metrics endpoint is responding
curl -s http://localhost:9000/metrics | grep federation_ | head -5

# Force Prometheus scrape
curl -X POST http://prometheus:9090/-/reload
```text

**If Logs Missing**:

```bash
# Check log file permissions
ls -la /var/log/fraiseql/

# Verify logging is initialized
kubectl logs -l app=fraiseql | grep -i "logging.*init\|tracing.*init" | tail -3

# Check disk space (logs might be failing due to full disk)
df -h /var/log/

# Restart logging
pkill -f "fraiseql" && docker-compose up -d fraiseql
```text

### Validation After Recovery

```bash
# Execute test query
TRACE_ID=$(uuidgen)
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -H "traceparent: 00-$TRACE_ID-0000000000000001-01" \
  -d '{"query": "{ user(id: \"1\") { id } }"}' | jq .

# Verify trace in Jaeger
curl -s "http://jaeger:16686/api/traces/$TRACE_ID" | jq '.data.spans[0].operationName'
# Should return "federation.query.execute"

# Verify metrics incremented
curl -s "http://prometheus:9090/api/v1/query?query=federation_entity_resolutions_total" | jq '.data.result[0].value[1]'
# Should be higher than before

# Verify logs contain trace_id
kubectl logs -l app=fraiseql --tail=50 | jq "select(.trace_id == \"$TRACE_ID\")" | jq '.message'
# Should return log messages with matching trace_id
```text

---

## Performance Baseline Analysis

**Objective**: Establish normal performance baselines to detect anomalies.

### Collect Baseline Data

```bash
# Run for 24 hours of normal traffic and collect:

# 1. Latency percentiles
prometheus_query='histogram_quantile(0.99, rate(federation_entity_resolution_duration_us[1h]))'
# Record hourly for 24 hours

# 2. Error rates
prometheus_query='rate(federation_errors_total[1h])'
# Record hourly for 24 hours

# 3. Cache hit rate
prometheus_query='federation_entity_cache_hits / (federation_entity_cache_hits + federation_entity_cache_misses)'
# Record hourly for 24 hours

# 4. Subgraph latencies (per subgraph)
prometheus_query='histogram_quantile(0.99, rate(federation_subgraph_request_duration_us{subgraph="users"}[1h]))'
# Record for each subgraph
```text

### Create Baseline Profile

```json
{
  "baseline_profile": {
    "entity_resolution": {
      "p50": "15ms",
      "p90": "32ms",
      "p99": "52ms",
      "error_rate": "0.02%"
    },
    "subgraph_requests": {
      "users_subgraph": {
        "p50": "25ms",
        "p90": "120ms",
        "p99": "280ms"
      },
      "posts_subgraph": {
        "p50": "18ms",
        "p90": "95ms",
        "p99": "320ms"
      }
    },
    "cache": {
      "hit_rate": "82%",
      "misses_per_sec": "45"
    },
    "mutations": {
      "p50": "35ms",
      "p90": "180ms",
      "p99": "850ms",
      "error_rate": "0.05%"
    }
  },
  "period": "2026-01-28 to 2026-01-29",
  "traffic_volume": "2.3M queries/day"
}
```text

### Configure Alerts Based on Baselines

```yaml
# prometheus-rules.yml

- alert: EntityResolutionLatencySLOBreach
  expr: histogram_quantile(0.99, rate(federation_entity_resolution_duration_us[5m])) / 1000 > 52 * 1.5  # 50% above baseline
  for: 5m

- alert: EntityResolutionErrorRateDegraded
  expr: rate(federation_entity_resolutions_errors[5m]) > 0.0002 * 1.5  # 50% above baseline
  for: 5m

- alert: CacheHitRateUnexpectedlyLow
  expr: (federation_entity_cache_hits / (federation_entity_cache_hits + federation_entity_cache_misses)) < 0.82 * 0.85  # 15% below baseline
  for: 10m
```text

### Quarterly Baseline Review

- [ ] Collect new baseline data
- [ ] Compare to previous quarter
- [ ] Document changes (intentional improvements or drift)
- [ ] Update alert thresholds if needed
- [ ] Share with team for context
- [ ] Archive old baseline for trend analysis

---

## Alert Threshold Tuning

**Objective**: Minimize false positives while catching real issues.

### Tuning Process

```text

1. Establish baseline (see above)
2. Set initial threshold at baseline + 50%
3. Monitor for false positives over 1 week
4. Adjust threshold based on observed patterns
5. Document threshold rationale
6. Review quarterly
```text

### Examples

**EntityResolutionLatencySLOBreach**:

```yaml
# Initial SLO: 100ms p99
# Baseline p99: 52ms
# Initial alert threshold: 52ms * 1.5 = 78ms

# After 1 week:
# - 0 false positives
# - Correctly caught 2 genuine slowdowns
# - Max p99 observed: 68ms

# Adjustment: Lower to 75ms (closer to SLO)
expr: histogram_quantile(0.99, federation_entity_resolution_duration_us) / 1000 > 75
```text

**EntityCacheHitRateLow**:

```yaml
# Baseline hit rate: 82%
# SLO: 70%
# Initial alert: < 70% for 10m (too strict, too many alerts)

# After 1 week:
# - 47 false positives (normal variation)
# - 1 real degradation (legitimate issue)

# Adjustment: Change to < 60% (only alert on serious degradation)
expr: (federation_entity_cache_hits / (...)) < 0.60
for: 15m  # Also increased duration for stability
```text

### False Positive Analysis

```bash
# When an alert fires, analyze:

# 1. Was it a real issue?
# Check if it affected query results or user experience

# 2. Is the threshold too sensitive?
# If baseline naturally varies ±20%, threshold should account for it

# 3. Is the duration too short?
# Transient spikes shouldn't trigger alerts (try duration: 5m → 10m)

# 4. Are there confounding factors?
# E.g., Cache hits drop during deployments (expected)
# E.g., Latency increases during batching (acceptable)
```text

### Documentation Template

```markdown
## Alert: EntityResolutionLatencySLOBreach

**Purpose**: Catch degraded entity resolution performance

**Threshold**: 75ms p99

**Rationale**:

- SLO target: 100ms
- Historical baseline: 52ms
- Threshold: 75ms (43% above baseline, 25% below SLO)
- Provides early warning without false positives

**False Positive Rate**: <0.5% (target)
- 1 false alert per 200 firings

**Response Time**: 15 minutes (avg, from alert to investigation start)

**Root Causes**:

- Database index missing (most common)
- Subgraph latency high
- High deduplication overhead (many unique entities)

**Last Reviewed**: 2026-01-28
**Next Review**: 2026-04-28 (quarterly)
```text

---

## Escalation Flowchart

```text
┌─────────────────────────┐
│ Observability Alert     │
│ Triggered               │
└────────────┬────────────┘
             │
    ┌────────▼────────┐
    │ Check severity  │
    │ (ERROR vs WARN) │
    └────┬────────┬───┘
         │        │
    ERROR│        │WARN
         │        │
    ┌────▼───┐ ┌──▼──────────┐
    │ Page   │ │ Create      │
    │ on-call│ │ ticket, no  │
    │ eng    │ │ paging      │
    └────┬───┘ └──┬──────────┘
         │        │
    ┌────▼────────▼──────┐
    │ Follow runbook     │
    │ for alert type     │
    └────┬───────────────┘
         │
    ┌────▼──────────────────┐
    │ Issue resolved        │
    │ within 15 minutes?    │
    └───┬──────────────┬────┘
        │YES           │NO
        │              │
    ┌───▼──┐      ┌────▼─────────┐
    │Close │      │Escalate to   │
    │      │      │engineering   │
    │      │      │for deep dive  │
    └──────┘      └───────────────┘
```text

---

## Quick Reference Card

### Key Prometheus Queries

```text
# Entity Resolution
Rate: rate(federation_entity_resolutions_total[5m])
Latency p99: histogram_quantile(0.99, federation_entity_resolution_duration_us) / 1000
Error rate: rate(federation_entity_resolutions_errors[5m]) / rate(federation_entity_resolutions_total[5m])

# Subgraph Requests
Rate: rate(federation_subgraph_requests_total[5m])
Latency p99: histogram_quantile(0.99, federation_subgraph_request_duration_us) / 1000
Error rate: rate(federation_subgraph_requests_errors[5m]) / rate(federation_subgraph_requests_total[5m])

# Cache
Hit rate: federation_entity_cache_hits / (federation_entity_cache_hits + federation_entity_cache_misses)
Misses/sec: rate(federation_entity_cache_misses[1m])

# Mutations
Error rate: rate(federation_mutations_errors[5m]) / rate(federation_mutations_total[5m])
Latency p99: histogram_quantile(0.99, federation_mutation_duration_us) / 1000
```text

### Key Log Fields for Filtering

```text
query_id      - Unique query identifier (follow single request)
trace_id      - Distributed trace ID (correlate with Jaeger)
typename      - GraphQL type being resolved
subgraph_name - Federated subgraph name
duration_ms   - Operation duration
status        - "started", "success", "error", "timeout"
error_message - Human-readable error description
```text

### Common Curl Commands

```bash
# Test Jaeger
curl -s http://jaeger:16686/api/services | jq .

# Test Prometheus
curl -s "http://prometheus:9090/api/v1/query?query=up{job=\"fraiseql\"}" | jq .

# Test federation health
curl -s http://localhost:8000/health | jq .

# Test observability endpoint
curl -s http://localhost:9000/metrics | grep federation_ | head -10
```text

---

## Support & Escalation

**Observability Team**: @fraiseql-observability on Slack
**On-Call Engineer**: See PagerDuty schedule
**Runbook Issues**: File issue in fraiseql/docs

Last Updated: 2026-01-28
Version: 1.0
