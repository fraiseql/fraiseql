# Phase 04: Add Incident Response Procedures

**Priority:** MEDIUM
**Time Estimate:** 1.5 hours
**Impact:** +0.5 point to Compliance & Governance score (16/20 → 16.5/20)
**Status:** ⬜ Not Started

---

## Problem Statement

Pentagon-Readiness Assessment recommends "Add incident response playbooks" as immediate action. While monitoring is documented, formal incident response procedures with playbooks for common security and operational scenarios are missing.

---

## Objective

Create comprehensive incident response procedures document with:
1. Severity level definitions (P0/P1/P2/P3)
2. Incident response team roles
3. Detailed playbooks for 3 critical scenarios
4. Communication templates (internal, external, post-mortem)
5. Reference commands and tools

**Deliverable:** `COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md` (400-600 lines)

---

## Context Files

**Review these files before writing (orchestrator will copy to `context/`):**
- `docs/production/MONITORING.md` - Monitoring and alerting setup
- `docs/security/PROFILES.md` - Security profiles and controls
- `COMPLIANCE/AUDIT/AUDIT_LOGGING.md` - Audit trail capabilities
- `.phases/01-operations-runbook/output/OPERATIONS_RUNBOOK.md` - Operations procedures (if completed)
- Any existing security incident documentation

**Reference Standards:**
- NIST Incident Response Guide: https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-61r2.pdf
- SANS Incident Response: https://www.sans.org/white-papers/

---

## Deliverable

**File:** `.phases/04-incident-response/output/INCIDENT_RESPONSE.md`

**Target Location:** `COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md`

---

## Required Structure

### 1. Incident Severity Levels

**Define 4 severity levels with clear criteria:**

| Level | Name | Description | Response Time | Escalation | Examples |
|-------|------|-------------|---------------|------------|----------|
| P0 | Critical | Service down, active security breach, data loss | 15 minutes | Immediate | Complete outage, active intrusion, data exfiltration |
| P1 | High | Severe degradation, security vulnerability exploited | 30 minutes | Within 1 hour | Partial outage, failed authentication surge, RLS bypass |
| P2 | Medium | Performance degradation, security vulnerability detected | 2 hours | Within 4 hours | Slow queries, high latency, CVE in dependency |
| P3 | Low | Minor issues, no user impact | 1 business day | As needed | Cosmetic bugs, warning logs, low-priority vulnerabilities |

---

### 2. Incident Response Team

**Define roles and responsibilities:**

- **Incident Commander (IC):** Coordinates overall response
- **Technical Lead (TL):** Investigates root cause and implements fixes
- **Communications Lead (CL):** Handles stakeholder notifications
- **Security Lead (SL):** Handles security-specific incidents

**On-call rotation and escalation paths:**
- Primary: On-call engineer (PagerDuty/Opsgenie)
- Secondary: Senior engineer
- Escalation: Engineering manager → CTO

---

### 3. General Incident Response Flow

**Provide flowchart and step-by-step process:**

```
Detection → Triage → Investigation → Containment → Remediation → Communication → Post-Mortem
```

**For each phase:**
- What happens
- Who is responsible
- Time expectations
- Key actions

---

### 4. Playbook 1: Security Breach Detection

**Scenario:** Unauthorized access, data exfiltration, or security policy violation

**Triggers:**
- Unusual authentication patterns (many failed attempts)
- Unauthorized data access (RLS policy violations)
- Suspicious query patterns
- Security scan alerts from vulnerability scanners
- External security researcher disclosure

**Response Steps:**

#### Immediate Actions (0-15 minutes)
- [ ] Confirm breach via audit logs
- [ ] Identify affected systems/data
- [ ] Preserve evidence (logs, database snapshots, network captures)
- [ ] Notify security team and incident commander
- [ ] If active attack: Consider isolating affected systems

#### Investigation (15-60 minutes)
- [ ] Identify attack vector (how did they get in?)
- [ ] Assess scope of compromise (what data was accessed?)
- [ ] Check audit logs for all unauthorized access
- [ ] Query OpenTelemetry traces for suspicious activity
- [ ] Review security controls (RLS policies, authentication logs)
- [ ] Identify if credentials were compromised

#### Containment (1-4 hours)
- [ ] Revoke compromised credentials/API keys
- [ ] Rotate encryption keys (if data was accessed)
- [ ] Deploy security patches (if vulnerability exploited)
- [ ] Update firewall/RLS policies
- [ ] Block malicious IP addresses
- [ ] Reset affected user passwords

#### Communication
- **Internal:** Immediate notification to leadership
- **External:** Notify affected users if PII/CUI was exposed
- **Regulatory:** File breach report if required (GDPR, HIPAA, DoD)
- **Timeline:** Within 72 hours for GDPR, immediately for classified data

#### Post-Incident (24-48 hours)
- [ ] Complete post-mortem (use template below)
- [ ] Update security controls and policies
- [ ] Conduct security training for team
- [ ] Review and test incident response plan
- [ ] Update vulnerability management process

**Investigation Commands:**

```bash
# Check recent failed authentication attempts
grep "authentication_failed" /var/log/fraiseql/security.log | tail -100

# Query audit trail for specific user
psql -c "SELECT * FROM audit_log WHERE user_id = '<user_id>' ORDER BY timestamp DESC LIMIT 100"

# Check for RLS policy violations
psql -c "SELECT * FROM audit_log WHERE event_type = 'RLS_VIOLATION' ORDER BY timestamp DESC"

# Query OpenTelemetry for anomalous traces
# Use Grafana to search for:
# - Traces with unusual query patterns
# - High-volume requests from single user
# - Access to sensitive tables outside normal hours
```

---

### 5. Playbook 2: Service Degradation

**Scenario:** Performance issues, high latency, or elevated error rates

**Triggers:**
- P95 latency exceeds SLO threshold
- Error rate > 0.1%
- Database connection pool exhaustion
- Memory or CPU saturation
- User reports of slow performance

**Response Steps:**

#### Immediate Actions (0-5 minutes)
- [ ] Check Grafana dashboards for anomalies
- [ ] Verify incident severity (P0/P1/P2?)
- [ ] Notify on-call engineer
- [ ] Check if recent deployment occurred

#### Investigation (5-30 minutes)
- [ ] Review recent deployments (rollback candidate?)
- [ ] Check application logs for errors
- [ ] Review database slow query log
- [ ] Check connection pool utilization
- [ ] Review OpenTelemetry traces for slow operations
- [ ] Check resource utilization (CPU, memory, disk I/O)

#### Remediation (30-60 minutes)
- [ ] If recent deployment: Rollback immediately
- [ ] If resource exhaustion: Scale horizontally/vertically
- [ ] If slow queries: Optimize or kill long-running queries
- [ ] If traffic spike: Enable rate limiting or circuit breakers
- [ ] If connection pool exhausted: Scale pool or identify connection leak

#### Communication
- **Internal:** Update incident channel every 15 minutes
- **External:** Post status page update if user-facing degradation
- **SLA:** Provide ETA for resolution (or "investigating")

#### Post-Incident (Within 48 hours)
- [ ] Complete post-mortem
- [ ] Update monitoring alerts (tune thresholds)
- [ ] Add regression test
- [ ] Document resolution in runbook

**Investigation Commands:**

```bash
# Check current resource usage
docker stats fraiseql  # If containerized
top -b -n 1 | head -20

# Check database connection pool
psql -c "SELECT count(*) FROM pg_stat_activity WHERE application_name = 'fraiseql'"
psql -c "SELECT state, count(*) FROM pg_stat_activity GROUP BY state"

# Identify slow queries
psql -c "SELECT query, mean_exec_time, calls FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 10"

# Check for long-running queries
psql -c "SELECT pid, now() - query_start AS duration, state, query FROM pg_stat_activity WHERE state = 'active' ORDER BY duration DESC"

# Review recent errors in logs
grep "ERROR\|CRITICAL" /var/log/fraiseql/app.log | tail -100

# Check OpenTelemetry traces for slowest operations
# Use Grafana Tempo to query traces with duration > 1s
```

---

### 6. Playbook 3: Data Integrity Issues

**Scenario:** RLS policy failures, data corruption, or unauthorized data modifications

**Triggers:**
- RLS policy violation alerts
- Data corruption detected in checksums/validation
- Unexpected data modifications in audit logs
- User reports of incorrect data
- Anomalies in data consistency checks

**Response Steps:**

#### Immediate Actions (0-15 minutes)
- [ ] Identify affected table(s)
- [ ] Stop writes to affected tables (if safe to do so)
- [ ] Take database snapshot/backup
- [ ] Preserve audit logs
- [ ] Assess scope of data corruption

#### Investigation (15-60 minutes)
- [ ] Query audit logs for data changes
- [ ] Review recent database migrations
- [ ] Check RLS policy definitions and enforcement
- [ ] Identify affected records and users
- [ ] Determine if malicious or accidental
- [ ] Check application logic for bugs

#### Remediation (1-4 hours)
- [ ] If corruption: Restore from backup (last known good state)
- [ ] If RLS failure: Fix policies and test thoroughly
- [ ] If application bug: Deploy hotfix
- [ ] Run data integrity checks to verify fix
- [ ] Verify fix with test queries

#### Communication
- **Internal:** Notify data owners and stakeholders
- **External:** Notify affected users if data loss or unauthorized exposure
- **Compliance:** Report to regulatory bodies if PII/CUI affected

#### Post-Incident (24-48 hours)
- [ ] Update RLS policies with better tests
- [ ] Add data integrity checks to CI/CD
- [ ] Review and improve backup/restore procedures
- [ ] Document data recovery process

**Investigation Commands:**

```bash
# Query audit trail for data modifications
psql -c "SELECT * FROM audit_log WHERE table_name = '<table>' AND operation IN ('UPDATE', 'DELETE') ORDER BY timestamp DESC LIMIT 100"

# Check RLS policies on table
psql -c "SELECT * FROM pg_policies WHERE tablename = '<table>'"

# Verify RLS is enabled
psql -c "SELECT tablename, rowsecurity FROM pg_tables WHERE schemaname = 'public' AND tablename = '<table>'"

# Test RLS policy enforcement
psql -c "SET ROLE test_user; SELECT count(*) FROM <table>; RESET ROLE;"

# Check data integrity
psql -c "SELECT count(*) FROM <table> WHERE <integrity_constraint>"

# Find records modified in time window
psql -c "SELECT * FROM audit_log WHERE table_name = '<table>' AND timestamp BETWEEN '<start>' AND '<end>'"
```

---

### 7. Communication Templates

#### Internal Notification (Slack/Email/PagerDuty)

```
🚨 INCIDENT: [P0/P1/P2] <Title>

Status: Investigating / Mitigating / Resolved
Severity: P0 (Critical) / P1 (High) / P2 (Medium)
Started: YYYY-MM-DD HH:MM UTC
Duration: <elapsed time>
Incident Commander: @name
Technical Lead: @name

Impact:
- Affected systems: <list>
- User impact: <description>
- Data impact: <if applicable>

Current Actions:
- <action 1>
- <action 2>

Next Update: <timestamp or "in 15 minutes">

Incident Channel: #incident-<id>
```

#### External Notification (User-facing)

```
Subject: Service Disruption Notice - [Date]

Dear Users,

We are currently experiencing [brief description of issue] affecting [affected services].

What we know:
- Issue started at: [time in user's timezone]
- Affected users: [scope or "all users"]
- Current status: [investigating/mitigating/resolved]

What we're doing:
- [action 1]
- [action 2]

Expected resolution: [estimate or "under investigation"]

We will provide updates every [frequency]. You can check our status page at [URL].

Thank you for your patience.

Best regards,
FraiseQL Operations Team
```

#### Post-Mortem Template

```markdown
# Post-Mortem: <Incident Title>

**Date:** YYYY-MM-DD
**Severity:** P0/P1/P2/P3
**Duration:** X hours Y minutes
**Incident Commander:** <name>
**Technical Lead:** <name>

## Summary
<2-3 sentence summary of what happened>

## Timeline (UTC)
- HH:MM - Detection: <how incident was detected>
- HH:MM - Investigation: <key findings>
- HH:MM - Mitigation: <actions taken>
- HH:MM - Resolved: <how it was resolved>

## Root Cause
<Detailed explanation of the root cause. What exactly failed and why?>

## Impact
- **Users affected:** <number or percentage>
- **Services impacted:** <list affected services>
- **Data loss:** Yes/No (if yes, describe scope)
- **Revenue impact:** <if applicable>
- **SLA breach:** Yes/No

## What Went Well
- <Thing that helped during incident response>
- <Good decision or preparation that paid off>

## What Went Wrong
- <Thing that made incident worse or slower to resolve>
- <Gap in monitoring, tooling, or documentation>

## Action Items
- [ ] <Action item 1> - Owner: <name> - Due: <date> - Priority: High/Medium/Low
- [ ] <Action item 2> - Owner: <name> - Due: <date> - Priority: High/Medium/Low
- [ ] <Action item 3> - Owner: <name> - Due: <date> - Priority: High/Medium/Low

## Lessons Learned
<Key takeaways. What should we do differently? What should we automate?>

## Supporting Links
- Incident Slack channel: #incident-<id>
- Grafana dashboard: <link>
- Related GitHub issue: <link>
- OpenTelemetry traces: <link>
```

---

### 8. Appendix: Useful Commands & Tools

#### Log Analysis
```bash
# Tail application logs
tail -f /var/log/fraiseql/app.log

# Search logs for errors with context
grep -C 10 -i "error\|exception\|fatal" /var/log/fraiseql/app.log | tail -100

# Count errors by type
grep "ERROR" /var/log/fraiseql/app.log | awk '{print $5}' | sort | uniq -c | sort -rn

# Check audit logs for specific user
grep "user_id=<user_id>" /var/log/fraiseql/audit.log | tail -50
```

#### Database Operations
```bash
# Check database health
psql -c "SELECT version(); SELECT now(); SELECT pg_database_size('fraiseql');"

# Check active connections by state
psql -c "SELECT state, count(*) FROM pg_stat_activity GROUP BY state"

# Kill long-running query
psql -c "SELECT pg_terminate_backend(<pid>)"

# Check replication lag (if using replication)
psql -c "SELECT * FROM pg_stat_replication"

# Vacuum and analyze (if corruption suspected)
psql -c "VACUUM ANALYZE <table>"
```

#### OpenTelemetry & Observability
```bash
# Access Grafana dashboards
open http://grafana.example.com/d/fraiseql-overview

# Query traces via Tempo (use Grafana UI)
# Search criteria:
# - service.name = "fraiseql"
# - http.status_code >= 500
# - duration > 1s
# - trace_id = "<known_trace_id>"
```

#### Security Operations
```bash
# Check for failed authentication attempts
grep "authentication_failed" /var/log/fraiseql/security.log | wc -l

# Find unique IP addresses with failed auth
grep "authentication_failed" /var/log/fraiseql/security.log | awk '{print $3}' | sort | uniq -c | sort -rn

# Check RLS policy enforcement
psql -c "SELECT schemaname, tablename, policyname, cmd, qual FROM pg_policies WHERE tablename = '<table>'"

# Review audit trail for sensitive operations
psql -c "SELECT * FROM audit_log WHERE event_type IN ('DELETE', 'UPDATE', 'RLS_VIOLATION') ORDER BY timestamp DESC LIMIT 50"
```

---

### 9. References

- **Internal Documentation:**
  - Operations Runbook: `OPERATIONS_RUNBOOK.md`
  - Monitoring Setup: `docs/production/MONITORING.md`
  - Security Profiles: `docs/security/PROFILES.md`
  - Audit Logging: `COMPLIANCE/AUDIT/AUDIT_LOGGING.md`

- **External Standards:**
  - NIST SP 800-61: Computer Security Incident Handling Guide
  - SANS Incident Response Process
  - DoD Incident Response (for classified deployments)

---

## Requirements Summary

**Content Quality:**
- [ ] 400-600 lines total
- [ ] 3 detailed playbooks (security breach, degradation, data integrity)
- [ ] Severity levels clearly defined
- [ ] Communication templates included
- [ ] All commands are copy-paste ready with placeholders
- [ ] Written for stressed operators during incidents

**Completeness:**
- [ ] Each playbook has: triggers, immediate actions, investigation, remediation, communication, post-incident
- [ ] Commands reference actual FraiseQL systems (logs, database, OpenTelemetry)
- [ ] Templates are realistic and usable
- [ ] Post-mortem template is comprehensive

---

## Verification (Orchestrator)

```bash
# Check file exists and line count
wc -l .phases/04-incident-response/output/INCIDENT_RESPONSE.md
# Should be 400-600 lines

# Verify required sections
grep -E "^## (Incident Severity|Incident Response Team|Playbook)" .phases/04-incident-response/output/INCIDENT_RESPONSE.md

# Check for communication templates
grep -E "^### (Internal Notification|External Notification|Post-Mortem)" .phases/04-incident-response/output/INCIDENT_RESPONSE.md

# Verify commands are present
grep -c '```bash' .phases/04-incident-response/output/INCIDENT_RESPONSE.md
# Should have many bash code blocks

# Test Markdown rendering
uv run python -m markdown .phases/04-incident-response/output/INCIDENT_RESPONSE.md > /dev/null
```

---

## Final Placement (Orchestrator)

```bash
# Create directory if needed
mkdir -p COMPLIANCE/SECURITY

# Move to final location
cp .phases/04-incident-response/output/INCIDENT_RESPONSE.md COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md

# Commit
git add COMPLIANCE/SECURITY/INCIDENT_RESPONSE.md
git commit -m "docs(compliance): add incident response procedures and playbooks

Add comprehensive incident response documentation:
- Severity level definitions (P0/P1/P2/P3)
- Incident response team roles and escalation
- Three detailed playbooks:
  - Security breach detection and containment
  - Service degradation and performance issues
  - Data integrity issues and RLS failures
- Communication templates (internal, external, post-mortem)
- Investigation commands for each scenario
- Post-incident procedures and lessons learned

Impact: +0.5 point to Compliance & Governance score (16/20 → 16.5/20)

Refs: Pentagon-Readiness Assessment - Phase 04"
```

---

## Tips for Documentation Writer

1. **Be specific:** Use actual FraiseQL components (audit_log table, RLS policies, OpenTelemetry)
2. **Commands should work:** Reference real log paths, database names, table names
3. **Think emergency:** Write for someone who's stressed and needs quick answers
4. **Templates are tools:** Make them copy-paste ready with clear placeholders
5. **Post-mortem matters:** This is where teams learn - make template comprehensive
6. **Cross-reference:** Link to other docs (monitoring, security profiles, operations runbook)

---

## Success Criteria

- [ ] File created: `INCIDENT_RESPONSE.md`
- [ ] 400-600 lines of content
- [ ] 3 detailed playbooks included
- [ ] Severity levels defined
- [ ] Communication templates included
- [ ] Investigation commands for each scenario
- [ ] Post-mortem template is comprehensive
- [ ] Written in clear, direct language for emergency use
