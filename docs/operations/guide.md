# FraiseQL v2 Production Operations Guide

**Version**: 1.0
**Last Updated**: March 14, 2026
**Audience**: DevOps teams, Site Reliability Engineers, Operations managers

---

## Overview

This guide provides production operations best practices for deploying and maintaining FraiseQL v2 in enterprise environments. It includes:

- **SLA/SLO Framework**: Define availability and performance targets
- **Monitoring & Alerting**: Health checks, metrics, dashboards
- **Incident Response**: Procedures, escalation, communication templates
- **Operational Runbooks**: Common procedures and troubleshooting
- **On-Call Setup**: Team structure, training, drills
- **Backup & Disaster Recovery**: RTO/RPO, restore procedures

This guide is based on production-tested procedures used by the FraiseQL team. You should customize it to your organization's specific needs.

---

## Quick Start (15 minutes)

### If You Have Limited Time

1. **Read**: [SLA/SLO Framework](#part-1-slaslo-framework) (5 min)
2. **Configure**: [Health Checks](#health-checks) (3 min)
3. **Set Up**: [Basic Alerting](#alerting-rules) (7 min)
4. **Return Later**: Incident response procedures, on-call setup, training

---

## Part 1: SLA/SLO Framework

### What Are SLA and SLO?

**Service Level Objective (SLO)**: Internal target for your service quality

- Example: "99.9% uptime, P95 latency <100ms, error rate <0.1%"
- Private goal, guides engineering

**Service Level Agreement (SLA)**: External commitment to customers

- Example: "99.5% uptime SLA with service credits"
- Public promise, legal/commercial implications

### Recommended Targets for FraiseQL

Based on the FraiseQL team's experience:

```yaml
SLO (Internal Target):
  availability: 99.9%        # ~8.7 hours downtime/year
  latency_p95: <100ms        # 95% of queries faster than this
  error_rate: <0.1%          # 1 error per 1000 requests

SLA (Customer Commitment):
  availability: 99.5%        # ~22 hours downtime/year (10% buffer from SLO)
  latency_p95: <150ms        # 50% buffer for customer expectations
  error_rate: <0.2%          # 2x buffer from SLO
  credits: 10% for 99-99.5%, 25% for 98-99%, 50% for <98%
```

### How to Calculate SLO Compliance

**Monthly Availability**:

```
Uptime % = (Uptime Minutes / Total Minutes in Month) × 100
Target: 99.9% = 43.2 minutes downtime allowed per month
```

**Latency SLI** (Service Level Indicator):

```
Latency SLI = (Queries with P95 < 100ms / Total Queries) × 100
Target: 99.9% of queries meet latency target
```

**Error Rate SLI**:

```
Error Rate SLI = (Successful Queries / Total Queries) × 100
Target: 99.9% success rate (0.1% error rate)
```

### Customization Checklist

- [ ] Define your SLO targets (based on your needs)
- [ ] Define your SLA commitment (with legal review if needed)
- [ ] Decide on credit policy (what % to refund for SLA breach?)
- [ ] Choose measurement window (monthly, quarterly, annual?)
- [ ] Document in your public SLA document

---

## Part 2: Monitoring & Observability

### Health Checks

FraiseQL provides three health check endpoints following Kubernetes probe semantics:

#### 1. `/health` - Overall Health Status

Returns overall system health including uptime:

```bash
curl http://localhost:8000/health
```

**Response** (HTTP 200):

```json
{
  "status": "healthy",
  "timestamp": 1706794800,
  "uptime_seconds": 3600
}
```

**Use Case**: General health monitoring, dashboards, observability

---

#### 2. `/ready` - Readiness Probe (Can Accept Requests?)

Checks if the server is ready to accept requests (database connectivity, cache available):

```bash
curl http://localhost:8000/ready
```

**Response - Ready** (HTTP 200):

```json
{
  "ready": true,
  "database_connected": true,
  "cache_available": true,
  "reason": null
}
```

**Response - Not Ready** (HTTP 503):

```json
{
  "ready": false,
  "database_connected": false,
  "cache_available": true,
  "reason": "Database unavailable"
}
```

**Kubernetes Configuration** (recommended):

```yaml
readinessProbe:
  httpGet:
    path: /ready
    port: 8000
  initialDelaySeconds: 5
  periodSeconds: 10
  timeoutSeconds: 2
  successThreshold: 1
  failureThreshold: 3
```

**What It Checks**:

- ✅ Database connectivity
- ✅ Cache/Redis connectivity (if enabled)
- ✅ Configuration validity

**Use Case**: Kubernetes readiness probe, load balancer health checks, startup dependencies

---

#### 3. `/live` - Liveness Probe (Process Alive?)

Checks if the process is still running (lightweight, no dependency checks):

```bash
curl http://localhost:8000/live
```

**Response** (HTTP 200):

```json
{
  "alive": true,
  "pid": 42157,
  "response_time_ms": 1
}
```

**Kubernetes Configuration** (recommended):

```yaml
livenessProbe:
  httpGet:
    path: /live
    port: 8000
  initialDelaySeconds: 30
  periodSeconds: 10
  timeoutSeconds: 2
  successThreshold: 1
  failureThreshold: 3
```

**What It Checks**:

- ✅ Process is running
- ✅ Response time (detects hangs)
- ✅ Process ID still valid

**Use Case**: Kubernetes liveness probe, container restart decisions (never restart on readiness failure, only on liveness failure)

---

### Complete Kubernetes Probe Configuration

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
spec:
  template:
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:2.0.0
        ports:
        - containerPort: 8000

        # Startup Probe - Wait for app to start (only Kubernetes 1.16+)
        startupProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 0
          periodSeconds: 10
          timeoutSeconds: 2
          failureThreshold: 30  # 30 * 10 = 300s max startup time

        # Readiness Probe - Remove from LB if not ready
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 2
          successThreshold: 1
          failureThreshold: 3

        # Liveness Probe - Restart if hung
        livenessProbe:
          httpGet:
            path: /live
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 2
          successThreshold: 1
          failureThreshold: 3

        # Termination Grace Period - Allow graceful shutdown
        terminationGracePeriodSeconds: 30
```

### Graceful Shutdown

FraiseQL handles graceful shutdown with signal handling:

**How It Works**:

1. Server receives `SIGTERM` signal
2. Stops accepting new requests
3. Waits for in-flight requests to complete (up to grace period)
4. Closes database connections
5. Exits cleanly

**Kubernetes Configuration**:

```yaml
terminationGracePeriodSeconds: 30  # Allow 30s for graceful shutdown
```

**Docker Configuration**:

```dockerfile
STOPSIGNAL SIGTERM
```

**Manual Testing**:

```bash
# Terminal 1: Start server
cargo run -p fraiseql-server

# Terminal 2: Send SIGTERM
kill -TERM <pid>

# Observe in Terminal 1:
# - "Shutdown requested"
# - "Draining in-flight requests..."
# - "Clean shutdown complete"
```

**Load Balancer Configuration**:

- Set connection drain timeout = grace period (30s)
- Stop sending new requests on SIGTERM
- Wait for in-flight requests to complete

### Load Balancer Configuration

Configure your load balancer to use the health endpoints:

**AWS ALB**:

```hcl
health_check {
  enabled = true
  healthy_threshold = 2
  unhealthy_threshold = 3
  timeout = 5
  interval = 30
  path = "/ready"
  port = "8000"
  protocol = "HTTP"
  matcher = "200"
}
```

**Nginx**:

```nginx
upstream fraiseql {
  server fraiseql-1:8000;
  server fraiseql-2:8000;
  keepalive 32;
}

server {
  location / {
    proxy_pass http://fraiseql;
    proxy_http_version 1.1;
    proxy_connect_timeout 5s;
    proxy_read_timeout 30s;
  }
}

# Health check (external configuration)
```

**HAProxy**:

```
backend fraiseql
  option httpchk GET /ready HTTP/1.1
  default-server inter 30s fall 3 rise 2
  server fraiseql-1 localhost:8000 check
  server fraiseql-2 localhost:8001 check
```

---

### Metrics Collection

FraiseQL exposes Prometheus metrics at `/metrics`:

**Key Metrics to Monitor**:

```
# Query execution
fraiseql_queries_total{status="success"}    # Total queries
fraiseql_query_duration_seconds             # Query latency
fraiseql_query_errors_total                 # Query errors
fraiseql_query_complexity_score             # GraphQL complexity

# API Key validation
fraiseql_api_keys_active                    # Active keys count
fraiseql_api_key_validations_total          # Validation attempts
fraiseql_api_key_validation_duration_ms     # Validation latency

# Database
fraiseql_db_connections_active              # Active connections
fraiseql_db_query_duration_seconds          # DB latency

# HTTP requests
fraiseql_http_requests_total{status="..."}  # All HTTP requests
fraiseql_http_request_duration_seconds      # HTTP latency

# System
fraiseql_uptime_seconds                     # Process uptime
```

**Prometheus Configuration**:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 30s
    scrape_timeout: 5s
```

---

### Alerting Rules

**Recommended Alert Rules** (using Prometheus/AlertManager):

```yaml
groups:
  - name: fraiseql.rules
    rules:
      # Service down
      - alert: ServiceDown
        expr: up{job="fraiseql"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "FraiseQL service down"

      # High error rate
      - alert: HighErrorRate
        expr: rate(fraiseql_query_errors_total[5m]) / rate(fraiseql_queries_total[5m]) > 0.005
        for: 5m
        labels:
          severity: high
        annotations:
          summary: "Error rate >0.5%"

      # High latency
      - alert: HighLatency
        expr: histogram_quantile(0.95, fraiseql_query_duration_seconds) > 0.2
        for: 5m
        labels:
          severity: high
        annotations:
          summary: "Query latency P95 >200ms"

      # Database connection pool
      - alert: DatabasePoolExhausted
        expr: fraiseql_db_connections_active / 100 > 0.9
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DB connection pool 90% full"

      # Disk capacity
      - alert: DiskFull
        expr: node_filesystem_avail_bytes / node_filesystem_size_bytes < 0.1
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Disk less than 10% available"
```

**Customize Alert Thresholds**:

1. Deploy FraiseQL to staging
2. Collect metrics for 1-2 weeks
3. Establish baseline metrics (P50, P95, P99)
4. Set alert thresholds at 2-3x baseline (avoid false positives)
5. Test alert notification flow
6. Deploy to production

---

### Grafana Dashboards

**Recommended Dashboards**:

**Dashboard 1: Production Health**

- Uptime percentage (target: 99.9%)
- Request rate (queries per minute)
- Error rate (target: <0.1%)
- Query latency P95 (target: <100ms)
- Database connections (warning: >80%)
- API key count (trend monitoring)

**Dashboard 2: Database Health**

- Database query latency P95
- Active connections vs max
- Replication lag (if applicable)
- Disk usage
- Query throughput

**Example Grafana Variables**:

```
$job = fraiseql (data source: Prometheus)
$environment = production
$cluster = us-east-1
```

---

### Logging & Observability

**Log Aggregation**:

FraiseQL logs in JSON format for easy parsing:

```json
{
  "timestamp": "2026-03-15T10:30:15.234Z",
  "level": "INFO",
  "message": "Query executed",
  "fields": {
    "query_hash": "abc123def456",
    "api_key_id": "key_xyz",
    "duration_ms": 45.3,
    "result_rows": 150,
    "status": "success"
  }
}
```

**Log Aggregation Setup** (e.g., Elasticsearch):

```yaml
# Filebeat configuration
filebeat.inputs:
  - type: log
    enabled: true
    paths:
      - /var/log/fraiseql/*.log

output.elasticsearch:
  hosts: ["elasticsearch.example.com:9200"]
  index: "fraiseql-%{+yyyy.MM.dd}"

setup.dashboards.enabled: true
setup.kibana.host: "kibana.example.com:5601"
```

**Log Retention**:

- Hot storage (Elasticsearch): 90 days (searchable)
- Cold storage (S3 Glacier): 7 years (compliance)
- Cost: ~$0.50/GB for hot, ~$0.004/GB for cold

---

## Part 3: Incident Response

### Incident Severity Levels

**CRITICAL** (Immediate Response Required)

```
Impact: Complete service outage or data loss
Examples:
  - Service completely unresponsive (0% success rate)
  - Data loss or corruption detected
  - Security breach confirmed

Response Target: <2 minutes to acknowledge
Escalation: Page all on-call staff immediately
Communication: Status page update + customer notification within 15 min
```

**HIGH** (Urgent Response Required)

```
Impact: Significant degradation, customer impact
Examples:
  - 25-99% of traffic failing
  - Latency spike (P95 >500ms for >5 min)
  - Error rate 1-5%
  - Security vulnerability disclosed (not yet exploited)

Response Target: <15 minutes
Escalation: Page if not acknowledged within 15 min
Communication: Customer notification if >30 min impact
```

**MEDIUM** (Standard Response)

```
Impact: Minor issues, customer may notice
Examples:
  - <25% of traffic affected
  - Latency 100-500ms
  - Error rate <1%
  - Non-critical security finding

Response Target: <1 hour
Escalation: Ticket queue with daily check-in
Communication: Slack notification only
```

**LOW** (Routine Response)

```
Impact: No immediate customer impact
Examples:
  - Warning-level alerts
  - Trend observations
  - Non-urgent configuration issues

Response Target: <24 hours
Escalation: Ticket system
Communication: Weekly summary
```

---

### 5-Phase Incident Response Workflow

**Phase 1: Detection & Alert** (0-5 min)

- Alert fires automatically (monitoring → Slack/PagerDuty)
- On-call engineer acknowledges within SLA
- Assess: Real incident or false positive?
- Determine severity level
- Open incident channel (`#incident-YYYYMMDD-NNN`)

**Phase 2: Triage & Assessment** (5-15 min)

- Check dashboards: Which metrics are affected?
- Check logs: What errors are occurring?
- Check recent changes: New deployments, config changes?
- Initial diagnosis: What's likely the root cause?
- Escalate if needed (unclear diagnosis or beyond knowledge)

**Phase 3: Mitigation** (15 min - ongoing)
Based on root cause, implement fix:

- Application bug? Deploy hotfix
- Database issue? Restart service, check connections
- Resource exhaustion? Scale up, restart
- External dependency down? Activate fallback

**Phase 4: Recovery & Verification** (varies)

- Verify fix works: Metrics return to normal, alerts clear
- Monitor for 30 minutes: Watch for resurrection of issue
- Update status: "Issue resolved, validating stability"
- Announce resolution when stable

**Phase 5: Post-Incident** (within 24 hours)

- Schedule RCA (Root Cause Analysis) meeting
- Identify preventive measures
- Create action items with owners and due dates
- Send customer notification (if SLA breached)
- Document lesson learned

---

### Communication Templates

#### Template 1: Incident Declaration (Slack)

```
:warning: **INCIDENT DECLARED: [SERVICE]**

**Severity**: [CRITICAL / HIGH / MEDIUM]
**Service**: [Service Name]
**Detected**: [Time] UTC
**Impact**:
  - [% of traffic or # requests affected]
  - [Customer-visible impact]

**Current Status**: Investigating
**Slack Channel**: #incident-YYYYMMDD-NNN
**Dashboard**: [Link to Grafana]
**Next Update**: In 5 minutes

**Assigned To**: @on-call-engineer
```

#### Template 2: Status Update

```
:clock1: **INCIDENT UPDATE** - [TIME] UTC

**Status**: [Investigating / Identified / Mitigating / Monitoring]

**Root Cause** (if identified):
[Technical description]

**Mitigation in Progress**:

- Action 1: [Status]
- Action 2: [Status]

**Current Impact**:

- Error rate: [X]%
- Latency: [Y]ms
- Affected customers: [~N]

**ETA**: [Estimated time to resolution]
```

#### Template 3: Resolution (Slack)

```
:white_check_mark: **INCIDENT RESOLVED** - [TIME] UTC

**Duration**: [HH:MM] (from [start] to [end] UTC)

**Root Cause**:
[Technical summary]

**Impact**:
[# queries affected, % of traffic, customer segments]

**What We'll Do**:

- [Preventive measure 1] - Target: [DATE]
- [Preventive measure 2] - Target: [DATE]

**RCA Meeting**: [DATE] [TIME] UTC
Questions? Ping @incident-commander
```

#### Template 4: Customer Notification (Email)

```
Subject: [RESOLVED] Service Disruption - [DATE]

Dear Valued Customer,

We experienced a service disruption on [DATE] from [TIME-TIME] UTC.

WHAT HAPPENED:
[Technical description for technical audience]

ROOT CAUSE:
[Why it happened]

IMPACT ON YOUR ACCOUNT:
[Did they experience it? How many queries failed?]

RESOLUTION:
We [describe what we did to fix it]

NEXT STEPS:
We will implement:

1. [Prevention 1] - Target: [DATE]
2. [Prevention 2] - Target: [DATE]

SERVICE CREDIT:
We've issued a [X]% service credit to your account.

APOLOGY:
We sincerely apologize for the disruption.

Contact: support@fraiseql.com
```

---

## Part 4: Operational Runbooks

### Runbook 1: Service Restart

**When to Use**: Service unresponsive, hanging, memory leak suspected

**Steps**:

1. Verify service is down:

   ```bash
   curl https://your-fraiseql-api.com/health
   # Expected: Connection refused or timeout
   ```

2. Check logs for recent errors:

   ```bash
   kubectl logs -f deployment/fraiseql-api
   # Look for: panic, segfault, out of memory
   ```

3. Restart service:

   ```bash
   kubectl rollout restart deployment/fraiseql-api
   # Or: systemctl restart fraiseql
   ```

4. Verify restart:

   ```bash
   kubectl get deployment fraiseql-api
   # Expected: Ready 1/1, Restarts: 1
   ```

5. Health check:

   ```bash
   curl https://your-fraiseql-api.com/health
   # Expected: 200 OK, status: healthy
   ```

6. Monitor metrics for 5 minutes:
   - Error rate returns to baseline
   - No cascading failures

**Estimated Time**: 2-3 minutes

**Success Criteria**:

- [ ] Health check returns 200 OK
- [ ] Error rate drops to baseline within 2 minutes
- [ ] No new errors in logs

---

### Runbook 2: Database Recovery from Backup

**When to Use**: Data corruption, data loss, database unresponsive

**Prerequisites**:

- Automated backups enabled (recommended: every 6 hours)
- Backups stored in S3 or similar (separate from primary database)
- Restore procedures tested monthly

**Steps**:

1. Assess damage:

   ```sql
   SELECT COUNT(*) FROM users;
   -- Is this number correct?
   ```

2. Stop application (prevent further writes):

   ```bash
   kubectl scale deployment fraiseql-api --replicas=0
   ```

3. Find latest backup:

   ```bash
   aws s3 ls s3://your-backup-bucket/ | sort
   # Find most recent backup before incident
   ```

4. Download and restore:

   ```bash
   aws s3 cp s3://your-backup-bucket/2026-03-15-12-00.sql.gz .
   gunzip 2026-03-15-12-00.sql.gz

   psql -h your-database.rds.amazonaws.com -U fraiseql -d fraiseql \
     < 2026-03-15-12-00.sql
   ```

5. Verify restoration:

   ```sql
   SELECT COUNT(*) FROM users;
   -- Should match expected count from before incident
   ```

6. Restart application:

   ```bash
   kubectl scale deployment fraiseql-api --replicas=3
   ```

7. Health check:

   ```bash
   curl https://your-fraiseql-api.com/health
   # Expected: 200 OK
   ```

8. Monitor metrics:
   - Error rate normal
   - No data inconsistencies

**Estimated Time**: 30-45 minutes

**Success Criteria**:

- [ ] Data restored to correct state
- [ ] Health check passes
- [ ] Error rate returns to baseline
- [ ] Spot-check random records verify correctness

---

### Runbook 3: API Key Revocation (Security)

**When to Use**: Compromised API key, suspected data exfiltration, brute force attack

**Steps**:

1. Identify compromised key:

   ```
   From anomaly alert:
   - Key: fraiseql_us_east_1_xxxxx
   - Activity: Rate spike + new field access (PII)
   ```

2. Verify compromise:

   ```bash
   # Check audit logs for suspicious queries
   curl -H "Authorization: Bearer $ES_TOKEN" \
     "elasticsearch.example.com/_search" \
     -d '{"query": {"term": {"api_key": "key_xyz"}}}'
   ```

3. Revoke key immediately:

   ```sql
   UPDATE api_keys
   SET revoked_at = NOW(),
       revoke_reason = 'Security incident - suspected compromise'
   WHERE api_key_id = 'key_xyz';
   ```

4. Verify revocation:

   ```bash
   curl -H "Authorization: Bearer fraiseql_us_east_1_xxxxx" \
     https://your-fraiseql-api.com/graphql
   # Expected: 401 Unauthorized
   ```

5. Generate replacement key:

   ```bash
   fraiseql-cli key generate \
     --name "replacement-key" \
     --tier premium \
     --permissions "read,write"
   ```

6. Notify customer (use template 4):
   - Explain what happened
   - Provide new key (secure channel)
   - Recommend rotation of other keys
   - Request to confirm no unauthorized data access

7. Investigate impact:

   ```bash
   # What data did they access?
   # Which tables/fields?
   # How many queries?
   ```

8. Document incident:
   - Time revoked: [TIME]
   - Reason: [REASON]
   - Customer impact: [IMPACT]
   - Root cause: [HOW WAS IT COMPROMISED?]

**Estimated Time**: 10-15 minutes to revoke, 1-2 hours for investigation

**Success Criteria**:

- [ ] Old key returns 401 Unauthorized
- [ ] New key works correctly
- [ ] Customer notified with new key
- [ ] Forensics investigation complete

---

### Runbook 4: Rate Limit Adjustment

**When to Use**: Legitimate traffic blocked, false positive alerts, customer hitting limits

**Steps**:

1. Identify which limit is triggered:

   ```bash
   # Check metrics
   curl https://your-fraiseql-api.com/metrics | grep rate_limit

   # Check logs
   grep "429\|rate.limit" /var/log/fraiseql/access.log
   ```

2. Verify it's legitimate traffic:

   ```sql
   SELECT api_key, COUNT(*) as requests, source_ip
   FROM audit_logs
   WHERE timestamp > now() - interval '5 minutes'
   GROUP BY api_key, source_ip
   ORDER BY requests DESC;
   ```

3. Increase limit temporarily (if legitimate):

   ```bash
   # Option 1: Redis (temporary, until service restart)
   redis-cli SET rate_limit:tier:premium:rps 2000
   redis-cli EXPIRE rate_limit:tier:premium:rps 3600

   # Option 2: Configuration file (permanent)
   # Edit config.toml
   # [rate_limits]
   # tier.premium.rps = 2000
   ```

4. Monitor impact:

   ```bash
   # Watch for 5 minutes:
   watch -n 1 'curl -s https://your-fraiseql-api.com/metrics | grep rate_limit'

   # Check error rate didn't increase
   ```

5. Permanent fix (if pattern repeats):

   ```bash
   # Deploy configuration update
   git commit -m "chore: increase rate limit for tier.premium to 2000 rps"
   git push origin main

   # Deploy
   kubectl rollout restart deployment/fraiseql-api
   ```

6. Document decision:
   - Why increased? [REASON]
   - For which customer/tier?
   - When revert? [DATE or "permanent"]

**Estimated Time**: 5-10 minutes

**Success Criteria**:

- [ ] Legitimate traffic flowing
- [ ] Error rate unchanged
- [ ] No security regression
- [ ] Metrics confirm limits working

---

## Part 5: On-Call Setup & Training

### On-Call Team Structure

**Roles**:

- **Primary On-Call**: Handles all alerts, first responder
- **Backup On-Call**: Takes over if primary unavailable
- **On-Call Manager**: Escalation point for complex decisions
- **Incident Commander** (for CRITICAL): Coordinates response

**Schedule**:

- Weekly rotation (Monday 00:00 UTC → next Monday)
- 30 min overlap for knowledge transfer
- Backup covers nights/weekends if primary unavailable

**On-Call Requirements**:

- [ ] Full training (5 days, see training plan)
- [ ] All tool access verified
- [ ] Knowledge assessment (80%+ pass)
- [ ] Mock incident drill passed
- [ ] Sign-off from incident commander

---

### Training Plan (5 Days)

**Day 1: System Overview** (2.5 hours)

- [ ] Architecture review
- [ ] Service dependencies
- [ ] SLA/SLO targets
- [ ] Dashboards tour
- [ ] Where to find runbooks

**Day 2: Alert Familiarization** (2.5 hours)

- [ ] Alert types and meanings
- [ ] Alert thresholds and why
- [ ] False positives and how to dismiss
- [ ] PagerDuty workflow
- [ ] Hands-on: Trigger alerts in staging

**Day 3: Runbook Exercises** (2.5 hours)

- [ ] Practice Runbook 1 (service restart) - staging
- [ ] Practice Runbook 2 (DB recovery) - staging
- [ ] Practice Runbook 3 (key revocation) - staging
- [ ] Practice Runbook 4 (rate limit tuning) - staging
- [ ] Q&A

**Day 4: Incident Response** (2.5 hours)

- [ ] Walk through 5-phase response workflow
- [ ] Communication templates
- [ ] Escalation procedures
- [ ] Mock incident drill (simulated alert)
- [ ] Practice full response

**Day 5: Integration & Shadowing** (2.5 hours)

- [ ] Full shift shadowing with current on-call
- [ ] Observe real alerts and responses
- [ ] Practice with actual dashboards
- [ ] Questions and clarifications
- [ ] Sign-off meeting

**Customization Checklist**:

- [ ] Adjust training schedule to your team
- [ ] Add organization-specific procedures
- [ ] Update emergency contact list
- [ ] Customize alert thresholds to your baselines
- [ ] Create role-specific training paths

---

### Mock Incident Drill Scenarios

**Scenario 1: Service Down** (30 min)

- Simulate: Service crashes or becomes unresponsive
- Expected response: Alert → diagnosis → restart → verify
- Success: Service back in <15 minutes

**Scenario 2: Database Failure** (45 min)

- Simulate: Database corruption or loss
- Expected response: Stop app → restore from backup → verify → restart
- Success: Service back in <45 minutes

**Scenario 3: Security Incident** (20 min)

- Simulate: Anomaly detection alert, suspicious key activity
- Expected response: Diagnose → revoke key → notify customer → investigate
- Success: Key revoked in <10 minutes, customer notified

---

### Continuous Improvement

**Monthly Incident Review**:

- [ ] Review all incidents from past month
- [ ] Discuss response times and effectiveness
- [ ] Identify patterns or trends
- [ ] Update procedures if needed
- [ ] Schedule quarterly training refresher

**Quarterly Training Refresher** (4 hours):

- [ ] Runbook exercises (practice all procedures)
- [ ] Mock incident drill (new scenario)
- [ ] System changes review (new features)
- [ ] Tool updates (Prometheus, Grafana, etc.)

---

## Part 6: Backup & Disaster Recovery

### Backup Strategy

**Schedule**:

```
Every 6 hours:
  00:00 UTC - Full backup
  06:00 UTC - Full backup
  12:00 UTC - Full backup
  18:00 UTC - Full backup
```

**Process**:

1. Export database (pg_dump)
2. Compress with gzip (10-15× reduction)
3. Encrypt with KMS
4. Upload to S3 (different region)
5. Verify integrity (monthly restore test)

**Retention**:

- Keep 30 days of backups (4 per day × 30 = 120 backups)
- Auto-delete older backups
- Cost: ~$1-2/month for standard backups

**Customization Checklist**:

- [ ] Set up backup schedule
- [ ] Configure S3 bucket with encryption
- [ ] Test restore procedure (weekly)
- [ ] Automate cleanup of old backups
- [ ] Document backup location and credentials

---

### RTO/RPO Targets

**Recovery Time Objective (RTO)**: <1 hour

- How long to recover from complete data loss?
- FraiseQL can restore typical database in ~30 minutes

**Recovery Point Objective (RPO)**: <5 minutes

- How much data loss is acceptable?
- With 6-hour backups: lose up to 6 hours of data
- To improve: Increase backup frequency or implement WAL archiving

**Improvement Options**:

- Higher backup frequency: Every hour (more cost)
- WAL archiving: Continuous log backup (best RTO/RPO)
- Replication: Multi-region replication

---

## Part 7: Customization Checklist

Before going to production, customize this guide for your deployment:

### Infrastructure

- [ ] Choose monitoring tool (Prometheus, DataDog, etc.)
- [ ] Set up health check monitoring
- [ ] Configure alerting (PagerDuty, Slack, etc.)
- [ ] Set up logging aggregation (ELK, Splunk, etc.)
- [ ] Configure backup automation
- [ ] Test backup restoration procedure

### Team & On-Call

- [ ] Define on-call schedule
- [ ] Assign team members
- [ ] Create emergency contact list
- [ ] Customize training materials
- [ ] Conduct team training
- [ ] Run mock incident drills

### SLA/SLO

- [ ] Define SLA targets (with business/legal)
- [ ] Document credit policy
- [ ] Set alert thresholds (based on your baseline)
- [ ] Create dashboards for compliance tracking
- [ ] Communicate SLA to customers

### Procedures

- [ ] Customize runbooks to your environment
- [ ] Update IP addresses, hostnames, credentials
- [ ] Customize communication templates
- [ ] Update emergency procedures (org-specific)
- [ ] Document where credentials are stored

### Documentation

- [ ] Update this guide with your values
- [ ] Create organization-specific procedures
- [ ] Link to your dashboards/tools
- [ ] Post in shared wiki/documentation site
- [ ] Share with team and maintain

---

## Appendix A: Glossary

**SLA**: Service Level Agreement (external commitment)
**SLO**: Service Level Objective (internal target)
**SLI**: Service Level Indicator (actual measurement)
**RTO**: Recovery Time Objective (max time to recover)
**RPO**: Recovery Point Objective (max acceptable data loss)
**P95**: 95th percentile (95% of values are below this)
**Latency**: Time for request to complete
**Throughput**: Requests per second
**Availability**: % of time service is operational

---

## Appendix B: Further Reading

- **FraiseQL Architecture**: [Link to architecture docs]
- **GraphQL Performance**: [Link to performance guide]
- **Database Configuration**: [Link to DB tuning guide]
- **Security Best Practices**: [Link to security guide]
- **Prometheus Documentation**: <https://prometheus.io/docs/>
- **Grafana Documentation**: <https://grafana.com/docs/>
- **PagerDuty Documentation**: <https://support.pagerduty.com/>

---

## Support & Questions

If you have questions about this guide or customizing it for your environment:

- **GitHub Issues**: [Link to FraiseQL GitHub]
- **Community Forum**: [Link to community forum]
- **Email**: <hello@fraiseql.com>

---

**Last Updated**: March 14, 2026
**Version**: 1.0
**Status**: Production-ready template

This guide is based on production-tested procedures. Customize for your organization's specific needs.
