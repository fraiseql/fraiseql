# DevOps Engineer Journey - Deploy FraiseQL to Production

**Time to Complete:** 4 hours
**Prerequisites:** Kubernetes/Docker experience, PostgreSQL operations, monitoring/observability knowledge
**Goal:** Deploy and operate FraiseQL in production with comprehensive monitoring and reliability

## Overview

As a DevOps engineer, you're responsible for deploying, monitoring, and maintaining FraiseQL applications in production. This journey covers deployment patterns, observability setup, incident response, and operational best practices.

By the end of this journey, you'll have:
- Production-ready deployment configurations
- Complete observability stack (metrics, logs, traces)
- Health checks and readiness probes configured
- Incident response runbooks
- Scaling and reliability patterns
- Backup and disaster recovery procedures

## Step-by-Step Deployment

### Step 1: Production Architecture Overview (30 minutes)

**Goal:** Understand FraiseQL's deployment architecture

**Read:** [Production Deployment Guide](../production/deployment.md)

**Production Architecture:**
```
┌─────────────────────────────────────────────────────────┐
│                     Load Balancer                        │
│              (NGINX / ALB / GCP LB)                      │
└────────────┬────────────────────────┬───────────────────┘
             │                        │
       ┌─────▼─────┐            ┌─────▼─────┐
       │ FraiseQL  │            │ FraiseQL  │  (3+ replicas)
       │ Pod 1     │            │ Pod 2     │
       └─────┬─────┘            └─────┬─────┘
             │                        │
             └────────┬───────────────┘
                      │
             ┌────────▼────────┐
             │   PostgreSQL    │
             │  (Primary +     │
             │   Read Replicas)│
             └─────────────────┘
                      │
             ┌────────▼────────┐
             │ Observability   │
             │ - Prometheus    │
             │ - Grafana       │
             │ - Loki          │
             └─────────────────┘
```

**Key Components:**
- **FraiseQL Pods:** Stateless Python application (3+ replicas)
- **PostgreSQL:** Primary for writes, read replicas for queries
- **Connection Pooling:** PgBouncer between FraiseQL and PostgreSQL
- **Monitoring:** Prometheus + Grafana + Loki stack

**Success Check:** You understand the production architecture and dependencies

### Step 2: Pre-Deployment Checklist (45 minutes)

**Goal:** Validate readiness for production deployment

**Follow:** [Production Deployment Checklist](../production/deployment-checklist.md)

**Essential Checks:**

#### Security & Compliance
- [ ] Security profile configured (STANDARD/REGULATED/RESTRICTED)
- [ ] HTTPS enforced (no HTTP allowed)
- [ ] Database credentials rotated
- [ ] KMS integration tested (if using REGULATED/RESTRICTED)
- [ ] Audit logging enabled and tested
- [ ] SLSA provenance verified (for compliance)

#### Database
- [ ] Connection pooling configured (20-50 connections per pod)
- [ ] Database backups automated (RTO/RPO acceptable)
- [ ] Views (v_*) created and tested
- [ ] Indexes on high-traffic tables
- [ ] Query performance tested (pg_stat_statements reviewed)
- [ ] Read replicas configured (for read-heavy workloads)

#### Application Configuration
- [ ] Environment variables secured (Kubernetes secrets)
- [ ] Resource limits set (CPU/memory)
- [ ] Health checks configured (/health endpoint)
- [ ] Readiness probes configured (database connectivity)
- [ ] Liveness probes configured (process health)

#### Observability
- [ ] Prometheus metrics endpoint enabled
- [ ] Grafana dashboards configured
- [ ] Loki (or equivalent) for log aggregation
- [ ] Alerts configured (error rate, latency, DB connection pool)
- [ ] Distributed tracing enabled (OpenTelemetry)

**Example Configuration:**
```yaml
# kubernetes/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secrets
              key: url
        - name: DATABASE_POOL_SIZE
          value: "20"
        - name: DATABASE_POOL_MAX_OVERFLOW
          value: "10"
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
```

> **Note:** The `/ready` endpoint checks database connectivity and application readiness. Use `/health` for liveness probes (process running) and `/ready` for readiness probes (ready to serve traffic).

**Success Check:** All checklist items are completed

### Step 3: Database Operations Setup (40 minutes)

**Goal:** Configure PostgreSQL for production workloads

**Read:** [Database Configuration](../database/table-naming-conventions.md)

**PostgreSQL Production Configuration:**

**1. Connection Pooling with PgBouncer:**
```ini
# pgbouncer.ini
[databases]
fraiseql = host=postgres port=5432 dbname=fraiseql

[pgbouncer]
pool_mode = transaction
max_client_conn = 200
default_pool_size = 25
reserve_pool_size = 5
```

**Why PgBouncer?**
- Reduces PostgreSQL connection overhead
- Handles 200+ clients with 25 actual DB connections
- Transaction pooling for stateless queries

**2. PostgreSQL Configuration:**
```conf
# postgresql.conf
max_connections = 100
shared_buffers = 4GB
effective_cache_size = 12GB
work_mem = 64MB
maintenance_work_mem = 1GB

# Logging for observability
log_min_duration_statement = 1000  # Log slow queries (>1s)
log_line_prefix = '%t [%p]: [%l-1] db=%d,user=%u,app=%a,client=%h '
```

**3. Read Replica Configuration:**
```yaml
# kubernetes/postgres-replica.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres-replica
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: postgres
        image: postgres:16
        env:
        - name: POSTGRES_PRIMARY_HOST
          value: postgres-primary
        - name: POSTGRES_REPLICATION_MODE
          value: slave
```

**Connection String Routing:**
```python
# FraiseQL configuration
from fraiseql import create_fraiseql_app

app = create_fraiseql_app(
    database_url_write="postgresql://pgbouncer:6432/fraiseql",  # Primary
    database_url_read="postgresql://pgbouncer-replica:6432/fraiseql",  # Replicas
    pool_size=20,
    pool_max_overflow=10
)
```

> **Note:** Explicit connection pooling parameters (pool_size, pool_max_overflow) are planned for `create_fraiseql_app()` in WP-027.

**4. Backup Configuration:**
```bash
# Automated backups with pg_dump
#!/bin/bash
# backup-postgres.sh
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
pg_dump -h postgres-primary -U fraiseql -Fc fraiseql > /backups/fraiseql_$TIMESTAMP.dump

# Retention: Keep 7 daily, 4 weekly, 12 monthly
find /backups -name "fraiseql_*.dump" -mtime +7 -delete
```

**Kubernetes CronJob:**
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: postgres-backup
spec:
  schedule: "0 2 * * *"  # 2 AM daily
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: postgres:16
            command: ["/bin/bash", "/scripts/backup-postgres.sh"]
```

**Success Check:** Database is configured with pooling, replicas, and backups

### Step 4: Observability Stack Setup (50 minutes)

**Goal:** Deploy comprehensive monitoring and logging

**Read:** [Observability Guide](../production/observability.md)

**Observability Stack:**

**1. Prometheus Metrics:**

FraiseQL exposes Prometheus metrics at `/metrics`:

```python
# Exposed metrics
fraiseql_requests_total{method="POST", endpoint="/graphql", status="200"}
fraiseql_request_duration_seconds{method="POST", endpoint="/graphql"}
fraiseql_db_connections_active
fraiseql_db_connections_idle
fraiseql_query_duration_seconds{query_name="posts"}
fraiseql_rust_pipeline_enabled
fraiseql_cache_hits_total
fraiseql_cache_misses_total
```

**Prometheus Configuration:**
```yaml
# prometheus.yaml
scrape_configs:
  - job_name: 'fraiseql'
    kubernetes_sd_configs:
    - role: pod
    relabel_configs:
    - source_labels: [__meta_kubernetes_pod_label_app]
      regex: fraiseql
      action: keep
    - source_labels: [__meta_kubernetes_pod_ip]
      target_label: __address__
      replacement: '${1}:8000'
    metrics_path: '/metrics'
    scrape_interval: 15s
```

**2. Grafana Dashboards:**

**Read:** [Monitoring Setup](../production/monitoring.md)

Pre-built dashboards available in `deployments/grafana/`:
- `fraiseql-overview.json` - High-level health metrics
- `fraiseql-database.json` - PostgreSQL performance
- `fraiseql-graphql.json` - GraphQL query analysis

**Import Dashboards:**
```bash
# Import FraiseQL dashboards
kubectl create configmap grafana-dashboards \
  --from-file=deployments/grafana/fraiseql-overview.json \
  --from-file=deployments/grafana/fraiseql-database.json
```

**3. Loki Log Aggregation:**

**Read:** [Loki Integration](../production/loki-integration.md)

**Loki Configuration:**
```yaml
# promtail.yaml
clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: kubernetes-pods
    kubernetes_sd_configs:
    - role: pod
    relabel_configs:
    - source_labels: [__meta_kubernetes_pod_label_app]
      regex: fraiseql
      action: keep
    pipeline_stages:
    - json:
        expressions:
          level: level
          timestamp: timestamp
          message: message
          trace_id: trace_id
    - labels:
        level:
        trace_id:
```

**Structured Logging in FraiseQL:**
```json
{
  "timestamp": "2025-12-08T14:30:00Z",
  "level": "INFO",
  "message": "GraphQL query executed",
  "query_name": "posts",
  "duration_ms": 45,
  "user_id": "uuid-123",
  "trace_id": "abc-def-ghi",
  "span_id": "123-456"
}
```

**4. Distributed Tracing (OpenTelemetry):**

FraiseQL supports OpenTelemetry auto-instrumentation:

```bash
# Enable tracing
export OTEL_EXPORTER_OTLP_ENDPOINT="http://jaeger:4318"
export OTEL_SERVICE_NAME="fraiseql-api"
export OTEL_TRACES_EXPORTER="otlp"

python app.py
```

**Trace Context Propagation:**
- HTTP headers: `traceparent`, `tracestate`
- Database queries tagged with trace_id
- Cross-service correlation

**Success Check:** Metrics, logs, and traces are flowing to observability stack

### Step 5: Health Checks and Alerting (35 minutes)

**Goal:** Configure health monitoring and alerting

**Read:** [Health Checks Guide](../production/health-checks.md)

**Health Check Endpoints:**

**1. Liveness Probe (`/health`):**
```bash
curl http://localhost:8000/health

# Response:
{
  "status": "healthy",
  "version": "1.8.0",
  "uptime_seconds": 3600
}
```

**Purpose:** Process is alive and can serve traffic

**2. Readiness Probe (`/ready`):**

```bash
# Test readiness endpoint
curl http://localhost:8000/ready

# Example response (ready):
{
  "status": "ready",
  "checks": {
    "database": "ok",
    "schema": "ok"
  },
  "timestamp": 1670500000.0
}
```

**Purpose:** Application is ready to serve traffic (database connected, schema loaded)

**3. Alerting Rules:**

```yaml
# prometheus-alerts.yaml
groups:
  - name: fraiseql
    rules:
      # High error rate
      - alert: HighErrorRate
        expr: rate(fraiseql_requests_total{status=~"5.."}[5m]) > 0.05
        for: 5m
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} per second"

      # Slow queries
      - alert: SlowQueries
        expr: histogram_quantile(0.95, fraiseql_query_duration_seconds) > 2
        for: 10m
        annotations:
          summary: "95th percentile query latency > 2s"

      # Database connection pool exhaustion
      - alert: DBPoolExhausted
        expr: fraiseql_db_connections_active / fraiseql_db_connections_max > 0.9
        for: 5m
        annotations:
          summary: "Database connection pool near capacity"

      # High memory usage
      - alert: HighMemoryUsage
        expr: container_memory_usage_bytes{pod=~"fraiseql.*"} / container_spec_memory_limit_bytes > 0.85
        for: 10m
        annotations:
          summary: "Pod {{ $labels.pod }} memory usage > 85%"
```

**4. Alerting Channels:**
```yaml
# alertmanager.yaml
receivers:
  - name: 'pagerduty'
    pagerduty_configs:
    - service_key: '<pagerduty-key>'
      severity: critical

  - name: 'slack'
    slack_configs:
    - api_url: '<slack-webhook>'
      channel: '#alerts'
      title: 'FraiseQL Alert'
      text: '{{ .CommonAnnotations.description }}'

route:
  receiver: 'slack'
  group_by: ['alertname', 'cluster']
  routes:
  - match:
      severity: critical
    receiver: pagerduty
    continue: true
```

**Success Check:** Health checks configured and alerts firing to correct channels

### Step 6: Deployment Strategy (30 minutes)

**Goal:** Implement safe deployment patterns

**Deployment Strategies:**

**1. Blue-Green Deployment (Recommended):**
```bash
# Deploy new version (green)
kubectl apply -f deployment-green.yaml

# Wait for readiness
kubectl wait --for=condition=ready pod -l version=green

# Switch traffic
kubectl patch service fraiseql-api -p '{"spec":{"selector":{"version":"green"}}}'

# Monitor for 15 minutes
# If stable, delete blue
kubectl delete deployment fraiseql-blue

# If issues, rollback
kubectl patch service fraiseql-api -p '{"spec":{"selector":{"version":"blue"}}}'
```

**2. Canary Deployment:**
```yaml
# Istio VirtualService
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: fraiseql-canary
spec:
  hosts:
  - fraiseql-api
  http:
  - match:
    - headers:
        x-canary:
          exact: "true"
    route:
    - destination:
        host: fraiseql-api
        subset: canary
  - route:
    - destination:
        host: fraiseql-api
        subset: stable
      weight: 95
    - destination:
        host: fraiseql-api
        subset: canary
      weight: 5
```

**3. Rolling Update (Default):**
```yaml
apiVersion: apps/v1
kind: Deployment
spec:
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0  # Zero-downtime
```

**Deployment Checklist:**
- [ ] Run smoke tests in staging
- [ ] Review recent production metrics (baseline)
- [ ] Notify team in Slack
- [ ] Deploy with chosen strategy
- [ ] Monitor key metrics for 15 minutes
- [ ] Verify no error spike
- [ ] Check database query performance
- [ ] Update runbook if needed

**Rollback Plan:**
```bash
# Quick rollback to previous version
kubectl rollout undo deployment/fraiseql-api

# Check rollout status
kubectl rollout status deployment/fraiseql-api
```

**Success Check:** You can deploy and rollback safely

### Step 7: Incident Response (40 minutes)

**Goal:** Prepare for production incidents

**Read:** [Operations Runbook](../deployment/operations-runbook.md)

**Common Incidents & Resolution:**

**Incident 1: High Error Rate**
```bash
# 1. Check recent deployments
kubectl rollout history deployment/fraiseql-api

# 2. View error logs
kubectl logs -l app=fraiseql --tail=100 | grep ERROR

# 3. Check database connectivity
kubectl exec -it fraiseql-pod -- psql $DATABASE_URL -c "SELECT 1"

# 4. If new deployment caused it, rollback
kubectl rollout undo deployment/fraiseql-api
```

**Incident 2: Slow Response Times**
```bash
# 1. Check database query performance
psql $DATABASE_URL -c "
  SELECT query, mean_exec_time, calls
  FROM pg_stat_statements
  ORDER BY mean_exec_time DESC
  LIMIT 10;
"

# 2. Check connection pool exhaustion
curl http://fraiseql:8000/metrics | grep db_connections

# 3. Identify slow queries in Grafana
# Dashboard: FraiseQL GraphQL -> Query Latency

# 4. Temporary fix: Scale up pods
kubectl scale deployment fraiseql-api --replicas=6
```

**Incident 3: Database Connection Pool Exhausted**
```bash
# 1. Check current pool usage
curl http://fraiseql:8000/metrics | grep db_connections

# 2. Review slow queries blocking connections
psql $DATABASE_URL -c "
  SELECT pid, now() - query_start as duration, query
  FROM pg_stat_activity
  WHERE state != 'idle'
  ORDER BY duration DESC;
"

# 3. Kill long-running queries (if safe)
psql $DATABASE_URL -c "SELECT pg_terminate_backend(<pid>)"

# 4. Increase pool size (temporary)
kubectl set env deployment/fraiseql-api DATABASE_POOL_SIZE=40
```

**Incident 4: Memory Leak / OOM**
```bash
# 1. Check memory usage
kubectl top pods -l app=fraiseql

# 2. Get heap dump (Python)
kubectl exec -it fraiseql-pod -- python -m memory_profiler

# 3. Restart affected pods
kubectl delete pod fraiseql-pod-abc123

# 4. Monitor memory over time in Grafana
```

**On-Call Checklist:**
- [ ] Access to Kubernetes cluster (kubectl configured)
- [ ] Access to Grafana/Prometheus dashboards
- [ ] Access to Loki logs
- [ ] Database credentials for emergency queries
- [ ] Slack/PagerDuty notifications configured
- [ ] Runbook bookmarked
- [ ] Escalation path defined (L2 support)

**MTTR Goal:** < 5 minutes for P0 incidents (5xx errors, downtime)

**Success Check:** You can diagnose and resolve common incidents

### Step 8: Scaling and Performance (30 minutes)

**Goal:** Optimize for scale and cost efficiency

**Horizontal Scaling:**
```yaml
# Horizontal Pod Autoscaler
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql-api
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300  # 5 min cooldown
      policies:
      - type: Pods
        value: 1
        periodSeconds: 60
```

**Database Scaling:**
```bash
# Read replicas for read-heavy workloads
# Route reads to replicas, writes to primary
DATABASE_URL_WRITE="postgresql://primary:5432/fraiseql"
DATABASE_URL_READ="postgresql://replica:5432/fraiseql"
```

**Cost Optimization:**
```yaml
# Use spot instances for non-critical environments
nodeSelector:
  node.kubernetes.io/instance-type: spot

# Request fewer resources for staging
resources:
  requests:
    memory: "256Mi"
    cpu: "250m"
  limits:
    memory: "1Gi"
    cpu: "1000m"
```

**Performance Tuning:**
1. **Enable Rust pipeline** (7-10x JSON performance)
2. **Database connection pooling** (PgBouncer)
3. **Read replicas** for query-heavy loads
4. **Caching** (Redis or in-memory)
5. **Database indexes** on frequent queries

**Success Check:** System scales automatically under load

## Production Deployment Summary

**Deployment:** ✅ Kubernetes with 3+ replicas, rolling updates
**Database:** ✅ PostgreSQL with PgBouncer, read replicas, automated backups
**Observability:** ✅ Prometheus metrics, Grafana dashboards, Loki logs, OpenTelemetry tracing
**Health Checks:** ✅ Liveness and readiness probes configured
**Alerting:** ✅ Critical alerts to PagerDuty, warnings to Slack
**Incident Response:** ✅ Runbook with <5 min MTTR for P0 incidents
**Scaling:** ✅ HPA configured for automatic scaling

## Next Steps

### Immediate Actions
1. **Run through checklist:** Complete pre-deployment checklist
2. **Deploy to staging:** Validate configuration
3. **Load test:** Verify scaling behavior
4. **Practice incident response:** Simulate common failures

### Advanced Topics
- **Multi-region deployment:** Active-active for HA
- **Disaster recovery:** Cross-region backups and failover
- **Cost optimization:** Reserved instances, spot nodes
- **GitOps workflow:** ArgoCD or Flux for declarative deployments

### Community Resources
- **Discord:** Ask DevOps questions in #deployment channel
- **Examples:** `deployments/kubernetes/` - Production manifests
- **Blog:** Case studies of large-scale deployments

## Troubleshooting

### Common Issues

**Pods not starting:**
```bash
kubectl describe pod fraiseql-pod-abc123
kubectl logs fraiseql-pod-abc123
```

**Database connection failures:**
```bash
# Check database is accessible
kubectl exec -it fraiseql-pod -- psql $DATABASE_URL -c "SELECT 1"

# Check secrets are mounted
kubectl get secret db-secrets -o yaml
```

**Metrics not appearing in Prometheus:**
```bash
# Check Prometheus is scraping
curl http://prometheus:9090/api/v1/targets

# Check metrics endpoint is accessible
kubectl exec -it fraiseql-pod -- curl localhost:8000/metrics
```

**High memory usage:**
- Check for memory leaks in custom code
- Reduce connection pool size
- Tune Python garbage collection

## Summary

You now have:
- ✅ Production-ready Kubernetes deployment
- ✅ Complete observability stack
- ✅ Health checks and monitoring configured
- ✅ Incident response runbooks
- ✅ Scaling and cost optimization strategies
- ✅ Backup and disaster recovery procedures

**Estimated Time to Production:** 1-2 weeks for infrastructure setup and validation

**Recommended Next Journey:** [Backend Engineer Journey](./backend-engineer.md) for API design patterns

---

**Questions?** Join our [Discord community](https://discord.gg/fraiseql) #deployment channel
