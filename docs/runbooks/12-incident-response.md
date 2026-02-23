# Runbook: General Incident Response Template

## Symptoms

- Any unexpected behavior, alert, or user report of FraiseQL malfunction
- System performing outside normal parameters
- Alerts triggered in monitoring system
- Customer reports of degraded service or errors
- Unexpected high error rate, latency, or resource usage

## Impact

- Unknown initially - follow investigation to determine scope
- Could affect single client or entire service
- Performance degradation vs complete outage

## Investigation

### 1. Initial Triage (First 2 Minutes)

```bash
# Quick status check
echo "=== INCIDENT TRIAGE $(date) ==="

# 1. Is FraiseQL server running?
docker ps | grep fraiseql-server
SERVER_STATUS=$?

# 2. Is it responding to requests?
curl -s -m 5 http://localhost:8815/health 2>&1 | head -5

# 3. Check basic metrics
curl -s http://localhost:8815/metrics | head -20

# 4. Get recent logs
docker logs fraiseql-server --tail 20 2>&1

# Summary
if [ $SERVER_STATUS -eq 0 ]; then
    echo "✓ Server running, checking health..."
else
    echo "✗ Server not running - CRITICAL"
fi
```

### 2. Categorize Incident (First 5 Minutes)

```bash
# Use this decision tree to identify runbook

# Question 1: Is FraiseQL server responding?
if curl -s http://localhost:8815/health > /dev/null 2>&1; then
    echo "Server is responding"

    # Question 2: Is database working?
    if psql $DATABASE_URL -c "SELECT 1" > /dev/null 2>&1; then
        echo "→ Database OK"
        # Check specific symptoms
        # See: Runbook 03 (High Latency), Runbook 06 (Rate Limiting), etc.
    else
        echo "→ Database down: Use Runbook 02"
    fi
else
    echo "Server not responding"

    # Question 3: Is it running?
    if docker ps | grep -q fraiseql-server; then
        echo "→ Process running but not responding: See Runbook 01, 04"
    else
        echo "→ Process crashed: See Runbook 01, 04"
    fi
fi

# Question 4: If authentication failing?
# See Runbook 05

# Question 5: If rate limited?
# See Runbook 06

# Question 6: If memory/CPU high?
# See Runbook 04

# Question 7: If database pool issues?
# See Runbook 07

# Question 8: If secrets/Vault issues?
# See Runbook 08

# Question 9: If cache/Redis issues?
# See Runbook 09

# Question 10: If TLS/certificate issues?
# See Runbook 10

# Question 11: If schema issues?
# See Runbook 11

# Question 12: If deployment-related?
# See Runbook 01
```

### 3. Gather Information

```bash
#!/bin/bash

# Execute this information gathering script
cat > /tmp/incident-info.sh << 'EOF'
#!/bin/bash

echo "=== INCIDENT INFORMATION GATHERING ==="
echo "Time: $(date)"
echo "Duration: Unknown (adjust after investigation)"
echo ""

# System information
echo "=== SYSTEM STATUS ==="
echo "Uptime:"
uptime
echo ""
echo "Disk space:"
df -h /
echo ""
echo "Memory:"
free -h
echo ""
echo "Load average:"
cat /proc/loadavg
echo ""

# FraiseQL status
echo "=== FRAISEQL STATUS ==="
echo "Container status:"
docker ps -a | grep fraiseql
echo ""
echo "Recent logs (last 50 lines):"
docker logs fraiseql-server --tail 50 2>&1 | head -50
echo ""

# Network/connectivity
echo "=== CONNECTIVITY ==="
echo "FraiseQL HTTP:"
curl -v -m 5 http://localhost:8815/health 2>&1 | grep -E "< HTTP|Connected"
echo ""
echo "Database:"
psql $DATABASE_URL -c "SELECT now();" 2>&1 | head -3
echo ""
echo "Redis (if applicable):"
redis-cli -u $REDIS_URL ping 2>&1
echo ""
echo "Vault (if applicable):"
curl -s -m 5 "$VAULT_ADDR/v1/sys/health" 2>&1 | head -3
echo ""

# Metrics
echo "=== KEY METRICS ==="
echo "Requests per second:"
curl -s http://localhost:8815/metrics 2>&1 | grep "requests_total" | head -3
echo ""
echo "Error rate:"
curl -s http://localhost:8815/metrics 2>&1 | grep "request_errors_total" | head -3
echo ""
echo "Latency:"
curl -s http://localhost:8815/metrics 2>&1 | grep "request_duration_seconds_bucket" | head -5
echo ""
echo "Database pool:"
curl -s http://localhost:8815/metrics 2>&1 | grep "db_pool_connections" | head -3
echo ""

# Environment
echo "=== ENVIRONMENT ==="
env | grep -E "^(DB_|REDIS_|VAULT_|RUST_LOG|PORT)" | sort
EOF

chmod +x /tmp/incident-info.sh
/tmp/incident-info.sh | tee /tmp/incident-info-$(date +%s).txt
```

### 4. Determine Severity

```bash
# Use this matrix to determine severity

echo "=== SEVERITY ASSESSMENT ==="

# Check impact
echo "1. How many users/services affected?"
echo "   - All users: CRITICAL"
echo "   - Subset of users: HIGH"
echo "   - Single user: MEDIUM"
echo "   - Internal/monitoring: LOW"

# Check error rate
ERROR_RATE=$(curl -s http://localhost:8815/metrics 2>/dev/null | grep "request_errors_total" | awk '{print $NF}' | head -1 || echo "0")
echo ""
echo "2. Error rate: $ERROR_RATE"
echo "   - > 50%: CRITICAL"
echo "   - 10-50%: HIGH"
echo "   - 1-10%: MEDIUM"
echo "   - < 1%: LOW"

# Check latency
P99=$(curl -s http://localhost:8815/metrics 2>/dev/null | grep 'request_duration_seconds_bucket{.*="0.5"}' | awk '{print $NF}' | head -1 || echo "0")
echo ""
echo "3. P99 latency:"
echo "   - > 5s: CRITICAL"
echo "   - 1-5s: HIGH"
echo "   - 100ms-1s: MEDIUM"
echo "   - < 100ms: LOW"

# Determine severity level
echo ""
echo "SEVERITY: [CRITICAL|HIGH|MEDIUM|LOW]"
echo "(Assign based on above criteria)"
```

## Mitigation (Varies by Issue)

### For All Incidents

1. **Notify incident commander and stakeholders** (immediately)

   ```bash
   # Create incident ticket
   # Example using curl to incident system:
   curl -X POST https://incidents.example.com/api/incidents \
     -H "Authorization: Bearer $INCIDENT_API_TOKEN" \
     -d '{
       "title": "FraiseQL service degraded",
       "severity": "HIGH",
       "description": "..."
     }'

   # Notify on Slack
   # @incident-commander, @on-call, @fraiseql-team
   ```

2. **Establish war room / comms channel**

   ```bash
   # Create Slack channel: #incident-fraiseql-20260219
   # Post regular updates
   # Designate incident commander
   ```

3. **Identify specific issue using decision tree** (from Investigation section)

   ```bash
   # Reference appropriate runbook from list
   # Runbook 01: Deployment issues
   # Runbook 02: Database down
   # Runbook 03: High latency
   # Runbook 04: Memory pressure
   # Runbook 05: Auth failures
   # Runbook 06: Rate limiting
   # Runbook 07: Connection pool
   # Runbook 08: Vault issues
   # Runbook 09: Redis issues
   # Runbook 10: Certificate issues
   # Runbook 11: Schema issues
   ```

4. **Implement immediate stabilization** (from identified runbook)

   ```bash
   # Depends on specific issue
   # See appropriate runbook for mitigation steps
   ```

## Resolution

### Incident Resolution Workflow

```bash
#!/bin/bash

echo "=== INCIDENT RESOLUTION WORKFLOW ==="

# Step 1: Identify specific issue
echo "1. Issue identification:"
echo "   - Use Investigation section above"
echo "   - Match symptoms to specific runbook"
echo "   - Document initial findings"

# Step 2: Follow specific runbook
echo ""
echo "2. Execute resolution steps from identified runbook"

# Step 3: Verify fix
echo ""
echo "3. Verification:"

# Health check
if curl -s http://localhost:8815/health | jq -e '.status == "healthy"' > /dev/null; then
    echo "   ✓ Server is healthy"
else
    echo "   ✗ Server health check failed"
    exit 1
fi

# Error rate check
ERROR_RATE=$(curl -s http://localhost:8815/metrics 2>/dev/null | grep "request_errors_total" | awk '{print $NF}' | head -1 || echo "0")
if (( $(echo "$ERROR_RATE < 0.01" | bc -l) )); then
    echo "   ✓ Error rate acceptable: $ERROR_RATE"
else
    echo "   ⚠ Error rate high: $ERROR_RATE"
fi

# Latency check
echo "   Checking latency..."
curl -s http://localhost:8815/metrics 2>/dev/null | grep "request_duration_seconds_bucket" | head -5

# Step 4: Monitor recovery
echo ""
echo "4. Post-recovery monitoring:"
echo "   - Monitor error rate for 15 minutes"
echo "   - Watch latency percentiles"
echo "   - Check database and resource usage"
echo "   - Verify all integrations (Vault, Redis, etc.) still working"
```

### Post-Incident Review

```bash
#!/bin/bash

cat > /tmp/incident-postmortem.md << 'EOF'
# Incident Postmortem

## Summary
- Incident ID:
- Start time:
- End time:
- Duration:
- Severity: [CRITICAL|HIGH|MEDIUM|LOW]

## Impact
- Services affected:
- Customers affected:
- Error rate during incident:
- Revenue/business impact:

## Root Cause
- Primary cause:
- Contributing factors:
- Why detection was slow/fast:

## Timeline
- HH:MM - Event occurred
- HH:MM - Detection/alert
- HH:MM - Investigation started
- HH:MM - Mitigation applied
- HH:MM - Service recovered
- HH:MM - Resolution complete

## Detection & Response
- Time to detect: (HH:MM - HH:MM) = X minutes
- Time to page on-call: (HH:MM - HH:MM) = X minutes
- Time to mitigation: (HH:MM - HH:MM) = X minutes
- Time to resolution: (HH:MM - HH:MM) = X minutes

## What Went Well
1.
2.
3.

## What Could Be Better
1.
2.
3.

## Action Items
- [ ] Action 1 (Owner: Name)
- [ ] Action 2 (Owner: Name)
- [ ] Action 3 (Owner: Name)
- [ ] Follow-up monitoring (Owner: Name)

## Attendees
- Incident Commander:
- On-Call Engineer:
- Database Team:
- Others:
EOF

echo "Postmortem template created: /tmp/incident-postmortem.md"
echo "Complete and share with team within 24 hours"
```

## Prevention

### Observability Setup

```bash
# Comprehensive monitoring to prevent future incidents

# 1. Alerting
cat > /etc/prometheus/rules/fraiseql-comprehensive.yml << 'EOF'
groups:
  - name: fraiseql_all
    interval: 30s
    rules:
      # Availability
      - alert: ServiceDown
        expr: up{job="fraiseql"} == 0
        for: 1m
        action: page

      # Errors
      - alert: HighErrorRate
        expr: rate(request_errors_total[5m]) > 0.05
        for: 5m
        action: page

      # Performance
      - alert: HighLatency
        expr: histogram_quantile(0.99, request_duration_seconds) > 1
        for: 10m
        action: notify

      # Resources
      - alert: HighCPU
        expr: rate(process_cpu_seconds_total[5m]) > 0.8
        for: 10m
        action: notify

      - alert: HighMemory
        expr: (process_resident_memory_bytes / 1e9) > 2
        for: 5m
        action: notify

      # Database
      - alert: DatabaseDown
        expr: db_connection_error_total > 0
        for: 2m
        action: page

      - alert: PoolExhausted
        expr: db_pool_connections_active == db_pool_connections_max
        for: 2m
        action: page

      # Authentication
      - alert: AuthFailureRate
        expr: rate(auth_failures_total[5m]) > 0.1
        for: 5m
        action: notify

      # Rate limiting
      - alert: RateLimitExceeded
        expr: rate(rate_limit_exceeded_total[5m]) > 0.1
        for: 5m
        action: notify
EOF

# 2. Logging
# Ensure logs are centralized and searchable
# Example: ELK Stack, Splunk, Cloud Logging

# 3. Tracing
# Enable distributed tracing for request flow
# Example: Jaeger, Datadog, New Relic

# 4. Health checks
# Regular synthetic monitoring
cat > /usr/local/bin/fraiseql-health-check.sh << 'EOF'
#!/bin/bash
# Run every 5 minutes
curl -s -m 10 http://localhost:8815/health | jq -e '.status == "healthy"' || \
  send_alert "FraiseQL health check failed"
EOF
```

### Runbook Maintenance

```bash
# Keep runbooks updated

# Quarterly review:
# 1. Review incidents from last quarter
# 2. Update runbooks with lessons learned
# 3. Add any new known issues
# 4. Test runbook procedures

# Schedule
# - Every incident: Update relevant runbook
# - Quarterly: Full runbook review
# - Annually: Complete rewrite if significant changes
```

## Escalation Chain

```bash
# Default escalation for incidents

# Level 1 (0-15 min): On-call engineer
# - Triage incident
# - Execute runbooks
# - Attempt mitigation

# Level 2 (15-30 min): Team lead/senior engineer
# - If on-call engineer cannot resolve
# - Complex troubleshooting
# - System architect knowledge needed

# Level 3 (30+ min): Engineering manager + all teams
# - Critical incident not resolving
# - Multiple teams need coordination
# - Potential infrastructure issues

# Level 4 (60+ min): VP Engineering + Incident Commander
# - Critical customer impact
# - Business continuity threatened
# - Executive communication needed

# Communication
# - Slack: #incident-channel
# - Phone: Use incident escalation numbers
# - Email: Incident digest to stakeholders
```

## Template for Quick Reference

```bash
# When responding to incident, use this template

cat > /tmp/incident-response-checklist.txt << 'EOF'
[  ] 1. Acknowledge incident (note time received)
[  ] 2. Assess severity and impact
[  ] 3. Notify incident commander
[  ] 4. Create incident ticket/channel
[  ] 5. Gather information (use info-gathering script)
[  ] 6. Identify specific issue
[  ] 7. Reference appropriate runbook
[  ] 8. Execute investigation steps
[  ] 9. Execute mitigation steps
[  ] 10. Execute resolution steps
[  ] 11. Verify service recovery
[  ] 12. Monitor for 15+ minutes post-incident
[  ] 13. Communicate all-clear to stakeholders
[  ] 14. Schedule postmortem within 24 hours
[  ] 15. Document findings for future prevention
EOF

cat /tmp/incident-response-checklist.txt
```

## Key Contact Information

```bash
# Update this with your organization's contacts

# On-Call Schedule: https://oncall.example.com
# Incident Channel: #incidents (Slack)
# War Room: Zoom link in channel
# Page-On-Call: Use PagerDuty / Opsgenie
# Escalation: See escalation chain above

# Key Teams
# - FraiseQL Team: @fraiseql-team
# - Database Team: @database-team
# - Infrastructure: @infrastructure-team
# - Security: @security-team

# External Contacts
# - Cloud Provider Support:
# - Database Support:
# - Authentication Provider:
```

## Common Incident Patterns

```bash
# These patterns appear frequently - watch for them:

# Pattern 1: Database down (most common)
# - Check PostgreSQL service
# - Check network connectivity
# - Check credentials in Vault

# Pattern 2: High latency (second most common)
# - Slow queries
# - Connection pool exhaustion
# - High system load

# Pattern 3: Authentication failures
# - Token expiration
# - Vault unavailable
# - OIDC provider down

# Pattern 4: Memory pressure
# - Cache explosion
# - Connection pool leak
# - Large query results

# Pattern 5: Rate limiting triggered
# - Legitimate traffic spike
# - DDoS attack
# - Misconfigured client

# Pattern 6: Deployment issue
# - Wrong configuration
# - Schema mismatch
# - Missing dependency
```

## Post-Incident Checklist

```bash
# After incident is resolved

# Within 1 hour
[  ] Confirm service stable
[  ] Notify all stakeholders
[  ] Document incident briefly
[  ] Plan postmortem meeting

# Within 24 hours
[  ] Complete postmortem
[  ] Identify root cause
[  ] Create action items
[  ] Share postmortem with team

# Within 1 week
[  ] Complete action items from postmortem
[  ] Update relevant runbooks
[  ] Update monitoring/alerting if needed
[  ] Send postmortem to stakeholders

# Within 1 month
[  ] Verify permanent fix is working
[  ] Check if similar incidents recurring
[  ] Update documentation
[  ] Train team on lessons learned
```

## Escalation

- **Incident triage**: On-call engineer or on-call manager
- **Service unavailable**: Incident commander + all teams
- **Data loss/security**: Security team + incident commander
- **Sustained outage (>1 hour)**: VP Engineering + CTO
- **Customer communication**: Support + Product + Management
