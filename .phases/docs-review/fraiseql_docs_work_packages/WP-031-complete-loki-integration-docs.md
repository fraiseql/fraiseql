# WP-031: Complete Loki Integration Documentation

**Assignee:** TW-CORE
**Priority:** P2 (Nice to Have - Enhancement)
**Estimated Hours:** 12
**Week:** 4
**Dependencies:** Loki integration implementation (if not complete)

---

## Objective

Complete the remaining sections in `docs/production/loki_integration.md` that are currently marked with TODO placeholders. These sections provide critical production guidance for Loki query optimization, decision-making frameworks, monitoring, and security.

**Current State:** The Loki integration document has 4 placeholder sections with detailed planned content outlines but no actual implementation.

**Target State:** All TODO sections completed with comprehensive, production-ready content that helps users:
1. Optimize Loki queries for performance
2. Understand when to use PostgreSQL vs Loki
3. Monitor Loki health and performance
4. Secure Loki in production environments

---

## Problem Statement

The `docs/production/loki_integration.md` file has 4 incomplete sections that were stubbed out during initial documentation. These sections reference `.phases/loki_fixes_implementation_plan.md` which contains the implementation roadmap.

Users implementing Loki in production need:
- **Query optimization guidance** - To avoid performance pitfalls
- **Decision framework** - To know when to use Loki vs PostgreSQL
- **Monitoring strategy** - To ensure Loki health in production
- **Security hardening** - To protect log data

Without these sections, the Loki integration documentation is incomplete and may lead to:
- Inefficient queries causing performance issues
- Confusion about when to use which system
- Lack of visibility into Loki health
- Security vulnerabilities in log infrastructure

---

## Scope

### Files to Update

**Primary File:**
- `docs/production/loki_integration.md`

**Sections to Complete (4 TODOs):**

1. **Line 619**: Query Optimization Best Practices
2. **Line 634**: PostgreSQL vs Loki: When to Use Each
3. **Line 647**: Monitoring Loki Itself
4. **Line 662**: Security Hardening

### Reference Materials

- `.phases/loki_fixes_implementation_plan.md` - Tasks 3.2, 3.3, 3.4, 3.5
- Existing Loki integration examples in the codebase
- Grafana Loki documentation (official)
- LogQL query language reference

---

## Detailed Tasks

### Task 1: Query Optimization Best Practices (Line 619)
**Estimated Time:** 3 hours

**Content to Add:**
```markdown
## Query Optimization Best Practices

### Label Filters vs JSON Filters

**Performance Impact:**
- Label filters are evaluated at index lookup time (FAST)
- JSON filters require log line parsing (SLOW)

**Best Practice:**
```logql
# ❌ SLOW - filters after parsing
{app="fraiseql"} | json | level="error"

# ✅ FAST - filters at index
{app="fraiseql", level="error"}
```

**Rule:** Use labels for high-cardinality, frequently-queried fields.

### Line Filters Before JSON Parsing

Always apply line filters before JSON extraction:
```logql
# ❌ SLOW - parses all lines
{app="fraiseql"} | json | error_code="500"

# ✅ FAST - filters before parsing
{app="fraiseql"} |= "error_code" | json | error_code="500"
```

### Time Range Optimization

**Recommendations:**
- Use the smallest time range possible
- Leverage time range picker in Grafana
- Consider data retention when querying historical logs

**Query Pattern:**
```logql
# Good - specific time range
{app="fraiseql"}[5m]

# Bad - unbounded or very long ranges
{app="fraiseql"}[24h]  # Only when necessary
```

### Cardinality Management

**Avoid High-Cardinality Labels:**
```yaml
# ❌ BAD - unique per request
labels:
  request_id: "uuid-here"

# ✅ GOOD - limited set of values
labels:
  endpoint: "/api/users"
  status_code: "200"
```

**Label Cardinality Limits:**
- Keep label value count < 100K per label
- Monitor cardinality with Prometheus metrics

### Common Query Patterns

**Pattern 1: Error Rate**
```logql
sum(rate({app="fraiseql", level="error"}[5m])) by (endpoint)
```

**Pattern 2: Top Errors**
```logql
topk(10,
  sum by (error_message) (
    rate({app="fraiseql", level="error"}[5m])
  )
)
```

**Pattern 3: Slow Queries**
```logql
{app="fraiseql"}
  | json
  | query_time > 1000
  | line_format "{{.query}} took {{.query_time}}ms"
```

### Performance Checklist

- [ ] Use label filters for all queries
- [ ] Apply line filters before JSON parsing
- [ ] Limit time ranges to what's needed
- [ ] Monitor query performance in Grafana
- [ ] Keep label cardinality low
- [ ] Use rate() for counting over time
- [ ] Leverage `|=` and `!=` for fast text filtering

### Further Reading

- [LogQL Performance Guide](https://grafana.com/docs/loki/latest/logql/performance/)
- [Query Optimization Tips](https://grafana.com/docs/loki/latest/best-practices/)
```

---

### Task 2: PostgreSQL vs Loki Decision Framework (Line 634)
**Estimated Time:** 3 hours

**Content to Add:**
```markdown
## PostgreSQL vs Loki: When to Use Each

### System Comparison

| Feature | PostgreSQL Errors | Loki Logs |
|---------|------------------|-----------|
| **Purpose** | Error tracking & management | Log context & debugging |
| **Query Speed** | Very fast (indexed) | Fast (log stream) |
| **Retention** | Long-term (years) | Medium-term (weeks) |
| **Cost** | Higher (structured storage) | Lower (compressed logs) |
| **Searchability** | Full-text + structured | Text search + labels |
| **Best For** | Aggregation, reports, alerts | Debugging, trace correlation |

### PostgreSQL Errors Table - Use When:

✅ **Error Tracking & Management**
- Need to track error resolution status
- Require error assignment to teams
- Want to close/reopen errors
- Need to link errors to issues/PRs

✅ **Long-Term Analysis**
- Retention beyond 90 days
- Historical trend analysis
- Compliance/audit requirements
- Error rate reporting

✅ **Structured Querying**
- Complex JOINs with other tables
- Aggregations by customer, tenant, endpoint
- Statistical analysis (percentiles, distributions)
- Business intelligence queries

✅ **Alerting on Thresholds**
- "Alert if error rate > 10/min"
- "Notify if new error type appears"
- "Alert on specific error_code patterns"

**Example Use Cases:**
- Track resolution of known errors
- Generate weekly error reports
- Analyze error trends over quarters
- Compliance audit trails

### Loki Logs - Use When:

✅ **Real-Time Debugging**
- Investigating active incidents
- Need full context around errors
- Trace correlation (span IDs)
- Step-by-step request flow

✅ **Log Context**
- See logs before/after error
- Understand request parameters
- View user journey
- Debug specific requests

✅ **Development Workflow**
- Local development log viewing
- CI/CD pipeline logs
- Test failure investigation
- Performance profiling

✅ **Cost-Effective Storage**
- High-volume non-critical logs
- Temporary debugging information
- Development environment logs
- Lower retention requirements

**Example Use Cases:**
- "What happened before this 500 error?"
- "Show me all logs for request_id X"
- "Trace this user's session flow"
- "Debug why this query is slow"

### Decision Matrix

| Scenario | Recommended System | Why |
|----------|-------------------|-----|
| Track error resolution | PostgreSQL | Structured tracking, long retention |
| Debug active incident | Loki | Real-time context, request tracing |
| Generate error reports | PostgreSQL | Fast aggregation, BI tools |
| View request flow | Loki | Full log context, trace IDs |
| Compliance audit | PostgreSQL | Long-term retention, structured |
| Local dev debugging | Loki | Cost-effective, high volume |
| Alert on error rates | PostgreSQL | Fast queries, thresholds |
| Investigate test failures | Loki | Full test run context |

### Recommended Workflow

**1. Loki for Initial Investigation**
```bash
# Developer sees alert, checks Loki first
Grafana → Explore → {app="fraiseql", level="error", request_id="abc"}
# Views full context, identifies root cause
```

**2. PostgreSQL for Tracking**
```sql
-- If error needs tracking, check PostgreSQL
SELECT * FROM errors
WHERE error_message ILIKE '%database timeout%'
  AND created_at > NOW() - INTERVAL '7 days';
-- Create ticket, assign to team, track resolution
```

**3. Both for Comprehensive Analysis**
```python
# Combine both sources
errors_summary = db.query("SELECT * FROM errors WHERE ...")  # PostgreSQL
for error in errors_summary:
    logs = loki.query(f"{{request_id='{error.request_id}'}}")  # Loki
    analyze_error_context(error, logs)
```

### Migration Strategy

**Phase 1: Start with PostgreSQL Only**
- Simpler setup, proven reliability
- Add Loki when debugging needs grow

**Phase 2: Add Loki for Development**
- Dev/staging environments use Loki
- Production still uses PostgreSQL

**Phase 3: Hybrid Production**
- Critical errors → PostgreSQL
- All logs → Loki
- Cross-reference via request_id

**Phase 4: Optimize Costs**
- Adjust retention periods
- Archive old PostgreSQL errors
- Tune Loki compaction settings

### Further Reading

- [Loki vs Traditional Logging](https://grafana.com/docs/loki/latest/fundamentals/overview/)
- [PostgreSQL Full-Text Search](https://www.postgresql.org/docs/current/textsearch.html)
```

---

### Task 3: Monitoring Loki Itself (Line 647)
**Estimated Time:** 3 hours

**Content to Add:**
```markdown
## Monitoring Loki Itself

### Key Metrics to Monitor

Loki exposes Prometheus metrics on `:3100/metrics`. Monitor these critical indicators:

#### 1. Ingestion Metrics

**Log Ingestion Rate:**
```promql
rate(loki_distributor_lines_received_total[5m])
```
**Alerts:**
- Ingestion rate drops to zero → Loki down
- Sudden spike → Application issue or attack

**Ingestion Latency:**
```promql
histogram_quantile(0.99,
  rate(loki_request_duration_seconds_bucket{route="push"}[5m])
)
```
**Alert:** P99 latency > 1s

#### 2. Query Performance

**Query Duration:**
```promql
histogram_quantile(0.99,
  rate(loki_query_frontend_queue_duration_seconds_bucket[5m])
)
```
**Alert:** P99 query time > 5s

**Failed Queries:**
```promql
rate(loki_query_frontend_failed_queries_total[5m])
```
**Alert:** Query failure rate > 1%

#### 3. Storage Health

**Active Series:**
```promql
loki_ingester_memory_streams
```
**Alert:** Too many streams → cardinality issue

**Chunk Age:**
```promql
loki_ingester_oldest_unshipped_block_seconds
```
**Alert:** Chunks not shipping → disk full risk

#### 4. Resource Usage

**Memory Usage:**
```promql
process_resident_memory_bytes{job="loki"}
```
**Alert:** Memory > 80% of limit

**Disk Usage:**
```promql
loki_boltdb_shipper_index_upload_duration_seconds
```
**Alert:** Slow uploads → storage issue

### Prometheus Alert Rules

Create `/etc/prometheus/loki_alerts.yml`:

```yaml
groups:
  - name: loki_alerts
    interval: 30s
    rules:
      # Ingestion stopped
      - alert: LokiIngestionStopped
        expr: rate(loki_distributor_lines_received_total[5m]) == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Loki is not receiving logs"
          description: "No logs received in 5 minutes"

      # High ingestion latency
      - alert: LokiIngestionSlow
        expr: |
          histogram_quantile(0.99,
            rate(loki_request_duration_seconds_bucket{route="push"}[5m])
          ) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Loki ingestion is slow"
          description: "P99 ingestion latency: {{ $value }}s"

      # Query failures
      - alert: LokiQueryFailures
        expr: rate(loki_query_frontend_failed_queries_total[5m]) > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Loki queries are failing"
          description: "Query failure rate: {{ $value | humanizePercentage }}"

      # High cardinality
      - alert: LokiHighCardinality
        expr: loki_ingester_memory_streams > 100000
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Loki has too many active streams"
          description: "{{ $value }} streams (limit: 100K)"

      # Disk usage
      - alert: LokiDiskUsage
        expr: |
          (node_filesystem_avail_bytes{mountpoint="/loki"} /
           node_filesystem_size_bytes{mountpoint="/loki"}) < 0.2
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Loki disk space low"
          description: "Only {{ $value | humanizePercentage }} free"
```

### Health Check Endpoints

**Liveness Probe:**
```bash
curl http://loki:3100/ready
# Response: ready (200 OK)
```

**Readiness Probe:**
```bash
curl http://loki:3100/loki/api/v1/status/buildinfo
# Returns: version, branch, goVersion
```

**Kubernetes Health Checks:**
```yaml
livenessProbe:
  httpGet:
    path: /ready
    port: 3100
  initialDelaySeconds: 30
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /ready
    port: 3100
  initialDelaySeconds: 15
  periodSeconds: 5
```

### Grafana Dashboards

**1. Loki Operations Dashboard**

Import official dashboard: [ID 13639](https://grafana.com/grafana/dashboards/13639)

**Key Panels:**
- Ingestion rate (lines/sec)
- Query latency (P50, P99)
- Active streams count
- Disk usage per component
- Error rate by component

**2. Custom FraiseQL Loki Dashboard**

Create dashboard with:
```json
{
  "title": "FraiseQL Loki Health",
  "panels": [
    {
      "title": "Log Ingestion Rate",
      "targets": [{
        "expr": "rate(loki_distributor_lines_received_total{job='loki'}[5m])"
      }]
    },
    {
      "title": "Query Performance",
      "targets": [{
        "expr": "histogram_quantile(0.99, rate(loki_query_frontend_queue_duration_seconds_bucket[5m]))"
      }]
    },
    {
      "title": "Active Streams",
      "targets": [{
        "expr": "loki_ingester_memory_streams"
      }]
    }
  ]
}
```

### Troubleshooting Guide

#### Problem: No Logs Appearing

**Check:**
1. Loki is running: `systemctl status loki`
2. Promtail is running: `systemctl status promtail`
3. Network connectivity: `curl http://loki:3100/ready`
4. Promtail config: Check `/etc/promtail/config.yml`
5. Logs in Promtail: `journalctl -u promtail -n 100`

**Debug:**
```bash
# Test log sending
echo "test log" | curl -H "Content-Type: application/json" \
  -XPOST -s "http://loki:3100/loki/api/v1/push" \
  --data-raw '{"streams": [{"stream": {"app": "test"}, "values": [["'$(date +%s)000000000'", "test log"]]}]}'
```

#### Problem: Slow Queries

**Check:**
1. Query time range: Reduce if > 1 hour
2. Label cardinality: Run `loki-canary` to check
3. Compaction status: Check Loki logs for compaction errors
4. Resource limits: Increase memory/CPU if needed

**Optimize:**
```logql
# Before optimization
{app="fraiseql"} | json | level="error"  # SLOW

# After optimization
{app="fraiseql", level="error"}  # FAST
```

#### Problem: High Cardinality

**Identify:**
```promql
# Check series count per label
topk(10, sum by (__name__) (loki_ingester_memory_streams))
```

**Fix:**
- Remove high-cardinality labels (UUIDs, timestamps)
- Use structured JSON instead of labels
- Configure label limits in Loki config

### Monitoring Checklist

- [ ] Prometheus scraping Loki metrics
- [ ] Alert rules configured and tested
- [ ] Grafana dashboards installed
- [ ] Health checks in Kubernetes/Docker
- [ ] Log retention policy set
- [ ] Disk space monitoring active
- [ ] Backup strategy for Loki data
- [ ] Runbook for common issues

### Further Reading

- [Loki Monitoring Guide](https://grafana.com/docs/loki/latest/operations/monitoring/)
- [Official Loki Dashboards](https://grafana.com/orgs/lokiproject/dashboards)
- [Troubleshooting Loki](https://grafana.com/docs/loki/latest/troubleshooting/)
```

---

### Task 4: Security Hardening (Line 662)
**Estimated Time:** 3 hours

**Content to Add:**
```markdown
## Security Hardening

### Authentication & Authorization

#### 1. Enable Authentication

**Nginx Reverse Proxy (Recommended):**
```nginx
server {
    listen 443 ssl;
    server_name loki.example.com;

    ssl_certificate /etc/ssl/certs/loki.crt;
    ssl_certificate_key /etc/ssl/private/loki.key;

    location / {
        auth_basic "Loki Access";
        auth_basic_user_file /etc/nginx/.htpasswd;

        proxy_pass http://loki:3100;
        proxy_set_header X-Scope-OrgID $remote_user;
    }
}
```

**Create users:**
```bash
htpasswd -c /etc/nginx/.htpasswd admin
htpasswd /etc/nginx/.htpasswd developer
```

#### 2. Multi-Tenancy with X-Scope-OrgID

**Enable in Loki config:**
```yaml
# loki-config.yaml
auth_enabled: true

limits_config:
  reject_old_samples: true
  reject_old_samples_max_age: 168h
  max_query_length: 721h

  # Per-tenant limits
  ingestion_rate_mb: 10
  ingestion_burst_size_mb: 20
  max_streams_per_user: 10000
  max_query_series: 1000
```

**Set tenant ID in Promtail:**
```yaml
# promtail-config.yaml
clients:
  - url: http://loki:3100/loki/api/v1/push
    tenant_id: production
```

**Query with tenant ID:**
```bash
curl -H "X-Scope-OrgID: production" \
  http://loki:3100/loki/api/v1/query?query={app="fraiseql"}
```

### Network Security

#### 1. Firewall Rules

**Allow only necessary ports:**
```bash
# Loki HTTP API
iptables -A INPUT -p tcp --dport 3100 -s 10.0.0.0/8 -j ACCEPT
iptables -A INPUT -p tcp --dport 3100 -j DROP

# Prometheus scraping
iptables -A INPUT -p tcp --dport 3100 -s PROMETHEUS_IP -j ACCEPT
```

#### 2. TLS Encryption

**Enable HTTPS in Loki:**
```yaml
# loki-config.yaml
server:
  http_listen_port: 3100
  http_tls_config:
    cert_file: /etc/loki/tls/server.crt
    key_file: /etc/loki/tls/server.key
    client_ca_file: /etc/loki/tls/ca.crt
    client_auth_type: RequireAndVerifyClientCert
```

**Generate certificates:**
```bash
# Self-signed (development)
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout server.key -out server.crt

# Production: Use Let's Encrypt
certbot certonly --standalone -d loki.example.com
```

#### 3. mTLS for Promtail → Loki

**Loki config:**
```yaml
server:
  http_tls_config:
    cert_file: /etc/loki/tls/server.crt
    key_file: /etc/loki/tls/server.key
    client_ca_file: /etc/loki/tls/ca.crt
    client_auth_type: RequireAndVerifyClientCert
```

**Promtail config:**
```yaml
clients:
  - url: https://loki:3100/loki/api/v1/push
    tls_config:
      cert_file: /etc/promtail/tls/client.crt
      key_file: /etc/promtail/tls/client.key
      ca_file: /etc/promtail/tls/ca.crt
```

### Data Security

#### 1. Log Scrubbing

**Remove sensitive data before sending:**
```yaml
# promtail-config.yaml
scrape_configs:
  - job_name: fraiseql
    pipeline_stages:
      # Scrub passwords
      - replace:
          expression: 'password=\S+'
          replace: 'password=***'

      # Scrub API keys
      - replace:
          expression: 'api_key=\S+'
          replace: 'api_key=***'

      # Scrub credit cards
      - replace:
          expression: '\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b'
          replace: 'XXXX-XXXX-XXXX-XXXX'

      # Scrub emails
      - replace:
          expression: '\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b'
          replace: '***@***.***'
```

#### 2. Access Logging

**Enable audit logs:**
```yaml
# loki-config.yaml
server:
  log_level: info
  log_format: json

audit:
  enabled: true
  log_path: /var/log/loki/audit.log
```

**Monitor access:**
```bash
tail -f /var/log/loki/audit.log | jq '{user: .user, query: .query, time: .time}'
```

#### 3. Data Retention & Deletion

**Configure retention:**
```yaml
# loki-config.yaml
compactor:
  retention_enabled: true
  retention_delete_delay: 2h

table_manager:
  retention_deletes_enabled: true
  retention_period: 720h  # 30 days
```

**Manual deletion:**
```bash
# Delete logs for specific tenant
curl -X POST -H "X-Scope-OrgID: production" \
  "http://loki:3100/loki/api/v1/delete?query={app=\"fraiseql\"}&start=1577836800&end=1609459200"
```

### Access Control

#### 1. Role-Based Access

**Grafana Integration:**
```yaml
# grafana.ini
[auth]
disable_login_form = false
oauth_auto_login = false

[users]
viewers_can_edit = false
editors_can_admin = false

[auth.proxy]
enabled = true
header_name = X-WEBAUTH-USER
header_property = username
auto_sign_up = true
```

**Loki Data Source Permissions:**
- **Viewers**: Read-only access to specific tenants
- **Editors**: Read/write access to dev tenants
- **Admins**: Full access to all tenants

#### 2. Query Restrictions

**Limit query scope:**
```yaml
# loki-config.yaml
limits_config:
  max_query_length: 30d
  max_query_lookback: 30d
  max_entries_limit_per_query: 10000

  # Prevent expensive queries
  split_queries_by_interval: 24h
  max_query_parallelism: 16
```

### Kubernetes Security

#### 1. Pod Security

**SecurityContext:**
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: loki
spec:
  template:
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        fsGroup: 10001
        seccompProfile:
          type: RuntimeDefault
      containers:
      - name: loki
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop:
              - ALL
```

#### 2. Network Policies

**Restrict traffic:**
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
    - Egress
  ingress:
    # Allow from Promtail
    - from:
      - podSelector:
          matchLabels:
            app: promtail
      ports:
      - protocol: TCP
        port: 3100
    # Allow from Grafana
    - from:
      - podSelector:
          matchLabels:
            app: grafana
      ports:
      - protocol: TCP
        port: 3100
  egress:
    # Allow to object storage
    - to:
      - podSelector:
          matchLabels:
            app: s3
      ports:
      - protocol: TCP
        port: 443
```

### Security Checklist

**Authentication:**
- [ ] Authentication enabled (auth_basic or OAuth)
- [ ] TLS/HTTPS configured
- [ ] mTLS for inter-service communication
- [ ] Strong passwords/keys used

**Authorization:**
- [ ] Multi-tenancy with X-Scope-OrgID
- [ ] Per-tenant rate limits configured
- [ ] Query restrictions in place
- [ ] RBAC configured in Grafana

**Data Protection:**
- [ ] Sensitive data scrubbed in Promtail
- [ ] Logs encrypted in transit (TLS)
- [ ] Logs encrypted at rest (storage encryption)
- [ ] Retention policy configured
- [ ] Audit logging enabled

**Network Security:**
- [ ] Firewall rules configured
- [ ] Network policies in Kubernetes
- [ ] Non-root user for Loki process
- [ ] Read-only root filesystem

**Compliance:**
- [ ] GDPR data deletion procedure documented
- [ ] Access logs retained for audit
- [ ] Incident response plan includes Loki
- [ ] Regular security audits scheduled

### Security Best Practices

1. **Principle of Least Privilege**: Grant minimum necessary access
2. **Defense in Depth**: Multiple layers of security
3. **Regular Updates**: Keep Loki and dependencies updated
4. **Monitor Access**: Alert on suspicious query patterns
5. **Incident Response**: Have runbook for security incidents

### Further Reading

- [Loki Security Best Practices](https://grafana.com/docs/loki/latest/operations/security/)
- [Kubernetes Security Hardening](https://kubernetes.io/docs/concepts/security/hardening-guide/)
- [OWASP Logging Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html)
```

---

## Acceptance Criteria

### Must Have (P0)
- [ ] All 4 TODO sections completed with comprehensive content
- [ ] Code examples tested and verified
- [ ] Alert rules validated
- [ ] Grafana dashboard configs tested
- [ ] Security configurations verified in test environment

### Should Have (P1)
- [ ] Performance optimization examples benchmarked
- [ ] Decision matrix validated with real use cases
- [ ] Monitoring dashboards imported and tested
- [ ] Security checklist reviewed by security team

### Nice to Have (P2)
- [ ] Video walkthrough of Loki setup and monitoring
- [ ] Terraform/Helm examples for deployment
- [ ] Integration test suite for Loki configuration
- [ ] Automated security scanning of Loki deployment

---

## Validation Steps

### Task 1: Query Optimization
1. Test each query pattern in Grafana
2. Benchmark performance of good vs bad patterns
3. Verify cardinality recommendations
4. Document actual performance numbers

### Task 2: Decision Framework
1. Validate decision matrix with team
2. Test hybrid PostgreSQL + Loki workflow
3. Verify migration strategy steps
4. Get feedback from 3 users on clarity

### Task 3: Monitoring
1. Deploy Prometheus alert rules
2. Test each alert condition
3. Import Grafana dashboards
4. Verify health check endpoints
5. Run through troubleshooting guide

### Task 4: Security
1. Test authentication methods
2. Verify TLS configuration
3. Test log scrubbing patterns
4. Validate access control rules
5. Run security checklist

---

## DO NOT

- ❌ Copy/paste from other projects without testing
- ❌ Include deprecated Loki configuration options
- ❌ Recommend insecure patterns (basic auth over HTTP)
- ❌ Skip testing of code examples
- ❌ Leave placeholder "TODO" comments in final content
- ❌ Include outdated Loki version references
- ❌ Recommend anti-patterns (high-cardinality labels)

---

## Dependencies

### Before Starting
- Verify `.phases/loki_fixes_implementation_plan.md` is accessible
- Review existing Loki integration implementation
- Test Loki setup in dev environment
- Access to production Loki metrics (if available)

### Blockers
- None (documentation work can proceed independently)

---

## Notes

### Implementation Reference
All sections reference `.phases/loki_fixes_implementation_plan.md`:
- Section 1 (Query Optimization): Task 3.2
- Section 2 (PostgreSQL vs Loki): Task 3.3
- Section 3 (Monitoring): Task 3.4
- Section 4 (Security): Task 3.5

### Content Quality
- All configurations should be production-ready
- All code examples must be tested
- All links must be valid and up-to-date
- All security recommendations must follow best practices

### Future Enhancements
Consider adding in future iterations:
- Loki in Kubernetes (full Helm chart)
- Multi-region Loki federation
- Advanced cardinality management
- Cost optimization strategies
- Performance tuning for high-volume logs

---

## Success Metrics

- Documentation section completeness: 100% (all 4 TODOs resolved)
- Code example test coverage: 100% (all examples verified)
- User feedback score: ≥4/5 stars
- Time to implement Loki: Reduced by 50% (vs incomplete docs)
- Support questions about Loki: Reduced by 30%

---

**Status:** Ready for Implementation
**Created:** 2025-12-08
**Last Updated:** 2025-12-08
