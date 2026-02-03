# FraiseQL v2 Operations Guide - Quick Start

**For**: DevOps teams and SREs deploying FraiseQL
**Time**: 15 minutes to read, 1-2 days to customize and implement
**Outcome**: Production-ready monitoring, alerting, and incident response

---

## What You're Getting

The FraiseQL v2 Operations Guide provides:

1. **SLA/SLO Framework** - Define your availability and performance targets
2. **Monitoring Setup** - Health checks, metrics, Grafana dashboards
3. **Alerting Rules** - Prometheus rules for production monitoring
4. **Incident Response** - Procedures, severity levels, communication templates
5. **Operational Runbooks** - 4 detailed procedures for common scenarios
6. **On-Call Training** - 5-day training plan and mock incident drills
7. **Backup & Recovery** - RTO/RPO targets and restore procedures

This is based on production-tested procedures from the FraiseQL team's own deployment.

---

## Getting Started (15 minutes)

### Step 1: Read the Guide (10 min)
Start with `docs/OPERATIONS_GUIDE.md`:

- **Quick Start**: Section at top
- **Parts 1-2**: SLA/SLO and monitoring (most important)
- **Parts 3-6**: Incident response, runbooks, on-call, backup

### Step 2: Assess Your Team (5 min)

- How many engineers will run FraiseQL?
- Who will be on-call?
- What's your organization's incident response style?
- What monitoring tools do you already use?

---

## Implementation Timeline

### Week 1: Core Monitoring (3-4 hours)

- [ ] Deploy FraiseQL v2
- [ ] Set up Prometheus to scrape `/metrics`
- [ ] Create Grafana dashboards (use templates from guide)
- [ ] Configure basic alerting (just ServiceDown + HighErrorRate)
- [ ] Test health check endpoint

### Week 2: Full Alerting (2-3 hours)

- [ ] Collect 1-2 weeks of metrics
- [ ] Establish baseline (P50, P95, P99)
- [ ] Configure all alert rules with tuned thresholds
- [ ] Set up Slack notifications
- [ ] Test alert flow

### Week 3: Incident Response Setup (4-5 hours)

- [ ] Set up PagerDuty (or on-call tool)
- [ ] Create incident response procedures (customize templates)
- [ ] Build runbooks for your environment
- [ ] Set up communication templates
- [ ] Create on-call schedule

### Week 4: Team Training (5 days, 2.5 hours each)

- [ ] Day 1: System overview
- [ ] Day 2: Alert familiarization
- [ ] Day 3: Runbook exercises
- [ ] Day 4: Incident response + mock drill
- [ ] Day 5: Shadowing and sign-off

---

## Customization Checklist

### Monitoring (Week 1)

**Prometheus Setup**:

- [ ] Update Prometheus config to scrape your FraiseQL instances
- [ ] Add `job_name: 'fraiseql'` with your targets
- [ ] Set scrape interval to 30s, timeout to 5s
- [ ] Verify metrics are being collected

**Grafana Dashboards**:

- [ ] Create two dashboards:
  - Dashboard 1: Production Health (see guide for panel list)
  - Dashboard 2: Database Health
- [ ] Use provided metrics names from guide
- [ ] Customize thresholds based on your values

**Health Check**:

- [ ] Configure your load balancer to check `/health`
- [ ] Set timeout to 5 seconds
- [ ] Test: `curl https://your-api.com/health`
- [ ] Verify response includes all dependency checks

---

### Alerting (Week 2)

**Baseline Collection** (first 1-2 weeks):
```bash
# After 1 week of data, extract baseline metrics:
curl -s https://prometheus.example.com/api/v1/query \
  'rate(fraiseql_queries_total[5m])'

# Extract P95, P99 latencies to understand your baseline
```

**Alert Threshold Tuning**:

1. Collect 1-2 weeks of baseline metrics
2. Calculate P95 latency (e.g., 45ms)
3. Set alert thresholds at 2-3× baseline (e.g., 150-200ms)
4. Test in staging first
5. Deploy to production
6. Monitor for false positives, adjust if needed

**PagerDuty Integration**:

- [ ] Create PagerDuty account
- [ ] Configure Prometheus to send alerts to PagerDuty
- [ ] Set up escalation policy (primary → backup → manager)
- [ ] Add SMS notifications for critical alerts
- [ ] Test alert flow

---

### Incident Response (Week 3)

**Customize Runbooks**:

- [ ] Update IP addresses, hostnames, credentials in all runbooks
- [ ] Test each runbook in staging environment
- [ ] Time how long each takes in your environment
- [ ] Update estimated times in guide

**Communication Templates**:

- [ ] Customize team names and contact info
- [ ] Add your company's logo/branding
- [ ] Update incident channel naming scheme
- [ ] Review with legal/compliance (especially SLA language)

**On-Call Schedule**:

- [ ] Create weekly rotation in PagerDuty
- [ ] Assign team members
- [ ] Set up escalation rules
- [ ] Test that pages go to correct person

---

### Team Training (Week 4)

**Before Training**:

- [ ] Verify all team members have tool access
  - [ ] Grafana login
  - [ ] Elasticsearch/log access
  - [ ] PagerDuty login
  - [ ] AWS/database access
- [ ] Prepare training environment (staging or read-only prod)
- [ ] Schedule 5 half-day sessions

**Training Materials**:

- [ ] Use the 3 training modules from OPERATIONS_GUIDE.md
- [ ] Customize system-specific examples
- [ ] Prepare staging environment for hands-on exercises
- [ ] Print quick-reference guides for on-call station

**After Training**:

- [ ] Conduct knowledge assessment (80% pass required)
- [ ] Run mock incident drill
- [ ] Get sign-off from incident commander
- [ ] Update on-call schedule with certified engineers

---

## Most Important Steps (if short on time)

If you can only do a few things:

**Absolute Minimum** (1 day):

1. Health check endpoint configured
2. Prometheus collecting metrics
3. Grafana dashboards showing key metrics
4. Basic alerting (ServiceDown + HighErrorRate)
5. PagerDuty on-call schedule active

**Recommended** (1 week):

1. All of above
2. Full alert rules with tuned thresholds
3. Incident response procedures documented
4. Team trained on basics
5. Backup procedures automated

**Best Practice** (2-3 weeks):

1. Everything above
2. Full team training and certification
3. Mock incident drills completed
4. Post-incident procedures documented
5. Continuous improvement plan established

---

## Common Customizations

### For Smaller Teams (<5 engineers)

- Single on-call person, no backup
- Simplified incident response (3 phases instead of 5)
- Fewer alert rules (focus on CRITICAL/HIGH only)
- Quarterly training instead of monthly reviews

### For Large Teams (10+ engineers)

- Multiple on-call rotations (primary + backup + manager)
- Detailed incident response with incident commanders
- More granular alerting by service
- Weekly incident reviews and continuous training

### For High-Availability (99.99%+ SLA)

- Stricter RTO/RPO targets (<15 min RTO)
- Multi-region replication
- Advanced backup strategies (WAL archiving)
- More aggressive alerting thresholds

### For Regulated Industries (HIPAA, PCI-DSS)

- Enhanced audit logging
- Encryption everywhere (at-rest + in-transit)
- Formal incident response procedures
- Quarterly security reviews and compliance audits

---

## Tools & Integrations

### Recommended Stack

**Monitoring**:

- Prometheus (open-source, included)
- Grafana (open-source, widely used)
- Alternatives: DataDog, New Relic, Splunk

**Alerting**:

- AlertManager (open-source, with Prometheus)
- PagerDuty (commercial, recommended)
- Alternatives: Opsgenie, VictorOps

**Logging**:

- Elasticsearch + Kibana (open-source)
- Splunk (commercial)
- CloudWatch (if on AWS)

**On-Call Management**:

- PagerDuty (industry standard)
- Opsgenie
- VictorOps

**Backup Storage**:

- AWS S3 (recommended, affordable)
- GCS, Azure Blob Storage
- On-premises NAS (for regulatory reasons)

---

## Validation Checklist (Before Go-Live)

- [ ] Health check endpoint returning 200 OK
- [ ] Prometheus collecting all metrics
- [ ] Grafana dashboards displaying data
- [ ] Alert rules firing correctly (test in staging)
- [ ] PagerDuty receiving alerts
- [ ] Team trained and certified
- [ ] Runbooks tested in staging
- [ ] Backup procedure tested (restore to staging)
- [ ] On-call schedule active
- [ ] Communication templates tested
- [ ] Post-incident procedures documented
- [ ] Management approves SLA/SLO targets

---

## After Go-Live

### Day 1

- [ ] Monitor alerts carefully (expect some noise)
- [ ] Team on high alert during first shift
- [ ] Document any issues or surprises

### Week 1

- [ ] Review all alerts triggered during first week
- [ ] Adjust thresholds if too many false positives
- [ ] Capture baseline metrics for future reference
- [ ] Conduct team retrospective

### Month 1

- [ ] Monthly incident review meeting
- [ ] Identify patterns and trends
- [ ] Update procedures based on learnings
- [ ] Plan continuous improvements

### Ongoing

- [ ] Monthly incident reviews
- [ ] Quarterly training refresher
- [ ] Quarterly SLA compliance review
- [ ] Continuous improvement cycle

---

## Support & Questions

Questions about the Operations Guide?

1. **Check the FAQ** (end of OPERATIONS_GUIDE.md)
2. **Review examples** in the guide for your specific tool
3. **Customize the templates** for your organization
4. **Test in staging** before deploying to production

For FraiseQL-specific questions:

- GitHub Issues: https://github.com/fraiseql/fraiseql-v2/issues
- Community Forum: [Link to community]
- Email: hello@fraiseql.com

---

## Document Overview

**This File (5 min read)**:
Quick overview, timeline, customization checklist

**OPERATIONS_GUIDE.md (30 min read)**:
Detailed guide covering all aspects of production operations

**Your Customized Guide (ongoing)**:
Keep a copy of OPERATIONS_GUIDE.md with your customizations
- Your SLA/SLO values
- Your IP addresses and hostnames
- Your alert thresholds
- Your team names
- Your procedures and workflows

---

## Success Metrics

After implementing this guide, you should have:

✅ **Visibility**: Know what's happening in your FraiseQL deployment at all times
✅ **Responsiveness**: Respond to incidents within SLA
✅ **Reliability**: Maintain your defined SLA/SLO targets
✅ **Confidence**: Team trained and confident handling incidents
✅ **Improvement**: Continuous learning from incidents

---

**Ready to get started?**

→ Read `docs/OPERATIONS_GUIDE.md` now

→ Customize for your environment

→ Deploy and train your team

→ Go live with confidence

Good luck! 🚀
