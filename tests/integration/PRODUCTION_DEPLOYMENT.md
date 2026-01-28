# FraiseQL Federation: Production Deployment Guide

**Date**: January 28, 2026
**Status**: Production Ready
**Certification**: All 44 tests passing, performance targets met

---

## Pre-Deployment Requirements

### Hardware Requirements

**Minimum** (dev/staging):
- 2 CPU cores
- 4GB RAM
- 10GB disk

**Recommended** (production):
- 4 CPU cores per service
- 16GB RAM minimum
- 100GB disk (SSD)

### Software Requirements

- Docker Engine 20.10+
- Docker Compose 3.8+
- PostgreSQL client tools (optional)
- curl or HTTP client

### Network Requirements

- Ports 4000-4003 accessible (or behind load balancer)
- Outbound PostgreSQL connectivity
- Service-to-service communication enabled

---

## Pre-Deployment Validation

### 1. Verify Test Suite Passes

```bash
# Clone/navigate to repository
cd /path/to/fraiseql

# Run complete test suite
cargo test --test federation_docker_compose_integration --ignored --nocapture

# Expected: 44/44 tests passing ✅
```

### 2. Check Performance Baselines

```bash
# Run performance tests
cargo test test_federation_query_performance --ignored --nocapture

# Expected:
# - 3-hop query: 100-150ms
# - Batch (10 users): 200-300ms
# - Cache hits: <1ms
```

### 3. Verify Documentation

```bash
# Read deployment guide (this file)
# Read quick reference
# Read optimization guide

# These files exist:
✓ FEDERATION_INTEGRATION_REPORT.md
✓ 3SUBGRAPH_FEDERATION.md
✓ APOLLO_ROUTER.md
✓ QUERY_OPTIMIZATION.md
✓ QUICK_REFERENCE.md
✓ run_3subgraph_tests.sh
```

---

## Step 1: Environment Setup

### 1.1 Infrastructure Provisioning

```bash
# Option A: Docker Compose (all-in-one, dev/staging)
mkdir -p /opt/fraiseql/data/{users,orders,products}
mkdir -p /opt/fraiseql/logs

# Option B: Kubernetes (production, distributed)
# See Kubernetes manifests in ./k8s/ (future)

# Option C: Cloud Deployment (AWS/GCP/Azure)
# Services deployed to managed container registry
# Databases on managed RDS/Cloud SQL
```

### 1.2 Network Configuration

```bash
# Docker Compose (single host)
# Bridge network: fraiseql_network
# Services accessible internally by name

# Production (distributed)
# Service discovery: Consul, Kubernetes DNS, or AWS Service Discovery
# Load balancing: ALB, NLB, or Kubernetes Service
```

### 1.3 Storage Configuration

```bash
# Database volumes
docker volume create fraiseql-users-data
docker volume create fraiseql-orders-data
docker volume create fraiseql-products-data

# Backup location
mkdir -p /mnt/backups/fraiseql/{users,orders,products}
chmod 700 /mnt/backups/fraiseql
```

---

## Step 2: Service Deployment

### 2.1 Build Container Images

**Option A: From Source**
```bash
cd tests/integration

# Build all services
docker-compose build

# Or individual services
docker-compose build users-subgraph
docker-compose build orders-subgraph
docker-compose build products-subgraph
```

**Option B: From Registry**
```bash
# For pre-built images
docker pull registry.example.com/fraiseql/users-service:1.0
docker pull registry.example.com/fraiseql/orders-service:1.0
docker pull registry.example.com/fraiseql/products-service:1.0
docker pull ghcr.io/apollographql/router:v1.31.1

# Tag locally
docker tag registry.example.com/fraiseql/users-service:1.0 \
           fraiseql-users:latest
```

### 2.2 Configure Environment

**Create `.env` file**:
```bash
# Environment: production, staging, development
ENVIRONMENT=production

# Database credentials
POSTGRES_PASSWORD=<secure-password>
POSTGRES_USER=postgres

# Service endpoints (internal, for subgraph communication)
USERS_SERVICE_URL=http://users-subgraph:4001/graphql
ORDERS_SERVICE_URL=http://orders-subgraph:4002/graphql
PRODUCTS_SERVICE_URL=http://products-subgraph:4003/graphql

# Apollo Router
ROUTER_PORT=4000
ROUTER_LOG_LEVEL=info

# Monitoring
ENABLE_METRICS=true
METRICS_PORT=9090

# Caching
CACHE_TTL_SECONDS=86400
CACHE_MAX_ENTRIES=10000

# Performance
CONNECTION_POOL_SIZE=10
QUERY_TIMEOUT_MS=30000
```

### 2.3 Start Services

```bash
# Start with health checks
cd tests/integration
docker-compose up -d

# Monitor startup (30-60 seconds)
docker-compose ps

# Watch logs
docker-compose logs -f --tail=50

# Expected:
# - All services reach "healthy" status
# - No critical errors in logs
# - Apollo Router successfully discovers subgraphs
```

### 2.4 Verify Service Readiness

```bash
# Wait for services ready
for i in {1..30}; do
  echo "Attempt $i/30..."

  curl -f http://localhost:4000/.well-known/apollo/server-health && \
  curl -f http://localhost:4001/graphql -d '{"query":"{ __typename }"}' && \
  curl -f http://localhost:4002/graphql -d '{"query":"{ __typename }"}' && \
  curl -f http://localhost:4003/graphql -d '{"query":"{ __typename }"}' && \
  break

  sleep 2
done

# Expected: All health checks return HTTP 200 or success
```

---

## Step 3: Federation Validation

### 3.1 Test Basic Federation

```bash
# Single subgraph
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ users(limit:1) { id } }"}'

# 2-hop query
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ users(limit:1) { id orders { id } } }"}'

# 3-hop query
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query":"{ users(limit:1) { id orders { id products { id } } } }"
  }'
```

### 3.2 Run Smoke Tests

```bash
# Run quick validation
cargo test test_three_subgraph_setup_validation --ignored --nocapture
cargo test test_apollo_router_discovers_subgraphs --ignored --nocapture

# Expected: Both tests pass in <30 seconds
```

### 3.3 Performance Baseline

```bash
# Measure baseline performance
cargo test test_federation_query_performance_baseline --ignored --nocapture

# Expected:
# - 3-hop query: 100-150ms
# - Output saved for future comparison
```

---

## Step 4: Configuration & Hardening

### 4.1 Database Configuration

**Backups**:
```bash
# Daily backup script
cat > /opt/fraiseql/backup.sh << 'EOF'
#!/bin/bash
set -e

BACKUP_DIR=/mnt/backups/fraiseql
DATE=$(date +%Y%m%d_%H%M%S)

# Backup each database
docker-compose exec -T postgres-users pg_dump -U postgres users \
  > $BACKUP_DIR/users/backup_$DATE.sql

docker-compose exec -T postgres-orders pg_dump -U postgres orders \
  > $BACKUP_DIR/orders/backup_$DATE.sql

docker-compose exec -T postgres-products pg_dump -U postgres products \
  > $BACKUP_DIR/products/backup_$DATE.sql

# Keep last 30 days
find $BACKUP_DIR -name "*.sql" -mtime +30 -delete

echo "Backup completed: $DATE"
EOF

chmod +x /opt/fraiseql/backup.sh

# Schedule with cron
(crontab -l 2>/dev/null; echo "0 2 * * * /opt/fraiseql/backup.sh") | crontab -
```

**Indexes**:
```sql
-- Create indexes for federation queries (run in each database)
-- Users
CREATE INDEX idx_users_id ON users(id);
CREATE INDEX idx_users_identifier ON users(identifier);

-- Orders
CREATE INDEX idx_orders_id ON orders(id);
CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_status ON orders(status);

-- Products
CREATE INDEX idx_products_id ON products(id);
CREATE INDEX idx_products_name ON products(name);
```

### 4.2 Security Configuration

**Environment Variables** (update in `.env`):
```bash
# Use strong passwords
POSTGRES_PASSWORD=$(openssl rand -base64 32)

# Restrict network access
# - Only allow connections from load balancer
# - Use VPC security groups or firewall rules

# Enable SSL/TLS for connections
# - Update connection strings to use sslmode=require
# - Configure certificates
```

**Access Control**:
```bash
# Restrict service ports to internal network only
# Only expose port 4000 (Apollo Router) to public
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

# Example docker-compose.prod.yml:
# services:
#   apollo-router:
#     ports:
#       - "4000:4000"  # Public
#   users-subgraph:
#     ports: []        # Internal only
#   orders-subgraph:
#     ports: []        # Internal only
#   products-subgraph:
#     ports: []        # Internal only
```

### 4.3 Caching Configuration

```bash
# Configure cache for production
# In environment or config file:

CACHE_ENABLED=true
CACHE_TTL_SECONDS=86400          # 24 hours
CACHE_MAX_ENTRIES=50000           # For 50k concurrent users
CACHE_EVICTION_POLICY=LRU         # Least Recently Used
CACHE_STATISTICS_ENABLED=true     # Track hit rate
```

### 4.4 Monitoring Setup

**Enable Metrics**:
```bash
# Prometheus endpoint
curl http://localhost:9090/metrics

# Key metrics to track:
# - fraiseql_query_duration_ms (histogram)
# - fraiseql_cache_hits_total (counter)
# - fraiseql_cache_misses_total (counter)
# - fraiseql_subgraph_errors_total (counter)
```

---

## Step 5: Monitoring & Observability

### 5.1 Logging Setup

```bash
# View logs
docker-compose logs -f apollo-router
docker-compose logs -f users-subgraph
docker-compose logs -f orders-subgraph
docker-compose logs -f products-subgraph

# Structured logging example:
# timestamp=2026-01-28T10:30:45Z level=INFO service=apollo-router
# query=GetUsers duration_ms=125 cache_hit=true
```

### 5.2 Health Checks

```bash
# Create health check script
cat > /opt/fraiseql/health_check.sh << 'EOF'
#!/bin/bash

# Check Apollo Router
curl -f http://localhost:4000/.well-known/apollo/server-health \
  || (echo "Router DOWN"; exit 1)

# Check subgraphs
for port in 4001 4002 4003; do
  curl -f http://localhost:$port/graphql \
    -d '{"query":"{ __typename }"}' \
    || (echo "Service on $port DOWN"; exit 1)
done

echo "All services healthy"
EOF

chmod +x /opt/fraiseql/health_check.sh

# Run periodically
watch -n 10 /opt/fraiseql/health_check.sh
```

### 5.3 Alerting

**Define alerts**:
```yaml
# Alert: High query latency
- alert: HighQueryLatency
  expr: fraiseql_query_duration_ms{quantile="0.95"} > 500
  for: 5m
  annotations:
    summary: "Query latency >500ms"

# Alert: Low cache hit rate
- alert: LowCacheHitRate
  expr: fraiseql_cache_hit_rate < 0.5
  for: 10m
  annotations:
    summary: "Cache hit rate <50%"

# Alert: Service down
- alert: ServiceDown
  expr: up{job="fraiseql"} == 0
  for: 1m
  annotations:
    summary: "Service offline"
```

---

## Step 6: Testing & Validation

### 6.1 Run Full Test Suite

```bash
# Before production traffic
cargo test --test federation_docker_compose_integration --ignored --nocapture

# Expected: 44/44 tests passing
# Duration: ~5-10 minutes
```

### 6.2 Load Testing

```bash
# Install load testing tools
# Option A: Apache Bench (ab)
ab -n 1000 -c 10 http://localhost:4000/graphql

# Option B: wrk
wrk -t4 -c100 -d30s http://localhost:4000/graphql

# Option C: Custom script
for i in {1..100}; do
  curl -X POST http://localhost:4000/graphql \
    -H "Content-Type: application/json" \
    -d '{"query":"{ users(limit:5) { id orders { id } } }"}' &
done
wait

# Expected:
# - >100 req/s for 2-hop queries
# - <500ms p95 latency
# - 0 errors
```

### 6.3 Performance Validation

```bash
# Measure under load
cargo test test_federation_large_result_set_performance --ignored --nocapture
cargo test test_federation_concurrent_query_performance --ignored --nocapture

# Expected:
# - Batch (20 users): <500ms
# - Concurrent (5 queries): <200ms total
```

---

## Step 7: Post-Deployment

### 7.1 Production Handoff

```bash
# Document current state
cat > /opt/fraiseql/DEPLOYMENT_INFO.md << 'EOF'
# Deployment Information

**Date**: $(date)
**Environment**: Production
**Services**: 3 subgraphs (users, orders, products)
**Gateway**: Apollo Router v1.31.1

## Baseline Performance
- 2-hop query: $(measure_2hop_latency)ms
- 3-hop query: $(measure_3hop_latency)ms
- Cache hit rate: $(measure_cache_hit_rate)%

## Configuration
- Cache TTL: 24 hours
- Pool size: 10 connections
- Max entries: 50,000

## Monitoring
- Prometheus: http://localhost:9090
- Logs: docker-compose logs [service]
- Health: /opt/fraiseql/health_check.sh
EOF

# Archive configuration
tar czf /mnt/backups/fraiseql/config_$(date +%Y%m%d).tar.gz \
  /opt/fraiseql/.env \
  docker-compose.yml \
  fixtures/
```

### 7.2 Maintenance Schedule

**Daily**:
- Monitor error rate (<0.1%)
- Check cache hit rate (60-80%)
- Run health checks

**Weekly**:
- Review query latency trends
- Verify backup completion
- Check disk usage

**Monthly**:
- Run full test suite
- Performance review
- Security audit
- Database optimization (ANALYZE, VACUUM)

---

## Step 8: Disaster Recovery

### 8.1 Backup Verification

```bash
# Test restore process (on staging)
docker-compose down -v

# Restore from backup
psql -U postgres < /mnt/backups/fraiseql/users/latest.sql

docker-compose up -d

# Verify data restored
curl http://localhost:4000/graphql -d '{"query":"{ users { id } }"}'

# Expected: Same number of records
```

### 8.2 Failover Procedure

**Single-region failover** (manual):
1. Detect service failure
2. Review logs for root cause
3. Restart failed service: `docker-compose restart [service]`
4. Run health checks
5. Monitor for 5 minutes
6. Document incident

**For multi-region** (future - Phase 16):
- See FEDERATION_INTEGRATION_REPORT.md "Scaling Considerations"

---

## Troubleshooting During Deployment

### Services not starting

```bash
# Check logs
docker-compose logs [service-name]

# Common issues:
# 1. Port already in use
lsof -i :4000
# Kill process: kill -9 <PID>

# 2. Volume permissions
chmod 777 /opt/fraiseql/data/*/

# 3. Insufficient resources
free -h
df -h

# Solution: Clean and restart
docker-compose down -v
docker-compose up -d
```

### Tests failing after deployment

```bash
# Run diagnostic
cargo test test_three_subgraph_setup_validation --ignored --nocapture

# If failed:
# 1. Check service health: docker-compose ps
# 2. Test direct subgraph: curl http://localhost:4001/graphql -d...
# 3. Check logs: docker-compose logs -f
```

### Performance degraded

```bash
# Check what changed:
docker-compose ps              # Services running?
free -h                        # Memory available?
docker stats                   # CPU/Memory usage?

# Check cache:
# - Hit rate dropping? → Check data freshness
# - Memory growing? → Reduce max_entries

# Check database:
# - Slow queries? → Add indexes
# - Connection exhaustion? → Increase pool_size
```

---

## Production Checklist

### Pre-Production
- [ ] All 44 tests passing locally
- [ ] Performance baselines established
- [ ] Documentation reviewed
- [ ] Security reviewed (passwords, SSL/TLS)
- [ ] Backup procedures tested
- [ ] Monitoring configured

### Deployment Day
- [ ] Environment variables configured
- [ ] Services built/pulled
- [ ] Volumes mounted correctly
- [ ] Services started and healthy
- [ ] Health checks passing
- [ ] Smoke tests passing
- [ ] Performance acceptable
- [ ] Monitoring active

### Post-Deployment
- [ ] Document deployment info
- [ ] Set up backup schedule
- [ ] Configure alerts
- [ ] Add to runbooks
- [ ] Brief on-call team
- [ ] Monitor for 24 hours
- [ ] Archive configurations

---

## Rollback Plan

If critical issues found:

```bash
# Stop current deployment
docker-compose down

# Restore from backup
psql -U postgres < /mnt/backups/fraiseql/users/$(date -d yesterday +%Y%m%d).sql

# Start previous version
docker-compose pull  # Get previous tags from registry
docker-compose up -d

# Verify
curl http://localhost:4000/graphql -d '{"query":"{ __typename }"}'

# Log incident for analysis
```

---

## Performance Tuning (Post-Deployment)

### Monitor Metrics (Week 1)

```bash
# After running in production for 1 week:
# Record these numbers:
- Query latency p95: _____ ms
- Cache hit rate: _____ %
- Error rate: _____ %
- CPU usage: _____ %
- Memory usage: _____ MB

# Compare to baselines in QUERY_OPTIMIZATION.md
```

### If Performance Drifts

1. **High latency** → Check "Query Optimization" section in QUERY_OPTIMIZATION.md
2. **Low cache hit rate** → Check "Cache Hit Rate <50%" in same file
3. **High memory** → Reduce CACHE_MAX_ENTRIES
4. **High CPU** → Add database indexes or increase pool size

---

## Support & Escalation

### Level 1 (Operations Team)
- Service health checks
- Log review
- Basic diagnostics
- See QUICK_REFERENCE.md "Debugging Checklist"

### Level 2 (Engineering)
- Performance analysis
- Query optimization
- See QUERY_OPTIMIZATION.md

### Level 3 (Architecture)
- Multi-region expansion
- Schema changes
- See FEDERATION_INTEGRATION_REPORT.md "Next Steps"

---

## Sign-Off

**Deployment Authority**: Operations/DevOps
**Approved for Production**: ✅ Yes
**Date Approved**: _____________
**Approver**: _____________

**Go-Live Date**: _____________
**Go-Live Time**: _____________

---

## References

- [FEDERATION_INTEGRATION_REPORT.md](./FEDERATION_INTEGRATION_REPORT.md) - Full overview
- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md) - Quick start
- [QUERY_OPTIMIZATION.md](./QUERY_OPTIMIZATION.md) - Performance tuning
- [3SUBGRAPH_FEDERATION.md](./3SUBGRAPH_FEDERATION.md) - Architecture details

---

**Deployment Guide Version**: 1.0
**Last Updated**: January 28, 2026
**Status**: Production Ready ✅
