# Operations Runbook: Production Deployment & Management

**Conducted By**: Site Reliability Engineer (SRE)
**Date**: January 26, 2026
**Purpose**: Operational procedures and best practices

---

## 1. Pre-Deployment Checklist

### 1.1 Infrastructure Requirements

**Compute**:
```
- Minimum: 2 vCPU, 4GB RAM per instance
- Recommended: 4 vCPU, 8GB RAM per instance
- For high-volume: 8+ vCPU, 16GB+ RAM
```

**Database**:
```
- PostgreSQL 10+ (SCRAM-SHA-256)
- PostgreSQL 11+ (SCRAM-SHA-256-PLUS recommended)
- Replication: Streaming replication for HA
- Backups: Continuous WAL archiving
- Storage: SSD for performance (3+ IOPS per GB)
```

**Network**:
```
- Load balancer: HTTP/2, TLS 1.2+
- Network isolation: Private subnets for services
- Firewalls: Minimal ingress/egress rules
- DNS: Health checks at 30-second intervals
```

**Caching (Optional but Recommended)**:
```
- Redis 6.0+: For CSRF state store, session cache
- In-memory: For single-instance deployments
- Cluster mode: For multi-instance failover
```

---

### 1.2 Configuration Pre-Flight

**Security Configuration**:
```toml
# /etc/fraiseql/config.toml

[server]
listen_addr = "127.0.0.1:4000"  # Never expose to public directly
tls_enabled = true
tls_cert_path = "/etc/fraiseql/certs/server.crt"
tls_key_path = "/etc/fraiseql/certs/server.key"
min_tls_version = "1.2"

[database]
url = "postgresql://fraiseql_user:${DB_PASSWORD}@postgres:5432/fraiseql"
pool_size = 50  # Tune based on connection count
connection_timeout = "30s"

[security]
security_profile = "REGULATED"  # For sensitive data
field_masking_enabled = true
error_redaction_enabled = true

[rate_limit]
enabled = true
rps_per_ip = 100
rps_per_user = 1000
burst_size = 500

[oidc]
cache_ttl_secs = 300  # 5 minutes
key_rotation_check = true

[audit]
enabled = true
log_level = "INFO"
destination = "postgres"  # Log to database
```

**Validation**:
```bash
# Verify TLS certificates
openssl x509 -in /etc/fraiseql/certs/server.crt -text -noout

# Verify PostgreSQL connection
psql -h postgres -U fraiseql_user -d fraiseql -c "SELECT 1"

# Verify configuration
fraiseql-server validate-config /etc/fraiseql/config.toml
```

---

## 2. Deployment Procedures

### 2.1 Single-Instance Deployment

**Step 1: Database Setup**
```bash
# Connect to PostgreSQL
psql -U postgres -h postgres

# Create database
CREATE DATABASE fraiseql;

# Create user with SCRAM authentication
CREATE USER fraiseql_user WITH PASSWORD 'STRONG_PASSWORD_HERE';

# Grant permissions
GRANT CONNECT ON DATABASE fraiseql TO fraiseql_user;
GRANT USAGE ON SCHEMA public TO fraiseql_user;
GRANT ALL ON ALL TABLES IN SCHEMA public TO fraiseql_user;
```

**Step 2: Deploy Service**
```bash
# Pull image
docker pull fraiseql:latest

# Run container
docker run -d \
  --name fraiseql \
  --restart always \
  --health-cmd='curl -f http://localhost:4000/health || exit 1' \
  --health-interval=30s \
  --health-timeout=10s \
  --health-retries=3 \
  -e DB_PASSWORD='STRONG_PASSWORD_HERE' \
  -v /etc/fraiseql:/etc/fraiseql:ro \
  -p 127.0.0.1:4000:4000 \
  fraiseql:latest
```

**Step 3: Health Checks**
```bash
# Wait for startup
sleep 5

# Check health endpoint
curl -s http://localhost:4000/health | jq .

# Check logs
docker logs fraiseql

# Verify GraphQL endpoint
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name } } }"}'
```

---

### 2.2 Multi-Instance Deployment (HA)

**Architecture**:
```
                        ┌─────────────────┐
                        │  Load Balancer  │
                        │  (TLS Offload)  │
                        └────────┬────────┘
                                 │
                ┌────────────────┼────────────────┐
                │                │                │
          ┌─────────┐      ┌─────────┐     ┌─────────┐
          │Instance1│      │Instance2│     │Instance3│
          └────┬────┘      └────┬────┘     └────┬────┘
               │                │                │
               └────────────────┼────────────────┘
                                │
                        ┌───────────────┐
                        │  PostgreSQL   │
                        │  (Replicated) │
                        └───────────────┘
                                │
                        ┌───────────────┐
                        │     Redis     │
                        │   (Cluster)   │
                        └───────────────┘
```

**Deployment Steps**:

```bash
#!/bin/bash
# 1. Start database
docker-compose up -d postgres
sleep 10  # Wait for DB startup

# 2. Run migrations
docker run --rm \
  --network host \
  fraiseql:latest \
  fraiseql-migrate --database-url postgres://...

# 3. Start Redis cluster
docker-compose up -d redis

# 4. Start service instances
for i in 1 2 3; do
  docker run -d \
    --name fraiseql-$i \
    --restart always \
    -e INSTANCE_ID="instance-$i" \
    -e REDIS_URL="redis://redis:6379" \
    fraiseql:latest
done

# 5. Register with load balancer
for instance in fraiseql-1 fraiseql-2 fraiseql-3; do
  docker exec load-balancer \
    add-backend $instance http://fraiseql:4000
done
```

---

## 3. Monitoring & Observability

### 3.1 Key Metrics

**Application Metrics**:
```
# Query metrics
fraiseql_queries_total{operation, status}
fraiseql_query_duration_seconds{operation, quantile}
fraiseql_query_complexity{operation}

# Database metrics
fraiseql_db_connections{state}
fraiseql_db_query_duration_seconds{query, quantile}
fraiseql_db_errors_total{operation, error_type}

# Cache metrics
fraiseql_cache_hits_total{cache_type}
fraiseql_cache_misses_total{cache_type}
fraiseql_cache_hit_ratio{cache_type}

# Security metrics
fraiseql_auth_failures_total{method, reason}
fraiseql_rate_limit_exceeded_total{key_type}
fraiseql_sql_injection_attempts_total
fraiseql_csrf_failures_total
```

**System Metrics**:
```
# Standard Prometheus
node_cpu_seconds_total
node_memory_bytes
node_network_bytes_total
node_disk_io_bytes_total

# Rust-specific
process_resident_memory_bytes
process_virtual_memory_bytes
```

---

### 3.2 Alerting Rules

**Critical Alerts**:
```yaml
# Alert: Service down
- alert: FraiseQLDown
  expr: up{job="fraiseql"} == 0
  for: 1m
  annotations:
    severity: critical
    runbook: /docs/runbook/fraiseql-down.md

# Alert: Database errors increasing
- alert: HighDatabaseErrorRate
  expr: rate(fraiseql_db_errors_total[5m]) > 10
  for: 5m
  annotations:
    severity: critical

# Alert: Memory leak detection
- alert: MemoryLeakSuspected
  expr: process_resident_memory_bytes > 7_000_000_000
  for: 30m
  annotations:
    severity: high
```

**Warning Alerts**:
```yaml
# Alert: High latency
- alert: HighQueryLatency
  expr: histogram_quantile(0.99, fraiseql_query_duration_seconds) > 5
  for: 5m
  annotations:
    severity: warning

# Alert: Cache hit ratio low
- alert: LowCacheHitRatio
  expr: fraiseql_cache_hit_ratio < 0.7
  for: 10m
  annotations:
    severity: warning

# Alert: Connection pool exhaustion
- alert: ConnectionPoolNearCapacity
  expr: fraiseql_db_connections{state="active"} > 45
  for: 5m
  annotations:
    severity: warning
```

---

## 4. Incident Response

### 4.1 Common Issues & Solutions

**Issue**: High Query Latency
```
Symptoms: P99 latency > 5s
Root Causes:
  - Database slow query
  - Connection pool exhausted
  - Network latency
  - Complex query

Resolution:
1. Check database slow query log
2. Verify connection pool: SHOW max_connections
3. Check network latency: mtr to database
4. Profile query execution: EXPLAIN ANALYZE
5. Consider caching or query optimization
```

**Issue**: Memory Leak
```
Symptoms: Memory growing over time
Root Causes:
  - Unclosed connections
  - Large query results
  - Circular references in cache
  - Long-running transactions

Resolution:
1. Check memory profile: pprof
2. Verify connection limits
3. Check cache eviction policies
4. Review long-running queries
5. Restart service if critical
```

**Issue**: Authentication Failures
```
Symptoms: Spike in auth failures
Root Causes:
  - PostgreSQL user issues
  - Certificate expiration
  - OIDC provider down
  - Rate limit triggered

Resolution:
1. Check PostgreSQL logs
2. Verify TLS certificate: openssl x509 -text
3. Check OIDC provider status
4. Monitor rate limiting metrics
5. Scale up if under attack
```

---

### 4.2 Incident Playbooks

**Playbook: Service Down**
```
1. Verify: curl http://localhost:4000/health
2. Check: docker logs fraiseql
3. Check: systemctl status fraiseql
4. Restart: systemctl restart fraiseql
5. Verify: Wait 30s for health checks
6. If still down:
   a. Check database connectivity
   b. Check configuration file
   c. Review recent changes
   d. Escalate to senior engineer
```

**Playbook: Database Unavailable**
```
1. Verify: psql -h postgres -U fraiseql_user -d fraiseql
2. Check: SELECT version();
3. Check: SELECT datname FROM pg_database WHERE datname='fraiseql';
4. Failover: If replica available, switch connection string
5. Alert: Page on-call database team
6. Communicate: Update status page
```

**Playbook: High Error Rate**
```
1. Check: Error metrics by type and endpoint
2. Identify: Most affected service/query
3. Isolate: Block problematic queries if needed
4. Mitigate: Scale up instances or disable features
5. Root cause: Analyze after incident resolved
6. Prevention: Add test case/monitoring
```

---

## 5. Backup & Disaster Recovery

### 5.1 Backup Strategy

**Database Backups**:
```bash
# Full backup (daily, 02:00 UTC)
0 2 * * * pg_dump fraiseql > /backups/fraiseql-$(date +%Y%m%d).sql

# WAL archiving (continuous)
archive_mode = on
archive_command = 'cp %p /backups/wal/%f'

# Backup retention: 30 days
find /backups -name 'fraiseql-*.sql' -mtime +30 -delete

# Verify backups
pg_restore --list fraiseql-20260126.sql | head -20
```

**Configuration Backups**:
```bash
# Backup configuration (hourly)
0 * * * * tar -czf /backups/config-$(date +%Y%m%d-%H%M%S).tar.gz /etc/fraiseql

# Backup retention: 90 days
find /backups -name 'config-*.tar.gz' -mtime +90 -delete
```

**Audit Log Backups**:
```bash
# Archive audit logs (daily)
0 1 * * * gzip -c fraiseql_audit_logs > /backups/audit-$(date +%Y%m%d).gz

# Verify integrity
gunzip -t /backups/audit-20260126.gz
```

---

### 5.2 Disaster Recovery Procedures

**RTO/RPO Targets**:
```
Recovery Time Objective (RTO): 1 hour
Recovery Point Objective (RPO): 5 minutes

Tier 1 (Critical): RTO 15min, RPO 1min
Tier 2 (High): RTO 1hr, RPO 5min
Tier 3 (Medium): RTO 4hr, RPO 1hr
Tier 4 (Low): RTO 24hr, RPO 24hr
```

**Failover Procedure**:
```bash
#!/bin/bash
# Automated failover script

# 1. Detect primary down
if ! curl -s http://primary:4000/health; then
  echo "Primary unavailable, starting failover"

  # 2. Promote replica
  ssh replica psql -c "SELECT pg_promote();"
  sleep 10

  # 3. Update connection strings
  sed -i "s/primary:5432/replica:5432/" /etc/fraiseql/config.toml

  # 4. Restart service
  systemctl restart fraiseql

  # 5. Verify
  curl -s http://localhost:4000/health | jq .

  # 6. Notify
  curl -X POST https://slack.webhook/... \
    -d '{"text":"Failover complete: Replica promoted"}'
fi
```

---

## 6. Scaling & Performance

### 6.1 Horizontal Scaling

**When to Scale Up**:
```
- CPU: > 70% sustained for 10min
- Memory: > 75% used
- Connections: > 80% pool capacity
- Query latency: P95 > 2s
- Disk: > 80% used
```

**Scaling Process**:
```bash
# 1. Prepare new instance
docker pull fraiseql:latest
docker run -d --name fraiseql-new fraiseql:latest

# 2. Verify health
curl http://fraiseql-new:4000/health

# 3. Register with load balancer
lb-cli add-backend fraiseql-new http://fraiseql-new:4000

# 4. Monitor
watch -n 1 'curl -s http://load-balancer/stats | grep fraiseql'

# 5. Remove old instance if scaling down
lb-cli remove-backend fraiseql-old
docker stop fraiseql-old && docker rm fraiseql-old
```

---

### 6.2 Database Scaling

**Vertical Scaling** (More powerful instance):
```
1. Backup production
2. Upgrade instance type
3. Run maintenance tasks
4. Verify performance
5. Monitor for issues
```

**Horizontal Scaling** (Read replicas):
```bash
# Create read replica
pg_basebackup -h primary -D /var/lib/postgresql/data -U postgres -v

# Configure as hot standby
standby_mode = 'on'
primary_conninfo = 'host=primary port=5432'

# Enable queries on replica
recovery_target_timeline = 'latest'
```

---

## 7. Maintenance & Updates

### 7.1 Patching Schedule

**Critical Patches**: Within 24 hours
**Security Patches**: Within 7 days
**Regular Updates**: Monthly maintenance window

**Procedure**:
```bash
# 1. Test in staging
docker run -e "TEST_ENV=true" fraiseql:patch-version

# 2. Create runbook
cat > /tmp/fraiseql-update.md << EOF
- Update version: X.Y.Z -> X.Y.Z+1
- Changes: Security patch for vulnerability ABC
- Risk: Low
- Rollback: Immediate restart with previous version
EOF

# 3. Schedule downtime
notify-customers "Scheduled maintenance 02:00-02:15 UTC"

# 4. Stop service
systemctl stop fraiseql

# 5. Backup
cp -r /etc/fraiseql /backups/config-pre-update

# 6. Update
docker pull fraiseql:X.Y.Z+1
docker run -d --name fraiseql fraiseql:X.Y.Z+1

# 7. Verify
curl http://localhost:4000/health

# 8. Resume
systemctl start fraiseql
```

---

### 7.2 Database Maintenance

**Weekly**:
```sql
-- Analyze query planner statistics
ANALYZE;

-- Vacuum dead tuples
VACUUM;

-- Check for long-running queries
SELECT query, state_change FROM pg_stat_activity WHERE state != 'idle';
```

**Monthly**:
```sql
-- Full maintenance
VACUUM FULL;

-- Reindex if fragmented
REINDEX TABLE *;

-- Update statistics
ANALYZE;
```

---

## 8. Security Operations

### 8.1 Regular Security Reviews

**Daily**:
```bash
# Check failed authentication attempts
SELECT COUNT(*) FROM fraiseql_audit_logs
  WHERE event_type = 'authentication_failed'
  AND timestamp > NOW() - INTERVAL '24 hours';

# Check SQL injection attempts
SELECT COUNT(*) FROM fraiseql_audit_logs
  WHERE event_type = 'sql_injection_attempt'
  AND timestamp > NOW() - INTERVAL '24 hours';

# Check CSRF failures
SELECT COUNT(*) FROM fraiseql_audit_logs
  WHERE event_type = 'csrf_failure'
  AND timestamp > NOW() - INTERVAL '24 hours';
```

**Weekly**:
```bash
# Review rate limiting events
SELECT user_id, COUNT(*) as blocked_attempts
  FROM fraiseql_audit_logs
  WHERE event_type = 'rate_limit_exceeded'
  AND timestamp > NOW() - INTERVAL '7 days'
  GROUP BY user_id ORDER BY blocked_attempts DESC;

# Check certificate expiration
openssl x509 -in /etc/fraiseql/certs/server.crt -noout -dates
```

**Monthly**:
```bash
# Security audit report
generate-security-report --month=$(date +%Y-%m) --output=/reports/security-$month.pdf

# Access logs review
analyze-access-logs --month=$(date +%Y-%m)

# Policy compliance check
compliance-checker --generate-report
```

---

## 9. Documentation

### 9.1 Runbooks to Create

- [ ] Service Down Runbook
- [ ] Database Unavailable Runbook
- [ ] High Error Rate Runbook
- [ ] Memory Leak Investigation
- [ ] Connection Pool Exhaustion
- [ ] TLS Certificate Renewal
- [ ] Disaster Recovery Procedures
- [ ] Failover Procedures
- [ ] Scaling Procedures

---

### 9.2 Dashboards to Create

- [ ] Service Health Dashboard
- [ ] Performance Dashboard
- [ ] Security Dashboard
- [ ] Database Dashboard
- [ ] Network Dashboard
- [ ] Capacity Planning Dashboard

---

## 10. Operational Checklist

### Pre-Production
- [ ] Load balancer configured and tested
- [ ] TLS certificates installed and verified
- [ ] PostgreSQL database initialized
- [ ] Redis cluster running (if multi-instance)
- [ ] Monitoring and alerting configured
- [ ] Log aggregation running
- [ ] Backup procedures tested
- [ ] Disaster recovery plan documented
- [ ] Incident response runbooks created
- [ ] Team trained on procedures

### Post-Deployment
- [ ] Health checks passing
- [ ] Metrics flowing to monitoring system
- [ ] Logs flowing to aggregation system
- [ ] Alerts firing correctly
- [ ] Load test completed successfully
- [ ] Performance baseline established
- [ ] Security scan completed
- [ ] Documentation updated
- [ ] Team notified of availability

---

## Conclusion

This runbook provides the operational procedures needed to deploy, manage, and maintain FraiseQL in production. Key principles:

1. **Automate Everything**: Reduce manual operations
2. **Monitor Everything**: Catch issues before customers do
3. **Document Everything**: Enable on-call rotations
4. **Test Everything**: Verify procedures work before needed
5. **Plan Everything**: Know how to handle failures

---

**Runbook Completed**: January 26, 2026
**Lead Author**: Site Reliability Engineer
**Next Review**: April 26, 2026
**Status**: Ready for implementation
