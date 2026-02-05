# FraiseQL Deployment Checklist

Use this checklist for pre-deployment verification.

## Pre-Deployment Phase (1-2 weeks before)

### Infrastructure

- [ ] Kubernetes cluster provisioned and healthy
- [ ] PostgreSQL database available and backed up
- [ ] Redis cache configured
- [ ] TLS certificates obtained and renewed schedule set
- [ ] DNS entries created
- [ ] Load balancer configured

### Security

- [ ] Security policies reviewed
- [ ] Network policies designed
- [ ] RBAC roles defined
- [ ] Secret management strategy selected
- [ ] Audit logging configured
- [ ] Secrets not committed to Git

### Monitoring

- [ ] Prometheus configured
- [ ] Alert rules created
- [ ] Dashboards prepared
- [ ] Log aggregation set up
- [ ] On-call rotation established

## Deployment Phase (Day of)

### 1. Image Preparation (2 hours before)

- [ ] Build Docker image locally
- [ ] Run security scan: `trivy image FraiseQL:latest`
- [ ] Generate SBOM: `./tools/generate-sbom.sh FraiseQL:latest`
- [ ] Fix any HIGH/CRITICAL vulnerabilities
- [ ] Push image to registry
- [ ] Verify image pull from Kubernetes

### 2. Pre-Flight Checks (1 hour before)

- [ ] Database migrations ready
- [ ] Configuration validated
- [ ] Secrets created in Kubernetes
- [ ] Network policies dry-run
- [ ] Health check endpoints working

### 3. Deployment

#### Database Migration

```bash
# Backup existing database
pg_dump $DATABASE_URL > backup-$(date +%Y%m%d).sql

# Run migrations
FraiseQL-cli migrate apply

# Verify schema
psql $DATABASE_URL -c "\d"
```

#### Kubernetes Deployment

```bash
# Apply hardened manifests
kubectl apply -f deploy/kubernetes/FraiseQL-hardened.yaml

# Wait for rollout
kubectl rollout status deployment/FraiseQL --timeout=5m

# Verify pods running
kubectl get pods -l app=FraiseQL -o wide
```

#### Helm Deployment (Alternative)

```bash
# Install/upgrade release
helm upgrade --install FraiseQL ./deploy/kubernetes/helm/FraiseQL

# Check rollout
helm status FraiseQL
```

### 4. Post-Deployment Verification (Immediately after)

- [ ] Pods are running: `kubectl get pods -l app=FraiseQL`
- [ ] Service endpoints available: `kubectl get svc FraiseQL`
- [ ] Health check passing: `curl http://FraiseQL/health`
- [ ] Metrics available: `curl http://prometheus:9090`
- [ ] Logs clean: `kubectl logs -l app=FraiseQL --tail=50`
- [ ] Alert manager has no alerts

### 5. Smoke Tests (30 minutes after)

```bash
# Test GraphQL endpoint
curl -X POST http://FraiseQL:8815/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ __schema { types { name } } }"}'

# Test cache functionality
# (make same query twice, verify cache hit)

# Test rate limiting
for i in {1..150}; do curl http://FraiseQL:8815/health; done

# Verify audit logs
kubectl logs deployment/FraiseQL | grep -i "request"
```

## Rollback Procedure (If needed)

### Immediate Rollback

```bash
# Using kubectl
kubectl rollout undo deployment/FraiseQL

# Using Helm
helm rollback FraiseQL 1
```

### Database Rollback

```bash
# Restore from backup if migration failed
psql $DATABASE_URL < backup-$(date +%Y%m%d).sql
```

## Post-Deployment Phase

### Day 1-7 Monitoring

- [ ] Monitor error rates (< 0.1%)
- [ ] Check p95 latency (< 1000ms)
- [ ] Verify no memory leaks
- [ ] Review audit logs daily
- [ ] Check backup completion

### Week 1 Review

- [ ] Collect metrics for baseline
- [ ] Document any issues
- [ ] Review performance profiles
- [ ] Plan optimizations

### Ongoing

- [ ] Weekly: Review audit logs
- [ ] Monthly: Security scan
- [ ] Quarterly: Full assessment
- [ ] Annually: Penetration test

## Common Issues & Recovery

### Pod Crashes

```bash
# Check logs
kubectl logs deployment/FraiseQL -c FraiseQL

# Check events
kubectl describe deployment FraiseQL

# Check resources
kubectl top pods -l app=FraiseQL
```

### Database Connection Issues

```bash
# Verify connectivity
kubectl run -it --rm debug --image=postgres --restart=Never -- \
  psql -h postgres -U FraiseQL -d FraiseQL -c "SELECT 1"

# Check connection pool
curl http://FraiseQL:8815/metrics | grep connections
```

### Slow Queries

```bash
# Enable query logging
kubectl set env deployment/FraiseQL \
  RUST_LOG=debug

# Check Prometheus for slow queries
# Query: fraiseql_query_duration_ms{quantile="0.95"}
```

## Sign-Off

- [ ] Infrastructure Owner:  _________________ Date: _____
- [ ] Security Owner:  _________________ Date: _____
- [ ] Application Owner: _________________ Date: _____
- [ ] Operations Owner: _________________ Date: _____
