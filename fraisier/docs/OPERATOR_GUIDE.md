# Fraisier Operator Guide

Comprehensive guide for operating Fraisier in production environments.

## Table of Contents

1. [Monitoring and Alerting](#monitoring-and-alerting)
2. [Error Recovery Procedures](#error-recovery-procedures)
3. [Database Management](#database-management)
4. [Performance Tuning](#performance-tuning)
5. [Troubleshooting](#troubleshooting)
6. [Maintenance](#maintenance)

---

## Monitoring and Alerting

### Starting the Metrics Exporter

```bash
# Start on default port (localhost:8001)
fraisier metrics

# Start on custom port
fraisier metrics --port 8080

# Listen on all interfaces (for remote monitoring)
fraisier metrics --address 0.0.0.0
```

### Prometheus Integration

#### Configuration

Add to `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'fraisier'
    static_configs:
      - targets: ['localhost:8001']
    scrape_interval: 30s  # Recommend higher interval for Fraisier
```

#### Key Metrics to Monitor

- `fraisier_deployments_total` - Total deployment attempts (should increase smoothly)
- `fraisier_deployment_errors_total` - Error count (should be low)
- `fraisier_active_deployments` - Currently running deployments (usually 0-3)
- `fraisier_deployment_duration_seconds` - Deployment time percentiles

### Grafana Dashboards

Import the provided dashboard from `monitoring/grafana-dashboard.json`:

1. Grafana → Dashboards → Import
2. Upload `grafana-dashboard.json`
3. Select Prometheus data source
4. Dashboard displays all key metrics

### Alert Rules

#### Alert: High Deployment Error Rate

```yaml
- alert: HighDeploymentErrorRate
  expr: rate(fraisier_deployment_errors_total[5m]) > 0.1
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "High deployment error rate ({{ $value }} errors/sec)"
    description: "Deployment errors > 10% in last 5 minutes"
```

#### Alert: Deployment Timeout

```yaml
- alert: DeploymentTimeout
  expr: histogram_quantile(0.95, rate(fraisier_deployment_duration_seconds_bucket[5m])) > 600
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Deployment p95 exceeds 10 minutes"
    description: "Deployments taking longer than expected"
```

#### Alert: Many Simultaneous Deployments

```yaml
- alert: ManyActiveDeployments
  expr: fraisier_active_deployments > 5
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Too many simultaneous deployments ({{ $value }})"
    description: "More than 5 deployments running at once"
```

#### Alert: Provider Unavailable

```yaml
- alert: ProviderUnavailable
  expr: fraisier_provider_availability == 0
  for: 2m
  labels:
    severity: critical
  annotations:
    summary: "Provider unavailable: {{ $labels.provider }}"
    description: "Provider has been unavailable for 2 minutes"
```

---

## Error Recovery Procedures

### Common Errors and Recovery

#### Deployment Timeout

**Symptoms:**
- Deployment runs for > configured timeout
- Service not responding after deployment

**Recovery:**
1. Check provider logs:
   ```bash
   fraisier status <fraise> <environment>
   ```

2. Check service health manually:
   - SSH to server and verify service status
   - Check systemd logs: `journalctl -u service-name -n 50`
   - Check application logs

3. If service is healthy, mark deployment as complete (manual intervention)

4. If service is unhealthy:
   ```bash
   # Trigger rollback
   fraisier rollback <fraise> <environment>
   ```

#### Provider Connection Error

**Symptoms:**
- Cannot connect to provider (SSH, Docker socket, Coolify API)
- "Connection refused" or "Timeout" errors

**Recovery:**
1. Test provider connectivity:
   ```bash
   fraisier provider-test bare_metal --config-file provider-config.yaml
   ```

2. Common issues:
   - **SSH**: Check key permissions, host key, network connectivity
   - **Docker**: Verify socket permissions, daemon running
   - **Coolify**: Check API key, URL, network access

3. Once fixed, retry deployment:
   ```bash
   fraisier deploy <fraise> <environment> --force
   ```

#### Health Check Failure

**Symptoms:**
- Deployment succeeds but health check fails
- Service running but not responding to health endpoint

**Recovery:**
1. Manually verify service:
   - SSH to server
   - Test health endpoint: `curl http://localhost:8000/health`
   - Check service logs for errors

2. If service is healthy, update health check configuration:
   - Check `fraises.yaml` for correct health_check URL/port
   - Consider increasing health check timeout if service is slow to start

3. If service is unhealthy:
   - Check recent logs: `fraisier status <fraise> <environment>`
   - Review deployment-specific logs
   - Investigate application errors

#### Database Lock Timeout

**Symptoms:**
- "Deployment lock acquisition timed out" error
- Unable to acquire deployment lock

**Recovery:**
1. Check if other deployments are running:
   ```bash
   fraisier status-all
   ```

2. Wait for other deployments to complete (recommended)

3. If lock is stuck:
   - SSH to database server
   - Query lock table: `SELECT * FROM tb_deployment_lock;`
   - Check if deployment process is still running

4. If lock is stale (process dead):
   - Delete lock: `DELETE FROM tb_deployment_lock WHERE ...;`
   - Retry deployment

### Manual Interventions

#### Force Deployment

Use `--force` flag to bypass version checks:

```bash
fraisier deploy <fraise> <environment> --force
```

**When to use:**
- Deploying same version to fix state
- Forcing redeploy after manual changes
- Emergency deployment

#### Dry Run Deployment

Preview what would happen without executing:

```bash
fraisier deploy <fraise> <environment> --dry-run
```

Output shows:

- Current version
- Target version
- Provider being used

#### Manual Rollback

Rollback to previous version:

```bash
fraisier rollback <fraise> <environment>
```

---

## Database Management

### Database Schema

Fraisier uses PostgreSQL (default) with the following tables:

#### tb_deployment
Main deployment records:
```sql
SELECT id, fraise, environment, status, old_version, new_version,
       started_at, completed_at, duration_seconds, error_message
FROM tb_deployment
ORDER BY started_at DESC
LIMIT 10;
```

#### tb_fraise_state
Current state for each fraise:
```sql
SELECT * FROM v_fraise_status;
```

#### tb_deployment_lock
Active locks (should be empty normally):
```sql
SELECT * FROM tb_deployment_lock;
```

### Backup and Restore

#### Backing Up

```bash
# Backup entire database
pg_dump fraisier > fraisier-$(date +%Y%m%d).sql

# Backup specific table
pg_dump -t tb_deployment fraisier > deployments-backup.sql
```

#### Restoring

```bash
# Restore entire database
psql fraisier < fraisier-backup.sql

# Restore specific table
psql fraisier < deployments-backup.sql
```

### Database Cleanup

#### Archive Old Deployments

```bash
# Move deployments older than 90 days to archive
psql fraisier -c "
  DELETE FROM tb_deployment
  WHERE completed_at < NOW() - INTERVAL '90 days'
  AND status != 'in_progress';
"
```

#### Vacuum and Analyze

```bash
# Optimize database performance
psql fraisier -c "VACUUM ANALYZE;"
```

---

## Performance Tuning

### Database Performance

#### Connection Pooling

Configure in `fraises.yaml`:

```yaml
database:
  host: localhost
  port: 5432
  pool_size: 20  # Adjust based on load
  pool_recycle: 3600
```

#### Index Optimization

```bash
# Check slow queries
psql fraisier -c "
  SELECT query, calls, mean_time
  FROM pg_stat_statements
  ORDER BY mean_time DESC
  LIMIT 10;
"
```

### Deployment Performance

#### Health Check Timeout

Increase timeout for slow-starting services:

```yaml
fraises:
  my_api:
    environments:
      production:
        health_check_url: http://localhost:8000/health
        health_check_timeout: 30  # seconds (default: 5)
        health_check_retries: 5   # retry attempts
```

#### Reduce Metrics Overhead

If metrics cause performance issues:

```bash
# Disable metrics recording for specific operations
export FRAISIER_METRICS_ENABLED=false
fraisier deploy <fraise> <environment>
```

---

## Troubleshooting

### Logs

#### View Recent Logs

```bash
# Show deployment history
fraisier history --limit 20

# Filter by fraise
fraisier history --fraise my_api --limit 10

# Filter by status
fraisier history | grep "failed"
```

#### Enable Debug Logging

```bash
# Run with verbose output
RUST_LOG=debug fraisier deploy <fraise> <environment>
```

#### Database Logs

```bash
# Check PostgreSQL logs
tail -f /var/log/postgresql/postgresql.log

# Or if using systemd
journalctl -u postgresql -f
```

### Common Issues

#### "Fraises.yaml not found"

Fraisier looks in these locations (in order):

1. File specified with `-c` flag
2. `/opt/fraisier/fraises.yaml`
3. `./fraises.yaml`
4. `./config/fraises.yaml`

**Solution:**
```bash
fraisier list -c /path/to/fraises.yaml
```

#### Webhook not processing

Check webhook configuration:

```bash
# View recent webhook events
fraisier webhooks --limit 10

# Look for "not processed" entries
```

If webhook isn't being processed:

1. Verify webhook URL is accessible from git provider
2. Check API key/token is valid
3. Verify webhook secret matches configuration
4. Check deployment logs for errors

#### Provider connection issues

```bash
# Test provider connectivity
fraisier provider-test bare_metal

# Test with custom config
fraisier provider-test bare_metal --config-file provider.yaml
```

### Health Check Issues

#### Service healthy but check fails

1. Verify endpoint returns correct status:
   ```bash
   curl -v http://localhost:8000/health
   ```

2. Check response time (may need timeout adjustment):
   ```bash
   time curl http://localhost:8000/health
   ```

3. If response is large, increase timeout in `fraises.yaml`

#### Check command not working

Verify command returns 0 on success:

```bash
# Test command locally
ssh user@host "command-here"
echo $?  # Should be 0 for success
```

---

## Maintenance

### Regular Tasks

#### Daily

- Monitor deployment history for errors
- Check provider availability
- Verify health check success rate

#### Weekly

- Review deployment statistics: `fraisier stats --days 7`
- Check database size and growth
- Clean up stale locks (if any)

#### Monthly

- Archive old deployments (>90 days)
- Analyze database performance
- Review and optimize slow queries
- Update provider credentials/keys

#### Quarterly

- Full database backup
- Performance baseline review
- Upgrade Fraisier if updates available

### Scheduled Maintenance

#### Database Maintenance (Weekly)

```bash
# Add to crontab
0 2 * * 0 /usr/bin/psql fraisier -c "VACUUM ANALYZE;"
```

#### Log Cleanup (Monthly)

```bash
# Add to crontab
0 3 1 * * find /var/log/fraisier -mtime +30 -delete
```

### Upgrade Procedure

1. **Backup database:**
   ```bash
   pg_dump fraisier > backup-$(date +%Y%m%d).sql
   ```

2. **Stop services:**
   ```bash
   systemctl stop fraisier-webhook
   ```

3. **Upgrade:**
   ```bash
   pip install --upgrade fraisier
   ```

4. **Verify:**
   ```bash
   fraisier --version
   fraisier provider-test bare_metal
   ```

5. **Start services:**
   ```bash
   systemctl start fraisier-webhook
   ```

---

## Support and Resources

- **Documentation**: See `docs/` directory
- **Issue Tracker**: GitHub issues page
- **Community**: FraiseQL community forum
- **Contact**: Support email (see README.md)

## Related Documents

- [Monitoring Guide](../monitoring/README.md)
- [Deployment Patterns](./DEPLOYMENT_PATTERNS.md)
- [API Reference](./API_REFERENCE.md)
