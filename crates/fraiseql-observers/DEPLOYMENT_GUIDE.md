# Phase 8 Production Deployment Guide

**Last Updated**: January 22, 2026
**Status**: ✅ Ready for Production

---

## Table of Contents

1. [Pre-Deployment Checklist](#pre-deployment-checklist)
2. [Deployment Strategies](#deployment-strategies)
3. [Step-by-Step Deployment](#step-by-step-deployment)
4. [Post-Deployment Verification](#post-deployment-verification)
5. [Monitoring & Alerting](#monitoring--alerting)
6. [Rollback Procedures](#rollback-procedures)
7. [Day-2 Operations](#day-2-operations)

---

## Pre-Deployment Checklist

### Documentation Review

- [ ] Read `RELEASE_NOTES_PHASE_8.md` (this release)
- [ ] Study `docs/ARCHITECTURE_PHASE_8.md` (system design)
- [ ] Review `docs/CONFIGURATION_EXAMPLES.md` (config options)
- [ ] Understand `docs/MIGRATION_GUIDE.md` (migration strategy)
- [ ] Bookmark `docs/TROUBLESHOOTING.md` (diagnostics)

### Infrastructure Readiness

- [ ] PostgreSQL 12+ deployed and healthy
- [ ] Redis 6+ deployed (if using caching/dedup)
- [ ] Elasticsearch 7+ deployed (if using search)
- [ ] Prometheus scrape configured
- [ ] Grafana dashboards imported
- [ ] Alert rules configured
- [ ] Log aggregation ready (ELK, Datadog, etc.)

### Team Readiness

- [ ] Operations team trained on CLI tools
- [ ] Incident response plan documented
- [ ] On-call schedule established
- [ ] Runbooks prepared
- [ ] Communication channels established

### Code Readiness

- [ ] Latest code pulled: `git pull origin main`
- [ ] Tests passing locally: `cargo test`
- [ ] Binary built: `cargo build --release`
- [ ] Linting clean: `cargo clippy`

---

## Deployment Strategies

Choose the deployment strategy that matches your risk tolerance:

### Strategy 1: Gradual Rollout (Recommended)

**Risk Level**: ⭐ Low
**Duration**: 4-6 weeks
**Rollback Effort**: Minimal

**Timeline**:

- **Week 1**: Deploy to non-production (staging/test)
- **Week 2**: Deploy to 10% of production capacity
- **Week 3**: Monitor metrics, deploy to 50%
- **Week 4**: Full production deployment
- **Weeks 5-6**: Stabilization and optimization

**Phases**:

1. Checkpoints enabled only (safety first)
2. Add concurrent execution (performance)
3. Add deduplication (correctness)
4. Add caching (throughput)
5. Add monitoring (observability)
6. Add failover (high availability)

### Strategy 2: Big Bang Deployment

**Risk Level**: ⭐⭐⭐ High
**Duration**: 1 week
**Rollback Effort**: Significant

**Timeline**:

- **Day 1**: Build and test
- **Day 2**: Production deployment
- **Days 3-7**: Stabilization

**Prerequisites**:

- All Phase 1-7 systems stable
- Experienced operations team
- Comprehensive monitoring
- Quick rollback plan

### Strategy 3: Canary Deployment

**Risk Level**: ⭐⭐ Medium
**Duration**: 2-3 weeks
**Rollback Effort**: Easy

**Canary Phases**:

- **Week 1**: Canary to 5% traffic
- **Week 2**: Expand to 25% traffic
- **Week 3**: 100% traffic

**Monitoring**:

- Compare canary vs baseline metrics
- Watch for anomalies
- Hold at each step for 48 hours

---

## Step-by-Step Deployment

### Pre-Deployment (Day 1)

#### 1.1 Verify Current System Health

```bash
# Check Phase 1-7 baseline
curl -s http://your-observer-endpoint/metrics | \
  grep observer_events_processed_total

# Record baseline numbers
# - Total events processed
# - Error rate
# - Latency (P99)
# - Cache hit rate (if applicable)
```

#### 1.2 Build Release Binary

```bash
cd crates/fraiseql-observers

# Clean previous builds
cargo clean

# Build release with optimizations
cargo build --release --locked

# Verify binary
./target/release/fraiseql-observers --version

# Expected output: "fraiseql-observers 8.0.0"
```

#### 1.3 Validate Configuration

```bash
# Validate configuration syntax
fraiseql-observers validate-config config.yaml

# Expected output: "Configuration is valid"
```

#### 1.4 Run Pre-Deployment Tests

```bash
# Run full test suite
cargo test --release

# Expected: 205 tests passing

# Run stress tests (if desired)
cargo test --test stress_tests -- --ignored --nocapture
```

### Staging Deployment (Day 1-2)

#### 2.1 Deploy to Staging

```bash
# Create staging directory
mkdir -p /opt/staging/fraiseql-observers

# Copy binary and config
cp target/release/fraiseql-observers /opt/staging/
cp config.yaml /opt/staging/

# Set permissions
chmod 755 /opt/staging/fraiseql-observers
chmod 644 /opt/staging/config.yaml

# Start service
/opt/staging/fraiseql-observers start --config /opt/staging/config.yaml
```

#### 2.2 Smoke Tests

```bash
# Wait for startup (5-10 seconds)
sleep 10

# Check status
fraiseql-observers status

# Expected: "Leader: listener-1 (1/1 healthy)"
```

#### 2.3 Run Staging Integration Tests

```bash
# Create test events
# (Depends on your event generation system)

# Monitor for 30 minutes
watch -n 5 'fraiseql-observers metrics'

# Check for:
# - Events being processed
# - No errors
# - Metrics incrementing
# - Normal latency
```

#### 2.4 Staging Validation

```bash
# Verify all features
fraiseql-observers status --detailed
fraiseql-observers dlq stats
fraiseql-observers metrics | grep observer_

# Expected:
# - Status: all listeners healthy
# - DLQ: 0 items (or expected baseline)
# - Metrics: incrementing normally
```

### Production Canary (Week 1 if using gradual strategy)

#### 3.1 Deploy Canary (5% traffic)

```bash
# Option 1: Separate listener instance
# Deploy new observer listening to same database
# Configure load balancer to send 5% traffic to new instance

# Option 2: Feature flags
# Deploy to all instances but enable features on 5% of events
# Use configuration to control rollout
```

#### 3.2 Monitor Canary

```bash
# Compare metrics between canary and baseline
while true; do
  echo "=== Canary Metrics ==="
  curl -s http://canary-endpoint/metrics | grep observer_events_processed_total

  echo "=== Baseline Metrics ==="
  curl -s http://baseline-endpoint/metrics | grep observer_events_processed_total

  echo "---"
  sleep 60
done

# Look for:
# - Equivalent event throughput
# - Similar error rates
# - No anomalies
```

#### 3.3 Canary Decision

```
If metrics are healthy for 48 hours:
  ✅ Proceed to next phase

If anomalies detected:
  ⚠️  Investigate and fix
  ⚠️  Consider rollback
```

### Production Expansion (Week 2-3 if using gradual strategy)

#### 4.1 Expand to 25% Traffic

```bash
# Update load balancer or feature flag configuration
# Gradually increase traffic: 5% → 10% → 25%

# Monitor continuously
watch -n 5 'curl -s http://prod-endpoint/metrics | grep observer'
```

#### 4.2 Monitor for Issues

```bash
# Check error rates
fraiseql-observers metrics | grep failed

# Check dead letter queue
fraiseql-observers dlq stats

# Check latency
fraiseql-observers metrics | grep action_duration

# Expected: No significant increase in errors or latency
```

#### 4.3 Expansion Decision

```
If metrics are healthy for 48+ hours:
  ✅ Proceed to 50% traffic

If issues detected:
  🔧 Debug using CLI tools
  🔧 Fix and redeploy
  🔧 Restart expansion
```

### Full Production Deployment (Week 4)

#### 5.1 Deploy to 100% Traffic

```bash
# Update all production instances
# Option 1: Rolling update
for instance in prod-01 prod-02 prod-03; do
  deploy_to_instance $instance
  wait_for_health $instance
done

# Option 2: Blue-green
# Deploy to new infrastructure
# Switch traffic in one operation
switch_load_balancer_to_new_infrastructure
```

#### 5.2 Verify Full Deployment

```bash
# Check all listeners healthy
fraiseql-observers status

# Expected: "Leader: listener-X (3/3 healthy)"

# Check metrics across all instances
fraiseql-observers metrics | grep observer_events_processed_total

# Expected: High throughput, normal error rate
```

#### 5.3 Post-Deployment Monitoring

```bash
# Monitor continuously for 24+ hours
watch -n 1 'fraiseql-observers status && fraiseql-observers metrics | head -20'

# Expected:
# - Stable event processing
# - No error spikes
# - Normal latency
# - Cache hit rate (if enabled)
```

---

## Post-Deployment Verification

### Immediate Verification (First Hour)

```bash
#!/bin/bash

echo "=== Phase 8 Post-Deployment Verification ==="
echo ""

# 1. System Status
echo "1. System Status:"
fraiseql-observers status
echo ""

# 2. Event Processing
echo "2. Event Processing:"
fraiseql-observers metrics | grep "observer_events_processed_total"
echo ""

# 3. Error Rate
echo "3. Error Rate:"
fraiseql-observers metrics | grep "observer_actions_failed_total"
echo ""

# 4. Latency
echo "4. Latency (P99):"
fraiseql-observers metrics | grep "action_duration_seconds" | head -5
echo ""

# 5. Dead Letter Queue
echo "5. Dead Letter Queue:"
fraiseql-observers dlq stats
echo ""

# 6. Cache Hit Rate (if enabled)
echo "6. Cache Status:"
fraiseql-observers metrics | grep "cache_" || echo "Cache not enabled"
echo ""

echo "✅ All systems operational"
```

### Extended Verification (First 24 Hours)

Monitor these metrics:

| Metric | Expected Value | Alert Threshold |
|--------|-----------------|-----------------|
| **Event Throughput** | Baseline level | < 50% baseline |
| **Error Rate** | < 1% | > 5% |
| **Latency (P99)** | < 100ms | > 300ms |
| **Cache Hit Rate** | > 50% (if enabled) | N/A |
| **DLQ Items** | Increasing slowly | > 10% baseline |

### Feature-Specific Verification

#### Checkpoints

```bash
# Verify checkpoint saving
# 1. Note current checkpoint
psql -d your_db -c "SELECT * FROM observer_checkpoints ORDER BY created_at DESC LIMIT 1;"

# 2. Wait 1 minute
sleep 60

# 3. Verify checkpoint updated
psql -d your_db -c "SELECT * FROM observer_checkpoints ORDER BY created_at DESC LIMIT 1;"

# Expected: newer timestamp
```

#### Concurrent Execution

```bash
# Check latency improvement
fraiseql-observers metrics | grep action_duration

# Expected P99: < 100ms (5x improvement)
```

#### Deduplication

```bash
# Check dedup cache
redis-cli INFO stats | grep total_commands_processed

# Or via CLI (if implemented)
fraiseql-observers cache-stats
```

#### Caching

```bash
# Check cache hit rate
fraiseql-observers metrics | grep cache_hit

# Expected: > 50% for typical workloads
```

---

## Monitoring & Alerting

### Prometheus Metrics to Monitor

**Event Processing**:
```yaml
observer_events_processed_total       # Total events
observer_events_matched_total         # Events matched by observers
observer_actions_executed_total       # Actions executed
observer_actions_failed_total         # Failed actions
```

**Performance**:
```yaml
observer_event_processing_duration_seconds     # Event latency
observer_action_duration_seconds               # Action duration
observer_cache_hit_rate                        # Cache effectiveness
```

**System Health**:
```yaml
observer_dlq_items_total              # DLQ size
observer_listener_health              # Listener health (1=healthy, 0=down)
observer_checkpoint_age_seconds       # Time since last checkpoint
```

### Alert Rules

**Critical Alerts** (immediate action):

```yaml
- name: HighErrorRate
  expr: observer_actions_failed_total / observer_actions_executed_total > 0.05
  for: 5m
  action: page_on_call

- name: ListenerDown
  expr: observer_listener_health == 0
  for: 1m
  action: page_on_call

- name: DLQExploding
  expr: rate(observer_dlq_items_total[5m]) > 100
  for: 5m
  action: page_on_call
```

**Warning Alerts** (notify team):

```yaml
- name: ElevatedLatency
  expr: observer_event_processing_duration_seconds > 0.5
  for: 10m
  action: notify_slack

- name: LowCacheHitRate
  expr: observer_cache_hit_rate < 0.3
  for: 15m
  action: notify_slack

- name: OldCheckpoint
  expr: observer_checkpoint_age_seconds > 300
  for: 5m
  action: notify_slack
```

### Grafana Dashboard

Import the provided Grafana dashboard for visualization:

```bash
# Copy dashboard to Grafana
curl -X POST \
  http://grafana:3000/api/dashboards/db \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $GRAFANA_TOKEN" \
  -d @grafana-dashboard-phase-8.json
```

---

## Rollback Procedures

### Quick Rollback (< 5 minutes)

If critical issues detected within first hour:

```bash
#!/bin/bash

echo "=== EMERGENCY ROLLBACK ==="

# 1. Stop Phase 8 listeners
kill $(pgrep -f "fraiseql-observers")

# 2. Revert to Phase 1-7 binary
cp /opt/backup/fraiseql-observers-phase-7 /opt/fraiseql-observers

# 3. Restart with old binary
/opt/fraiseql-observers start --config config.yaml

# 4. Verify rollback
fraiseql-observers status

echo "Rollback complete - Phase 1-7 restored"
```

### Feature-Level Rollback

If specific feature causes issues:

```bash
# 1. Edit configuration to disable problematic feature
# Example: disable checkpoints if causing issues
cat > config.yaml << EOF
observers:
  checkpoints:
    enabled: false  # Disable this feature
  concurrent_execution:
    enabled: true   # Keep others enabled
EOF

# 2. Validate new config
fraiseql-observers validate-config config.yaml

# 3. Restart with new config
kill $(pgrep -f "fraiseql-observers")
/opt/fraiseql-observers start --config config.yaml

# 4. Verify
fraiseql-observers status
```

### Database Rollback (if needed)

If database schema changes cause issues:

```bash
# 1. Stop observers
systemctl stop fraiseql-observers

# 2. Restore database backup
# (Your backup procedure)

# 3. Restart observers
systemctl start fraiseql-observers
```

---

## Day-2 Operations

### Daily Health Check

```bash
#!/bin/bash

# Run daily health checks
echo "=== Daily Health Check ==="

# 1. Check listener status
echo "Listeners:"
fraiseql-observers status

# 2. Check error rate
echo "Error rate (should be < 1%):"
fraiseql-observers metrics | grep observer_actions_failed_total

# 3. Check DLQ
echo "Dead letter queue:"
fraiseql-observers dlq stats

# 4. Check performance
echo "Latency (P99 should be < 100ms):"
fraiseql-observers metrics | grep action_duration | grep p99

echo "✅ Health check complete"
```

### Weekly Optimization Review

```bash
# 1. Review metrics
# - Are we hitting performance targets?
# - What's the cache hit rate?
# - Are there patterns in failures?

# 2. Check configuration
# - Are batch sizes optimal?
# - Are timeouts appropriate?
# - Is worker pool sized correctly?

# 3. Review logs
# - Any unexpected errors?
# - Are retry policies working?
# - Any performance anomalies?

# 4. Adjust as needed
# - Tune batch sizes
# - Adjust worker pool
# - Update retry policies
```

### Monthly Disaster Recovery Test

```bash
# 1. Simulate listener failure
# Kill primary listener and verify failover

# 2. Verify checkpoint recovery
# Stop and restart listener, verify resume point

# 3. Test DLQ recovery
# Trigger failures and verify retry mechanisms

# 4. Verify backups
# Restore from backup and verify integrity

# Document any issues found
```

---

## Troubleshooting Deployment Issues

### Issue: Deployment Hangs

```bash
# 1. Check listener status
fraiseql-observers status

# 2. Check logs
journalctl -u fraiseql-observers -f

# 3. Check database connectivity
psql -d your_db -c "LISTEN test_event;"

# 4. Check Redis connectivity (if enabled)
redis-cli ping
```

### Issue: High Error Rate Post-Deployment

```bash
# 1. Check DLQ
fraiseql-observers dlq list --limit 20

# 2. Examine specific error
fraiseql-observers dlq show dlq-001

# 3. Review recent events
fraiseql-observers debug-event --history 10

# 4. Check configuration
fraiseql-observers validate-config --detailed
```

### Issue: Performance Degradation

```bash
# 1. Check cache hit rate
fraiseql-observers metrics | grep cache_hit

# 2. Check for slow queries
# Examine PostgreSQL slow query log

# 3. Check CPU/memory usage
top -p $(pgrep -f fraiseql-observers)

# 4. Examine latency distribution
fraiseql-observers metrics | grep action_duration
```

---

## Next Steps

1. **Review**: Share this guide with ops team
2. **Plan**: Schedule deployment window
3. **Prepare**: Run through checklist
4. **Execute**: Follow step-by-step deployment
5. **Monitor**: Watch metrics for 24 hours
6. **Celebrate**: Phase 8 is in production! 🎉

---

## Support & Help

- **Documentation**: `docs/ARCHITECTURE_PHASE_8.md`
- **Troubleshooting**: `docs/TROUBLESHOOTING.md`
- **CLI Reference**: `docs/CLI_TOOLS.md`
- **Performance**: `docs/PERFORMANCE_TUNING.md`

---

**Deployment Guide Version**: 1.0
**Last Updated**: January 22, 2026
**Status**: Ready for Production

