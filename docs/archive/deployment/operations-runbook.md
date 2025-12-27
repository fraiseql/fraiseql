# Operations Runbook

## Incident Response Procedures

### Severity Levels

#### **SEV-1: Critical** 🚨
- **Definition**: Complete system outage, data loss, or security breach
- **Response Time**: Immediate (within 15 minutes)
- **Communication**: All stakeholders notified immediately
- **Escalation**: On-call engineer + management

#### **SEV-2: High** ⚠️
- **Definition**: Major functionality degraded, performance issues affecting users
- **Response Time**: Within 1 hour
- **Communication**: Engineering team + product owners
- **Escalation**: On-call engineer

#### **SEV-3: Medium** 📊
- **Definition**: Minor functionality issues, monitoring alerts
- **Response Time**: Within 4 hours
- **Communication**: Engineering team
- **Escalation**: Next business day if unresolved

#### **SEV-4: Low** ℹ️
- **Definition**: Cosmetic issues, informational alerts
- **Response Time**: Within 24 hours
- **Communication**: Internal engineering
- **Escalation**: Weekly review

### Incident Response Process

#### 1. Detection & Triage (0-15 minutes)

**For SEV-1 incidents:**
```bash
# Immediately assess system status
curl -f https://yourdomain.com/health || echo "Application DOWN"

# Check database connectivity
docker-compose exec db pg_isready -U fraiseql -d fraiseql_prod

# Check Redis connectivity
docker-compose exec redis redis-cli ping

# Check system resources
docker stats --no-stream

# Notify incident response team
# - Slack: #incidents
# - PagerDuty: Trigger incident
# - Email: incident@company.com
```

**Initial Assessment Checklist:**
- [ ] Confirm incident scope and impact
- [ ] Determine severity level
- [ ] Notify appropriate stakeholders
- [ ] Start incident timeline documentation
- [ ] Begin investigation

#### 2. Investigation (15-60 minutes)

**Log Analysis:**
```bash
# Check application logs
docker-compose logs --tail=100 -f fraiseql

# Check nginx access/error logs
tail -f /var/log/nginx/access.log
tail -f /var/log/nginx/error.log

# Check system logs
journalctl -u docker -f --since "1 hour ago"

# Database query analysis
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT state, count(*) FROM pg_stat_activity GROUP BY state;
SELECT * FROM pg_stat_activity WHERE state != 'idle';
"
```

**Performance Metrics Check:**
```bash
# Check Prometheus metrics
curl http://localhost:9090/api/v1/query?query=up

# Check application metrics
curl https://yourdomain.com/metrics

# Database performance
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT * FROM pg_stat_user_tables ORDER BY n_tup_ins DESC LIMIT 10;
SELECT * FROM pg_stat_user_indexes WHERE idx_scan = 0;
"
```

#### 3. Containment (30-120 minutes)

**Common Containment Actions:**

**For Application Issues:**
```bash
# Restart application
docker-compose restart fraiseql

# Scale up resources if needed
docker-compose up -d --scale fraiseql=2

# Rollback to previous version
docker-compose pull fraiseql:previous-version
docker-compose up -d fraiseql
```

**For Database Issues:**
```bash
# Check connection pool
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SHOW max_connections;
SELECT count(*) FROM pg_stat_activity;
"

# Restart database if needed
docker-compose restart db

# Failover to replica (if available)
# kubectl patch deployment postgres -p '{"spec":{"template":{"spec":{"containers":[{"name":"postgres","env":[{"name":"POSTGRES_MASTER","value":"replica-host"}]}}]}}}}
```

**For Infrastructure Issues:**
```bash
# Check disk space
df -h

# Check memory usage
free -h

# Check network connectivity
ping -c 5 google.com
traceroute yourdomain.com

# Restart services
systemctl restart docker
systemctl restart nginx
```

#### 4. Recovery (60-240 minutes)

**Recovery Procedures:**

**Application Recovery:**
```bash
# Verify application health
curl https://yourdomain.com/health

# Run smoke tests
npm test -- --grep "smoke"

# Gradually increase traffic
# Use load balancer to slowly route traffic back
```

**Data Recovery:**
```bash
# Restore from backup if needed
gunzip /opt/fraiseql/backups/fraiseql_backup.sql.gz
docker-compose exec -T db psql -U fraiseql fraiseql_prod < /opt/fraiseql/backups/fraiseql_backup.sql

# Verify data integrity
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT count(*) FROM your_table;
SELECT max(updated_at) FROM your_table;
"
```

#### 5. Post-Incident Review (24-72 hours)

**Incident Review Process:**
1. **Timeline Reconstruction**: Document all events chronologically
2. **Root Cause Analysis**: Identify underlying causes
3. **Impact Assessment**: Quantify user/business impact
4. **Action Items**: Define preventive measures
5. **Documentation Update**: Update runbooks and procedures

**Post-Incident Report Template:**
```markdown
# Incident Report: [INC-YYYY-MM-DD-N]

## Summary
[Brief description of incident]

## Timeline
- **Detection**: [Time] - [How detected]
- **Response**: [Time] - [Initial response]
- **Resolution**: [Time] - [How resolved]

## Impact
- **Users Affected**: [Number/Percentage]
- **Duration**: [Time period]
- **Business Impact**: [Financial/operational impact]

## Root Cause
[Detailed analysis of what caused the incident]

## Resolution
[Steps taken to resolve the incident]

## Prevention
[Action items to prevent recurrence]

## Lessons Learned
[Key takeaways and improvements]
```

## Maintenance Procedures

### Daily Maintenance

#### Morning Health Check (9:00 AM)
```bash
#!/bin/bash
# Daily health check script

echo "=== Daily Health Check ==="

# Application health
curl -f https://yourdomain.com/health || echo "❌ Application health check failed"

# Database connectivity
docker-compose exec db pg_isready -U fraiseql -d fraiseql_prod || echo "❌ Database connectivity failed"

# Redis connectivity
docker-compose exec redis redis-cli ping || echo "❌ Redis connectivity failed"

# Disk space check
DISK_USAGE=$(df / | tail -1 | awk '{print $5}' | sed 's/%//')
if [ "$DISK_USAGE" -gt 90 ]; then
    echo "❌ Disk usage critical: ${DISK_USAGE}%"
fi

# Memory usage check
MEMORY_USAGE=$(free | grep Mem | awk '{printf "%.0f", $3/$2 * 100.0}')
if [ "$MEMORY_USAGE" -gt 90 ]; then
    echo "❌ Memory usage critical: ${MEMORY_USAGE}%"
fi

# Certificate expiry check
CERT_EXPIRY=$(openssl x509 -enddate -noout -in /etc/letsencrypt/live/yourdomain.com/cert.pem | cut -d= -f2)
CERT_DAYS=$(( ($(date -d "$CERT_EXPIRY" +%s) - $(date +%s)) / 86400 ))
if [ "$CERT_DAYS" -lt 30 ]; then
    echo "⚠️  SSL certificate expires in ${CERT_DAYS} days"
fi

echo "✅ Health check completed"
```

#### Log Rotation
```bash
# Rotate application logs
docker-compose exec fraiseql logrotate /etc/logrotate.d/fraiseql

# Rotate nginx logs
logrotate /etc/logrotate.d/nginx

# Clean old logs (keep 30 days)
find /var/log -name "*.log.*" -mtime +30 -delete
```

### Weekly Maintenance

#### Security Updates (Monday 2:00 AM)
```bash
# Update system packages
apt update && apt upgrade -y

# Update Docker images
docker-compose pull

# Restart services with new images
docker-compose up -d

# Run security scans
trivy image --exit-code 1 --severity HIGH,CRITICAL your-registry/fraiseql:latest
```

#### Database Maintenance
```bash
# Vacuum and analyze database
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "VACUUM ANALYZE;"

# Reindex if needed
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
REINDEX DATABASE fraiseql_prod;
REINDEX SYSTEM fraiseql_prod;
"

# Check for unused indexes
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT schemaname, tablename, indexname
FROM pg_indexes
WHERE schemaname = 'public'
AND indexname NOT IN (
    SELECT indexname
    FROM pg_stat_user_indexes
    WHERE idx_scan > 0
);
"
```

#### Performance Optimization
```bash
# Analyze slow queries
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT query, calls, total_time, mean_time, rows
FROM pg_stat_statements
ORDER BY total_time DESC
LIMIT 10;
"

# Check table bloat
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT schemaname, tablename, n_dead_tup, n_live_tup
FROM pg_stat_user_tables
WHERE n_dead_tup > 0
ORDER BY n_dead_tup DESC;
"
```

### Monthly Maintenance

#### Capacity Planning Review
- Review resource utilization trends
- Plan for scaling requirements
- Update infrastructure provisioning

#### Security Audit
```bash
# Run comprehensive security scan
trivy fs --exit-code 1 --severity HIGH,CRITICAL .

# Check for exposed secrets
gitleaks detect --verbose --redact

# Review access logs for suspicious activity
grep " 40[0-9] " /var/log/nginx/access.log | head -20
```

#### Backup Verification
```bash
# Test backup restoration
BACKUP_FILE=$(ls -t /opt/fraiseql/backups/*.sql.gz | head -1)
echo "Testing backup: $BACKUP_FILE"

# Create test database
docker-compose exec db createdb -U fraiseql fraiseql_test_restore

# Restore backup
gunzip -c "$BACKUP_FILE" | docker-compose exec -T db psql -U fraiseql fraiseql_test_restore

# Verify restoration
docker-compose exec db psql -U fraiseql -d fraiseql_test_restore -c "SELECT count(*) FROM your_table;"

# Clean up
docker-compose exec db dropdb -U fraiseql fraiseql_test_restore
```

## Monitoring and Alerting

### Key Metrics to Monitor

#### Application Metrics
- Response time (P50, P95, P99)
- Error rate (4xx, 5xx responses)
- Throughput (requests per second)
- Active connections

#### Database Metrics
- Connection pool utilization
- Query execution time
- Deadlocks and timeouts
- Table/index bloat

#### Infrastructure Metrics
- CPU utilization
- Memory usage
- Disk I/O and space
- Network traffic

#### Business Metrics
- User activity
- API usage patterns
- Data growth rates

### Alert Configuration

#### Critical Alerts (Immediate Response)
```
- Application down (health check fails)
- Database unreachable
- High error rate (>5% 5xx responses)
- Certificate expiry (<30 days)
- Disk space critical (<10% free)
```

#### Warning Alerts (Review Within Hours)
```
- High memory usage (>90%)
- Slow response times (>2s P95)
- Database connection pool near capacity
- Unusual traffic patterns
```

#### Informational Alerts (Review Daily)
```
- Performance degradation trends
- Resource usage spikes
- New error patterns
```

## Backup and Recovery

### Backup Strategy

#### Database Backups
- **Frequency**: Daily full backups + hourly incremental
- **Retention**: 30 days for dailies, 7 days for incrementals
- **Storage**: Encrypted off-site storage
- **Testing**: Monthly restoration tests

#### Configuration Backups
- **Frequency**: After every change
- **Retention**: 90 days
- **Storage**: Git repository + encrypted backups

#### Application Backups
- **Frequency**: Before deployments
- **Retention**: 7 days
- **Storage**: Container registry tags

### Recovery Procedures

#### Complete System Recovery
```bash
# 1. Provision new infrastructure
terraform apply

# 2. Restore configuration
git clone https://github.com/yourorg/infrastructure.git
cd infrastructure && git checkout production

# 3. Deploy base services
docker-compose up -d db redis

# 4. Wait for services to be ready
sleep 60

# 5. Restore database
./restore-backup.sh latest

# 6. Deploy application
docker-compose up -d fraiseql nginx

# 7. Run health checks
curl https://yourdomain.com/health
```

#### Database-Only Recovery
```bash
# Stop application
docker-compose stop fraiseql

# Restore database
./restore-backup.sh latest

# Verify data integrity
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "
SELECT count(*) FROM users;
SELECT max(created_at) FROM users;
"

# Restart application
docker-compose start fraiseql
```

## Emergency Contacts

### On-Call Rotation
- **Primary**: [Engineer Name] - [Phone] - [Email]
- **Secondary**: [Engineer Name] - [Phone] - [Email]
- **Management**: [Manager Name] - [Phone] - [Email]

### External Resources
- **Cloud Provider Support**: [Support Contact]
- **Database Support**: [PostgreSQL Support]
- **Security Team**: [Security Contact]

### Escalation Path
1. **Level 1**: On-call engineer
2. **Level 2**: Engineering manager
3. **Level 3**: CTO/Executive team
4. **Level 4**: Board/Crisis team

---

*This runbook is living documentation. Update it after every incident and improvement.*
