# Loki Integration Fixes - Implementation Plan

**Status:** Planning
**Priority:** High (Critical production issues identified)
**Estimated Effort:** 2-3 hours
**TDD Applicable:** No (Configuration-only changes)

---

## Executive Summary

The Loki integration commit `c4e7632` contains **4 critical issues** and **10 important improvements** that must be addressed before production use. The most severe issues are:

1. **Cardinality explosion** from `trace_id`/`span_id` labels (will crash Loki)
2. **Deprecated schema config** (boltdb-shipper instead of TSDB)
3. **Incorrect LogQL queries** in documentation (syntax errors)
4. **Missing security hardening** (default passwords, Docker socket exposure)

**TDD Assessment:** ❌ **Not applicable** - This is purely configuration and documentation fixes. No business logic to test. Verification will be done via:
- Docker Compose smoke tests (stack starts successfully)
- Manual LogQL query validation against running Loki instance
- Documentation review

---

## Phase 1: Critical Configuration Fixes (MUST DO)

### Objective
Fix issues that will cause immediate production failures (crashes, query errors, security vulnerabilities).

**Estimated Time:** 45 minutes

---

### Task 1.1: Fix Schema Configuration (CRITICAL)

**File:** `examples/observability/loki/loki-config.yaml`

**Problem:** Using deprecated `boltdb-shipper` and `schema: v11`

**Changes:**
```yaml
# BEFORE (lines 19-27):
schema_config:
  configs:
    - from: 2024-01-01
      store: boltdb-shipper
      object_store: filesystem
      schema: v11
      index:
        prefix: index_
        period: 24h

# AFTER:
schema_config:
  configs:
    - from: 2024-01-01
      store: tsdb
      object_store: filesystem
      schema: v13
      index:
        prefix: index_
        period: 24h
```

**Also remove deprecated `table_manager` (lines 58-60):**
```yaml
# DELETE these lines:
table_manager:
  retention_deletes_enabled: true
  retention_period: 720h
```

**Add to `limits_config` (line 36):**
```yaml
limits_config:
  retention_period: 720h  # Moved from table_manager
  reject_old_samples: true
  reject_old_samples_max_age: 168h
  # ... rest of config
```

**Verification:**
```bash
# Start Loki with new config
docker-compose -f examples/observability/docker-compose.loki.yml up -d loki

# Check for schema warnings
docker logs fraiseql-loki 2>&1 | grep -i "schema\|deprecated\|warning"

# Should see: "using schema version v13" (no warnings)
```

---

### Task 1.2: Fix High-Cardinality Labels (CRITICAL)

**File:** `examples/observability/loki/promtail-config.yaml`

**Problem:** Extracting `trace_id`, `span_id`, `fingerprint` as labels creates millions of unique streams.

**Impact:** Will cause Loki to crash with OOM or extreme slowness.

**Changes Required:**

**Location 1: `fraiseql-app` job (lines 34-39):**
```yaml
# BEFORE:
- labels:
    level:
    trace_id:        # ❌ REMOVE
    span_id:         # ❌ REMOVE
    exception_type:

# AFTER:
- labels:
    level:
    exception_type:
```

**Location 2: `fraiseql-errors` job (lines 73-77):**
```yaml
# BEFORE:
- labels:
    exception_type:
    fingerprint:     # ❌ REMOVE
    trace_id:        # ❌ REMOVE
    span_id:         # ❌ REMOVE

# AFTER:
- labels:
    exception_type:
```

**Location 3: `docker` job (lines 131-135):**
```yaml
# BEFORE:
- labels:
    level:
    stream:
    container:
    trace_id:        # ❌ REMOVE

# AFTER:
- labels:
    level:
    stream:
    container:
```

**Keep JSON parsing (DO NOT REMOVE):**
```yaml
# Keep this - we still extract as JSON fields, just not labels
- json:
    expressions:
      trace_id: trace_id
      span_id: span_id
      fingerprint: fingerprint
      # ... etc
```

**Verification:**
```bash
# Restart Promtail
docker-compose -f examples/observability/docker-compose.loki.yml restart promtail

# Check stream count (should be LOW)
curl -s http://localhost:3100/loki/api/v1/labels | jq '.data | length'
# Expected: < 10 labels

# Test trace_id query (should still work via JSON parsing)
curl -G http://localhost:3100/loki/api/v1/query \
  --data-urlencode 'query={job="fraiseql-app"} | json | trace_id="test"' | jq
```

---

### Task 1.3: Fix LogQL Query Syntax Errors (CRITICAL)

**File:** `docs/production/loki_integration.md`

**Problem:** Multiple LogQL queries have syntax errors that won't execute.

**Changes Required:**

**Query #1 (Line 211):**
```markdown
# BEFORE:
{job="fraiseql-app"} | json | level="error" [1h]

# AFTER:
{job="fraiseql-app"} | json | level="error"

# Note: Time range is set in Grafana UI, not in query
# For range queries, use count_over_time:
count_over_time({job="fraiseql-app"} | json | level="error" [1h])
```

**Query #3 (Line 225):**
```markdown
# BEFORE:
rate({job="fraiseql-app"} | json | level="error" [5m])

# AFTER:
rate(count_over_time({job="fraiseql-app"} | json | level="error" [5m]))
```

**Query #6 (Line 247):**
```markdown
# BEFORE:
{job="postgresql"} | regexp "duration: (?P<duration>\\d+\\.\\d+) ms" | duration > 1000

# AFTER:
{job="postgresql"}
  | regexp "duration: (?P<duration>\\d+\\.\\d+) ms"
  | unwrap duration
  | __error__=""
  | duration > 1000
```

**Query #9 (Line 264):**
```markdown
# BEFORE:
sum by (context_tenant_id) (
  count_over_time({job="fraiseql-app"} | json [1h])
)

# AFTER:
# Option 1: If tenant_id is a label (requires Promtail config change)
sum by (tenant_id) (
  count_over_time({job="fraiseql-app"} [1h])
)

# Option 2: If tenant_id is JSON field (current config)
# Note: Cannot aggregate by JSON field - must extract as label first
# Add to docs: "To aggregate by tenant, add tenant_id as label in Promtail"
```

**Add new section after line 278:**
```markdown
### Query Syntax Notes

**Important:** The `[time]` syntax (e.g., `[1h]`, `[5m]`) is ONLY for range vector operations:
- ✅ `count_over_time({job="app"} [1h])` - Correct
- ✅ `rate(count_over_time({job="app"} [5m]))` - Correct
- ❌ `{job="app"} [1h]` - Invalid (time range set in Grafana)

**Label vs JSON Field:**
- Labels: Indexed, fast filtering, used in `{}` selector
- JSON fields: Not indexed, slower, used after `| json`
- Rule: Filter by labels first, then parse JSON
```

**Verification:**
```bash
# Test each corrected query against running Loki
LOKI_URL="http://localhost:3100"

# Query 1
curl -G "$LOKI_URL/loki/api/v1/query" \
  --data-urlencode 'query={job="fraiseql-app"} | json | level="error"' \
  --data-urlencode 'time='"$(date +%s)" | jq '.status'
# Expected: "success"

# Query 3
curl -G "$LOKI_URL/loki/api/v1/query_range" \
  --data-urlencode 'query=rate(count_over_time({job="fraiseql-app"} | json | level="error" [5m]))' \
  --data-urlencode 'start='"$(($(date +%s)-3600))" \
  --data-urlencode 'end='"$(date +%s)" \
  --data-urlencode 'step=60' | jq '.status'
# Expected: "success"
```

---

### Task 1.4: Fix Security Issues (CRITICAL)

**File:** `examples/observability/docker-compose.loki.yml`

**Changes:**

**1. Fix Grafana default password (lines 44-45):**
```yaml
# BEFORE:
- GF_SECURITY_ADMIN_PASSWORD=admin

# AFTER:
- GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD:-changeme}
```

**2. Add security note to compose file (after line 1):**
```yaml
# Security Warning: Set environment variables before deploying:
#   export GRAFANA_ADMIN_PASSWORD='your-secure-password'
# For production, see docs/production/loki_integration.md#security
```

**3. Add Docker socket proxy (add new service after line 36):**
```yaml
  docker-socket-proxy:
    image: tecnativa/docker-socket-proxy:latest
    container_name: fraiseql-docker-proxy
    environment:
      - CONTAINERS=1
      - SERVICES=0
      - TASKS=0
      - NETWORKS=0
      - IMAGES=0
      - INFO=0
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
    networks:
      - observability
    restart: unless-stopped

  promtail:
    # ... existing config ...
    volumes:
      - ./loki/promtail-config.yaml:/etc/promtail/config.yml
      - /var/log:/var/log:ro
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      # REMOVE: - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - DOCKER_HOST=tcp://docker-socket-proxy:2375
    depends_on:
      - docker-socket-proxy
      - loki
```

**File:** `docs/production/loki_integration.md`

**Add before line 43 (Quick Start):**
```markdown
### Security Setup (Required)

**Before starting the stack:**

```bash
# Set secure Grafana password
export GRAFANA_ADMIN_PASSWORD='your-secure-password-here'

# For production, also configure:
# - TLS/SSL certificates
# - Authentication (see Security section)
# - Network isolation
```

**⚠️ Warning:** The default setup uses:
- Filesystem storage (not suitable for production scale)
- Single instance (no high availability)
- Docker socket access (security risk)

See [Production Configuration](#production-configuration) for hardening.
```

**Verification:**
```bash
# Test with environment variable
export GRAFANA_ADMIN_PASSWORD='test123'
docker-compose -f examples/observability/docker-compose.loki.yml up -d

# Verify password is NOT 'admin'
curl -u admin:admin http://localhost:3000/api/org 2>&1 | grep -q "401 Unauthorized"
echo $?  # Should be 0 (password is not 'admin')

curl -u admin:test123 http://localhost:3000/api/org | jq '.name'
# Should return org name (authenticated)
```

---

## Phase 2: Important Configuration Improvements (SHOULD DO)

### Objective
Add missing production-critical features and improve performance.

**Estimated Time:** 45 minutes

---

### Task 2.1: Add Missing Loki Configuration Options

**File:** `examples/observability/loki/loki-config.yaml`

**Add after line 44 (after `limits_config`):**
```yaml
# Query performance optimization
query_scheduler:
  max_outstanding_requests_per_tenant: 2048

frontend:
  max_outstanding_per_tenant: 256
  compress_responses: true
  log_queries_longer_than: 5s

# Chunk caching for better query performance
chunk_store_config:
  max_look_back_period: 720h  # 30 days
  chunk_cache_config:
    enable_fifocache: true
    fifocache:
      max_size_bytes: 1073741824  # 1GB
      ttl: 1h

# Index caching
storage_config:
  boltdb_shipper:
    active_index_directory: /loki/boltdb-shipper-active
    cache_location: /loki/boltdb-shipper-cache
    cache_ttl: 24h
    shared_store: filesystem
  index_queries_cache_config:
    enable_fifocache: true
    fifocache:
      max_size_bytes: 268435456  # 256MB
      ttl: 24h
```

**Update comments at end of file (after line 62):**
```yaml
# For production, switch to object storage (S3/GCS):
#
# storage_config:
#   aws:
#     s3: s3://region/bucket-name
#     s3forcepathstyle: false
#     bucketnames: your-loki-bucket
#     region: us-east-1
#     sse_encryption: true
#
#   # For GCS:
#   gcs:
#     bucket_name: loki-chunks
#     chunk_buffer_size: 10485760  # 10MB
#     request_timeout: 60s
#
# # For high availability (3+ instances):
# common:
#   replication_factor: 3
#   ring:
#     kvstore:
#       store: memberlist
#
# memberlist:
#   join_members:
#     - loki-1:7946
#     - loki-2:7946
#     - loki-3:7946
```

---

### Task 2.2: Fix Grafana Trace Correlation

**File:** `examples/observability/loki/grafana-datasources.yaml`

**Problem:** Regex won't match JSON format logs.

**Replace entire file:**
```yaml
apiVersion: 1

datasources:
  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    isDefault: true
    editable: true
    jsonData:
      maxLines: 1000
      derivedFields:
        # Link logs to traces via trace_id (JSON format)
        - datasourceUid: tempo
          matcherRegex: '"trace_id"\s*:\s*"([0-9a-f]{32})"'
          name: TraceID
          url: '$${__value.raw}'
          urlDisplayLabel: 'View Trace in Tempo'

        # Also match plain text format: trace_id=abc123
        - datasourceUid: tempo
          matcherRegex: 'trace_id=([0-9a-f]{32})'
          name: TraceID
          url: '$${__value.raw}'
          urlDisplayLabel: 'View Trace in Tempo'

  # Note: This requires Tempo datasource to be configured
  # Add to your Tempo datasource config:
  # - name: Tempo
  #   uid: tempo
  #   type: tempo
  #   url: http://tempo:3200
```

**Verification:**
```bash
# Test with sample JSON log containing trace_id
echo '{
  "timestamp": "2025-12-04T10:00:00Z",
  "level": "error",
  "trace_id": "abc123def456789012345678901234ab",
  "message": "Test error"
}' | grep -oP '"trace_id"\s*:\s*"\K[0-9a-f]{32}'

# Should output: abc123def456789012345678901234ab
```

---

### Task 2.3: Add Optional Tenant Label Extraction

**File:** `examples/observability/loki/promtail-config.yaml`

**Add after line 32 (in `fraiseql-app` pipeline):**
```yaml
    pipeline_stages:
      # Parse JSON logs
      - json:
          expressions:
            timestamp: timestamp
            level: level
            message: message
            trace_id: trace_id
            span_id: span_id
            exception_type: exception_type
            fingerprint: fingerprint
            tenant_id: context.tenant_id  # NEW: Extract tenant from context

      # Extract labels for efficient filtering
      - labels:
          level:
          exception_type:
          tenant_id:  # NEW: Add as label (low-medium cardinality)

      # Drop if tenant_id is missing (optional - prevents null labels)
      - match:
          selector: '{tenant_id=""}'
          action: drop
```

**Add comment explaining cardinality:**
```yaml
      # Cardinality note: Only add tenant_id as label if you have:
      # - < 1000 tenants (low cardinality)
      # - Need to filter/aggregate by tenant frequently
      #
      # If you have > 1000 tenants, keep as JSON field and query with:
      # {job="fraiseql-app"} | json | tenant_id="specific_tenant"
```

---

## Phase 3: Documentation Improvements (SHOULD DO)

### Objective
Fix incorrect information and add missing critical sections.

**Estimated Time:** 60 minutes

---

### Task 3.1: Fix Storage Estimates

**File:** `docs/production/loki_integration.md`

**Replace section starting at line 377:**
```markdown
### Storage Estimates

**Assumptions:**
- 100 req/sec = ~8.6M requests/day
- Average **5-10 log entries per request** (start, end, DB queries, errors)
- Average log entry: 500 bytes
- Compression ratio: 10:1 (Loki uses efficient compression)

**Calculations:**

```
Logs per day:
  100 req/sec × 5 logs/req × 86,400 sec/day = 43M logs/day

Raw size:
  43M logs × 500 bytes = 21.5 GB/day (uncompressed)

Compressed (Loki storage):
  21.5 GB ÷ 10 = 2.15 GB/day
```

**Storage Requirements:**

| Retention | Compressed Size | Raw Size (if exported) |
|-----------|----------------|------------------------|
| 7 days    | ~15 GB         | ~150 GB                |
| 30 days   | ~65 GB         | ~645 GB                |
| 90 days   | ~195 GB        | ~1.9 TB                |

**For production monitoring:**

```bash
# Check actual storage usage
docker exec fraiseql-loki du -sh /loki/chunks

# Monitor ingestion rate
curl http://localhost:3100/metrics | grep loki_distributor_bytes_received_total

# Calculate daily ingestion (bytes over 24h)
# Set alerts if usage exceeds estimates by 50%
```

**Storage Optimization:**

1. **Drop debug logs** (saves ~40% in development)
2. **Sample high-volume info logs** (keep 10%, saves ~30%)
3. **Shorter retention** for non-error logs
4. **Use S3/GCS** with lifecycle policies (archive to Glacier after 90 days)
```

---

### Task 3.2: Add Query Optimization Section

**File:** `docs/production/loki_integration.md`

**Add new section after line 278 (after Query #10):**
```markdown
---

## Query Optimization Best Practices

### Rule 1: Label Filters ALWAYS Come First

**Why:** Loki indexes labels, not log content. Filtering by labels is 100-1000× faster.

```logql
# ✅ FAST: Filter by labels first (1-10ms)
{job="fraiseql-app", level="error"} | json

# ❌ SLOW: Parse all logs, then filter (1000-10000ms)
{job="fraiseql-app"} | json | level="error"
```

**Performance Impact:**
- Label filter: Scans 1 stream (error logs only)
- JSON filter: Scans ALL streams, parses JSON, then filters

### Rule 2: Limit Time Range

```logql
# ✅ GOOD: Narrow time range
{job="fraiseql-app", level="error"}  # Last 1h in Grafana

# ❌ BAD: Wide time range
{job="fraiseql-app", level="error"}  # Last 30 days in Grafana
```

**Recommendations:**
- Exploratory queries: 1h
- Dashboards: 5m-15m
- Alerting: 5m
- Avoid: > 24h (very slow)

### Rule 3: Use Line Filters Before JSON Parsing

```logql
# ✅ FAST: Filter lines before parsing
{job="fraiseql-app"} |= "DatabaseConnectionError" | json

# ❌ SLOW: Parse all logs, then filter
{job="fraiseql-app"} | json | exception_type="DatabaseConnectionError"
```

**Line filter operators:**
- `|=` : Contains (fast text search)
- `!=` : Not contains
- `|~` : Regex match (slower, but still faster than JSON parsing)
- `!~` : Regex not match

### Rule 4: Avoid High-Cardinality Labels in Aggregations

```logql
# ❌ VERY SLOW: Aggregating by trace_id (millions of values)
sum by (trace_id) (count_over_time({job="fraiseql-app"} [1h]))

# ✅ FAST: Aggregate by low-cardinality label
sum by (level) (count_over_time({job="fraiseql-app"} [1h]))
```

**Cardinality Guidelines:**
- Low (< 10 values): level, environment, job
- Medium (10-100): exception_type, container
- High (100-1000): tenant_id (OK if needed)
- Very High (> 1000): trace_id, user_id, request_id ⚠️ NEVER use as label

### Rule 5: Use `count_over_time` for Metrics

```logql
# ✅ CORRECT: Count logs over time
rate(count_over_time({job="fraiseql-app", level="error"} [5m]))

# ❌ INCORRECT: Missing count_over_time
rate({job="fraiseql-app", level="error"} [5m])  # Syntax error
```

### Query Performance Checklist

Before running a query, verify:

- [ ] Label filters in `{}` selector (not after `| json`)
- [ ] Time range < 1h for ad-hoc queries
- [ ] Line filters (`|=`, `|~`) before JSON parsing
- [ ] No high-cardinality labels in aggregations
- [ ] Using `count_over_time` for rate calculations

### Common Query Patterns

**Pattern: Find all logs for a trace**
```logql
{job="fraiseql-app"} | json | trace_id="abc123def456"
```

**Pattern: Error rate per minute**
```logql
sum(rate(count_over_time({job="fraiseql-app", level="error"} [1m])))
```

**Pattern: Top error types**
```logql
topk(10,
  sum by (exception_type) (
    count_over_time({job="fraiseql-app", level="error"} [1h])
  )
)
```

**Pattern: Logs matching pattern (fast)**
```logql
{job="fraiseql-app", level="error"} |= "database" |= "connection" | json
```

**Pattern: Extract numeric values and filter**
```logql
{job="postgresql"}
  |= "duration:"
  | regexp "duration: (?P<ms>\\d+\\.\\d+) ms"
  | unwrap ms
  | __error__=""
  | ms > 1000
```
```

---

### Task 3.3: Add PostgreSQL vs Loki Clarification

**File:** `docs/production/loki_integration.md`

**Add new section after line 35 (after "Benefits"):**
```markdown
---

## FraiseQL Integration: PostgreSQL + Loki

FraiseQL uses **both** PostgreSQL and Loki for complementary purposes:

### PostgreSQL `monitoring.errors` Table
**Purpose:** Error tracking and management

**Use for:**
- ✅ **Error metadata** (fingerprint, exception_type, occurred_at)
- ✅ **Error state management** (resolved_at, assignee, ignored)
- ✅ **Long-term error statistics** (trends, top errors, resolution time)
- ✅ **Error deduplication** (group by fingerprint)
- ✅ **Queryable error catalog** (SQL queries, JOINs with other tables)

**Storage:** ~1 KB per unique error (metadata only)
**Retention:** Indefinite (or years)

### Loki Logs
**Purpose:** Log context and debugging

**Use for:**
- ✅ **Full log context** (logs before/after error)
- ✅ **Trace correlation** (jump from log → trace in Grafana)
- ✅ **Real-time log streaming** (tail -f equivalent)
- ✅ **Pattern matching** (find similar errors via regex)
- ✅ **Temporary debugging data** (retained 30-90 days, then discarded)

**Storage:** ~500 bytes per log entry (all application logs)
**Retention:** 30-90 days

### Recommended Workflow

**1. Application logs an error:**
```python
# Log to Loki (full context)
logger.error(
    "Database connection failed",
    extra={
        "trace_id": trace_id,
        "exception_type": "DatabaseConnectionError",
        "context": {"pool_size": 10, "timeout": 5}
    }
)

# Also store in PostgreSQL (metadata)
await db.execute("""
    INSERT INTO monitoring.errors (
        fingerprint, exception_type, message, trace_id, occurred_at
    ) VALUES ($1, $2, $3, $4, NOW())
    ON CONFLICT (fingerprint) DO UPDATE
        SET last_occurred_at = NOW(), occurrence_count = errors.occurrence_count + 1
""", fingerprint, "DatabaseConnectionError", message, trace_id)
```

**2. Developer investigates error:**

```sql
-- Query PostgreSQL: See error frequency and trends
SELECT
    exception_type,
    COUNT(*) as occurrences,
    MAX(occurred_at) as last_seen
FROM monitoring.errors
WHERE resolved_at IS NULL
GROUP BY exception_type
ORDER BY occurrences DESC;

-- Find specific error with trace_id
SELECT trace_id, message, context
FROM monitoring.errors
WHERE fingerprint = 'db_connection_timeout'
ORDER BY occurred_at DESC
LIMIT 1;
```

In Grafana:
1. Open trace in Tempo (using trace_id from PostgreSQL)
2. Click span with error
3. Click "Logs for this span" → View full context in Loki
4. See all logs before/after error (application state, DB queries, etc.)

**3. Developer resolves error:**
```sql
-- Mark as resolved in PostgreSQL
UPDATE monitoring.errors
SET resolved_at = NOW(), assignee = 'dev@company.com'
WHERE fingerprint = 'db_connection_timeout';
```

Loki logs remain until retention expires (no cleanup needed).

### Decision Matrix: PostgreSQL or Loki?

| Question | Use PostgreSQL | Use Loki |
|----------|---------------|----------|
| Store error for long-term analysis? | ✅ Yes | ❌ No (30-90 day retention) |
| Track error resolution status? | ✅ Yes | ❌ No |
| Assign error to developer? | ✅ Yes | ❌ No |
| Query error trends (SQL)? | ✅ Yes | ❌ No (use LogQL metrics) |
| View full application log context? | ❌ No | ✅ Yes |
| Correlate with distributed traces? | ❌ No (only stores trace_id) | ✅ Yes (native Grafana integration) |
| Real-time log streaming? | ❌ No | ✅ Yes |
| Pattern matching across logs? | ❌ No | ✅ Yes (LogQL regex) |

**Summary:** PostgreSQL = **error management**, Loki = **log debugging**. Use both together for complete observability.
```

---

### Task 3.4: Add Monitoring & Alerting Section

**File:** `docs/production/loki_integration.md`

**Add new section before line 591 (before "References"):**
```markdown
---

## Monitoring Loki Itself

For production deployments, monitor Loki's health and performance.

### Key Metrics

**Ingestion Health:**
```promql
# Ingestion rate (should be steady)
rate(loki_distributor_bytes_received_total[5m])

# Ingestion errors (should be 0)
rate(loki_distributor_lines_received_total{status="error"}[5m])

# Stream creation rate (watch for cardinality explosion)
rate(loki_ingester_streams_created_total[5m])
```

**Query Performance:**
```promql
# Query duration P99 (should be < 5s)
histogram_quantile(0.99,
  rate(loki_request_duration_seconds_bucket{route="loki_api_v1_query_range"}[5m])
)

# Slow queries (> 5s)
rate(loki_slow_queries_total[5m])
```

**Storage Health:**
```promql
# Chunk flush failures (should be 0)
rate(loki_ingester_chunks_flush_errors_total[5m])

# Compaction success rate
rate(loki_compactor_runs_completed_total[5m])
```

### Recommended Alerts

**Alert Rules for Prometheus:**
```yaml
groups:
  - name: loki
    interval: 30s
    rules:
      # CRITICAL: High stream creation (cardinality explosion)
      - alert: LokiHighCardinality
        expr: rate(loki_ingester_streams_created_total[5m]) > 100
        for: 5m
        severity: critical
        annotations:
          summary: "Loki stream creation rate is high"
          description: "Creating {{ $value }} streams/sec. Check for high-cardinality labels (trace_id, user_id, etc.)"

      # CRITICAL: Ingestion failing
      - alert: LokiIngestionFailing
        expr: rate(loki_distributor_lines_received_total{status="error"}[5m]) > 10
        for: 2m
        severity: critical
        annotations:
          summary: "Loki failing to ingest logs"
          description: "{{ $value }} errors/sec. Check Promtail and Loki logs."

      # WARNING: Queries are slow
      - alert: LokiSlowQueries
        expr: |
          histogram_quantile(0.99,
            rate(loki_request_duration_seconds_bucket{route="loki_api_v1_query_range"}[5m])
          ) > 10
        for: 5m
        severity: warning
        annotations:
          summary: "Loki queries are slow (P99 > 10s)"
          description: "Consider optimizing queries or scaling Loki."

      # CRITICAL: Chunk flush failures
      - alert: LokiChunkFlushFailing
        expr: rate(loki_ingester_chunks_flush_errors_total[5m]) > 0
        for: 5m
        severity: critical
        annotations:
          summary: "Loki failing to flush chunks to storage"
          description: "Check storage backend (S3/GCS) connectivity and permissions."

      # WARNING: Disk usage high (filesystem storage only)
      - alert: LokiDiskUsageHigh
        expr: |
          (node_filesystem_size_bytes{mountpoint="/loki"} - node_filesystem_avail_bytes{mountpoint="/loki"})
          / node_filesystem_size_bytes{mountpoint="/loki"} > 0.85
        for: 10m
        severity: warning
        annotations:
          summary: "Loki storage disk usage > 85%"
          description: "Consider reducing retention or scaling storage."
```

### Health Check Endpoints

```bash
# Loki readiness (should return 200)
curl -f http://localhost:3100/ready

# Loki metrics (Prometheus format)
curl http://localhost:3100/metrics

# Promtail readiness
curl -f http://localhost:9080/ready

# Promtail targets (check which logs are being tailed)
curl http://localhost:9080/targets | jq
```

### Dashboard Recommendations

**Import Grafana Dashboards:**

1. **Loki Operational Dashboard** (ID: 13407)
   - Ingestion rate and volume
   - Query performance
   - Storage usage

2. **Loki Logs Dashboard** (ID: 13639)
   - Log volume by job and level
   - Error rate trends
   - Top loggers

3. **Promtail Dashboard** (ID: 15443)
   - Files being tailed
   - Parsing errors
   - Lag and backlog

```bash
# Import via API
curl -X POST http://localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -u admin:${GRAFANA_ADMIN_PASSWORD} \
  -d '{"dashboard": {"id": 13407}, "overwrite": true}'
```

### Troubleshooting with Metrics

**Problem: Logs not appearing in Loki**

```bash
# Check Promtail is reading files
curl http://localhost:9080/metrics | grep promtail_read_lines_total
# Should be increasing

# Check Promtail is sending to Loki
curl http://localhost:9080/metrics | grep promtail_sent_entries_total
# Should match read_lines_total

# Check Loki is receiving
curl http://localhost:3100/metrics | grep loki_distributor_lines_received_total
# Should be increasing
```

**Problem: High memory usage**

```bash
# Check stream count (should be < 100,000)
curl http://localhost:3100/loki/api/v1/labels | jq '.data | length'

# Check for high-cardinality labels
curl http://localhost:3100/metrics | grep loki_ingester_streams_created_total
# If > 100/sec, you have cardinality issues
```

**Problem: Slow queries**

```bash
# Enable query logging in loki-config.yaml
frontend:
  log_queries_longer_than: 1s

# Check slow query log
docker logs fraiseql-loki | grep "slow query"
```
```

---

### Task 3.5: Add Security Hardening Section

**File:** `docs/production/loki_integration.md`

**Replace section starting at line 517 (entire Security section):**
```markdown
---

## Security Hardening

### Development vs Production Security

**Development (current config):**
- ❌ No authentication (`auth_enabled: false`)
- ❌ Default Grafana password
- ❌ Docker socket access (security risk)
- ✅ OK for local development

**Production requirements:**
- ✅ Authentication enabled
- ✅ TLS/SSL for all connections
- ✅ Network isolation
- ✅ Secrets management
- ✅ Log sanitization

---

### Authentication

#### Option 1: Multi-Tenancy (Recommended)

**Best for:** Multiple teams or applications sharing Loki

**Loki config:**
```yaml
auth_enabled: true

# Each tenant is isolated by X-Scope-OrgID header
```

**Promtail config:**
```yaml
clients:
  - url: http://loki:3100/loki/api/v1/push
    tenant_id: tenant-production  # Sets X-Scope-OrgID header
```

**Grafana datasource:**
```yaml
jsonData:
  httpHeaderName1: 'X-Scope-OrgID'
secureJsonData:
  httpHeaderValue1: 'tenant-production'
```

**Query isolation:**
```bash
# Tenant A can only see their logs
curl -H "X-Scope-OrgID: tenant-a" http://loki:3100/loki/api/v1/query?query={job="app"}

# Tenant B sees different logs
curl -H "X-Scope-OrgID: tenant-b" http://loki:3100/loki/api/v1/query?query={job="app"}
```

#### Option 2: Reverse Proxy with Basic Auth

**Best for:** Single team, simple auth

**Nginx config:**
```nginx
upstream loki {
    server loki:3100;
}

server {
    listen 443 ssl;
    server_name loki.company.com;

    ssl_certificate /etc/nginx/ssl/server.crt;
    ssl_certificate_key /etc/nginx/ssl/server.key;

    location /loki/api/v1/push {
        auth_basic "Loki Push";
        auth_basic_user_file /etc/nginx/.htpasswd-push;
        proxy_pass http://loki;
    }

    location /loki/api/v1/query {
        auth_basic "Loki Query";
        auth_basic_user_file /etc/nginx/.htpasswd-query;
        proxy_pass http://loki;
    }
}
```

**Create auth file:**
```bash
htpasswd -c /etc/nginx/.htpasswd-push promtail
htpasswd -c /etc/nginx/.htpasswd-query grafana
```

**Promtail with basic auth:**
```yaml
clients:
  - url: https://loki.company.com/loki/api/v1/push
    basic_auth:
      username: promtail
      password: ${PROMTAIL_PASSWORD}
    tls_config:
      ca_file: /etc/ssl/certs/ca.crt
```

#### Option 3: OAuth2 Proxy

**Best for:** Enterprise with existing OAuth provider (Okta, Google, Azure AD)

```bash
# Deploy oauth2-proxy in front of Loki
docker run -d \
  -p 4180:4180 \
  quay.io/oauth2-proxy/oauth2-proxy \
  --provider=oidc \
  --client-id=your-client-id \
  --client-secret=your-client-secret \
  --oidc-issuer-url=https://your-idp.com \
  --upstream=http://loki:3100 \
  --email-domain=company.com
```

---

### TLS/SSL Encryption

**Loki server TLS:**
```yaml
server:
  http_listen_port: 3100
  grpc_listen_port: 9096
  http_tls_config:
    cert_file: /etc/loki/tls/server.crt
    key_file: /etc/loki/tls/server.key
    client_auth_type: RequireAndVerifyClientCert
    client_ca_file: /etc/loki/tls/ca.crt
  grpc_tls_config:
    cert_file: /etc/loki/tls/server.crt
    key_file: /etc/loki/tls/server.key
    client_auth_type: RequireAndVerifyClientCert
    client_ca_file: /etc/loki/tls/ca.crt
```

**Promtail client TLS:**
```yaml
clients:
  - url: https://loki:3100/loki/api/v1/push
    tls_config:
      ca_file: /etc/promtail/tls/ca.crt
      cert_file: /etc/promtail/tls/client.crt
      key_file: /etc/promtail/tls/client.key
      insecure_skip_verify: false
```

**Generate self-signed certs (development only):**
```bash
# CA certificate
openssl req -x509 -newkey rsa:4096 -days 365 -nodes \
  -keyout ca.key -out ca.crt \
  -subj "/CN=Loki CA"

# Server certificate
openssl req -newkey rsa:4096 -nodes \
  -keyout server.key -out server.csr \
  -subj "/CN=loki"

openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out server.crt -days 365

# Client certificate
openssl req -newkey rsa:4096 -nodes \
  -keyout client.key -out client.csr \
  -subj "/CN=promtail"

openssl x509 -req -in client.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out client.crt -days 365
```

**For production:** Use Let's Encrypt or your organization's PKI.

---

### Network Isolation

**Docker network isolation:**
```yaml
networks:
  loki-internal:
    internal: true  # No external internet access
  loki-external:
    internal: false  # Only for ingress

services:
  loki:
    networks:
      - loki-internal  # Only accessible within Docker network

  promtail:
    networks:
      - loki-internal

  nginx-proxy:
    networks:
      - loki-internal
      - loki-external  # Exposes to internet
    ports:
      - "443:443"
```

**Kubernetes NetworkPolicy:**
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: loki-network-policy
spec:
  podSelector:
    matchLabels:
      app: loki
  policyTypes:
    - Ingress
  ingress:
    # Only allow from Promtail and Grafana
    - from:
      - podSelector:
          matchLabels:
            app: promtail
      - podSelector:
          matchLabels:
            app: grafana
      ports:
        - protocol: TCP
          port: 3100
```

---

### Log Sanitization

**Problem:** Logs may contain sensitive data (passwords, tokens, PII).

**Solution 1: Sanitize at log source (Python):**
```python
import re

class SanitizingFormatter(logging.Formatter):
    SENSITIVE_PATTERNS = [
        (r'password["\s:=]+([^"\s,}]+)', r'password=***'),
        (r'token["\s:=]+([^"\s,}]+)', r'token=***'),
        (r'api[_-]?key["\s:=]+([^"\s,}]+)', r'api_key=***'),
        (r'\b\d{3}-\d{2}-\d{4}\b', r'***-**-****'),  # SSN
        (r'\b\d{16}\b', r'****************'),  # Credit card
    ]

    def format(self, record):
        message = super().format(record)
        for pattern, replacement in self.SENSITIVE_PATTERNS:
            message = re.sub(pattern, replacement, message, flags=re.IGNORECASE)
        return message
```

**Solution 2: Sanitize in Promtail:**
```yaml
pipeline_stages:
  - json:
      expressions:
        message: message

  # Replace sensitive patterns
  - replace:
      expression: '(password|token|api_key)["\s:=]+([^"\s,}]+)'
      replace: '\1=***'

  - replace:
      expression: '\b\d{3}-\d{2}-\d{4}\b'
      replace: '***-**-****'
```

**Solution 3: Drop sensitive logs entirely:**
```yaml
pipeline_stages:
  - match:
      selector: '{job="fraiseql-app"} |~ "(?i)(password|secret|token)"'
      action: drop  # Don't send to Loki
```

---

### Docker Socket Security

**Problem:** Promtail needs Docker socket access to read container logs, which is a security risk.

**Solution: Use Docker socket proxy (limited permissions):**
```yaml
services:
  docker-socket-proxy:
    image: tecnativa/docker-socket-proxy:latest
    environment:
      - CONTAINERS=1     # Allow listing containers
      - SERVICES=0       # Deny service management
      - TASKS=0          # Deny task management
      - NETWORKS=0       # Deny network access
      - IMAGES=0         # Deny image access
      - VOLUMES=0        # Deny volume access
      - INFO=0           # Deny system info
      - BUILD=0          # Deny builds
      - COMMIT=0         # Deny commits
      - EXEC=0           # Deny exec
      - SWARM=0          # Deny swarm
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
    networks:
      - loki-internal

  promtail:
    environment:
      - DOCKER_HOST=tcp://docker-socket-proxy:2375
    # Remove direct socket mount:
    # volumes:
    #   - /var/run/docker.sock:/var/run/docker.sock  # REMOVE THIS
```

**Alternative:** Use log files instead of Docker API:
```yaml
# Write container logs to files, tail files with Promtail
# Configure Docker daemon to use json-file logging:
# /etc/docker/daemon.json
{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3"
  }
}
```

---

### Secrets Management

**❌ BAD: Hardcoded secrets**
```yaml
clients:
  - url: https://loki:3100/loki/api/v1/push
    basic_auth:
      username: promtail
      password: hardcoded-password  # ❌ Never do this
```

**✅ GOOD: Environment variables**
```yaml
clients:
  - url: https://loki:3100/loki/api/v1/push
    basic_auth:
      username: ${LOKI_USERNAME}
      password: ${LOKI_PASSWORD}
```

**✅ BETTER: Docker secrets**
```yaml
services:
  promtail:
    secrets:
      - loki_password
    environment:
      - LOKI_PASSWORD_FILE=/run/secrets/loki_password

secrets:
  loki_password:
    external: true
```

**✅ BEST: Kubernetes secrets + sealed-secrets**
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: loki-auth
type: Opaque
data:
  username: cHJvbXRhaWw=  # base64 encoded
  password: <encrypted-by-sealed-secrets>
```

---

### Production Security Checklist

- [ ] Authentication enabled (`auth_enabled: true` or reverse proxy)
- [ ] TLS/SSL configured for all connections
- [ ] Secrets stored in vault/secrets manager (not hardcoded)
- [ ] Network isolation (internal network for Loki/Promtail)
- [ ] Log sanitization (remove passwords, tokens, PII)
- [ ] Docker socket proxy (limited permissions)
- [ ] Grafana admin password changed from default
- [ ] Regular security updates (Loki, Promtail, Grafana images)
- [ ] Monitoring alerts configured (failed authentication, high error rate)
- [ ] Backup strategy for Loki configuration and data

**For classified environments (DoD IL4/IL5):**
- [ ] FIPS 140-2 compliant encryption
- [ ] Audit logging enabled
- [ ] No internet egress (air-gapped deployment)
- [ ] Government-approved PKI certificates
- [ ] STIG hardening applied to all containers
```

---

## Phase 4: Testing & Validation (MUST DO)

### Objective
Verify all fixes work correctly before committing.

**Estimated Time:** 30 minutes

---

### Task 4.1: Smoke Test Docker Compose Stack

**Create test script:**
```bash
#!/bin/bash
# File: examples/observability/test-loki-stack.sh

set -e

echo "=== Loki Integration Smoke Test ==="

# Set test password
export GRAFANA_ADMIN_PASSWORD='test123'

# Clean up any existing containers
echo "Cleaning up..."
docker-compose -f docker-compose.loki.yml down -v

# Start stack
echo "Starting Loki stack..."
docker-compose -f docker-compose.loki.yml up -d

# Wait for services to be healthy
echo "Waiting for services..."
timeout 60 bash -c 'until curl -sf http://localhost:3100/ready; do sleep 2; done'
echo "✓ Loki ready"

timeout 60 bash -c 'until curl -sf http://localhost:9080/ready; do sleep 2; done'
echo "✓ Promtail ready"

timeout 60 bash -c 'until curl -sf http://localhost:3000/api/health; do sleep 2; done'
echo "✓ Grafana ready"

# Test log ingestion
echo "Testing log ingestion..."
curl -X POST http://localhost:3100/loki/api/v1/push \
  -H "Content-Type: application/json" \
  -d '{
    "streams": [{
      "stream": {"job": "test", "level": "info"},
      "values": [["'$(date +%s)000000000'", "Smoke test log entry"]]
    }]
  }'
echo "✓ Log pushed to Loki"

# Query log back
sleep 2
RESULT=$(curl -sG http://localhost:3100/loki/api/v1/query \
  --data-urlencode 'query={job="test"}' \
  | jq -r '.data.result | length')

if [ "$RESULT" -gt "0" ]; then
  echo "✓ Log query successful ($RESULT results)"
else
  echo "✗ Log query failed (no results)"
  exit 1
fi

# Test Grafana auth (default password should NOT work)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  -u admin:admin http://localhost:3000/api/org)

if [ "$HTTP_CODE" = "401" ]; then
  echo "✓ Grafana default password blocked"
else
  echo "✗ Grafana still uses default password"
  exit 1
fi

# Test with correct password
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  -u admin:test123 http://localhost:3000/api/org)

if [ "$HTTP_CODE" = "200" ]; then
  echo "✓ Grafana auth working"
else
  echo "✗ Grafana auth failed with test password"
  exit 1
fi

# Check for deprecated warnings in Loki logs
if docker logs fraiseql-loki 2>&1 | grep -qi "deprecated\|boltdb-shipper"; then
  echo "✗ Loki config has deprecated settings"
  docker logs fraiseql-loki 2>&1 | grep -i "deprecated"
  exit 1
else
  echo "✓ No deprecated config warnings"
fi

# Check stream count (should be low)
LABELS=$(curl -s http://localhost:3100/loki/api/v1/labels | jq '.data | length')
if [ "$LABELS" -lt "20" ]; then
  echo "✓ Label cardinality is low ($LABELS labels)"
else
  echo "⚠ Label cardinality is high ($LABELS labels) - possible cardinality issue"
fi

echo ""
echo "=== All smoke tests passed! ==="
echo ""
echo "Access services:"
echo "  Loki:    http://localhost:3100"
echo "  Grafana: http://localhost:3000 (admin / test123)"
echo ""
echo "Clean up with: docker-compose -f docker-compose.loki.yml down -v"
```

**Run test:**
```bash
chmod +x examples/observability/test-loki-stack.sh
cd examples/observability
./test-loki-stack.sh
```

---

### Task 4.2: Validate LogQL Queries

**Create query test script:**
```bash
#!/bin/bash
# File: examples/observability/test-logql-queries.sh

set -e

LOKI_URL="http://localhost:3100"

echo "=== Testing LogQL Queries ==="

# Helper function
test_query() {
  local name="$1"
  local query="$2"
  local endpoint="${3:-query}"

  echo -n "Testing: $name... "

  if [ "$endpoint" = "query_range" ]; then
    RESULT=$(curl -sG "$LOKI_URL/loki/api/v1/$endpoint" \
      --data-urlencode "query=$query" \
      --data-urlencode "start=$(($(date +%s)-3600))" \
      --data-urlencode "end=$(date +%s)" \
      --data-urlencode "step=60" \
      | jq -r '.status')
  else
    RESULT=$(curl -sG "$LOKI_URL/loki/api/v1/$endpoint" \
      --data-urlencode "query=$query" \
      | jq -r '.status')
  fi

  if [ "$RESULT" = "success" ]; then
    echo "✓"
  else
    echo "✗ FAILED"
    echo "  Query: $query"
    exit 1
  fi
}

# Query 1: Simple filter
test_query "All errors" \
  '{job="fraiseql-app"} | json | level="error"'

# Query 2: Trace filter
test_query "Logs for specific trace" \
  '{job="fraiseql-app"} | json | trace_id="abc123"'

# Query 3: Error rate
test_query "Rate of errors" \
  'rate(count_over_time({job="fraiseql-app"} | json | level="error" [5m]))' \
  "query_range"

# Query 4: Top error types
test_query "Top 10 error types" \
  'topk(10, sum by (exception_type) (count_over_time({job="fraiseql-app"} | json | level="error" [1h])))' \
  "query_range"

# Query 5: Pattern matching
test_query "Pattern matching" \
  '{job="fraiseql-app"} |~ "authentication|unauthorized"'

# Query 6: Line filter before JSON
test_query "Line filter optimization" \
  '{job="fraiseql-app"} |= "error" | json'

echo ""
echo "=== All LogQL queries passed! ==="
```

**Run test:**
```bash
chmod +x examples/observability/test-logql-queries.sh
cd examples/observability
./test-logql-queries.sh
```

---

### Task 4.3: Test Trace Correlation Regex

**Create regex test:**
```bash
#!/bin/bash
# Test Grafana derived field regex

# Sample JSON log
JSON_LOG='{"timestamp":"2025-12-04T10:00:00Z","level":"error","trace_id":"abc123def456789012345678901234ab","message":"Test"}'

# Test regex
REGEX='"trace_id"\s*:\s*"([0-9a-f]{32})"'

if echo "$JSON_LOG" | grep -oP "$REGEX" | grep -q "abc123def456789012345678901234ab"; then
  echo "✓ Trace correlation regex works"
else
  echo "✗ Trace correlation regex failed"
  exit 1
fi
```

---

## Phase 5: Documentation & Commit (MUST DO)

### Objective
Update documentation and commit all fixes.

**Estimated Time:** 15 minutes

---

### Task 5.1: Update Commit Message

**Commit all changes with descriptive message:**
```bash
git add -A

git commit -m "$(cat <<'EOF'
fix(observability): critical Loki configuration fixes and improvements

CRITICAL FIXES:
- Update schema from deprecated boltdb-shipper to TSDB v13
- Remove high-cardinality labels (trace_id, span_id, fingerprint)
- Fix LogQL query syntax errors in documentation
- Fix Grafana trace correlation regex for JSON logs
- Fix security issues (default Grafana password, Docker socket)

IMPROVEMENTS:
- Add query performance optimization guide
- Add comprehensive security hardening section
- Add Loki monitoring and alerting configuration
- Clarify PostgreSQL vs Loki use cases
- Add storage estimate corrections (2GB/day, not 430MB)
- Add production HA configuration examples
- Add smoke tests and query validation scripts

BREAKING CHANGES:
- Schema migration required: Delete existing Loki data or update schema
- Promtail config changes: Remove trace_id/span_id from labels
- Docker Compose requires GRAFANA_ADMIN_PASSWORD env var

Migration:
1. Stop Loki: docker-compose down
2. Delete old data: rm -rf loki-data
3. Set password: export GRAFANA_ADMIN_PASSWORD='secure-password'
4. Start: docker-compose up -d
5. Verify: ./test-loki-stack.sh

Fixes identified in expert review (see .phases/loki_fixes_implementation_plan.md)

Impact: Prevents production failures (cardinality explosion, deprecated configs)
EOF
)"
```

---

### Task 5.2: Create Migration Guide

**File:** `docs/production/loki_migration_v1_to_v2.md`

```markdown
# Loki Configuration Migration Guide

**From:** Initial Loki integration (commit c4e7632)
**To:** Fixed configuration (current)

## Summary of Changes

This migration fixes **critical issues** that would cause production failures:
1. Cardinality explosion from high-cardinality labels
2. Deprecated schema configuration
3. LogQL syntax errors

## Migration Steps

### Step 1: Backup Current Data (Optional)

```bash
# If you have important logs, backup Loki data
docker exec fraiseql-loki tar czf /tmp/loki-backup.tar.gz /loki
docker cp fraiseql-loki:/tmp/loki-backup.tar.gz ./loki-backup-$(date +%Y%m%d).tar.gz
```

### Step 2: Stop Loki Stack

```bash
cd examples/observability
docker-compose -f docker-compose.loki.yml down
```

### Step 3: Delete Old Data (Schema Change)

```bash
# Remove old boltdb-shipper data (incompatible with TSDB)
docker volume rm observability_loki-data
```

### Step 4: Pull New Configuration

```bash
git pull origin main  # Or your branch with fixes
```

### Step 5: Set Environment Variables

```bash
# Required: Set Grafana admin password
export GRAFANA_ADMIN_PASSWORD='your-secure-password'

# Optional: For production
export LOKI_RETENTION_DAYS=90
```

### Step 6: Start Loki Stack

```bash
docker-compose -f docker-compose.loki.yml up -d
```

### Step 7: Verify Migration

```bash
# Run smoke tests
./test-loki-stack.sh

# Check for warnings
docker logs fraiseql-loki 2>&1 | grep -i "warn\|error\|deprecated"
# Should see no deprecated warnings

# Check label count (should be < 20)
curl -s http://localhost:3100/loki/api/v1/labels | jq '.data | length'
```

### Step 8: Update Application Logging

If your application extracts labels in code, update to match new config:

```python
# OLD: Don't extract trace_id as label
# logger.info("message", extra={"trace_id": trace_id})  # ❌

# NEW: Just log normally, Promtail will extract from JSON
logger.info("message", extra={"trace_id": trace_id})  # ✅ Same, but Promtail doesn't make it a label
```

No code changes needed - just ensure logs are JSON format.

## Breaking Changes

### 1. Schema Version Change

- **Old:** `boltdb-shipper` + `schema: v11`
- **New:** `tsdb` + `schema: v13`
- **Impact:** Must delete old data (incompatible schema)

### 2. Label Changes

**Removed labels:**
- `trace_id` (now JSON field only)
- `span_id` (now JSON field only)
- `fingerprint` (now JSON field only)

**Query impact:**

```logql
# OLD (worked before, still works now via JSON)
{job="fraiseql-app", trace_id="abc123"}  # ❌ No longer works

# NEW (use JSON parsing)
{job="fraiseql-app"} | json | trace_id="abc123"  # ✅ Works
```

**Why:** Prevents cardinality explosion (millions of streams).

### 3. Grafana Password

- **Old:** Default `admin`/`admin`
- **New:** Set via `GRAFANA_ADMIN_PASSWORD` env var
- **Impact:** Must set env var before `docker-compose up`

## Rollback Plan

If migration fails:

```bash
# Stop new stack
docker-compose -f docker-compose.loki.yml down

# Restore old data backup
docker volume create observability_loki-data
docker run --rm -v observability_loki-data:/loki -v $(pwd):/backup alpine \
  tar xzf /backup/loki-backup-YYYYMMDD.tar.gz -C /

# Revert config
git checkout c4e7632  # Old commit

# Start old stack
docker-compose -f docker-compose.loki.yml up -d
```

## Support

If you encounter issues:

1. Check logs: `docker logs fraiseql-loki`
2. Run smoke tests: `./test-loki-stack.sh`
3. See troubleshooting: `docs/production/loki_integration.md#troubleshooting`
```

---

## Summary

### TDD Assessment: ❌ Not Applicable

**Reasoning:**
1. **No business logic:** Pure configuration changes (YAML, markdown)
2. **No code to unit test:** No Python/TypeScript functions to test
3. **Integration tests only:** Smoke tests verify services start correctly

**Verification Strategy Instead:**
- ✅ Smoke tests (Docker Compose stack starts)
- ✅ LogQL query validation (queries execute without errors)
- ✅ Manual review (regex testing, documentation accuracy)
- ✅ Integration testing (end-to-end log flow)

**When TDD WOULD apply:**
- If adding Python logging formatter → Test log output format
- If adding FraiseQL observability API → Test API responses
- If adding custom Loki client library → Test client behavior

---

### Implementation Checklist

**Phase 1: Critical Fixes (MUST DO)** - 45 min
- [ ] Task 1.1: Update schema to TSDB v13
- [ ] Task 1.2: Remove high-cardinality labels
- [ ] Task 1.3: Fix LogQL syntax errors
- [ ] Task 1.4: Fix security issues

**Phase 2: Important Improvements (SHOULD DO)** - 45 min
- [ ] Task 2.1: Add missing Loki config options
- [ ] Task 2.2: Fix Grafana trace correlation
- [ ] Task 2.3: Add optional tenant label extraction

**Phase 3: Documentation (SHOULD DO)** - 60 min
- [ ] Task 3.1: Fix storage estimates
- [ ] Task 3.2: Add query optimization guide
- [ ] Task 3.3: Clarify PostgreSQL vs Loki
- [ ] Task 3.4: Add monitoring & alerting
- [ ] Task 3.5: Expand security section

**Phase 4: Testing (MUST DO)** - 30 min
- [ ] Task 4.1: Create and run smoke tests
- [ ] Task 4.2: Validate LogQL queries
- [ ] Task 4.3: Test trace correlation

**Phase 5: Documentation & Commit (MUST DO)** - 15 min
- [ ] Task 5.1: Commit with detailed message
- [ ] Task 5.2: Create migration guide

**Total Estimated Time:** 2 hours 45 minutes

---

### Risk Assessment

**High Risk (Will cause production failure):**
- Cardinality explosion from trace_id labels → OOM crashes
- Deprecated schema → Incompatible with future Loki versions
- Syntax errors in queries → Dashboards won't work

**Medium Risk (Security/Performance):**
- Default Grafana password → Unauthorized access
- Missing query optimization → Slow dashboards
- Incorrect storage estimates → Disk space issues

**Low Risk (Nice-to-have):**
- Missing monitoring section → Harder to troubleshoot
- Missing HA config → Manual setup required

---

### Next Steps

1. **Review this plan** with stakeholders
2. **Execute Phase 1** (critical fixes) immediately
3. **Test thoroughly** with Phase 4 smoke tests
4. **Deploy to development** and monitor for issues
5. **Execute Phases 2-3** (improvements) as time permits
6. **Update production** with migration guide

**Estimated delivery:** Same day (if starting now)
