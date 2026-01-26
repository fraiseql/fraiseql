# Deployment Guide: Production Ready

**Conducted By**: DevOps Lead
**Date**: January 26, 2026

---

## Pre-Deployment Checklist

### Infrastructure
- [ ] Compute: 4vCPU, 8GB RAM, SSD storage
- [ ] Network: Private subnets, load balancer, firewalls
- [ ] Database: PostgreSQL 10+, replicated, backed up
- [ ] Caching: Redis 6.0+ (if multi-instance)
- [ ] Monitoring: Prometheus, ELK, alerting

### Security
- [ ] TLS certificates (valid, renewed before expiry)
- [ ] SSH keys rotated
- [ ] Secrets management (Vault/Secrets Manager)
- [ ] Network policies defined
- [ ] Firewall rules minimized

### Configuration
- [ ] Configuration management tool (Terraform, Ansible)
- [ ] Environment variables set
- [ ] Database connection string verified
- [ ] OIDC provider configured
- [ ] Rate limiting configured

---

## Deployment Strategies

### Strategy 1: Blue-Green Deployment

```
1. Deploy new version to "green" environment
2. Run smoke tests
3. Route traffic to green
4. Keep blue as rollback
5. After 24h, decommission blue
```

**Advantages**: Zero downtime, instant rollback
**Risks**: Requires 2x infrastructure

---

### Strategy 2: Canary Deployment

```
1. Deploy to 5% of servers
2. Monitor metrics (latency, errors)
3. If OK, roll to 25%
4. If OK, roll to 100%
5. Takes ~1 hour total
```

**Advantages**: Gradual rollout, catches issues early
**Risks**: Complex to manage

---

### Strategy 3: Rolling Update

```
1. Take 1 server offline
2. Deploy new version
3. Run health checks
4. If OK, bring online
5. Repeat for all servers
```

**Advantages**: Simple, low overhead
**Risks**: Brief periods of reduced capacity

---

## Database Deployment

### Zero-Downtime Migrations

```sql
-- Step 1: Add new column (safe, non-blocking)
ALTER TABLE v_user ADD COLUMN email_lower VARCHAR;

-- Step 2: Backfill in batches (outside transaction)
UPDATE v_user SET email_lower = LOWER(email)
  WHERE id BETWEEN $1 AND $2;

-- Step 3: Add index (CONCURRENTLY)
CREATE INDEX CONCURRENTLY idx_email_lower ON v_user(email_lower);

-- Step 4: Old code handles both columns
-- New code uses email_lower

-- Step 5: Remove old column (when ready)
ALTER TABLE v_user DROP COLUMN email;
```

---

## Rollback Procedures

### Immediate Rollback

```bash
# If critical issues detected (error rate > 5%)
1. Revert load balancer to previous version
2. Monitor error rate (should drop immediately)
3. Investigate issue
4. Deploy fix
5. Try again
```

### Database Rollback

```bash
# If schema changes cause issues
1. Restore from backup (to specific point in time)
2. Replay WAL logs up to failure point
3. Verify data integrity
4. Resume operations
```

---

## Health Checks

### Endpoint: /health

```json
{
  "status": "healthy",
  "version": "2.0.0",
  "database": {
    "status": "connected",
    "latency_ms": 2
  },
  "cache": {
    "status": "connected",
    "latency_ms": 1
  },
  "uptime_seconds": 3600
}
```

### Probe Specifications

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 4000
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /health/ready
    port: 4000
  initialDelaySeconds: 5
  periodSeconds: 5
  timeoutSeconds: 2
  failureThreshold: 1
```

---

## Monitoring Integration

### Prometheus Scrape Config

```yaml
- job_name: 'fraiseql'
  scrape_interval: 15s
  static_configs:
    - targets: ['localhost:4000']
  metric_path: '/metrics'
```

### Key Alerts

```yaml
- alert: ServiceDown
  expr: up{job="fraiseql"} == 0
  for: 1m

- alert: HighErrorRate
  expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.05
  for: 5m

- alert: HighLatency
  expr: histogram_quantile(0.99, http_request_duration_seconds) > 5
  for: 5m
```

---

## Configuration Management

### Using Terraform

```hcl
# main.tf
resource "aws_instance" "fraiseql" {
  ami           = "ami-0c55b159cbfafe1f0"
  instance_type = "t3.large"

  vpc_security_group_ids = [aws_security_group.fraiseql.id]

  tags = {
    Name = "fraiseql-prod"
  }
}

resource "aws_rds_instance" "database" {
  identifier   = "fraiseql-db"
  engine       = "postgres"
  engine_version = "14.0"
  instance_class = "db.t3.large"

  # ... other config
}
```

---

## Secrets Management

### Using HashiCorp Vault

```bash
# Store secrets
vault kv put secret/fraiseql \
  db_password="$(openssl rand -base64 32)" \
  api_key="$(openssl rand -hex 32)"

# Retrieve in application
let secret = vault.read("secret/fraiseql");
db_password = secret.data.db_password;
```

---

## Deployment Runbook

### Pre-Deployment
1. [ ] Code reviewed and approved
2. [ ] All tests passing
3. [ ] Security scan passed
4. [ ] Performance baseline established
5. [ ] Rollback plan documented

### Deployment
1. [ ] Create backup of database
2. [ ] Set maintenance mode
3. [ ] Deploy new version
4. [ ] Run smoke tests
5. [ ] Enable traffic gradually

### Post-Deployment
1. [ ] Monitor metrics for 1 hour
2. [ ] Verify all functionality
3. [ ] Collect user feedback
4. [ ] Document any issues
5. [ ] Archive deployment artifacts

---

## Incident Response During Deployment

### Issue: New version causes errors

**Timeline**:
- T+0: Error rate spike detected (alert fired)
- T+2: On-call engineer investigates
- T+5: Decision made to rollback
- T+10: Rollback complete, service restored
- T+20: Root cause analysis starts

**Procedure**:
```bash
# 1. Declare incident
declare-incident "New deployment error spike"

# 2. Page on-call team
page-oncall "fraiseql"

# 3. Trigger rollback
./scripts/rollback-to-previous.sh

# 4. Verify recovery
curl http://localhost:4000/health

# 5. Create postmortem
create-postmortem "Deployment issue analysis"
```

---

## Capacity Planning

### Metrics to Track

- Requests per second
- Average latency
- Error rate
- Database connections
- Memory usage
- Disk usage

### Scaling Triggers

```
Scale up if:
- CPU > 70% for 10 minutes
- Memory > 80% for 10 minutes
- Requests/sec > threshold
- Database connections > 80% of pool
```

---

## Documentation

### To Create

- [ ] Deployment runbook
- [ ] Rollback procedure
- [ ] Configuration guide
- [ ] Monitoring alert guide
- [ ] On-call handbook
- [ ] Incident response playbooks

---

**Guide Completed**: January 26, 2026
**Lead: DevOps Lead
**Status**: Ready for use
