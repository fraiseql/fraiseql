<!-- Skip to main content -->
---
title: Example: Production Deployment at Scale
description: This example demonstrates deploying observability in a **high-traffic production environment** with best practices for monitoring, maintenance, and continuous o
keywords: ["deployment", "scaling", "code", "fullstack", "production", "performance", "monitoring", "troubleshooting"]
tags: ["documentation", "reference"]
---

# Example: Production Deployment at Scale

## Overview

This example demonstrates deploying observability in a **high-traffic production environment** with best practices for monitoring, maintenance, and continuous optimization.

**Scenario**: SaaS platform with 10,000 qps, multi-region deployment

**Scale**: 50M queries/day, 100GB database

---

## Production Architecture

```text
<!-- Code example in TEXT -->
┌──────────────────────────────────────────────────────────┐
│  Load Balancer (10,000 qps)                              │
└────────────────┬─────────────────────────────────────────┘
                 │
        ┌────────┴────────┐
        ↓                 ↓
┌──────────────┐  ┌──────────────┐
│  FraiseQL    │  │  FraiseQL    │
│  API (x4)    │  │  API (x4)    │
│  us-east-1   │  │  us-west-2   │
└──────┬───────┘  └──────┬───────┘
       │                  │
       ↓                  ↓
┌──────────────────────────────────┐
│  PostgreSQL Primary (us-east-1)  │
│  Read Replicas (2x)              │
└──────────────┬───────────────────┘
               │
               ↓
┌──────────────────────────────────┐
│  Metrics DB (Separate Instance)  │
│  Time-series optimized           │
└──────────────────────────────────┘
```text
<!-- Code example in TEXT -->

---

## Configuration

### High-Traffic Settings

```toml
<!-- Code example in TOML -->
[observability]
enabled = true
sample_rate = 0.01  # 1% sampling (500K queries/day sampled)
retention_days = 14  # Shorter retention due to volume

[observability.metrics]
buffer_size = 500
flush_interval_secs = 30
batch_size = 1000  # Large batches for efficiency

[observability.database]
# Dedicated metrics instance
url = "postgres://metrics:pass@metrics-db.internal:5432/metrics"
pool_size = 50  # High concurrency
timeout_secs = 10  # Fail fast

[observability.analysis]
min_frequency = 10000  # Only high-traffic patterns
min_speedup = 10.0     # Only major wins
```text
<!-- Code example in TEXT -->

---

## Monitoring & Alerting

### Key Metrics

```yaml
<!-- Code example in YAML -->
# Prometheus alerts

- alert: ObservabilityMetricsLag
  expr: time() - max(fraiseql_metrics_last_write) > 300
  annotations:
    summary: "Metrics collection lagging > 5 minutes"

- alert: ObservabilityBufferFull
  expr: fraiseql_metrics_buffer_size / fraiseql_metrics_buffer_capacity > 0.9
  annotations:
    summary: "Metrics buffer 90% full"

- alert: OptimizationOpportunity
  expr: fraiseql_analysis_suggestions_high_priority > 0
  annotations:
    summary: "New high-priority optimizations available"
```text
<!-- Code example in TEXT -->

### Dashboards

- **Metrics Collection Health**: Write rate, lag, buffer size
- **Query Performance**: p50/p95/p99 by query type
- **Storage Growth**: Metrics table sizes, retention cleanup
- **Optimization Impact**: Before/after comparison

---

## Weekly Analysis Workflow

### Automated Analysis

```bash
<!-- Code example in BASH -->
#!/bin/bash
# weekly-analysis.sh (runs every Monday 2 AM)

REPORT_DIR="/var/reports/FraiseQL/$(date +%Y-%m)"
mkdir -p $REPORT_DIR

# Run analysis
FraiseQL-cli analyze \
  --database postgres://metrics-db:5432/metrics \
  --format json > $REPORT_DIR/analysis-$(date +%Y%m%d).json

# Generate SQL migrations
FraiseQL-cli analyze \
  --database postgres://metrics-db:5432/metrics \
  --format sql > $REPORT_DIR/migrations-$(date +%Y%m%d).sql

# Count high-priority suggestions
HIGH_PRIORITY=$(jq '.suggestions | map(select(.priority == "high" or .priority == "critical")) | length' \
  $REPORT_DIR/analysis-$(date +%Y%m%d).json)

# Alert if suggestions exist
if [ $HIGH_PRIORITY -gt 0 ]; then
  echo "⚠️  $HIGH_PRIORITY high-priority optimization opportunities found" | \
    slack-notify --channel=#database-ops
  
  # Attach report
  slack-upload $REPORT_DIR/analysis-$(date +%Y%m%d).json --channel=#database-ops
fi
```text
<!-- Code example in TEXT -->

---

## Migration Strategy

### Blue-Green Deployment

1. **Deploy to Green Environment**
   - Apply migrations to green database
   - Deploy updated schema to green servers
   - Run smoke tests

2. **Gradual Traffic Shift**
   - Route 10% traffic to green → monitor 1 hour
   - Route 50% traffic to green → monitor 1 hour
   - Route 100% traffic to green → monitor 24 hours

3. **Rollback Plan**
   - Keep blue environment running for 48 hours
   - One-click rollback if issues detected

---

## Cost Analysis

### Monthly Costs (AWS us-east-1)

| Resource | Configuration | Monthly Cost |
|----------|---------------|--------------|
| Metrics Database | db.r5.xlarge (4 vCPU, 32GB RAM) | $580 |
| Metrics Storage | 500 GB SSD | $50 |
| Data Transfer | 100 GB/month | $9 |
| Backup Storage | 500 GB | $24 |
| **Total** | | **$663/month** |

### ROI Calculation

**Benefits**:

- 5 major optimizations applied in 3 months
- Average 10x speedup per optimization
- Reduced server count from 8 to 4 (due to efficiency)
- **Savings**: $2,400/month in server costs

**Net Savings**: $2,400 - $663 = **$1,737/month**
**Annual ROI**: $20,844/year

---

## Maintenance Tasks

### Daily

- ✅ Check metrics collection health
- ✅ Monitor buffer/lag alerts
- ✅ Review error logs

### Weekly

- ✅ Run automated analysis
- ✅ Review optimization suggestions
- ✅ Plan migrations for next sprint

### Monthly

- ✅ Review retention policy
- ✅ Optimize metrics table indexes
- ✅ Archive old analysis reports
- ✅ Update cost estimates

### Quarterly

- ✅ Major optimization sprint
- ✅ Review overall performance trends
- ✅ Adjust sampling rates if needed
- ✅ Database capacity planning

---

## Security Considerations

### Access Control

```sql
<!-- Code example in SQL -->
-- Metrics database: Separate users
CREATE USER fraiseql_collector WITH PASSWORD '...';
GRANT INSERT ON fraiseql_metrics.* TO fraiseql_collector;

CREATE USER fraiseql_analyst WITH PASSWORD '...';
GRANT SELECT ON fraiseql_metrics.* TO fraiseql_analyst;

-- No access to application database from metrics service
REVOKE ALL ON public.* FROM fraiseql_collector;
```text
<!-- Code example in TEXT -->

### Network Isolation

```yaml
<!-- Code example in YAML -->
# Kubernetes NetworkPolicy
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: FraiseQL-metrics-egress
spec:
  podSelector:
    matchLabels:
      app: FraiseQL-api
  policyTypes:
  - Egress
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: metrics-db
    ports:
    - protocol: TCP
      port: 5432
```text
<!-- Code example in TEXT -->

---

## Lessons Learned

### What Worked Well

1. **Dedicated metrics database** - Isolated observability overhead
2. **1% sampling** - Sufficient for pattern detection at scale
3. **Automated weekly analysis** - Continuous improvement
4. **Blue-green deployments** - Zero-downtime migrations

### Challenges

1. **Initial storage costs** - Solved with aggressive retention (14 days)
2. **Metrics lag during peak** - Increased buffer size and batch writes
3. **Analysis performance** - Added indexes to metrics tables

---

## Next Steps

- **Implement materialized views** for expensive aggregates
- **Add multi-region metrics collection** for global deployments
- **Integrate with DataDog/Grafana** for unified monitoring
- **Machine learning for predictive optimization**

---

*Last updated: 2026-01-12*
