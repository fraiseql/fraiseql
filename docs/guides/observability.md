# Observability Guide for FraiseQL

**Status:** ✅ Production Ready
**Audience:** DevOps, SREs, Architects
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

---

## Prerequisites

### Required Knowledge:

- Observability fundamentals (logs, metrics, traces - the three pillars)
- Structured logging and JSON formats
- Time-series metrics and Prometheus concepts
- Distributed tracing and span concepts
- Change Data Capture (CDC) principles
- Audit logging and compliance requirements
- Multi-tenancy data isolation patterns

### Required Software:

- FraiseQL v2.0.0-alpha.1 or later
- PostgreSQL 14+ (for change log tables)
- Prometheus (for metrics collection)
- Grafana (for visualization) or alternative dashboarding tool
- Jaeger or Zipkin (for distributed tracing)
- Log aggregation tool (ELK, Splunk, DataDog, New Relic, or similar)
- curl or API client for testing

### Required Infrastructure:

- FraiseQL server instance
- PostgreSQL database with CDC support
- Prometheus scrape-compatible endpoint
- Log collection infrastructure (syslog, vector, fluentd, etc.)
- Trace backend (Jaeger collector, Zipkin server)
- Metrics storage (Prometheus or similar time-series database)
- Grafana or visualization tool
- Network connectivity between all components

#### Optional but Recommended:

- Kubernetes monitoring (Prometheus Operator)
- Alert manager for anomaly detection
- Custom grafana dashboards/templates
- Distributed tracing sampling strategies
- Log retention and archival policies
- Metrics correlation tools

**Time Estimate:** 1-3 hours for basic setup, 4-8 hours for production configuration with alerting

## 1. Overview

Observability in FraiseQL means understanding **what's happening** in your system through three pillars:

1. **Logs** — Detailed records of what occurred (mutations, errors, decisions)
2. **Metrics** — Aggregated measurements (rates, latencies, counts)
3. **Traces** — Request flows from entry to exit with timing

FraiseQL's observability is **database-first**: The database is the source of truth for both operational metrics and audit trails. This enables:

- **Deterministic debugging** — Exact state changes recorded in `tb_entity_change_log`
- **Compliance audits** — Complete mutation history with user/tenant context
- **Performance analysis** — Query execution patterns visible in logs
- **Real-time alerts** — Stream mutations to monitoring systems via CDC
- **Multi-tenant isolation** — All observations scoped by tenant

---

## 2. Mutation Observability

### 2.1 Entity Change Log — Source of Truth

The **`tb_entity_change_log` table** (see **docs/specs/schema-conventions.md section 6**) is the centralized audit log for all entity writes. This table provides:

- **Debezium envelope format** for CDC compatibility (see schema-conventions.md section 6.2)
- **Helper functions** for logging and response building (see schema-conventions.md section 6.4)
- **Status taxonomy** for machine-readable outcome tracking (see schema-conventions.md section 6.3)

```sql
-- Query recent mutations for a user
SELECT
    created_at,
    object_type,
    object_id,
    modification_type,
    change_status,
    object_data->>'before' AS before_state,
    object_data->>'after' AS after_state
FROM core.tb_entity_change_log
WHERE fk_customer_org = $tenant_id
  AND created_at > NOW() - INTERVAL '1 hour'
ORDER BY created_at DESC;
```

### What's recorded:

- **Before/After state** — Full entity snapshots (Debezium envelope)
- **Operation type** — INSERT, UPDATE, DELETE, or NOOP
- **Status** — Success, error, conflict, validation, noop, blocked
- **User context** — Who made the change
- **Tenant context** — Which organization it belongs to
- **Timestamp** — When it happened
- **Metadata** — Request ID, trigger source, custom fields

### 2.2 Mutation Metrics

#### Success/Failure Rates:

```sql
-- Mutation success rate by entity type (last 24 hours)
SELECT
    object_type,
    change_status LIKE 'success%' OR change_status IN ('new','updated','deleted') AS is_success,
    COUNT(*) AS count,
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE change_status LIKE 'success%' OR change_status IN ('new','updated','deleted'))
        / COUNT(*),
        2
    ) AS success_rate
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY object_type, is_success
ORDER BY object_type;
```

### Status Distribution:

```sql
-- Distribution of mutation outcomes (last 24 hours)
SELECT
    object_type,
    change_status,
    COUNT(*) AS count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (PARTITION BY object_type), 2) AS pct
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY object_type, change_status
ORDER BY object_type, count DESC;
```

### Common statuses:

- `new`, `updated`, `deleted`, `success` — Success
- `failed:*`, `not_found`, `forbidden` — Errors
- `conflict:*`, `duplicate:*` — Conflicts
- `validation:*` — Validation errors
- `noop:*`, `blocked:*` — No-ops

### Mutation Latency:

```sql
-- Mutations taking longer than 1 second (slow mutation detection)
SELECT
    created_at,
    object_type,
    object_id,
    change_status,
    extra_metadata->>'request_id' AS request_id,
    extra_metadata->>'user_id' AS user_id
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '1 hour'
  AND CAST(extra_metadata->>'duration_ms' AS INTEGER) > 1000
ORDER BY created_at DESC;
```

### Cascade Operation Counts:

```sql
-- Mutations that triggered cascade operations
SELECT
    object_type,
    change_status,
    COUNT(*) AS mutations,
    SUM(CAST(extra_metadata->>'cascade_count' AS INTEGER)) AS total_cascades,
    AVG(CAST(extra_metadata->>'cascade_count' AS INTEGER)) AS avg_cascades
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '24 hours'
  AND extra_metadata->>'cascade_count' IS NOT NULL
GROUP BY object_type, change_status;
```

### 2.3 Mutation Tracing

#### Correlation IDs Link Requests:

```sql
-- All mutations from a single API request
SELECT
    created_at,
    object_type,
    object_id,
    change_status,
    modification_type
FROM core.tb_entity_change_log
WHERE extra_metadata->>'request_id' = $request_id
ORDER BY created_at ASC;
```

### Trace Cascade Operations:

```sql
-- Follow cascade chain from parent deletion
WITH RECURSIVE cascade_chain AS (
    -- Base: find the original mutation
    SELECT
        pk_entity_change_log,
        object_type,
        object_id,
        change_status,
        created_at,
        0 AS depth,
        ARRAY[pk_entity_change_log] AS chain
    FROM core.tb_entity_change_log
    WHERE object_type = 'User'
      AND object_id = $user_id
      AND modification_type = 'DELETE'

    UNION ALL

    -- Find cascaded mutations
    SELECT
        c.pk_entity_change_log,
        c.object_type,
        c.object_id,
        c.change_status,
        c.created_at,
        cc.depth + 1,
        cc.chain || ARRAY[c.pk_entity_change_log]
    FROM cascade_chain cc
    JOIN core.tb_entity_change_log c
        ON c.extra_metadata->>'parent_mutation_id' = cc.pk_entity_change_log::TEXT
    WHERE cc.depth < 5  -- Prevent infinite loops
)
SELECT * FROM cascade_chain ORDER BY created_at, depth;
```

---

## 3. Query Observability

### 3.1 Query Performance Monitoring

#### Slow Query Detection:

```sql
-- Log slow queries in your application
-- INSERT INTO monitoring.slow_query_log
-- (request_id, query_name, execution_time_ms, where_complexity, row_count)

-- Then aggregate:
SELECT
    query_name,
    COUNT(*) AS count,
    MIN(execution_time_ms) AS min_ms,
    AVG(execution_time_ms) AS avg_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY execution_time_ms) AS p95_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY execution_time_ms) AS p99_ms
FROM monitoring.slow_query_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY query_name
HAVING AVG(execution_time_ms) > 50  -- Threshold
ORDER BY avg_ms DESC;
```

### Query Execution Plans (PostgreSQL):

```sql
-- Analyze query performance with EXPLAIN
-- EXPLAIN (ANALYZE, BUFFERS)
-- SELECT * FROM v_user WHERE email = $email;

-- Look for:

-- - Sequential scans (should be index scans)
-- - High costs (optimization opportunity)
-- - Buffer hits (cache effectiveness)
```

### N+1 Query Detection:

```sql
-- Count queries by type/entity (in application logs)
-- If you see many separate queries for same entity type,
-- likely N+1 pattern. Solution: use view composition
-- or batch queries instead of loop

SELECT
    query_type,
    COUNT(*) AS execution_count,
    SUM(execution_time_ms) AS total_time
FROM monitoring.query_log
WHERE request_id = $request_id
GROUP BY query_type
ORDER BY execution_count DESC;
```

### 3.2 Query Metrics

#### Query Execution Counts:

```sql
-- Top queries by frequency (last 24 hours)
SELECT
    query_name,
    COUNT(*) AS executions,
    SUM(execution_time_ms) AS total_time,
    AVG(execution_time_ms) AS avg_time,
    MIN(execution_time_ms) AS min_time,
    MAX(execution_time_ms) AS max_time
FROM monitoring.query_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY query_name
ORDER BY executions DESC
LIMIT 20;
```

### Cache Hit/Miss Rates:

```sql
-- Track cache effectiveness
SELECT
    query_name,
    COUNT(*) FILTER (WHERE cache_hit = true) AS cache_hits,
    COUNT(*) FILTER (WHERE cache_hit = false) AS cache_misses,
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE cache_hit = true) / COUNT(*),
        2
    ) AS hit_rate_pct
FROM monitoring.query_cache_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY query_name
ORDER BY hit_rate_pct ASC;
```

### WHERE Clause Complexity:

```sql
-- Track filter complexity
SELECT
    query_name,
    COUNT(*) AS executions,
    AVG(CAST(metadata->>'where_conditions' AS INTEGER)) AS avg_conditions,
    MAX(CAST(metadata->>'where_conditions' AS INTEGER)) AS max_conditions
FROM monitoring.query_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY query_name
ORDER BY avg_conditions DESC;
```

### 3.3 Query Tracing

#### Execution Phase Timing:

```sql
-- Track time spent in each execution phase
SELECT
    query_name,
    ROUND(AVG(CAST(timing->>'validation_ms' AS NUMERIC)), 2) AS validation_ms,
    ROUND(AVG(CAST(timing->>'auth_ms' AS NUMERIC)), 2) AS auth_ms,
    ROUND(AVG(CAST(timing->>'planning_ms' AS NUMERIC)), 2) AS planning_ms,
    ROUND(AVG(CAST(timing->>'execution_ms' AS NUMERIC)), 2) AS execution_ms,
    ROUND(AVG(CAST(timing->>'projection_ms' AS NUMERIC)), 2) AS projection_ms
FROM monitoring.query_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY query_name
ORDER BY execution_ms DESC;
```

### Authorization Decision Logging:

```sql
-- Track authorization checks
SELECT
    rule_name,
    COUNT(*) FILTER (WHERE authorized = true) AS allowed,
    COUNT(*) FILTER (WHERE authorized = false) AS denied,
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE authorized = false) / COUNT(*),
        2
    ) AS denial_rate_pct
FROM monitoring.auth_log
WHERE logged_at > NOW() - INTERVAL '24 hours'
GROUP BY rule_name
ORDER BY denial_rate_pct DESC;
```

---

## 4. Request Tracing

### 4.1 Correlation IDs

Every request should have a **request ID** that traces through:

1. GraphQL request entry
2. SQL query execution
3. CDC event emission
4. Mutation logging

```json
// GraphQL request
{
  "request_id": "req_550e8400-e29b-41d4-a716-446655440000",
  "user_id": "uuid",
  "tenant_id": "uuid",
  "timestamp": "2026-01-11T15:00:00Z"
}

// Stored in mutation log
{
  "request_id": "req_550e8400...",
  "user_id": "uuid",
  "trigger": "api_create"
}

// Emitted in CDC event
{
  "request_id": "req_550e8400...",
  "source": {
    "organization": "uuid"
  }
}
```

### Propagate correlation IDs:

```sql
-- Stored procedure receives correlation ID
CREATE OR REPLACE FUNCTION fn_create_user(
    input_request_id UUID,
    input_user_id UUID,
    input_email TEXT
)
RETURNS app.mutation_response
AS $$
BEGIN
    -- Log with correlation ID
    RETURN core.log_and_return_mutation(
        ... ,
        input_extra_metadata := jsonb_build_object(
            'request_id', input_request_id,
            'user_id', input_user_id
        )
    );
END;
$$;
```

### 4.2 Trace Context

Store in every log entry:

| Field | Purpose | Example |
|-------|---------|---------|
| `request_id` | Link all operations in a request | `req_550e8400...` |
| `user_id` | Who initiated the request | `uuid` |
| `tenant_id` | Which organization | `uuid` |
| `session_id` | User session | `sess_abc123` |
| `trace_id` | Distributed tracing | `trace_550e8400...` |
| `span_id` | Operation within trace | `span_001` |

---

## 5. Metrics & Telemetry

### 5.1 Database Metrics (PostgreSQL)

```sql
-- Connection pool utilization
SELECT
    state,
    COUNT(*) AS count
FROM pg_stat_activity
GROUP BY state;

-- Active queries and duration
SELECT
    pid,
    usename,
    application_name,
    state,
    state_change,
    EXTRACT(EPOCH FROM (NOW() - state_change)) AS duration_sec,
    query
FROM pg_stat_activity
WHERE state != 'idle'
ORDER BY state_change ASC;

-- Table/index sizes
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

### 5.2 Runtime Metrics

#### Request Throughput:

```sql
-- Requests per second over time
SELECT
    DATE_TRUNC('minute', created_at) AS minute,
    COUNT(*) AS mutations,
    ROUND(COUNT() / 60.0, 2) AS mutations_per_sec
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY minute
ORDER BY minute DESC;
```

### Response Time Distribution:

```sql
-- Percentiles of response latency
SELECT
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY duration_ms) AS p99_ms,
    PERCENTILE_CONT(0.999) WITHIN GROUP (ORDER BY duration_ms) AS p999_ms
FROM monitoring.mutation_log
WHERE logged_at > NOW() - INTERVAL '24 hours';
```

#### Error Rates:

```sql
-- Mutation failure rate
SELECT
    ROUND(
        100.0 * COUNT(*) FILTER (WHERE change_status LIKE 'failed:%' OR change_status IN ('not_found', 'forbidden'))
        / COUNT(*),
        2
    ) AS error_rate_pct
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '24 hours';
```

### 5.3 Business Metrics

#### Entity Creation Rates:

```sql
-- New entities per day
SELECT
    DATE(created_at) AS date,
    object_type,
    COUNT(*) FILTER (WHERE modification_type = 'INSERT') AS new_entities
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY DATE(created_at), object_type
ORDER BY date DESC;
```

### Entity Update Frequency:

```sql
-- How often entities are updated
SELECT
    object_type,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY update_count) AS median_updates,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY update_count) AS p95_updates
FROM (
    SELECT
        object_type,
        object_id,
        COUNT(*) FILTER (WHERE modification_type = 'UPDATE') AS update_count
    FROM core.tb_entity_change_log
    WHERE created_at > NOW() - INTERVAL '30 days'
    GROUP BY object_type, object_id
) stats
GROUP BY object_type;
```

---

## 6. Logging Patterns

### 6.1 Structured Logging

All logs should be structured JSON for easy parsing:

```json
{
  "timestamp": "2026-01-11T15:00:00.123456Z",
  "level": "INFO",
  "message": "User created successfully",
  "service": "fraiseql",
  "component": "mutation",
  "request_id": "req_550e8400-e29b-41d4-a716-446655440000",
  "user_id": "user_550e8400-e29b-41d4-a716-446655440001",
  "tenant_id": "org_550e8400-e29b-41d4-a716-446655440002",
  "entity_type": "User",
  "entity_id": "550e8400-e29b-41d4-a716-446655440003",
  "change_status": "new",
  "duration_ms": 245,
  "query_count": 2,
  "cascade_count": 0
}
```

### 6.2 Log Levels

| Level | Purpose | Examples |
|-------|---------|----------|
| **ERROR** | Unexpected failures | Mutation failed, database connection lost, authorization denied |
| **WARN** | Expected but notable | Validation failure, no-op, conflict, rate limit |
| **INFO** | Normal operations | Mutation completed, query executed, auth check passed |
| **DEBUG** | Development troubleshooting | Query plan details, authorization decision, field projection |

### 6.3 Log Aggregation

Use `tb_entity_change_log` as source of truth:

```sql
-- Query logs by various filters
WHERE fk_customer_org = $tenant_id              -- Single tenant
  AND created_at > NOW() - INTERVAL '1 hour'   -- Time range
  AND change_status LIKE 'failed:%'             -- Filter by status
  AND object_type = 'User'                      -- Filter by entity type
  AND extra_metadata->>'request_id' = $req_id   -- Correlate requests
```

---

## 7. Monitoring & Alerting

### 7.1 Key Metrics to Monitor

| Metric | Threshold | Action |
|--------|-----------|--------|
| **Mutation error rate** | > 5% | Page on-call |
| **Query p95 latency** | > 100ms | Investigate slow queries |
| **Database connection pool** | > 80% | Add connections or optimize |
| **Authorization denials** | > 1% of requests | Review auth rules |
| **Cascade operations** | Avg > 5 per mutation | Review mutation design |

### 7.2 Alert Patterns

#### Spike in Failed Mutations:

```sql
-- Alert if error rate increases suddenly
WITH rates AS (
    SELECT
        DATE_TRUNC('minute', created_at) AS minute,
        ROUND(
            100.0 * COUNT(*) FILTER (WHERE change_status LIKE 'failed:%')
            / COUNT(*),
            2
        ) AS error_rate
    FROM core.tb_entity_change_log
    WHERE created_at > NOW() - INTERVAL '30 minutes'
    GROUP BY minute
    ORDER BY minute DESC
    LIMIT 2
)
SELECT * FROM rates
WHERE error_rate > 10.0  -- Alert if > 10%
  AND error_rate > (SELECT error_rate FROM rates OFFSET 1 LIMIT 1) * 1.5;
```

### Slow Query Detection:

```sql
-- Alert on slow queries
SELECT *
FROM monitoring.query_log
WHERE execution_time_ms > 1000  -- > 1 second
  AND logged_at > NOW() - INTERVAL '5 minutes';
```

### Cascade Operation Anomaly:

```sql
-- Alert if cascade counts spike
SELECT object_type
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY object_type
HAVING AVG(CAST(extra_metadata->>'cascade_count' AS INTEGER)) > 10;
```

---

## 8. Debugging Workflows

### 8.1 Debugging Failed Mutations

#### Workflow:

1. Find the mutation in `tb_entity_change_log`
2. Check `change_status` for error category
3. Examine `object_data` for before/after state
4. Check `extra_metadata` for context

```sql
-- Find failed mutation
SELECT
    *,
    object_data->>'before' AS before_state,
    object_data->>'after' AS after_state
FROM core.tb_entity_change_log
WHERE object_id = $entity_id
  AND change_status LIKE 'failed:%'
ORDER BY created_at DESC
LIMIT 1;

-- Analyze the error
-- - conflict:* → Data conflict (duplicate, constraint)
-- - validation:* → Invalid data
-- - failed:* → Operation error
-- - Check extra_metadata for details
```

### 8.2 Debugging Slow Queries

#### Workflow:

1. Identify slow queries from monitoring
2. Check execution plan with EXPLAIN ANALYZE
3. Verify indexes exist
4. Check for N+1 patterns

```sql
-- EXPLAIN ANALYZE shows:

-- - Sequential scans → Need index
-- - High costs → Optimization opportunity
-- - Buffer hits → Good cache performance

EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM v_user WHERE email = $email;
```

### 8.3 Debugging Authorization Failures

#### Workflow:

1. Check auth context (user_id, roles, tenant_id)
2. Verify CompiledSchema auth rules
3. Check field-level auth for partial results

```sql
-- Verify auth context was passed
SELECT
    extra_metadata->>'user_id' AS user_id,
    extra_metadata->>'user_roles' AS user_roles
FROM core.tb_entity_change_log
WHERE pk_entity_change_log = $log_id;

-- Check if mutation was blocked
WHERE change_status LIKE 'blocked:%'
  OR change_status = 'forbidden'
  OR change_status = 'unauthorized';
```

---

## 9. CDC Event Streaming

### 9.1 Change Log → CDC Events

The `tb_entity_change_log` is the source for CDC events (see **docs/specs/cdc-format.md** for complete event structure and **docs/architecture/core/execution-model.md section 9.3** for execution model integration):

#### Key References:

- **docs/specs/cdc-format.md section 2** — Complete CDC event structure with all fields
- **docs/specs/schema-conventions.md section 6.2** — Debezium envelope format stored in change log's `object_data` column
- **docs/architecture/core/execution-model.md section 9** — Mutation execution pipeline and cache invalidation

```json
// tb_entity_change_log row becomes CDC event
{
  "version": "1.0",
  "event_type": "entity:updated",
  "event_id": "evt_550e8400...",
  "entity": {
    "entity_type": "User",
    "entity_id": "550e8400...",
    "tenant_id": "org_550e8400..."
  },
  "operation": {
    "type": "UPDATE",
    "before": { ... },  // From object_data
    "after": { ... }     // From object_data
  },
  "cascade": { ... },    // Cascade information
  "metadata": { ... }    // From extra_metadata
}
```

### Stream mutations to monitoring:

```sql
-- Consume change log and emit CDC events
-- Typically via PostgreSQL LISTEN/NOTIFY or trigger:

CREATE OR REPLACE FUNCTION emit_cdc_event()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify(
        'cdc_events',
        jsonb_build_object(
            'event_type', 'entity:' || LOWER(NEW.modification_type),
            'entity_type', NEW.object_type,
            'entity_id', NEW.object_id,
            'tenant_id', NEW.fk_customer_org,
            'timestamp', NEW.created_at
        )::text
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER cdc_trigger
AFTER INSERT ON core.tb_entity_change_log
FOR EACH ROW
EXECUTE FUNCTION emit_cdc_event();
```

### 9.2 Real-Time Observability

#### Stream mutations to dashboards:

```python
# Python example: consume CDC events
import psycopg
import json

conn = psycopg.connect("dbname=production")
conn.autocommit = True

with conn.cursor() as cur:
    cur.execute("LISTEN cdc_events")

    for notify in conn.notifies():
        event = json.loads(notify.payload)

        # Send to monitoring system
        send_to_monitoring(event)

        # Update real-time dashboards
        update_dashboard(event)

        # Trigger anomaly detection
        check_anomalies(event)
```

---

## 10. Database-Specific Observability

### 10.1 PostgreSQL

```sql
-- pg_stat_statements: Most expensive queries
SELECT
    calls,
    total_exec_time,
    mean_exec_time,
    query
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 20;

-- EXPLAIN ANALYZE: Understand query plans
EXPLAIN (ANALYZE, BUFFERS, VERBOSE)
SELECT * FROM v_user WHERE email = $email;

-- pg_stat_user_tables: Table access patterns
SELECT
    schemaname,
    tablename,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch
FROM pg_stat_user_tables
ORDER BY seq_scan DESC;
```

### 10.2 SQLite

```sql
-- SQLite query analysis
EXPLAIN QUERY PLAN
SELECT * FROM v_user WHERE email = $email;

-- Trace execution
PRAGMA trace_status(ON);

-- Profile queries
SELECT COUNT(*), query, total_time_us
FROM sqlite_stat_execution
GROUP BY query
ORDER BY total_time_us DESC;
```

### 10.3 MySQL / SQL Server

- Use native performance monitoring tools
- Database-specific profiling and tracing
- Monitor change data capture mechanisms
- Implement custom change logging if needed

---

## 11. Production Patterns

### 11.1 Change Log Archival

```sql
-- Archive old logs (older than 90 days)
CREATE TABLE core.tb_entity_change_log_archive
    (LIKE core.tb_entity_change_log);

INSERT INTO core.tb_entity_change_log_archive
SELECT *
FROM core.tb_entity_change_log
WHERE created_at < NOW() - INTERVAL '90 days';

DELETE FROM core.tb_entity_change_log
WHERE created_at < NOW() - INTERVAL '90 days';

-- Partition by created_at for faster queries
CREATE TABLE core.tb_entity_change_log_2026_01 PARTITION OF core.tb_entity_change_log
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
```

### 11.2 Performance Considerations

#### Async Logging:

```sql
-- Use PERFORM (fire and forget) for logging
PERFORM log_mutation_event(...)  -- Non-blocking

-- vs

INSERT INTO audit_log ...         -- Blocking, slower
```

### Batch CDC Emission:

```sql
-- Emit CDC events in batches (PostgreSQL)
-- Instead of one trigger per row, batch events:

INSERT INTO cdc_queue (event_payload)
SELECT json_agg(...) FROM change_log WHERE NOT emitted
GROUP BY created_at::DATE;
```

### 11.3 Multi-Tenant Observability

#### Per-Tenant Dashboards:

```sql
-- Dashboard filtered by tenant
SELECT ...
FROM core.tb_entity_change_log
WHERE fk_customer_org = $tenant_id  -- Always filter by tenant
  AND created_at > NOW() - INTERVAL '24 hours';
```

### Cross-Tenant Anomaly Detection:

```sql
-- Alert if one tenant has unusual activity
SELECT
    fk_customer_org,
    COUNT(*) AS mutation_count,
    COUNT(*) FILTER (WHERE change_status LIKE 'failed:%') AS error_count
FROM core.tb_entity_change_log
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY fk_customer_org
HAVING COUNT(*) > (
    SELECT AVG(cnt)
    FROM (
        SELECT COUNT(*) AS cnt
        FROM core.tb_entity_change_log
        WHERE created_at > NOW() - INTERVAL '24 hours'
        GROUP BY fk_customer_org
    ) stats
) * 2;  -- Alert if 2x average
```

---

## Summary

### Observability in FraiseQL is:

1. **Database-first** — `tb_entity_change_log` is source of truth
2. **Multi-tenant aware** — All logs scoped by tenant
3. **Deterministic** — Exact state changes recorded
4. **Traceable** — Correlation IDs link requests through system
5. **Queryable** — Use SQL to aggregate metrics
6. **Real-time capable** — Stream mutations via CDC
7. **Audit-ready** — Complete history for compliance

### Key files:

- `tb_entity_change_log` — Mutation audit trail (schema-conventions.md section 6)
- `CDC events` — Real-time stream (cdc-format.md)
- Query logs — Application-level logging
- Database metrics — PostgreSQL pg_stat_statements, etc.

---

---

## Troubleshooting

### "Entity change log table missing or not populated"

**Cause:** CDC not enabled or table not created.

#### Diagnosis:

1. Check if table exists: `SELECT * FROM information_schema.tables WHERE table_name = 'tb_entity_change_log';`
2. Query table: `SELECT COUNT(*) FROM tb_entity_change_log;`
3. Check FraiseQL config: Is CDC enabled?

#### Solutions:

- Run migrations: `fraiseql migrate --target latest`
- Verify table was created by migration: Check database logs
- Enable CDC in fraiseql.toml: `[cdc] enabled = true`
- Check application logs for migration errors

### "Change log has data but CDC consumers aren't receiving events"

**Cause:** Consumer not connected or subscription has lag.

#### Diagnosis:

1. Check consumer status: `SELECT * FROM tb_cdc_consumer_offset WHERE consumer_id = 'X';`
2. Verify connection: `SELECT COUNT(*) FROM tb_entity_change_log WHERE id > last_offset;`
3. Check for errors in consumer logs

#### Solutions:

- Restart consumer service
- Reset consumer offset to catch up: `UPDATE tb_cdc_consumer_offset SET offset = 0 WHERE consumer_id = 'X';`
- Verify network connectivity to CDC source
- Check authentication credentials for consumer
- Monitor lag: Alert if lag > 1000 events

### "Query logs missing recent mutations"

**Cause:** Logging not enabled or query log table full.

#### Diagnosis:

1. Check logging level: `grep RUST_LOG fraiseql.toml`
2. Check query log table size: `SELECT pg_size_pretty(pg_total_relation_size('tb_query_log'));`
3. Verify queries are actually running: `SELECT COUNT(*) FROM tb_query_log WHERE created_at > NOW() - INTERVAL '1 hour';`

#### Solutions:

- Enable query logging: `RUST_LOG=info,fraiseql::query_log=debug`
- Implement log rotation: Clean up old logs older than 30 days
- Increase retention window: `VACUUM ANALYZE tb_query_log;`
- Stream logs to external system (Splunk, DataDog) instead of storing in database

### "Correlation IDs not present in logs"

**Cause:** Client not sending X-Correlation-ID header or application not passing through.

#### Diagnosis:

1. Check request headers: Add `X-Correlation-ID` to all requests
2. Verify logs include correlation ID: `grep -i correlation application.log`
3. Check FraiseQL version supports correlation IDs

#### Solutions:

- Always send correlation ID from client: `curl -H "X-Correlation-ID: abc-123" ...`
- Propagate correlation ID to subgraph calls
- Verify logging configuration includes correlation ID
- Use `X-Request-ID` as fallback if correlation ID missing

### "Audit trail doesn't show who made a change"

**Cause:** User/tenant context not captured or not included in log.

#### Diagnosis:

1. Check for user_id in change log: `SELECT DISTINCT user_id FROM tb_entity_change_log;`
2. Verify token contains user info
3. Check if middleware extracts user from JWT

#### Solutions:

- Ensure all mutations include user context (from JWT or session)
- Middleware should extract user_id and inject into query context
- Store user_id in change log: `INSERT INTO tb_entity_change_log (..., user_id) VALUES (..., current_user_id);`
- For compliance: Store full user record snapshot in audit log

### "Performance degradation after enabling detailed observability"

**Cause:** Logging and CDC adds overhead - database I/O or CPU bound.

#### Diagnosis:

1. Compare before/after: Measure query latency with/without logging
2. Check database CPU: `SELECT * FROM pg_stat_statements ORDER BY mean_exec_time DESC;`
3. Monitor disk I/O: May be bottleneck if log table very large

#### Solutions:

- Use log sampling: Log 1 in 100 queries to reduce I/O
- Async logging: Queue logs to background writer (don't block mutations)
- Archival: Move old logs to separate table/schema
- Use external log aggregation (Splunk, DataDog) instead of database
- Disable debug-level logging in production (use info level)

### "Tenant data leaked in observability logs"

**Cause:** Sensitive data logged or not scoped correctly.

#### Diagnosis:

- Audit logs to find if PII/sensitive data present
- Check log filtering: Does it respect data isolation?
- Review who has access to observability systems

#### Solutions:

- Sanitize logs: Hash or mask PII before logging
- Scope all observations by tenant: Use tenant_id in WHERE clauses
- Implement access controls on observability data
- Regular audit of log contents for compliance
- Implement field-level encryption for sensitive fields

---

## See Also

- **[Monitoring & Observability Guide](./monitoring.md)** - Prometheus, OpenTelemetry, health checks setup
- **[Observability Architecture](../architecture/observability/observability-model.md)** - Technical architecture and design
- **[Production Deployment](./production-deployment.md)** - Observability in production environments
- **[Database Fundamentals](../architecture/database/database-targeting.md)** - Understanding database-centric logging
- **[CDC Format Specification](../specs/cdc-format.md)** - Change data capture event structure
- **[Troubleshooting Guide](../observability/troubleshooting.md)** - Using observability data for debugging


