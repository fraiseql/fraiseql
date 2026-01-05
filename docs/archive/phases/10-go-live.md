# Federation Go-Live Strategy

## Executive Summary

This document outlines the production deployment strategy for Federation, covering:
- Canary rollout procedures
- Real-time monitoring and metrics
- Automated health checks
- Rollback strategies
- Long-term operational practices

---

## Go-Live Overview

### Timeline

```
Day 1: Canary Phase (5% → 25% → 50%)
Day 2: Gradual Rollout (50% → 75% → 100%)
Days 3-7: Intensive Monitoring
Weeks 2-4: Continuous Optimization
```

### Success Criteria

**Immediate (Day 1):**
- ✅ Canary deployment stable (error rate < 1%)
- ✅ No memory leaks detected
- ✅ Batch effectiveness > 80%
- ✅ Queries reduced by 80%

**Week 1:**
- ✅ All services federation-enabled
- ✅ Performance baseline confirmed
- ✅ No regressions from Week 5
- ✅ Team confident in operations

**Month 1:**
- ✅ Federation stable in production
- ✅ Operational runbooks updated
- ✅ Support team trained
- ✅ Cost savings validated

---

## Canary Rollout Strategy

### Phase 1: Canary (5% Traffic) - 15 minutes

**Deployment:**
```bash
# 1. Deploy to canary infrastructure
kubectl apply -f k8s/federation-canary-v1.0.0.yaml

# 2. Verify canary is healthy
kubectl get pods -l app=user-service-canary
kubectl logs -f deployment/user-service-canary

# 3. Route 5% traffic via load balancer
kubectl patch service user-service -p \
  '{"spec":{"sessionAffinity":"ClientIP"}}'
# Set canary weight = 5, production weight = 95
```

**Monitoring (every 2 minutes):**
```bash
#!/bin/bash
# Canary health check script

while true; do
  echo "=== Canary Health Check ==="

  # Error rate
  ERROR_RATE=$(curl -s http://prometheus:9090/api/v1/query \
    --data-urlencode 'query=rate(http_requests_total{pod=~"user-service-canary.*"}[5m])' | \
    jq '.data.result[0].value[1]')
  echo "Error rate: ${ERROR_RATE}%"

  # Latency p99
  LATENCY=$(curl -s http://prometheus:9090/api/v1/query \
    --data-urlencode 'query=histogram_quantile(0.99, http_request_duration_seconds{pod=~"user-service-canary.*"})' | \
    jq '.data.result[0].value[1]')
  echo "P99 Latency: ${LATENCY}ms"

  # Memory
  MEMORY=$(kubectl top pod -l app=user-service-canary | tail -1 | awk '{print $3}')
  echo "Memory: ${MEMORY}Mi"

  # Batch effectiveness
  BATCH_RATE=$(curl -s http://localhost:4001/metrics | grep federation_batch_effectiveness | tail -1)
  echo "${BATCH_RATE}"

  if [ "$ERROR_RATE" -gt 1 ]; then
    echo "❌ ERROR RATE TOO HIGH - INITIATING ROLLBACK"
    exit 1
  fi

  sleep 120
done
```

**Canary Success Criteria:**
- ✅ Error rate: < 1%
- ✅ P99 latency: < 500ms
- ✅ Memory: < 500MB
- ✅ Batch effectiveness: > 80%

**Decision Point:**
- ✅ Healthy? → Proceed to Phase 2
- ❌ Issues? → Automatic rollback (see rollback section)

### Phase 2: Early Adopters (25% Traffic) - 30 minutes

**Deployment:**
```bash
# Update load balancer weights
# Canary: 25%, Production: 75%

# Monitor same metrics for 30 minutes
./canary-health-check.sh
```

**Additional Checks:**
```bash
# Verify database query patterns changed
SELECT operation, COUNT(*) as count FROM federation_operations
  WHERE timestamp > now() - interval '30 minutes'
  GROUP BY operation;

# Expected:
# - federation_batch: HIGH count (new)
# - federation_direct: LOW count (old - should decrease)
```

**Success Criteria:**
- ✅ Error rate: < 1%
- ✅ P99 latency: < 500ms
- ✅ Database query count down 80%
- ✅ No memory growth over 30 minutes

### Phase 3: Expansion (50% Traffic) - 30 minutes

**Deployment:**
```bash
# Update load balancer weights
# Canary: 50%, Production: 50%

# Monitor for 30 minutes
./canary-health-check.sh
```

**Business Metrics Check:**
```bash
# Are customers experiencing better performance?
SELECT
  COUNT(*) as total_requests,
  AVG(response_time_ms) as avg_latency,
  PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY response_time_ms) as p99_latency,
  SUM(CASE WHEN response_time_ms > 1000 THEN 1 ELSE 0 END) as slow_requests
FROM request_log
WHERE timestamp > now() - interval '30 minutes'
  AND routed_to = 'federation';
```

**Decision Point:**
- ✅ All metrics healthy? → Proceed to Phase 4
- ⚠️  Minor issues? → Hold at 50% for extended monitoring
- ❌ Major issues? → Rollback

### Phase 4: Full Rollout (100% Traffic) - Complete

**Deployment:**
```bash
# Option 1: Gradual (recommended)
# Update load balancer weights in 10% increments
# 60% → 70% → 80% → 90% → 100%
# 10 minutes between each increment

# Option 2: Immediate (if high confidence)
# Set canary weight = 100%, production weight = 0%

# Monitor continuously
./canary-health-check.sh
```

**Verify All Services Updated:**
```bash
# Check that all instances are running v1.0.0
kubectl get pods -o custom-columns=NAME:.metadata.name,IMAGE:.spec.containers[].image

# Expected: All showing federation:v1.0.0
```

**Final Validation:**
```bash
# Run comprehensive test suite against production
pytest tests/federation/integration/ --prod-endpoints

# Expected: All tests pass with production performance
```

---

## Real-Time Monitoring

### Key Metrics Dashboard

**Prometheus Queries:**

```promql
# Federation batch effectiveness (should be > 80%)
rate(federation_batched_queries[5m]) / rate(federation_total_queries[5m])

# Error rate (should be < 1%)
rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m])

# P99 latency (should be < 500ms)
histogram_quantile(0.99, http_request_duration_seconds)

# Memory usage (should be stable, < 1GB)
container_memory_usage_bytes{pod=~"user-service.*"}

# DataLoader batch sizes (should be > 50)
rate(federation_batch_size_sum[5m]) / rate(federation_batch_size_count[5m])

# Entity resolution time (should be < 100ms)
histogram_quantile(0.99, federation_entity_resolution_duration_seconds)
```

### Grafana Dashboard Setup

```yaml
# Create dashboard in Grafana
POST /api/dashboards/db

dashboard:
  title: "Federation Go-Live"
  panels:
    - title: "Error Rate"
      query: "rate(errors_total[5m])"
      thresholds:
        - {value: 0.01, color: "red"}     # > 1% is critical
        - {value: 0.005, color: "yellow"} # > 0.5% is warning

    - title: "P99 Latency"
      query: "histogram_quantile(0.99, duration)"
      thresholds:
        - {value: 2000, color: "red"}     # > 2s is critical
        - {value: 1000, color: "yellow"}  # > 1s is warning

    - title: "Batch Effectiveness"
      query: "federation_batch_rate"
      thresholds:
        - {value: 0.80, color: "green"}   # > 80% is expected
        - {value: 0.60, color: "red"}     # < 60% means issue

    - title: "Memory Usage"
      query: "container_memory_bytes"
      thresholds:
        - {value: 1000, color: "red"}     # > 1GB is critical
```

### Alert Rules

```yaml
# Prometheus alerting rules (alert_rules.yml)

groups:
  - name: federation_alerts
    interval: 30s
    rules:
      - alert: FederationHighErrorRate
        expr: 'rate(errors_total[5m]) > 0.05'
        for: 5m
        annotations:
          summary: "Federation error rate > 5%"
          action: "Check federation logs, consider rollback"

      - alert: FederationHighLatency
        expr: 'histogram_quantile(0.99, duration) > 2'
        for: 5m
        annotations:
          summary: "Federation P99 latency > 2s"
          action: "Check database performance, consider rollback"

      - alert: FederationMemoryLeak
        expr: 'increase(memory_bytes[1h]) > 100000000'  # 100MB in 1 hour
        for: 5m
        annotations:
          summary: "Federation memory growing (possible leak)"
          action: "Review memory profiling, consider rollback"

      - alert: FederationBatchingIneffective
        expr: 'rate(federation_batched[5m]) / rate(federation_total[5m]) < 0.6'
        for: 10m
        annotations:
          summary: "Federation batching effectiveness < 60%"
          action: "Check DataLoader configuration, review logs"
```

### Automated Rollback Triggers

```python
class FederationHealthMonitor:
    """Automatically rolls back federation if health degrades."""

    async def check_health(self) -> HealthStatus:
        """Check federation health and auto-rollback if needed."""

        metrics = await self.get_metrics()

        # Check error rate
        if metrics['error_rate'] > 0.05:  # > 5% error rate
            await self.trigger_rollback(
                reason="Error rate exceeded threshold",
                error_rate=metrics['error_rate']
            )
            return HealthStatus.CRITICAL

        # Check latency
        if metrics['p99_latency'] > 2.0:  # > 2 seconds
            await self.trigger_rollback(
                reason="P99 latency exceeded threshold",
                latency=metrics['p99_latency']
            )
            return HealthStatus.CRITICAL

        # Check memory
        if metrics['memory_growth_rate'] > 100_000_000:  # 100MB/hour
            await self.trigger_rollback(
                reason="Memory growth rate indicates leak",
                growth_rate=metrics['memory_growth_rate']
            )
            return HealthStatus.CRITICAL

        # Check batching effectiveness
        if metrics['batch_effectiveness'] < 0.60:  # < 60%
            logger.warning(
                "Batching effectiveness below target",
                effectiveness=metrics['batch_effectiveness']
            )
            return HealthStatus.DEGRADED

        return HealthStatus.HEALTHY
```

---

## Monitoring Best Practices

### Log Aggregation

**Logs to collect:**
```yaml
# Application logs
federation_service.log:
  - Field: timestamp
  - Field: request_id (for tracing)
  - Field: operation (batch_load, entity_resolve, etc.)
  - Field: duration_ms
  - Field: entity_count
  - Field: batch_size
  - Field: error (if any)

# Example log entry
{
  "timestamp": "2025-01-02T10:30:45Z",
  "request_id": "req-12345",
  "operation": "federation_batch_load",
  "typename": "User",
  "batch_size": 42,
  "duration_ms": 15,
  "entity_count": 42,
  "cached_count": 5
}
```

**Log queries:**
```bash
# Find slow federation operations
field operation=federation_batch_load AND duration_ms > 100
| stats count() as slow_operations

# Find batching inefficiencies
field batch_size < 10 AND !cached
| stats avg(batch_size) as avg_batch_size

# Monitor error distribution
field error != NULL
| stats count() as total_errors by error_type
```

### Tracing Across Services

**Use distributed tracing (Jaeger/Zipkin):**
```python
# In federation code
from jaeger_client import Config

jaeger_config = Config(
    config={
        'sampler': {'type': 'const', 'param': 1},
        'logging': True,
    },
    service_name='federation-service',
)

tracer = jaeger_config.initialize_tracer()

# Trace federation operations
with tracer.start_active_span('federation_batch_load') as scope:
    span = scope.span
    span.set_tag('typename', 'User')
    span.set_tag('batch_size', len(requests))

    result = await loader.load_batch(requests)

    span.set_tag('duration_ms', result.duration)
```

**Benefits:**
- ✅ See full request path across services
- ✅ Identify slow operations
- ✅ Debug cross-service issues
- ✅ Validate batching is working

---

## Operational Runbook

### Daily Operations

**Morning Standup Checklist:**
```bash
#!/bin/bash
# Run each morning after go-live

echo "=== Federation Status Check ==="

# 1. Service health
echo "1. Service Health"
kubectl get pods -l app=user-service
kubectl get pods -l app=post-service
kubectl get pods -l app=product-service

# 2. Error rate
echo "2. Error Rate (last 24h)"
curl -s "http://prometheus:9090/api/v1/query?query=\
  rate(errors_total[24h])" | jq '.data.result[0].value[1]'

# 3. Performance metrics
echo "3. Performance Metrics"
curl -s http://prometheus:9090/api/v1/query?query=\
  avg(http_request_duration_seconds) | jq '.data.result[0].value[1]'

# 4. Federation batching
echo "4. Federation Batching Effectiveness"
curl -s http://prometheus:9090/api/v1/query?query=\
  rate(federation_batched_queries[24h]) | jq '.data.result[0].value[1]'

# 5. Recent errors
echo "5. Recent Errors (last 6h)"
kubectl logs -l app=user-service --since=6h | grep ERROR | head -10

# 6. Database health
echo "6. Database Connection Pool"
psql -h $DB_HOST -d $DB_NAME -c \
  "SELECT count(*) as active_connections FROM pg_stat_activity;"
```

### Common Issues and Responses

**Issue 1: Error Rate Spiking**

```bash
# Immediate response
1. Check federation logs
   kubectl logs -f deployment/user-service | grep ERROR

2. Check database connectivity
   psql -h $DB_HOST -c "SELECT 1"

3. Check DataLoader health
   curl http://localhost:4001/debug/dataloader

4. If critical: Trigger rollback
   kubectl patch deployment/user-service -p \
     '{"spec":{"template":{"spec":{"env":[{"name":"FEDERATION_ENABLED","value":"false"}]}}}}'
```

**Issue 2: Memory Leak Detected**

```bash
# Memory continuously growing
1. Check for unbounded caches
   curl http://localhost:4001/debug/cache_stats

2. Generate heap dump
   jmap -dump:live,format=b,file=heap.bin $PID

3. Analyze heap dump
   python -m memory_profiler heap.bin

4. If critical: Trigger rollback
   ./rollback.sh
```

**Issue 3: Batching Ineffective**

```bash
# Batch effectiveness < 60%
1. Check batch sizes
   curl http://localhost:4001/debug/federation | jq '.batch_stats'

2. Check query patterns
   tail -100 /var/log/federation.log | grep batch_size

3. Review DataLoader configuration
   # Check loader.max_batch_size setting

4. If misconfigured: Update and redeploy
   # No rollback needed, just config update
```

---

## Long-Term Operational Practices

### Weekly Reviews

**Every Friday (30 minutes):**
```yaml
1. Review metrics trends
   - Error rate trend (should be stable or decreasing)
   - Latency trend (should be consistent)
   - Memory trend (should be flat)
   - Batching effectiveness (should be > 80%)

2. Review errors from past week
   - Any patterns?
   - Any recurring issues?
   - Any action items?

3. Performance benchmarks
   - Compare with Week 5 baseline
   - Expected: 7-10x improvement maintained
   - Any regressions?

4. Cost analysis
   - Database cost savings achieved?
   - Expected 40-60% reduction
   - Any unexpected costs?

5. Team feedback
   - Any operational challenges?
   - Any training needs?
   - Update runbooks if needed
```

### Monthly Reviews

**End of each month:**
```yaml
1. Business impact
   - Revenue/cost metrics
   - Customer satisfaction scores
   - Performance improvements vs baseline

2. Technical health
   - Uptime: Target 99.95%
   - Error rate: Should be < 0.1%
   - Performance: 7-10x improvement vs baseline

3. Operational maturity
   - Are runbooks up to date?
   - Is team confident with operations?
   - Any gaps in monitoring?

4. Planning next month
   - What optimizations to attempt?
   - Any additional services to federate?
   - Any process improvements?
```

### Quarterly Architecture Review

**Every quarter:**
```yaml
1. Federation architecture assessment
   - Are we hitting expected performance?
   - Are there bottlenecks?
   - Should we optimize further?

2. Scaling analysis
   - How many entities can we handle?
   - What's the batch size limit?
   - Do we need infrastructure changes?

3. Feature gaps
   - Are there features needed?
   - Any missing observability?
   - Any reliability improvements?

4. Planning next quarter
   - What services to federate next?
   - What improvements to implement?
   - What resources needed?
```

---

## Success Metrics

### Immediate (Day 1)

| Metric | Target | Actual |
|--------|--------|--------|
| Error Rate | < 1% | ____ |
| P99 Latency | < 500ms | ____ |
| Memory Usage | < 1GB | ____ |
| Batch Effectiveness | > 80% | ____ |

### Week 1

| Metric | Baseline | Target | Actual |
|--------|----------|--------|--------|
| Query Count | 100 | 5-20 | ____ |
| Response Time | 1000ms | 100-150ms | ____ |
| DB Load | 100% | 20-30% | ____ |
| Error Rate | N/A | < 0.5% | ____ |

### Month 1

| Metric | Target | Actual |
|--------|--------|--------|
| Uptime | 99.95% | ____ |
| Cost Savings | 40-60% | ____ |
| Customer Satisfaction | +20% | ____ |
| Operational Incidents | < 5 | ____ |

---

## Summary

**Go-Live Strategy:**
1. ✅ Canary phases with automatic health checks
2. ✅ Real-time monitoring with alerting
3. ✅ Automated rollback triggers
4. ✅ Operational runbooks
5. ✅ Weekly/monthly review cadences

**Key Success Factors:**
- Continuous monitoring (don't assume things are OK)
- Automated rollbacks (fast recovery if issues arise)
- Team training (everyone understands federation)
- Operational discipline (follow runbooks)
- Regular reviews (catch issues early)

**Next Steps:**
1. Set up Prometheus/Grafana dashboards
2. Configure alert rules and auto-rollback
3. Test rollback procedures in staging
4. Brief team on monitoring procedures
5. Execute go-live per canary phases

---

## References

- [09-release-checklist.md](09-release-checklist.md) - Pre-deployment checklist
- [08-migration-guide.md](08-migration-guide.md) - Service migration
- [07-testing-guide.md](07-testing-guide.md) - Test patterns
- [06-observability.md](06-observability.md) - Metrics and monitoring
