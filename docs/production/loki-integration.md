# Loki Log Aggregation Integration

Integration guide for Loki log aggregation with FraiseQL applications.

## Overview

**Loki** is a horizontally-scalable, highly-available log aggregation system inspired by Prometheus. It indexes metadata (labels) rather than full-text, making it cost-effective for large-scale deployments.

**Architecture:**
```
┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
│   FraiseQL      │      │    Promtail     │      │      Loki       │
│   Application   │─────▶│  (Log Agent)    │─────▶│  (Aggregation)  │
│                 │ logs │                 │ push │                 │
└─────────────────┘      └─────────────────┘      └────────┬────────┘
                                                            │
                                                            ▼
                                                   ┌─────────────────┐
                                                   │    Grafana      │
                                                   │   (Query/UI)    │
                                                   └─────────────────┘
```

**Components:**
- **Loki:** Log storage and indexing engine
- **Promtail:** Log collection agent (tails log files)
- **Grafana:** Query interface and dashboards

**Benefits:**
- Label-based indexing (cost-effective at scale)
- Native Grafana integration
- LogQL query language (similar to PromQL)
- Correlation with OpenTelemetry traces via `trace_id`
- No full-text indexing overhead

---

## Quick Start

### 1. Start Loki Stack (Docker Compose)

```bash
# Navigate to examples directory
cd examples/observability

# Start Loki, Promtail, and Grafana
docker-compose -f docker-compose.loki.yml up -d

# Verify Loki is running
curl http://localhost:3100/ready

# Verify Promtail is running
curl http://localhost:9080/ready

# Access Grafana
open http://localhost:3000
# Login: admin / admin
```

### 2. Verify Log Ingestion

```bash
# Send test log to Loki
curl -X POST http://localhost:3100/loki/api/v1/push \
  -H "Content-Type: application/json" \
  -d '{
    "streams": [{
      "stream": {
        "job": "test",
        "level": "info"
      },
      "values": [
        ["'$(date +%s)000000000'", "Test log message from FraiseQL"]
      ]
    }]
  }'

# Query logs via LogQL
curl -G http://localhost:3100/loki/api/v1/query \
  --data-urlencode 'query={job="test"}' | jq
```

### 3. Query Logs in Grafana

1. Open Grafana: http://localhost:3000
2. Go to **Explore** → Select **Loki** data source
3. Run query: `{job="fraiseql-app"}`

---

## Configuration

### Development Configuration

The provided `docker-compose.loki.yml` uses:
- **Filesystem storage:** Logs stored in Docker volume
- **Single instance:** No replication
- **30-day retention:** Automatic log deletion after 30 days

**Files:**
- `examples/observability/loki/loki-config.yaml` - Loki server config
- `examples/observability/loki/promtail-config.yaml` - Promtail agent config
- `examples/observability/docker-compose.loki.yml` - Docker Compose stack

### Production Configuration

For production deployments, update `loki-config.yaml`:

```yaml
storage_config:
  aws:
    s3: s3://us-east-1/your-loki-bucket
    s3forcepathstyle: false
    bucketnames: your-loki-bucket
    region: us-east-1

# Or for GCS
storage_config:
  gcs:
    bucket_name: your-loki-bucket
    chunk_buffer_size: 10485760  # 10MB
    request_timeout: 60s

# Scale with multiple instances
common:
  replication_factor: 3
  ring:
    kvstore:
      store: consul
      consul:
        host: consul:8500
```

**Production Recommendations:**
- Use S3/GCS for object storage (not filesystem)
- Deploy 3+ Loki instances for HA
- Use Consul or etcd for ring coordination
- Increase retention to 90+ days
- Configure compaction for storage efficiency
- Enable query caching for performance

---

## FraiseQL Log Format

FraiseQL logs should be in **JSON format** for efficient parsing by Promtail.

### Expected Log Format

```json
{
  "timestamp": "2025-12-04T10:15:30.123Z",
  "level": "error",
  "message": "Database connection failed",
  "trace_id": "abc123def456",
  "span_id": "789ghi012jkl",
  "exception_type": "DatabaseConnectionError",
  "fingerprint": "db_connection_timeout",
  "context": {
    "user_id": "user_789",
    "tenant_id": "tenant_123",
    "operation": "create_post"
  }
}
```

### Configure Python Logging

```python
import logging
import json
from datetime import datetime

class JSONFormatter(logging.Formatter):
    def format(self, record):
        log_data = {
            "timestamp": datetime.utcnow().isoformat() + "Z",
            "level": record.levelname.lower(),
            "message": record.getMessage(),
            "logger": record.name,
        }

        # Add trace context if available
        if hasattr(record, "trace_id"):
            log_data["trace_id"] = record.trace_id
        if hasattr(record, "span_id"):
            log_data["span_id"] = record.span_id

        # Add exception info if present
        if record.exc_info:
            log_data["exception_type"] = record.exc_info[0].__name__
            log_data["stack_trace"] = self.formatException(record.exc_info)

        return json.dumps(log_data)

# Configure logger
handler = logging.FileHandler("/var/log/fraiseql/app.log")
handler.setFormatter(JSONFormatter())
logger = logging.getLogger("fraiseql")
logger.addHandler(handler)
logger.setLevel(logging.INFO)
```

---

## LogQL Query Examples

### 1. All Errors in Last Hour

```logql
{job="fraiseql-app"} | json | level="error"
```

**Note:** Time range is set in Grafana UI, not in the query. For range queries, use `count_over_time`.

### 2. Logs for Specific Trace

```logql
{job="fraiseql-app"} | json | trace_id="abc123def456"
```

**Use Case:** Jump from trace in Tempo to related logs in Loki.

### 3. Rate of Errors Per Minute

```logql
rate(count_over_time({job="fraiseql-app"} | json | level="error" [5m]))
```

### 4. Top 10 Error Types

```logql
topk(10,
  sum by (exception_type) (
    count_over_time({job="fraiseql-app"} | json | level="error" [1h])
  )
)
```

### 5. Filter by User or Tenant

```logql
{job="fraiseql-app"} | json | context_user_id="user_789"
```

### 6. Slow Query Detection

```logql
{job="postgresql"}
  | regexp "duration: (?P<duration>\\d+\\.\\d+) ms"
  | unwrap duration
  | __error__=""
  | duration > 1000
```

### 7. Database Connection Errors

```logql
{job="fraiseql-app"} | json | exception_type="DatabaseConnectionError"
```

### 8. Pattern Matching in Messages

```logql
{job="fraiseql-app"} |~ "authentication failed|unauthorized access"
```

### 9. Aggregate by Tenant

```logql
sum by (context_tenant_id) (
  count_over_time({job="fraiseql-app"} | json [1h])
)
```

### 10. Correlation with Metrics

```logql
# Count errors by fingerprint
sum by (fingerprint) (
  count_over_time({job="fraiseql-errors"} | json [5m])
)
```

---

## Correlation with OpenTelemetry Traces

### Enable Trace-Log Correlation

**1. Configure Grafana Data Source** (already done in `grafana-datasources.yaml`):

```yaml
derivedFields:
  - datasourceUid: tempo
    matcherRegex: "trace_id=(\\w+)"
    name: TraceID
    url: "$${__value.raw}"
```

**2. Ensure `trace_id` in Logs:**

FraiseQL logs must include `trace_id` field:

```python
import logging
from opentelemetry import trace

def log_with_trace_context(message, level="info"):
    span = trace.get_current_span()
    trace_id = span.get_span_context().trace_id

    extra = {"trace_id": format(trace_id, "032x")}
    logger.log(getattr(logging, level.upper()), message, extra=extra)
```

**3. Jump from Trace to Logs:**

In Grafana:
1. Open trace in Tempo
2. Click on span with errors
3. Click **"Logs for this span"** → Opens Loki with filtered query

**4. Jump from Logs to Trace:**

In Grafana Explore (Loki):
1. View log entry with `trace_id`
2. Click on trace_id link → Opens trace in Tempo

---

## Dashboards

### Pre-built Grafana Dashboards

**1. Log Volume Dashboard:**
- Total log rate by job
- Log levels distribution (info, warn, error)
- Top 10 loggers by volume

**2. Error Dashboard:**
- Error rate over time
- Top error types
- Error distribution by tenant
- Recent errors table

**3. Performance Dashboard:**
- Slow query logs (PostgreSQL)
- High-latency requests (FraiseQL)
- Database connection pool usage

**Import Dashboards:**
- Grafana Dashboard ID: 13639 (Loki + Promtail)
- Grafana Dashboard ID: 12611 (Logs / App / Loki)

---

## Retention and Storage

### Default Retention

**Development:** 30 days (720 hours)
**Production:** 90 days recommended

### Configure Retention

Edit `loki-config.yaml`:

```yaml
limits_config:
  retention_period: 2160h  # 90 days

table_manager:
  retention_deletes_enabled: true
  retention_period: 2160h  # 90 days

compactor:
  retention_enabled: true
  retention_delete_delay: 2h
```

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
- Check actual storage usage: `docker exec fraiseql-loki du -sh /loki/chunks`
- Monitor ingestion rate: `curl http://localhost:3100/metrics | grep loki_distributor_bytes_received_total`
- Set alerts if usage exceeds estimates by 50%
- Use S3/GCS with lifecycle policies (archive to Glacier after 90 days)

---

## Performance Tuning

### Optimize Log Volume

**1. Drop Debug Logs in Production:**

Edit `promtail-config.yaml`:

```yaml
pipeline_stages:
  - drop:
      source: level
      expression: "debug"
```

**2. Sample High-Volume Logs:**

```yaml
pipeline_stages:
  - match:
      selector: '{job="fraiseql-app",level="info"}'
      stages:
        - sampling:
            rate: 0.1  # Keep 10% of info logs
```

**3. Increase Ingestion Limits:**

Edit `loki-config.yaml`:

```yaml
limits_config:
  ingestion_rate_mb: 32  # Default: 16
  ingestion_burst_size_mb: 64  # Default: 32
```

### Query Performance

**1. Use Label Filters First:**

```logql
# Good: Filter by labels first
{job="fraiseql-app", level="error"} | json

# Bad: Filter after parsing
{job="fraiseql-app"} | json | level="error"
```

**2. Limit Time Range:**

- Prefer shorter time ranges (1h instead of 24h)
- Use `[5m]` range vectors for rate calculations

**3. Use Parallelization:**

Edit `loki-config.yaml`:

```yaml
limits_config:
  max_query_parallelism: 64  # Default: 32
```

---

## Troubleshooting

### Loki Not Receiving Logs

**Check Promtail:**

```bash
# View Promtail logs
docker logs fraiseql-promtail

# Check Promtail targets
curl http://localhost:9080/targets | jq
```

**Common Issues:**
- Log file path incorrect in `promtail-config.yaml`
- Log file permissions (Promtail needs read access)
- Loki URL incorrect (should be `http://loki:3100`)

### Promtail Parsing Errors

**Test Regex Parsing:**

```bash
# Check Promtail metrics
curl http://localhost:9080/metrics | grep promtail_read_errors_total
```

**Fix JSON Parsing:**

If logs aren't JSON, use regex instead:

```yaml
pipeline_stages:
  - regex:
      expression: '^(?P<timestamp>\S+) (?P<level>\S+) (?P<message>.*)$'
  - labels:
      level:
```

### Storage Issues

**Check Disk Usage:**

```bash
docker exec fraiseql-loki du -sh /loki/chunks
docker exec fraiseql-loki df -h /loki
```

**Compaction Not Running:**

```bash
# Check compactor logs
docker logs fraiseql-loki | grep compactor

# Force compaction
docker exec fraiseql-loki wget -O- http://localhost:3100/compactor/ring
```

---

## Security

### Authentication

For production, enable authentication:

```yaml
auth_enabled: true

server:
  http_listen_port: 3100
  grpc_listen_port: 9096

# Add basic auth or OAuth
auth:
  type: basic
  basic_auth:
    username: admin
    password_file: /etc/loki/.htpasswd
```

### TLS/SSL

Enable TLS for Loki and Promtail:

```yaml
server:
  http_tls_config:
    cert_file: /etc/loki/tls/server.crt
    key_file: /etc/loki/tls/server.key
```

### Network Isolation

Use Docker networks or VPC:

```yaml
networks:
  observability:
    internal: true  # No external access
```

---

## Migration from Other Systems

### From ELK Stack

**Differences:**
- Loki indexes labels, not full-text (cheaper at scale)
- LogQL vs Lucene query syntax
- No Kibana (use Grafana)

**Migration Steps:**
1. Run Loki + ELK in parallel
2. Configure Promtail to tail same logs
3. Validate queries in Grafana
4. Switch queries to Loki
5. Decommission ELK

### From Splunk

**Cost Savings:**
- Splunk: ~$150/GB ingested
- Loki: ~$0.023/GB (S3 storage) + compute

**Migration:**
- Loki is not a Splunk replacement (no full-text search)
- Best for structured logs (JSON)
- Use labels for filtering, not grep-style searches

---

## Query Optimization Best Practices

> **TODO:** Comprehensive query optimization guide
>
> Planned content:
> - Label filters vs JSON filters (performance impact)
> - Line filters before JSON parsing
> - Time range optimization
> - Cardinality management
> - Common query patterns with performance notes
>
> See implementation plan: `.phases/loki_fixes_implementation_plan.md` Task 3.2

---

## PostgreSQL vs Loki: When to Use Each

> **TODO:** Clarification of PostgreSQL errors table vs Loki logs
>
> Planned content:
> - PostgreSQL: Error tracking, management, long-term storage
> - Loki: Log context, debugging, trace correlation
> - Decision matrix and recommended workflow
>
> See implementation plan: `.phases/loki_fixes_implementation_plan.md` Task 3.3

---

## Monitoring Loki Itself

> **TODO:** Comprehensive Loki monitoring and alerting
>
> Planned content:
> - Key metrics (ingestion, query performance, storage)
> - Prometheus alert rules
> - Health check endpoints
> - Grafana dashboards for Loki operations
> - Troubleshooting guide
>
> See implementation plan: `.phases/loki_fixes_implementation_plan.md` Task 3.4

---

## Security Hardening

> **TODO:** Production security configuration
>
> Planned content:
> - Authentication options (multi-tenancy, basic auth, OAuth2)
> - TLS/SSL encryption
> - Network isolation
> - Log sanitization (removing PII/secrets)
> - Docker socket security
> - Secrets management
>
> See implementation plan: `.phases/loki_fixes_implementation_plan.md` Task 3.5

---

## References

**Official Documentation:**
- Loki: https://grafana.com/docs/loki/latest/
- Promtail: https://grafana.com/docs/loki/latest/clients/promtail/
- LogQL: https://grafana.com/docs/loki/latest/logql/

**FraiseQL Integration:**
- Observability Overview: `docs/production/observability.md`
- OpenTelemetry Setup: `docs/production/monitoring.md`
- Error Tracking: `docs/production/observability.md#error-tracking`

**Configuration Files:**
- Loki Config: `examples/observability/loki/loki-config.yaml`
- Promtail Config: `examples/observability/loki/promtail-config.yaml`
- Docker Compose: `examples/observability/docker-compose.loki.yml`
