# Fraisier Deployment Patterns

Advanced deployment patterns and strategies for production deployments with Fraisier.

## Table of Contents

1. [Rolling Deployments](#rolling-deployments)
2. [Canary Deployments](#canary-deployments)
3. [Blue-Green Deployments](#blue-green-deployments)
4. [Health-Check Based Rollback](#health-check-based-rollback)
5. [Database Migrations](#database-migrations)
6. [Multi-Provider Deployments](#multi-provider-deployments)
7. [Emergency Procedures](#emergency-procedures)

---

## Rolling Deployments

The standard deployment pattern: gradually roll out new version to all instances.

### When to Use

- Standard deployments with no downtime requirement
- Services with multiple instances
- Low-risk changes

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: bare_metal
        instances:
          - api-1.prod.example.com
          - api-2.prod.example.com
          - api-3.prod.example.com
        deployment_strategy: rolling
        rolling_config:
          batch_size: 1  # Deploy to 1 instance at a time
          wait_between_batches: 30  # seconds
          health_check_retries: 5
          rollback_on_health_check_failure: true
```

### Execution

```bash
# Deploy to all instances (one at a time)
fraisier deploy api production

# Monitor deployment status
fraisier status api production

# View deployment history
fraisier history --fraise api --limit 5
```

### Flow Diagram

```
Initial State:
  api-1: v1.0
  api-2: v1.0
  api-3: v1.0

After deployment:
  api-1: v2.0 ✓ (health check passed)
  api-2: v2.0 ✓
  api-3: v2.0 ✓

All instances running v2.0
```

---

## Canary Deployments

Deploy to a small subset first, monitor, then roll out to remaining instances.

### When to Use

- High-risk changes
- Performance-sensitive changes
- New features that need validation
- Large refactorings

### Strategy

1. Deploy to 1-2 "canary" instances
2. Monitor metrics (errors, latency, resource usage)
3. If metrics acceptable, deploy to remaining instances
4. If issues detected, rollback immediately

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: bare_metal
        instances:
          - api-1.prod.example.com  # Canary
          - api-2.prod.example.com
          - api-3.prod.example.com
        deployment_strategy: canary
        canary_config:
          canary_instances: 1
          canary_duration: 300  # Monitor for 5 minutes
          canary_error_threshold: 0.01  # 1% error rate max
          canary_latency_threshold: 100  # ms p99 max
          metrics_to_monitor:
            - error_rate
            - response_latency_p99
            - cpu_usage
            - memory_usage
```

### Execution

```bash
# Deploy canary instances first
fraisier deploy api production --canary-only

# Monitor metrics (wait for canary_duration)
# Fraisier will automatically check metrics

# If canary passes, deploy to remaining
fraisier deploy api production --complete-rollout

# Manual rollback if issues detected
fraisier rollback api production
```

### Monitoring During Canary

During the canary period, monitor:

```bash
# Check error rates (should be similar to before)
fraisier stats --fraise api --days 1

# Check response times
# (Use Grafana dashboard or monitoring system)

# Check resource usage on canary instances
ssh api-1.prod.example.com
  systemctl status api
  top -p $(pgrep -f "api.*process")
```

---

## Blue-Green Deployments

Maintain two complete environments (Blue=current, Green=new). Switch traffic after validation.

### When to Use

- Zero-downtime requirement with immediate rollback capability
- Complex application state
- Database schema changes
- Major version upgrades

### Setup

```yaml
fraises:
  api:
    type: api
    environments:
      production_blue:
        name: "Production (Blue)"
        provider: bare_metal
        instances:
          - api-blue-1.prod.example.com
          - api-blue-2.prod.example.com
          - api-blue-3.prod.example.com
      production_green:
        name: "Production (Green)"
        provider: bare_metal
        instances:
          - api-green-1.prod.example.com
          - api-green-2.prod.example.com
          - api-green-3.prod.example.com

load_balancer:
  type: nginx
  target: production_blue  # Current active environment
```

### Deployment Procedure

```bash
# 1. Deploy to Green (inactive)
fraisier deploy api production_green

# 2. Verify Green deployment
fraisier status api production_green

# 3. Run integration tests against Green
./tests/integration/e2e-test.sh http://api-green-1.prod.example.com:8000

# 4. If tests pass, switch load balancer
# Update load balancer config to route to Green
./switch-lb-target.sh production_green

# 5. Monitor for issues
fraisier stats --fraise api --days 1

# 6. Once stable, Blue becomes inactive for next deployment
```

### Rollback

Rollback is instant:

```bash
# Switch load balancer back to Blue
./switch-lb-target.sh production_blue

# Verify
curl https://api.prod.example.com/health
```

---

## Health-Check Based Rollback

Automatically rollback if health checks fail post-deployment.

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: bare_metal
        health_check_type: http
        health_check_url: http://localhost:8000/health
        health_check_timeout: 10

        # Rollback configuration
        rollback_config:
          enabled: true
          rollback_on_health_check_failure: true
          health_check_retries: 5
          retry_delay: 2
          max_consecutive_failures: 3
          rollback_timeout: 300
```

### How It Works

1. Deployment completes
2. Fraisier performs health checks (with retries)
3. If all health checks pass:
   - Deployment is considered successful
   - Logs success to database
4. If health checks fail:
   - Logs failure
   - Automatically triggers rollback
   - Previous version restored
   - Alert sent to operators

### Example Output

```
Deploying api to production...
Deployment completed in 45.2s
Running health checks...
  Attempt 1: FAILED (Connection refused)
  Attempt 2: FAILED (Connection refused)
  Attempt 3: PASSED (HTTP 200)
  Attempt 4: PASSED (HTTP 200)
  Attempt 5: PASSED (HTTP 200)
Health checks passed!
Deployment successful! v1.2.3 → v1.3.0
```

---

## Database Migrations

Coordinating deployments with database schema changes.

### Pattern 1: Schema First (Recommended)

1. Deploy schema changes (backward-compatible)
2. Application uses new schema
3. Remove old schema columns later

```bash
# 1. Run database migration
psql production_db < migrations/001_add_new_columns.sql

# 2. Verify migration
psql production_db -c "SELECT * FROM deployment_logs LIMIT 1;"

# 3. Deploy application
fraisier deploy api production

# 4. Remove old columns in future release
# (after confirming all old code is gone)
```

### Pattern 2: Two-Phase Commit

For complex migrations with data transformation:

```bash
# Phase 1: Prepare (create new schema, migrate data)
psql production_db < migrations/001_prepare.sql

# Phase 2: Deploy application using new schema
fraisier deploy api production

# Phase 3: Cleanup (remove old schema)
psql production_db < migrations/002_cleanup.sql
```

### Configuration Example

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: bare_metal
        pre_deployment_hooks:
          - name: database_migration
            type: database
            command: "psql production_db < migrations/pending.sql"
            timeout: 600
            on_failure: rollback
        post_deployment_hooks:
          - name: verify_schema
            type: script
            command: "./scripts/verify_schema.sh"
            timeout: 60
            on_failure: alert
```

---

## Multi-Provider Deployments

Deploy simultaneously to multiple providers (e.g., Bare Metal + Coolify).

### Use Case

Hybrid cloud setup: primary deployment on bare metal, failover on cloud.

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        deployment_mode: multi_provider
        providers:
          - name: bare_metal_prod
            type: bare_metal
            priority: 1  # Primary
            instances:
              - api-1.prod.example.com
              - api-2.prod.example.com
              - api-3.prod.example.com
          - name: coolify_cloud
            type: coolify  # Fallback
            priority: 2
            url: https://coolify.cloud.com
            application_id: xxx-yyy-zzz
```

### Deployment with Fallback

```bash
# Deploy to primary provider
fraisier deploy api production

# If primary fails, automatically tries fallback
# Deployment logs show which provider was used
fraisier history --fraise api --limit 1
```

---

## Emergency Procedures

### Emergency Rollback

When critical issues are discovered:

```bash
# Immediate rollback (no health checks)
fraisier rollback api production --force

# Verify previous version is running
curl https://api.example.com/health

# Once stable, investigate issue
fraisier history --fraise api --limit 1
```

### Kill Deployment in Progress

If deployment is stuck or going wrong:

```bash
# View active deployments
fraisier status-all

# For stuck deployment (last resort):
# 1. Manually kill process
ssh deploy@prod.example.com "pkill -f fraisier-deploy"

# 2. Verify service state
ssh deploy@prod.example.com "systemctl status api"

# 3. Clean up deployment lock
psql production_db -c "DELETE FROM tb_deployment_lock WHERE service='api';"

# 4. Re-run deployment
fraisier deploy api production
```

### Disaster Recovery

Complete failure recovery:

```bash
# 1. Restore database from backup
pg_restore production_db < backup-2024-01-22.sql

# 2. Manually deploy to one instance
ssh deploy@api-1.prod.example.com
  cd /opt/api
  git fetch origin
  git checkout <stable-commit>
  systemctl restart api

# 3. Verify health
curl http://api-1.prod.example.com:8000/health

# 4. Proceed with normal deployment
fraisier deploy api production --force
```

---

## Advanced Scenarios

### Gradual Traffic Shift (A/B Testing)

Route percentage of traffic to new version:

```yaml
traffic_shifting:
  enabled: true
  schedule:
    - percentage: 10
      duration: 300  # 5 minutes
    - percentage: 25
      duration: 300
    - percentage: 50
      duration: 600  # 10 minutes
    - percentage: 100
      duration: 0  # Permanent
```

### Scheduled Deployments

Deploy only during maintenance windows:

```bash
# Schedule deployment for 2am UTC
fraisier deploy api production --scheduled "02:00 UTC"

# Fraisier will queue deployment and execute at scheduled time
# Useful for low-traffic periods
```

### Parallel Deployments

Deploy to multiple services simultaneously:

```bash
# Deploy multiple services in parallel
fraisier deploy api production &
fraisier deploy etl production &
fraisier deploy worker production &
wait

echo "All deployments complete"
```

---

## Monitoring Deployments

### Key Metrics During Deployment

```yaml
deployment_slo:
  success_rate: 95%  # 95% of deployments should succeed
  error_rate_max: 1%  # Max 1% error rate during/after deployment
  latency_p99: 500ms  # p99 response time < 500ms
  rollback_rate: 5%  # Automatic rollback should be < 5%
```

### Health Check Patterns

```yaml
health_checks:
  api:
    - name: http_health
      type: http
      url: /health
      timeout: 5
      interval: 10
    - name: tcp_connectivity
      type: tcp
      port: 8000
      timeout: 2
      interval: 5
    - name: dependency_check
      type: exec
      command: "curl http://dependency.service/health"
      timeout: 10
      interval: 30
```

---

## Related Documents

- [Operator Guide](./OPERATOR_GUIDE.md) - Operations and troubleshooting
- [Monitoring Guide](../monitoring/README.md) - Metrics and dashboards
- [Error Handling](../docs/ERROR_HANDLING.md) - Error types and recovery
