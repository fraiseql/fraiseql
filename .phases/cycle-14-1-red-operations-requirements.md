# Phase 14, Cycle 1 - RED: Operations & Maturity Requirements

**Date**: March 3-7, 2026
**Phase Lead**: Operations Lead + Site Reliability Engineer (SRE)
**Status**: RED (Defining Operations & Maturity Requirements)

---

## Objective

Define comprehensive operations and maturity requirements for FraiseQL v2, specifying backup/recovery procedures, SLA/SLO targets, incident management workflows, and operational runbooks for sustainable, reliable production deployment.

---

## Background: Readiness from Phase 13

From Phase 13 (Security Hardening) completion:
- ✅ Security posture: Enterprise-grade with 5-layer defense-in-depth
- ✅ Compliance: SOC2/GDPR/HIPAA ready, external pentest approved
- ✅ Monitoring: Real-time anomaly detection with 7 rules
- ✅ Incident Response: Procedures documented, <2-minute response time

Phase 14 focuses on **operational excellence**:
1. **Availability**: SLA/SLO definition, uptime targets
2. **Reliability**: Backup/recovery procedures, RTO/RPO targets
3. **Observability**: Logging, metrics, tracing strategy
4. **Resilience**: Disaster recovery, failover procedures
5. **Runbooks**: Documented procedures for common scenarios

---

## SLA/SLO Definition

### Service Level Objectives (SLOs)

**Availability Target**: 99.9% uptime (99.9 SLO)

```
Monthly downtime budget: 43.2 minutes
Quarterly downtime budget: 2 hours 10 minutes
Annual downtime budget: 8 hours 45 minutes
```

**Rationale**:
- Enterprise customers expect 99.9% for critical APIs
- 99.95% would require redundancy (Phase 15+)
- SLA marketing commitment: 99.5% (10 minutes buffer)

---

### Performance Targets (SLOs)

**Query Latency P95**: <100ms

**Why P95 instead of P99?**
- P95 catches 95% of user experience
- P99 is heavily influenced by outliers
- Focus on typical user experience

**Target Breakdown**:
- GraphQL parsing: <5ms
- Query validation: <10ms
- Complexity scoring: <2ms
- Authorization check: <5ms
- Database query: <70ms
- Response encoding: <5ms
- **Total P95**: ~100ms

---

**Error Rate SLO**: <0.1% (1 error per 1000 requests)

**Why <0.1%?**
- Temporary glitches acceptable (<1 in 1000)
- Systematic errors should trigger alerts
- Below detection threshold of anomaly system

---

### Service Level Agreements (SLAs)

**SLA Commitment to Customers**:
- Uptime: 99.5% monthly
- Query latency: P95 <150ms (10% buffer vs SLO)
- Error rate: <0.2% (2x SLO buffer)

**SLA Credits**:
- 99.0-99.5% uptime: 10% service credit
- 98.0-99.0% uptime: 25% service credit
- <98.0% uptime: 50% service credit

---

## Backup & Disaster Recovery

### RTO/RPO Targets

**Recovery Time Objective (RTO)**: <1 hour

Meaning: Complete system recovery from complete data loss in <1 hour

**Recovery Point Objective (RPO)**: <5 minutes

Meaning: Lose at most 5 minutes of data

**Rationale**:
- <1 hour RTO: Enterprise expectation for critical APIs
- <5 minutes RPO: Protects against most failure scenarios
- Stricter targets (15 min RTO) in Phase 15+ with multi-region

---

### Backup Strategy

**Primary Database Backup**:

```
Schedule: Every 6 hours (4 backups/day)
Retention: 30 days (rotating 4-week window)
Type: Full backup + incremental logs
Storage: AWS S3 (separate region from primary)
Encryption: KMS-backed encryption
Verification: Monthly restore test
```

**RTO Calculation**:
- Backup restoration: ~15 minutes (100GB database)
- Service startup: ~5 minutes
- Health checks: ~5 minutes
- **Total RTO**: ~25 minutes ✅ (meets <1 hour target)

---

**Audit Log Backup**:

```
Schedule: Continuous (Kafka → S3)
Retention: 7 years (compliance requirement)
Type: Immutable append-only logs
Storage: Tiered (90 days hot S3, 7yr cold Glacier)
Redundancy: Elasticsearch replica for searchability
```

---

### Disaster Recovery Scenarios

**Scenario 1: Database Corruption**
- **Detection**: Data quality check fails, anomaly detection alerts
- **Response**: Automated failover to warm replica (Phase 15+)
- **Current (Phase 14)**: Manual restore from backup
- **RTO**: <1 hour via restore procedure

**Scenario 2: Complete Data Loss**
- **Detection**: Monitoring dashboard shows no data
- **Response**: S3 backup recovery procedure
- **RTO**: <1 hour (backup restoration + verification)
- **RPO**: <5 minutes (last 6-hour backup)

**Scenario 3: API Key Compromise**
- **Detection**: Anomaly detection rule triggers (rate spike + new fields)
- **Response**: Automated revocation + customer notification
- **RTO**: <2 minutes (detection to revocation)
- **Communication**: Automatic email + support ticket

**Scenario 4: DDoS Attack**
- **Detection**: Rate limiting triggered globally
- **Response**: Rate limits increase, alerting escalates
- **Mitigation**: AWS Shield (managed by infrastructure)
- **Fallback**: Graceful degradation (serve cached queries)

---

## Incident Management

### Incident Severity Levels

**CRITICAL** (Immediate Response)
- Complete service outage
- Data loss or corruption
- Security breach confirmed
- Response time: <2 minutes (automated alert)
- On-call: Immediate page

**HIGH** (Urgent Response)
- Partial outage (>25% traffic affected)
- Severe performance degradation (P95 >500ms)
- Security vulnerability disclosed
- Response time: <15 minutes
- On-call: Page within 15 min

**MEDIUM** (Standard Response)
- Minor outage (<25% traffic affected)
- Performance degradation (P95 100-500ms)
- Non-critical security finding
- Response time: <1 hour
- On-call: Ticket in queue

**LOW** (Routine Response)
- Warnings from monitoring
- Edge case issues
- Feature requests categorized as incidents
- Response time: <24 hours
- Ticket system only

---

### Incident Response Procedure

```
Detection (Automated)
  ↓
Alert Generated (Slack → On-call)
  ↓
Acknowledgement (On-call within SLA)
  ↓
Initial Assessment (<5 min)
  ├→ Scope: Which systems affected?
  ├→ Severity: CRITICAL/HIGH/MEDIUM/LOW?
  └→ Action: Escalate? Declare incident? Mitigate?
  ↓
Mitigation (<15 min for CRITICAL)
  ├→ Automated: Restart services, failover, rate limit increase
  └→ Manual: Code fix deploy, configuration change
  ↓
Recovery Verification
  ├→ Service health checks pass
  ├→ Metrics return to normal
  └→ Customer communication sent
  ↓
Post-Incident (Within 24 hours)
  ├→ Root cause analysis
  ├→ Preventive measures identified
  └→ Incident report created
```

---

## Monitoring & Observability

### Key Metrics to Monitor

**Availability Metrics**:
- Uptime percentage (target: 99.9%)
- Error rate by endpoint (target: <0.1%)
- Request success rate (target: >99.9%)

**Performance Metrics**:
- Query latency P50/P95/P99 (target P95: <100ms)
- Database query latency (target P95: <70ms)
- Authorization check latency (target P95: <5ms)

**Resource Metrics**:
- CPU utilization (warning: >70%, critical: >90%)
- Memory utilization (warning: >80%, critical: >95%)
- Database connection pool (warning: >80%, critical: >95%)
- Disk usage (warning: >80%, critical: >95%)

**Security Metrics**:
- Failed authentication attempts (baseline-based alert)
- Rate limit triggers (per IP, per key)
- API key rotations (monthly target: 100%)
- Anomalies detected (severity-based alert)

**Business Metrics**:
- Active API keys (trend analysis)
- Query volume by tier (usage trends)
- Data volume growth (capacity planning)
- Customer onboarding success rate

---

### Observability Components

**Logging**:
```
Application logs → Elasticsearch
  ├→ Request logs (query, latency, status)
  ├→ Error logs (exceptions, stack traces)
  ├→ Audit logs (API key operations, authz checks)
  └→ Security logs (anomalies, rate limits)

Retention: 90 days hot, 7 years cold (GDPR)
Searchability: Real-time Elasticsearch queries
```

**Metrics**:
```
Prometheus scrape
  ├→ Rust metrics (via Prometheus client)
  ├→ Database metrics (connection pool, query latency)
  ├→ AWS metrics (EC2, RDS, S3)
  └→ Custom metrics (query volume, API key count)

Retention: 15 days (high resolution), 1 year (1-hour aggregates)
Visualization: Grafana dashboards
```

**Distributed Tracing**:
```
OpenTelemetry instrumentation (Phase 14+)
  ├→ Query execution trace
  ├→ Database operation trace
  ├→ Authorization trace
  └→ Performance bottleneck identification

Sampling: 1% of requests (configurable)
Storage: Jaeger or similar
```

---

### Alerting Rules

**Availability Alerts**:
- Service down: Immediate page
- Error rate >0.5%: Page within 5 min
- Error rate >0.1%: Ticket within 15 min

**Performance Alerts**:
- Latency P95 >200ms: Ticket within 30 min
- Latency P95 >500ms: Page within 15 min
- Query timeout rate >1%: Page immediately

**Resource Alerts**:
- CPU >90%: Page immediately
- Memory >95%: Page immediately
- Disk >90%: Ticket within 1 hour

**Security Alerts**:
- Anomaly detected: Slack notification (no page)
- Rate limit triggered (HIGH): Slack notification
- Failed auth spike: Slack + ticket
- Potential breach: Immediate page + incident

---

## On-Call Operations

### On-Call Schedule

**Primary On-Call**:
- Weekly rotation (Monday 00:00 UTC to next Monday)
- 2-person team (primary + backup)
- Responsibilities: Respond to pages, acknowledge alerts, initial triage

**On-Call Escalation**:
- No response in 2 minutes: Escalate to backup
- No response in 5 minutes: Escalate to manager
- CRITICAL incident: Immediate full team notification

**On-Call Handoff**:
- 30-minute overlap at rotation boundary
- Knowledge transfer: Recent incidents, ongoing issues
- Tool access verification (Slack, PagerDuty, AWS console)

---

### On-Call Requirements

**Required Access**:
- AWS console (EC2, RDS, S3, KMS)
- Kubernetes cluster (if applicable)
- Database directly (PostgreSQL psql client)
- Elasticsearch (direct queries)
- Git repository (code rollback)

**Required Knowledge**:
- System architecture (from Phase 13)
- Common failure modes (documented in runbooks)
- Escalation procedures (alert severity levels)
- Customer communication templates (prepared in advance)

**Equipment**:
- Laptop with VPN access
- Phone for escalation calls
- Notifications: Slack + SMS (via PagerDuty)

---

## Operational Runbooks

### Runbook 1: Service Restart Procedure

**When to Use**: Service completely unresponsive, memory leak suspected

**Steps**:
```
1. Verify service is down
   $ curl https://api.fraiseql.com/health
   Expected: Connection refused or timeout

2. Check logs for recent errors
   $ aws logs tail /fraiseql/production --follow --since 5m

3. Restart service
   $ kubectl rollout restart deployment/fraiseql-api

4. Verify restart
   $ kubectl get deployment fraiseql-api
   Expected: Ready 1/1, Restarts: 1

5. Check health endpoint
   $ curl https://api.fraiseql.com/health
   Expected: 200 OK

6. Monitor metrics for 5 minutes
   $ grafana dashboard: Production Health
   Expected: Metrics return to normal
```

**Estimated Time**: 2-3 minutes
**Success Criteria**: Health checks pass, error rate returns to baseline

---

### Runbook 2: Database Recovery from Backup

**When to Use**: Data corruption detected, data loss from misconfiguration

**Steps**:
```
1. Assess damage
   $ SELECT COUNT(*) FROM users;
   Verify data is missing/corrupted

2. Stop service to prevent further writes
   $ kubectl scale deployment fraiseql-api --replicas=0

3. Find latest backup
   $ aws s3 ls s3://fraiseql-backups/
   Expected: Daily backups at 00:00, 06:00, 12:00, 18:00 UTC

4. Download backup
   $ aws s3 cp s3://fraiseql-backups/2026-03-07-00-00.sql.gz .
   $ gunzip 2026-03-07-00-00.sql.gz

5. Restore to new database (test first)
   $ psql -h fraiseql-test.c7nfx5zqp7h.us-east-1.rds.amazonaws.com \
           -U fraiseql -d fraiseql < 2026-03-07-00-00.sql

6. Verify restoration
   $ psql -h fraiseql-test.c7nfx5zqp7h.us-east-1.rds.amazonaws.com \
           -U fraiseql -d fraiseql -c "SELECT COUNT(*) FROM users;"
   Expected: Matches backup count (may be <2 hours old)

7. If verified, promote test to production
   $ aws rds promote-read-replica fraiseql-test-replica
   (Alternative: Manual switchover via RDS console)

8. Restart service
   $ kubectl scale deployment fraiseql-api --replicas=3

9. Verify service health
   $ curl https://api.fraiseql.com/health
   Expected: 200 OK

10. Communication
    $ Send customer notification:
      "We experienced data loss at 14:23 UTC. Recovered from backup
       (13:00 UTC). Affected queries from 13:00-14:23 may be lost.
       Root cause: [TBD]. Actions taken: [TBD]"
```

**Estimated Time**: 30-45 minutes
**Success Criteria**: Data restored, service health confirmed

---

### Runbook 3: API Key Revocation (Security Incident)

**When to Use**: Compromised API key, credential leak, suspicious activity

**Steps**:
```
1. Identify affected key
   $ Pentest alert: "High rate spike from api_key_xyz"

2. Verify malicious activity
   $ kubectl exec -it fraiseql-api-pod -- \
     curl http://localhost:9090/metrics | grep api_key_xyz_requests
   Look for suspicious patterns: new tables, PII fields, high volume

3. Create customer incident ticket
   $ jira create \
     -p SECURITY \
     -i Incident \
     -s "API Key Compromise: key_xyz" \
     -d "Key shows suspicious activity. Revoking immediately."

4. Notify customer
   $ Send email template: "API_KEY_COMPROMISE_NOTICE"
     Subject: "ACTION REQUIRED: Your API Key was Compromised"
     Body: "We detected suspicious activity. Immediately revoke the key
            in your console. New key generated and queued in dashboard."

5. Revoke key in system
   $ UPDATE api_keys
     SET revoked_at = NOW(), revoke_reason = 'Security incident'
     WHERE api_key_id = 'key_xyz';

6. Verify revocation
   $ curl -H "Authorization: Bearer fraiseql_us_east_1_key_xyz" \
          https://api.fraiseql.com/graphql
   Expected: 401 Unauthorized

7. Provide new key to customer
   $ Generate replacement key with same permissions
   $ Send via secure channel (email with 2FA confirmation)

8. Post-incident analysis
   $ How was key compromised?
   $ What access did attacker gain?
   $ Did they access PII? (Check audit logs)
   $ What preventive measures?
```

**Estimated Time**: 5-10 minutes (to revocation), 1-2 hours (investigation)
**Success Criteria**: Key revoked, customer notified, investigation complete

---

### Runbook 4: Rate Limit Bypass/Tuning

**When to Use**: Legitimate traffic blocked, false positive rate high

**Steps**:
```
1. Identify issue
   $ grafana: "Rate limiting blocks legitimate traffic"
   $ Customer reports: "Getting 429 Too Many Requests"

2. Check current limits
   $ redis-cli GET rate_limit:global:requests_per_minute
   $ redis-cli GET rate_limit:tier:premium:requests_per_minute

3. Assess legitimacy
   $ Verify: Is this a known bulk operation?
   $ Customer context: "Daily data export" vs "Suspicious spike"

4. Adjust if legitimate
   $ Update tier limit temporarily
   $ redis-cli SET rate_limit:tier:premium:requests_per_minute 2000
   $ redis-cli EXPIRE rate_limit:tier:premium:requests_per_minute 3600

5. Monitor impact
   $ grafana: "Error rate" + "Rate limit blocks"
   $ Expected: Error rate decreases, 429s disappear

6. Permanent fix (if pattern repeats)
   $ Adjust configuration: tier.premium.rps = 2000
   $ Deploy configuration change in next release

7. Document decision
   $ Ticket: "Rate limit adjustment rationale"
   $ Why increased?
   $ When revert to standard?
```

**Estimated Time**: 5-10 minutes
**Success Criteria**: Legitimate traffic flows, security not compromised

---

## Capacity Planning

### Current Capacity Baseline

**Database**:
- Current size: ~50GB
- Growth rate: ~10GB/month (query logs + audit logs)
- Projection in 1 year: ~170GB

**API Key Storage**:
- Current keys: ~500
- Growth rate: ~50 keys/month
- Projection in 1 year: ~1,100 keys

**Audit Log Storage**:
- Current volume: 86.4M events/day
- Growth rate: ~20% quarterly
- 90-day hot S3 capacity: ~250GB
- 7-year cold Glacier: ~2TB

---

### Scaling Triggers

**Database Scaling**:
- Trigger: Database size >400GB OR read latency P95 >100ms
- Action: Upgrade RDS instance class (Phase 15+)
- Lead time: 1-2 hours per upgrade

**Connection Pool Scaling**:
- Trigger: Active connections >90 OR query queue length >50
- Action: Increase max connections, add read replicas
- Lead time: 30 minutes

**Cache Scaling** (Phase 15+):
- Trigger: Cache hit rate <80% OR eviction rate >5%
- Action: Increase Redis memory, add cluster node
- Lead time: 15 minutes

---

## Success Criteria (Phase 14, Cycle 1 - RED)

- [x] SLA/SLO targets defined (99.5% uptime, <100ms P95 latency)
- [x] RTO/RPO targets specified (<1 hour RTO, <5 min RPO)
- [x] Backup strategy documented (6-hour schedule, 30-day retention)
- [x] 4 disaster recovery scenarios defined
- [x] Incident severity levels established (CRITICAL/HIGH/MEDIUM/LOW)
- [x] Incident response procedure documented
- [x] Monitoring metrics and alerting rules defined
- [x] On-call schedule and requirements specified
- [x] 4 operational runbooks created (restart, recovery, revocation, rate-limit)
- [x] Capacity planning baseline and scaling triggers documented

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (Implement Operations & Monitoring)
**Target Date**: March 3-7, 2026

