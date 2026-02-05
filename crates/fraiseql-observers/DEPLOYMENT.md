# FraiseQL Observer Deployment Guide

Complete guide for deploying FraiseQL observers in production with Docker Compose.

## Table of Contents

- [Quick Start](#quick-start)
- [Deployment Topologies](#deployment-topologies)
- [Prerequisites](#prerequisites)
- [Configuration](#configuration)
- [Docker Compose Deployment](#docker-compose-deployment)
- [Kubernetes Deployment](#kubernetes-deployment)
- [Monitoring & Troubleshooting](#monitoring--troubleshooting)
- [Migration Guides](#migration-guides)
- [Production Checklist](#production-checklist)

---

## Quick Start

```bash
# Clone repository
git clone https://github.com/your-org/fraiseql.git
cd fraiseql/crates/fraiseql-observers

# Choose topology (see decision tree below)
# Option 1: PostgreSQL-Only (simplest)
docker-compose -f docker-compose.postgres-only.yml up

# Option 2: PostgreSQL + Redis (recommended for production)
docker-compose -f docker-compose.postgres-redis.yml up

# Option 3: NATS Distributed (high availability)
docker-compose -f docker-compose.nats-distributed.yml up

# Option 4: Multi-Database (multiple databases)
docker-compose -f docker-compose.multi-database.yml up
```

---

## Deployment Topologies

### Decision Tree

```
                          START
                            |
                   ┌────────┴────────┐
                   │ Single DB?      │
                   └────────┬────────┘
                      Yes ┌─┴─┐ No
                          │   └──────────────────┐
                   ┌──────┴──────┐              │
                   │ Event volume?│              │
                   └──────┬──────┘              │
                    <1K ┌─┴─┐ >1K               │
                        │   │                   │
                  ┌─────┘   └──────┐            │
                  │                │            │
            [PostgreSQL-Only]  [PostgreSQL     │
                               + Redis]        │
                                                │
                                    ┌───────────┘
                                    │
                             ┌──────┴──────┐
                             │ HA required?│
                             └──────┬──────┘
                              Yes ┌─┴─┐ No
                                  │   │
                    ┌─────────────┘   └──────────────┐
                    │                                 │
            [NATS Distributed]              [Multi-Database]
```

### Topology Comparison

| Topology | Complexity | Throughput | HA | Horizontal Scaling | Best For |
|----------|------------|------------|----|--------------------|----------|
| **PostgreSQL-Only** | ⭐ | 1K/s | ❌ | ❌ | Dev/test, simple deployments |
| **PostgreSQL + Redis** | ⭐⭐ | 5K/s | ❌ | ❌ | Production single DB |
| **NATS Distributed** | ⭐⭐⭐ | 50K/s | ✅ | ✅ | HA production systems |
| **Multi-Database** | ⭐⭐⭐⭐ | 100K+/s | ✅ | ✅ | Multi-tenant, microservices |

---

## Prerequisites

### Software Requirements

- **Docker**: 20.10+ (with Compose V2)
- **PostgreSQL**: 12+ (for database migrations)
- **Optional**:
  - **Redis**: 6+ (for topologies 2-4)
  - **NATS**: 2.9+ (for topologies 3-4)
  - **Kubernetes**: 1.25+ (for K8s deployment)

### Hardware Requirements

| Topology | CPU | Memory | Disk | Network |
|----------|-----|--------|------|---------|
| PostgreSQL-Only | 2 cores | 2 GB | 20 GB | 1 Gbps |
| PostgreSQL + Redis | 4 cores | 4 GB | 50 GB | 1 Gbps |
| NATS Distributed | 8 cores | 8 GB | 100 GB | 10 Gbps |
| Multi-Database | 16 cores | 16 GB | 200 GB | 10 Gbps |

*Recommendations for production workloads*

---

## Configuration

### Environment Variables

All configuration can be overridden via environment variables:

```bash
# Transport
export FRAISEQL_OBSERVER_TRANSPORT=postgres|nats|in_memory
export FRAISEQL_NATS_URL=nats://nats-cluster:4222
export FRAISEQL_NATS_ENABLE_BRIDGE=true|false
export FRAISEQL_NATS_RUN_EXECUTORS=true|false

# Redis
export FRAISEQL_REDIS_URL=redis://redis:6379
export FRAISEQL_REDIS_POOL_SIZE=10
export FRAISEQL_REDIS_DEDUP_WINDOW_SECS=300
export FRAISEQL_REDIS_CACHE_TTL_SECS=60

# Performance
export FRAISEQL_ENABLE_DEDUP=true|false
export FRAISEQL_ENABLE_CACHING=true|false
export FRAISEQL_ENABLE_CONCURRENT=true|false
export FRAISEQL_MAX_CONCURRENT_ACTIONS=10

# Runtime
export FRAISEQL_CHANNEL_CAPACITY=1000
export FRAISEQL_MAX_CONCURRENCY=50
export FRAISEQL_SHUTDOWN_TIMEOUT=30s
```

### TOML Configuration Files

See `examples/` directory for complete configuration examples:

- `01-postgresql-only.toml` - PostgreSQL LISTEN/NOTIFY
- `02-postgresql-redis.toml` - PostgreSQL + Redis
- `03-nats-distributed.toml` - NATS workers
- `04-multi-database-bridge.toml` - PostgreSQL → NATS bridges

---

## Docker Compose Deployment

### Topology 1: PostgreSQL-Only

**File**: `docker-compose.postgres-only.yml`

**Services**:

- PostgreSQL (database + triggers)
- Observer worker (in-process)

**Deploy**:
```bash
docker-compose -f docker-compose.postgres-only.yml up -d

# Check status
docker-compose -f docker-compose.postgres-only.yml ps

# View logs
docker-compose -f docker-compose.postgres-only.yml logs -f observer

# Stop
docker-compose -f docker-compose.postgres-only.yml down
```

**Use Case**: Development, testing, simple deployments

---

### Topology 2: PostgreSQL + Redis

**File**: `docker-compose.postgres-redis.yml`

**Services**:

- PostgreSQL (database + triggers)
- Redis (dedup + cache)
- Observer worker

**Deploy**:
```bash
docker-compose -f docker-compose.postgres-redis.yml up -d

# Check Redis keys
docker-compose -f docker-compose.postgres-redis.yml exec redis redis-cli KEYS "*"

# Monitor dedup hit rate
docker-compose -f docker-compose.postgres-redis.yml exec redis redis-cli INFO stats | grep keyspace_hits

# View logs
docker-compose -f docker-compose.postgres-redis.yml logs -f observer

# Stop
docker-compose -f docker-compose.postgres-redis.yml down
```

**Use Case**: Production single database with reliability and performance requirements

---

### Topology 3: NATS Distributed

**File**: `docker-compose.nats-distributed.yml`

**Services**:

- PostgreSQL (database + triggers)
- NATS JetStream (event bus)
- Redis (shared dedup + cache)
- Bridge (PostgreSQL → NATS)
- Workers (3+ instances, scalable)

**Deploy**:
```bash
# Start with 3 workers
docker-compose -f docker-compose.nats-distributed.yml up -d

# Scale to 10 workers
docker-compose -f docker-compose.nats-distributed.yml up -d --scale worker=10

# Check NATS stream
docker-compose -f docker-compose.nats-distributed.yml exec nats nats stream info fraiseql_events

# Check consumer lag
docker-compose -f docker-compose.nats-distributed.yml exec nats nats consumer report fraiseql_events

# Monitor throughput
curl http://localhost:8222/varz | jq

# View worker logs
docker-compose -f docker-compose.nats-distributed.yml logs -f worker

# Stop
docker-compose -f docker-compose.nats-distributed.yml down
```

**Use Case**: High availability, high throughput, horizontal scaling

---

### Topology 4: Multi-Database

**File**: `docker-compose.multi-database.yml`

**Services**:

- PostgreSQL x3 (3 databases)
- NATS JetStream (centralized event bus)
- Redis (shared dedup + cache)
- Bridges x3 (one per database)
- Workers (10+ instances, scalable)

**Deploy**:
```bash
# Start all services
docker-compose -f docker-compose.multi-database.yml up -d

# Scale workers to 20
docker-compose -f docker-compose.multi-database.yml up -d --scale worker=20

# Check all bridge checkpoints
for db in db1 db2 db3; do
  echo "=== Database: $db ==="
  docker-compose -f docker-compose.multi-database.yml exec postgres-$db \
    psql -U fraiseql -d fraiseql_$db -c "SELECT * FROM tb_observer_checkpoint;"
done

# Monitor NATS throughput
curl http://localhost:8222/varz | jq '.in_msgs, .out_msgs'

# View bridge logs
docker-compose -f docker-compose.multi-database.yml logs -f bridge-db1

# Stop
docker-compose -f docker-compose.multi-database.yml down
```

**Use Case**: Multi-tenant, microservices, multiple business units

---

## Kubernetes Deployment

### Helm Chart Installation

```bash
# Add FraiseQL Helm repository
helm repo add fraiseql https://fraiseql.io/charts
helm repo update

# Install with default values (PostgreSQL + Redis)
helm install fraiseql-observers fraiseql/observers

# Install NATS distributed topology
helm install fraiseql-observers fraiseql/observers \
  --set topology=nats-distributed \
  --set worker.replicas=10 \
  --set redis.enabled=true \
  --set nats.enabled=true

# Upgrade deployment
helm upgrade fraiseql-observers fraiseql/observers \
  --set worker.replicas=20

# Uninstall
helm uninstall fraiseql-observers
```

### Custom Kubernetes Manifests

See `k8s/` directory for example manifests:

- `deployment-bridge.yaml` - PostgreSQL → NATS bridge
- `deployment-worker.yaml` - Observer workers (with autoscaling)
- `statefulset-nats.yaml` - NATS JetStream cluster
- `statefulset-redis.yaml` - Redis cluster
- `configmap.yaml` - Configuration
- `secret.yaml` - Credentials

**Deploy**:
```bash
# Create namespace
kubectl create namespace fraiseql

# Deploy PostgreSQL (if not using external DB)
kubectl apply -f k8s/statefulset-postgres.yaml -n fraiseql

# Deploy NATS cluster
kubectl apply -f k8s/statefulset-nats.yaml -n fraiseql

# Deploy Redis cluster
kubectl apply -f k8s/statefulset-redis.yaml -n fraiseql

# Deploy configuration
kubectl apply -f k8s/configmap.yaml -n fraiseql
kubectl apply -f k8s/secret.yaml -n fraiseql

# Deploy bridges
kubectl apply -f k8s/deployment-bridge.yaml -n fraiseql

# Deploy workers (with autoscaling)
kubectl apply -f k8s/deployment-worker.yaml -n fraiseql
kubectl apply -f k8s/hpa-worker.yaml -n fraiseql

# Check status
kubectl get pods -n fraiseql
kubectl logs -f deployment/fraiseql-worker -n fraiseql

# Scale workers manually
kubectl scale deployment fraiseql-worker --replicas=20 -n fraiseql
```

---

## Monitoring & Troubleshooting

### Health Checks

```bash
# PostgreSQL
docker-compose exec postgres pg_isready -U fraiseql

# Redis
docker-compose exec redis redis-cli ping

# NATS
curl http://localhost:8222/healthz
```

### Common Issues

**Issue**: Bridge not publishing events

**Solution**:
```bash
# Check checkpoint table
docker-compose exec postgres psql -U fraiseql -d fraiseql \
  -c "SELECT * FROM tb_observer_checkpoint WHERE transport_name = 'pg_to_nats';"

# Reset checkpoint (re-publishes all events)
docker-compose exec postgres psql -U fraiseql -d fraiseql \
  -c "DELETE FROM tb_observer_checkpoint WHERE transport_name = 'pg_to_nats';"
```

**Issue**: Workers not receiving events

**Solution**:
```bash
# Check NATS consumer lag
docker-compose exec nats nats consumer info fraiseql_events fraiseql_observer_worker_group

# Check NATS stream messages
docker-compose exec nats nats stream info fraiseql_events
```

**Issue**: High Redis memory usage

**Solution**:
```bash
# Check Redis memory
docker-compose exec redis redis-cli INFO memory

# Adjust TTL (reduce cache duration)
export FRAISEQL_REDIS_CACHE_TTL_SECS=30

# Adjust maxmemory policy (already set to allkeys-lru)
docker-compose exec redis redis-cli CONFIG SET maxmemory-policy allkeys-lru
```

**Issue**: Duplicate events being processed

**Solution**:
```bash
# Verify dedup is enabled
docker-compose exec observer env | grep FRAISEQL_ENABLE_DEDUP

# Check Redis dedup keys
docker-compose exec redis redis-cli KEYS "event:*" | wc -l

# Verify dedup window
docker-compose exec redis redis-cli TTL event:<event_id>
```

### Metrics & Dashboards

**Prometheus Metrics** (when Phase 8.7 is complete):
```
# Observer metrics
fraiseql_observer_events_processed_total
fraiseql_observer_actions_executed_total
fraiseql_observer_cache_hits_total
fraiseql_observer_cache_misses_total
fraiseql_observer_dedup_hits_total
fraiseql_observer_dedup_misses_total

# System metrics
fraiseql_observer_backlog_size
fraiseql_observer_processing_duration_seconds
```

**Grafana Dashboard**: See `monitoring/grafana-dashboard.json`

---

## Migration Guides

### PostgreSQL-Only → PostgreSQL + Redis

1. Deploy Redis:
```bash
docker-compose -f docker-compose.postgres-redis.yml up -d redis
```

2. Update environment variables:
```bash
export FRAISEQL_REDIS_URL=redis://redis:6379
export FRAISEQL_ENABLE_DEDUP=true
export FRAISEQL_ENABLE_CACHING=true
```

3. Restart observer:
```bash
docker-compose -f docker-compose.postgres-redis.yml up -d observer
```

4. Verify Redis connection:
```bash
docker-compose -f docker-compose.postgres-redis.yml logs observer | grep "Redis connected"
```

### PostgreSQL + Redis → NATS Distributed

1. Deploy NATS:
```bash
docker-compose -f docker-compose.nats-distributed.yml up -d nats
```

2. Deploy bridge (publishes historical events):
```bash
docker-compose -f docker-compose.nats-distributed.yml up -d bridge
```

3. Deploy workers:
```bash
docker-compose -f docker-compose.nats-distributed.yml up -d --scale worker=3
```

4. Stop old PostgreSQL observer:
```bash
docker-compose -f docker-compose.postgres-redis.yml stop observer
```

5. Verify NATS stream:
```bash
docker-compose -f docker-compose.nats-distributed.yml exec nats nats stream info fraiseql_events
```

---

## Production Checklist

### Security

- [ ] Change default passwords (`POSTGRES_PASSWORD`, etc.)
- [ ] Use TLS for NATS connections (`nats://` → `tls://`)
- [ ] Enable Redis authentication (`requirepass`)
- [ ] Use secrets management (Kubernetes Secrets, AWS Secrets Manager)
- [ ] Restrict network access (firewall rules, security groups)
- [ ] Enable audit logging

### Reliability

- [ ] Configure persistent volumes (not using Docker named volumes in production)
- [ ] Set up backups (PostgreSQL, NATS, Redis)
- [ ] Configure health checks and auto-restart policies
- [ ] Enable deduplication (prevent duplicate processing)
- [ ] Set appropriate checkpoint intervals
- [ ] Configure Dead Letter Queue (DLQ)

### Performance

- [ ] Enable caching (100x performance improvement)
- [ ] Enable concurrent execution
- [ ] Tune Redis maxmemory and eviction policy
- [ ] Tune NATS JetStream limits (max_msgs, max_bytes)
- [ ] Configure connection pools (PostgreSQL, Redis)
- [ ] Set appropriate worker counts (CPU cores × 2-4)

### Monitoring

- [ ] Enable Prometheus metrics export
- [ ] Set up Grafana dashboards
- [ ] Configure alerting (PagerDuty, Slack, etc.)
- [ ] Monitor backlog size
- [ ] Monitor cache hit rate
- [ ] Monitor dedup hit rate
- [ ] Track action execution duration

### Disaster Recovery

- [ ] Document failover procedures
- [ ] Test checkpoint recovery
- [ ] Test bridge crash recovery
- [ ] Test worker crash recovery
- [ ] Document runbook for common issues
- [ ] Set up automated testing of recovery scenarios

---

## Support

- **Documentation**: https://fraiseql.io/observers
- **Issues**: https://github.com/your-org/fraiseql/issues
- **Discussions**: https://github.com/your-org/fraiseql/discussions
- **Slack**: https://fraiseql.slack.com

---

## License

FraiseQL Observer System is licensed under the MIT License. See [LICENSE](LICENSE) for details.
