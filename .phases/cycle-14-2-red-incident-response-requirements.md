# Phase 14, Cycle 2 - RED: Incident Response & On-Call Requirements

**Date**: March 10-14, 2026
**Phase Lead**: Operations Lead + Security Lead
**Status**: RED (Defining Incident Response & On-Call Requirements)

---

## Objective

Define comprehensive incident response and on-call requirements, including team structure, escalation procedures, incident response workflows, communication protocols, and post-incident analysis procedures.

---

## Background: Readiness from Phase 14, Cycle 1

From Cycle 1 (Operations & Monitoring):
- ✅ SLA/SLO framework operational
- ✅ Health checks and metrics collecting
- ✅ Alerting rules configured and tuned
- ✅ Backup/recovery procedures tested
- ✅ 4 operational runbooks documented

Cycle 2 focuses on **operational execution**:
1. **On-Call Operations**: Team structure, scheduling, escalation
2. **Incident Response**: Workflows, communication, coordination
3. **Team Training**: Knowledge transfer, runbook exercises
4. **Post-Incident**: Root cause analysis, preventive measures

---

## On-Call Team Structure

### Organization

**Primary Roles**:
- **On-Call Engineer** (Primary): Handles all alerts and incidents
- **On-Call Backup**: Takes over if primary doesn't respond
- **On-Call Manager**: Escalation point for complex issues
- **Incident Commander** (CRITICAL only): Coordinates response across teams

**Team Distribution** (for solo dev scenario):
- Primary: Solo dev (you) during certain hours
- Backup: Consultant/contractor on standby
- Manager: You (dual role)
- Commander: You (dual role for critical)

**Recommended (future scaling)**:
- 2-3 engineers per rotation
- On-call manager separate from engineers
- Incident commander on-call for CRITICAL only

---

### On-Call Schedule

**Weekly Rotation**:
```
Monday 00:00 UTC → Next Monday 00:00 UTC
Primary: Engineer A
Backup:  Engineer B

Overlap Period: Monday 00:00-02:00 (knowledge transfer)
Handoff Meeting: Monday 00:00 (15 min call)
```

**Responsibilities**:
- **Primary**: Responds to all alerts, acknowledges within SLA
- **Backup**: Takes over if primary doesn't respond within 2 minutes
- **Manager**: Escalation point, high-level decisions

**For Solo Developer**:
- Primary: You (during primary hours, e.g., 06:00-22:00 UTC)
- Backup: Standby contractor (nights/weekends)
- Escalation: Your manager or tech lead

---

### On-Call Tools & Access

**Required Tools**:
1. **Slack** - Alert notifications, incident chat channel
2. **PagerDuty** - Escalation management, on-call schedule
3. **AWS Console** - Service management, resource access
4. **Database CLI** - Direct database access (psql)
5. **Git** - Code deployment, rollback
6. **Elasticsearch** - Log search and forensics
7. **Grafana** - Metric visualization and dashboards

**Required Access**:
- [ ] AWS IAM credentials (with EC2, RDS, S3, KMS, SNS permissions)
- [ ] Database credentials (read/write access, emergency access)
- [ ] Kubernetes cluster (if applicable)
- [ ] VPN access from home
- [ ] SSH keys for production servers

**Credentials Management**:
- All credentials stored in secure vault (1Password, LastPass, AWS Secrets Manager)
- Credentials rotated quarterly
- Emergency access procedures documented
- Audit logging of all access

---

## Incident Response Framework

### Incident Severity Classification

**CRITICAL** (Immediate Response Required)
```
Impact: Complete service outage or data loss
Examples:
  - Service completely unresponsive (0 successful requests)
  - Data loss or corruption detected
  - Security breach confirmed (attacker has access)
  - All queries failing (100% error rate)

Response Target: <2 minutes to acknowledge
Escalation: Immediate page to on-call
Communication: Slack #incidents, customer notification within 15 min
```

**HIGH** (Urgent Response Required)
```
Impact: Significant service degradation, customer impact
Examples:
  - 25-99% of traffic affected
  - Latency spike (P95 >500ms for >5 min)
  - Error rate 1-5% (elevated but not critical)
  - Security vulnerability disclosed (not yet exploited)
  - Rate limiting triggered (legitimate traffic blocked)

Response Target: <15 minutes to acknowledge
Escalation: Page within 15 minutes if not acknowledged
Communication: Slack #incidents, customer notification if >30 min
```

**MEDIUM** (Standard Response)
```
Impact: Minor service issues, customer may notice
Examples:
  - <25% of traffic affected
  - Latency elevation (P95 100-500ms)
  - Error rate <1% (elevated from baseline)
  - Non-critical security finding
  - Resource utilization warning (>80%)

Response Target: <1 hour to acknowledge
Escalation: Ticket in queue, daily check-in
Communication: Slack #monitoring, customer notification if >1 hour
```

**LOW** (Routine Response)
```
Impact: No immediate customer impact
Examples:
  - Warning-level alerts
  - Trends and pattern observations
  - Non-urgent configuration issues
  - Documentation gaps

Response Target: <24 hours
Escalation: Ticket system only
Communication: Weekly summary email
```

---

## Incident Response Workflow

### Phase 1: Detection & Initial Response (0-5 minutes)

**Automated Detection**:
- Alert triggered by monitoring system (Prometheus → AlertManager → Slack)
- Alert includes: Severity, affected service, metric value, alert rules
- Slack notification includes runbook link

**On-Call Action** (within SLA):
- [ ] Receive alert notification (via Slack + PagerDuty)
- [ ] Acknowledge alert in PagerDuty (starts timer)
- [ ] Open incident channel in Slack (#incident-2026-03-10-001)
- [ ] Join call/video if CRITICAL

**Assessment** (within 2 minutes):
```
Q1: Is this a real incident or false positive?
    - Check dashboard: Does metric confirm alert?
    - Check logs: Any error messages?
    → Decision: Real / False positive

Q2: What is the scope of impact?
    - Check customer-facing API: Responding?
    - Check error rate: How many queries failing?
    - Check customer reports: Any in Slack?
    → Decision: Severity level

Q3: Do I need to escalate?
    - Is this beyond my knowledge/authority?
    - Should I call incident commander?
    → Decision: Escalate / Handle myself

Q4: Should I declare an incident?
    - Is this affecting customers?
    - Should customer be notified?
    → Decision: Declare incident / Monitor
```

---

### Phase 2: Triage & Initial Mitigation (5-15 minutes)

**Triage** (determine root cause):
- [ ] Check application logs (Elasticsearch)
- [ ] Check system metrics (Grafana dashboard)
- [ ] Check recent deployments (git log)
- [ ] Check infrastructure status (AWS console)
- [ ] Check external dependencies (third-party status pages)

**Initial Diagnosis**:
```
Likely Root Causes (in priority order):
1. [Most likely cause based on symptoms]
2. [Second most likely]
3. [Third most likely]

Evidence for #1:
  - [Metric showing issue]
  - [Log entry showing error]
  - [Recent change related to symptom]

Next Steps:
  - [What to do immediately]
  - [What to monitor]
  - [When to escalate]
```

**Mitigation** (if possible):
- [ ] Restart service (if hanging/crashed)
- [ ] Increase rate limits (if legitimate traffic blocked)
- [ ] Activate backup database (if primary failed)
- [ ] Deploy hotfix (if code bug confirmed)
- [ ] Revert recent deployment (if introduced regression)

**Escalation Decision** (if needed):
- Unclear root cause → Call incident commander
- Beyond on-call authority → Call incident commander
- Security incident → Call security lead immediately
- Customer communication → Call manager

---

### Phase 3: Investigation & Resolution (15 min - ongoing)

**Deep Dive Investigation**:
- [ ] Collect logs covering entire incident window
- [ ] Analyze metrics for patterns
- [ ] Review code changes since last good state
- [ ] Check audit logs for unusual activity (security incidents)
- [ ] Correlate data points (logs + metrics + changes)

**Root Cause Hypothesis**:
```
Hypothesis: [What likely caused this]
Evidence:
  - [Supporting metric]
  - [Supporting log entry]
  - [Supporting change/event]

Alternative Hypotheses:
  - [If #1 doesn't fit all evidence]
  - [Other possibilities]

Confidence Level: [High/Medium/Low]
```

**Resolution Plan**:
```
Option A: [Quick fix, less complete]
  - Pros: Fast, can be done in 5 minutes
  - Cons: May not address root cause
  - Estimated time: 5 minutes

Option B: [Full fix, more thorough]
  - Pros: Fixes root cause, prevents recurrence
  - Cons: Takes longer, requires code review
  - Estimated time: 30 minutes

Recommendation: [Which option given current situation]
```

**Implementation**:
- [ ] Implement chosen option
- [ ] Test in staging (if time permits)
- [ ] Deploy with caution (canary/rolling update)
- [ ] Monitor metrics during/after deployment
- [ ] Verify issue resolved

---

### Phase 4: Recovery & Validation (varies)

**Verification**:
- [ ] Alert threshold returned to normal
- [ ] Error rate returned to baseline
- [ ] Customer reports resolved
- [ ] Metrics showing healthy state
- [ ] No secondary issues detected

**Communication**:
- [ ] Update incident channel: "Issue resolved, verifying..."
- [ ] Wait 5-10 minutes for metrics confirmation
- [ ] Announce resolution in Slack: "Issue resolved at HH:MM UTC"
- [ ] Send customer notification (if SLA required)
- [ ] Close incident in PagerDuty (incident state: "resolved")

**Standown**:
- [ ] Continue monitoring for 30 minutes
- [ ] Watch for any resurrection of issue
- [ ] Ensure no cascading failures
- [ ] Document preliminary timeline

---

### Phase 5: Post-Incident Analysis (within 24 hours)

**RCA Meeting** (within 24 hours):
- [ ] Schedule meeting with incident commander, on-call, relevant engineers
- [ ] Timeline: When did alerts fire, when was impact first noticed, when resolved?
- [ ] Root cause: What actually caused this?
- [ ] Contributing factors: What made it worse?
- [ ] Detection gaps: Why did we not catch this earlier?
- [ ] Response evaluation: Did response follow procedures?

**Post-Incident Report**:
```
Incident ID: INC-2026-03-10-001
Title: Database connection pool exhaustion
Severity: HIGH
Start Time: 2026-03-10 14:23 UTC
End Time: 2026-03-10 14:47 UTC
Duration: 24 minutes
Impact: 450 requests failed, ~0.8% of traffic

Timeline:
14:20 - Sudden spike in query volume detected (2000+ qpm vs 500 baseline)
14:23 - Alert: DB connection pool >80% triggered
14:25 - On-call acknowledged alert
14:28 - Investigation: Identified slow queries holding connections
14:32 - Solution: Killed long-running query, connections released
14:35 - Verified: Metrics returned to normal
14:47 - RCA completed: Root cause was unoptimized query from new user

Root Cause:
  New customer ran unoptimized bulk query, held 40 connections for 12 min

Contributing Factors:
  - No query timeout in application
  - No per-query connection limit
  - Alert threshold was appropriate but late

Prevention:
  - Add 30-second query timeout (default)
  - Add query complexity limit (existing, but bypass happened)
  - Improve onboarding to warn about query optimization

Follow-up Actions:
  - [ ] Implement query timeout (Owner: Engineering, Due: 3/17)
  - [ ] Review customer's query and optimize (Owner: Support, Due: 3/12)
  - [ ] Update onboarding docs (Owner: Docs, Due: 3/14)
```

**Action Items**:
- [ ] Assign owners and due dates
- [ ] Link to github issues or project tracker
- [ ] Schedule follow-up verification meeting (1 week)

---

## Incident Communication

### Internal Communication (Team)

**Slack Channel**: `#incident-YYYYMMDD-NNN`
```
Format: #incident-20260310-001

Used for:
- Incident updates (every 5-10 min if ongoing)
- Technical discussion and coordination
- Links to dashboards, logs, runbooks
- Status: Investigating, Mitigation in progress, Resolved

Example Update:
"14:25 UTC: Investigating database connection pool exhaustion.
DB has 92 active connections (max 100). Likely cause: long-running query.
Currently analyzing logs to identify query.
Will update in 5 minutes."
```

**Escalation Call**:
- If CRITICAL: Incident commander calls a bridge call
- Bridge call: Slack huddle or Zoom
- Participants: On-call, incident commander, relevant subject matter experts
- Format: Status updates every 5 minutes (if ongoing)

---

### External Communication (Customers)

**Status Page**:
- Update public status page (statuspage.io or similar)
- Incident detected → "Investigating"
- Mitigation in progress → "Identified root cause, deploying fix"
- Resolved → "Issue resolved, service fully operational"

**Email Notification** (if SLA breached):
```
Subject: [INCIDENT RESOLVED] Service Disruption on March 10, 2026

Dear Valued Customer,

We experienced a service disruption from 14:23-14:47 UTC on March 10, 2026,
during which approximately 450 queries failed (0.8% of traffic).

Root Cause:
A customer query execution triggered database connection pool exhaustion.

Resolution:
We terminated the long-running query and implemented additional safeguards.

Impact:
Your account was [affected/not affected] by this incident.
[If affected] We have issued a service credit of [X]% to your account.

Next Steps:
We will implement query timeouts and improved query complexity limits
to prevent similar incidents. Implementation is scheduled for March 17, 2026.

We apologize for any disruption this caused.

Best regards,
FraiseQL Operations Team
```

---

### Status Codes

**Incident Status**:
- **Investigating**: Alert fired, initial diagnosis underway
- **Identified**: Root cause identified
- **Mitigating**: Fix/workaround deployed
- **Monitoring**: Issue resolved, validating stability
- **Resolved**: Confirmed stable, incident closed
- **Post-Incident**: RCA completed

---

## On-Call Procedures

### Handoff Process (Weekly)

**Handoff Meeting** (15 minutes, weekly, Monday 00:00 UTC):

**Outgoing On-Call** (Engineer A):
- [ ] Review past week's incidents (if any)
- [ ] Highlight recent changes/deployments
- [ ] Discuss any ongoing issues
- [ ] Hand over temporary notes (sticky issues to watch)
- [ ] Verify Backup has full access to tools

**Incoming On-Call** (Engineer B):
- [ ] Receive context from outgoing
- [ ] Verify all tool access working
- [ ] Confirm PagerDuty schedule shows them as primary
- [ ] Do a quick health check: `curl https://api.fraiseql.com/health`
- [ ] Ask questions about recent incidents/changes

**Handoff Checklist**:
- [ ] Access verified: AWS, Database, Kubernetes, Slack, PagerDuty
- [ ] Recent incident context shared
- [ ] Outstanding issues reviewed
- [ ] Runbooks location confirmed
- [ ] Emergency contact numbers verified
- [ ] Fallback procedures reviewed

---

### During Shift

**Beginning of Shift**:
- [ ] Review health dashboard (Grafana Production Health)
- [ ] Scan logs for any warnings (Elasticsearch)
- [ ] Check PagerDuty for any pending alerts
- [ ] Verify communication channels working (Slack test message)

**Throughout Shift**:
- [ ] Monitor Slack #monitoring for alerts
- [ ] Respond to alerts within SLA
- [ ] Keep incident channel updated
- [ ] Document decisions and actions
- [ ] Sleep/rest as possible (especially if multi-day incident)

**End of Shift**:
- [ ] Prepare handoff notes
- [ ] List any ongoing issues
- [ ] Ensure backup is ready
- [ ] Complete handoff meeting

---

### Break/Vacation Relief

**If On-Call Engineer Unavailable**:
- [ ] Notify manager immediately
- [ ] Promote backup to primary
- [ ] Bring in another team member as backup
- [ ] All procedures same as normal rotation

**Guidelines**:
- On-call engineer is expected to respond within SLA even on vacation
- If unavailable: Manager finds replacement immediately
- Coverage is mandatory (no exceptions)

---

## Training & Knowledge Transfer

### Initial On-Call Training

**Duration**: 1 week (5 half-days, 2.5 hours each)

**Day 1: System Overview** (2.5 hours)
- [ ] Architecture review (Phases 1-13 deliverables)
- [ ] Service dependencies (Database, Elasticsearch, Redis, KMS)
- [ ] SLA/SLO targets and calculations
- [ ] Dashboards: How to read each panel
- [ ] Where to find runbooks and documentation

**Day 2: Alert Familiarization** (2.5 hours)
- [ ] Alert types: What does each alert mean?
- [ ] Alert thresholds: Why these values?
- [ ] False positives: Known issues, how to dismiss
- [ ] PagerDuty workflow: Acknowledge, escalate, resolve
- [ ] Hands-on: Trigger each alert type in dev environment

**Day 3: Runbook Exercises** (2.5 hours)
- [ ] Runbook 1: Service restart (practice in dev)
- [ ] Runbook 2: Database recovery (practice in dev)
- [ ] Runbook 3: API key revocation (practice, no real keys)
- [ ] Runbook 4: Rate limit tuning (practice with staging limits)
- [ ] Q&A: Clarify any confusion

**Day 4: Incident Response** (2.5 hours)
- [ ] Incident response procedures (walk through workflow)
- [ ] Communication templates (Slack, email, status page)
- [ ] Post-incident analysis process
- [ ] Escalation procedures and who to call
- [ ] Mock incident drill: Simulated alert, full response

**Day 5: Integration & Shadowing** (2.5 hours)
- [ ] Full shift shadowing with current on-call
- [ ] Observe real alerts and responses
- [ ] Practice on actual dashboards/logs
- [ ] Review of week's learning
- [ ] Sign-off: Ready to be primary on-call

**Sign-Off Criteria**:
- [ ] Can navigate Grafana, Elasticsearch, AWS console
- [ ] Understands incident severity classification
- [ ] Can execute all 4 runbooks
- [ ] Knows how to escalate appropriately
- [ ] Can write incident communication
- [ ] Passed mock incident drill

---

### Ongoing Training

**Monthly Incident Review**:
- [ ] Review incidents from past month
- [ ] Discuss what went well
- [ ] Discuss what could be improved
- [ ] Update procedures based on learnings

**Quarterly Training Refresher**:
- [ ] Runbook exercises (practice all procedures)
- [ ] Mock incident drill (2+ scenarios)
- [ ] System changes review (new features, architecture changes)

---

## Success Criteria (Phase 14, Cycle 2 - RED)

- [x] On-call team structure defined
- [x] On-call schedule template created
- [x] Required tools and access specified
- [x] Incident severity levels defined (CRITICAL/HIGH/MEDIUM/LOW)
- [x] Incident response workflow documented (5 phases)
- [x] Triage and diagnosis procedures specified
- [x] Mitigation strategies documented
- [x] Communication protocols defined (internal + external)
- [x] Post-incident analysis process documented
- [x] On-call handoff procedure documented
- [x] On-call training plan created (5 days)
- [x] Ongoing training schedule established
- [x] Sign-off criteria for new on-call engineers

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (Implement Incident Response System)
**Target Date**: March 10-14, 2026

