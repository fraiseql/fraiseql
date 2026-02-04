# FraiseQL Deployment Guide

This guide covers deploying FraiseQL in production environments with focus on security, reliability, and observability.

## Quick Start

### Local Development with Docker Compose

```bash
# Start all services (FraiseQL, PostgreSQL, Redis, Prometheus)
docker-compose up -d

# Check services are running
docker-compose ps

# Stop all services
docker-compose down
```

Services will be available at:
- FraiseQL API: http://localhost:8815
- Prometheus: http://localhost:9090
- PostgreSQL: localhost:5432
- Redis: localhost:6379

### Kubernetes Deployment

#### Using Helm (Recommended)

```bash
# Add FraiseQL Helm repository
helm repo add fraiseql https://charts.fraiseql.io
helm repo update

# Install release
helm install fraiseql fraiseql/fraiseql \
  --namespace default \
  --values custom-values.yaml

# Verify deployment
kubectl get deployments
kubectl get pods -l app=fraiseql
```

#### Using Static Manifests

```bash
# Deploy basic configuration
kubectl apply -f deploy/kubernetes/deployment.yaml
kubectl apply -f deploy/kubernetes/service.yaml
kubectl apply -f deploy/kubernetes/configmap.yaml

# Deploy with hardened security
kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml

# Verify deployment
kubectl get all -l app=fraiseql
```

## Architecture

### Components

1. **FraiseQL Server** - GraphQL execution engine
   - Compiled schema execution
   - Query caching with TTL
   - Rate limiting middleware
   - Audit logging

2. **PostgreSQL** - Primary data store
   - Connection pooling
   - Prepared statements
   - Backup strategy

3. **Redis** - Cache and queue
   - Query result caching
   - Session management
   - Background jobs

4. **Prometheus** - Metrics collection
   - Performance monitoring
   - Alert generation
   - Historical data

## Configuration

### Environment Variables

```bash
# Logging
RUST_LOG=info                    # Log level (debug, info, warn, error)

# Database
DATABASE_URL=postgresql://user:pass@host:5432/db
DB_POOL_MIN=5
DB_POOL_MAX=20
DB_TIMEOUT=30

# Server
PORT=8815
GRAPHQL_PATH=/graphql
COMPLEXITY_LIMIT=1000

# Security
RATE_LIMIT_ENABLED=true
AUDIT_LOG_ENABLED=true
```

### Configuration File (fraiseql.toml)

```toml
[server]
port = 8815
graphql_path = "/graphql"

[security]
rate_limiting.enabled = true
audit_logging.enabled = true

[database]
pool.min_size = 5
pool.max_size = 20
```

## Deployment Checklist

- [ ] Docker image built and scanned for vulnerabilities
- [ ] SBOM generated and reviewed
- [ ] Database migrations completed
- [ ] Environment variables configured
- [ ] TLS certificates installed
- [ ] Health checks verified
- [ ] Monitoring configured
- [ ] Backup strategy implemented
- [ ] Disaster recovery tested
- [ ] Security policies enforced

## Monitoring

### Health Checks

```bash
# Liveness probe (is service running?)
curl http://localhost:8815/health

# Readiness probe (can it handle traffic?)
curl http://localhost:8815/ready
```

### Metrics

Access Prometheus at http://localhost:9090

Key metrics:
- `fraiseql_query_duration_ms` - Query execution time
- `fraiseql_cache_hits` - Cache hit rate
- `fraiseql_errors_total` - Error count
- `fraiseql_connections_active` - Active connections

### Logs

View logs:
```bash
# Docker Compose
docker-compose logs -f fraiseql

# Kubernetes
kubectl logs -f deployment/fraiseql
```

## Scaling

### Horizontal Scaling

Kubernetes automatically scales based on CPU/memory:
```bash
kubectl autoscale deployment fraiseql --min=3 --max=10
```

### Performance Tuning

1. **Query Caching**: Configure TTL in configuration
2. **Connection Pooling**: Adjust pool size based on load
3. **Index Optimization**: Monitor slow queries

## Backup & Recovery

### Database Backup

```bash
# PostgreSQL backup
pg_dump -h localhost -U fraiseql fraiseql > backup.sql

# Restore
psql -h localhost -U fraiseql fraiseql < backup.sql
```

### Disaster Recovery

See DEPLOYMENT_RUNBOOKS.md for recovery procedures.

## Troubleshooting

### Connection Issues

```bash
# Check PostgreSQL connectivity
psql -h localhost -U fraiseql -d fraiseql -c "SELECT 1"

# Check Redis connectivity
redis-cli -h localhost ping
```

### High Memory Usage

1. Reduce query cache TTL
2. Decrease connection pool size
3. Monitor for memory leaks

### Slow Queries

1. Check Prometheus metrics
2. Review query plans with EXPLAIN ANALYZE
3. Add appropriate indexes

## Security

- All traffic over TLS
- Non-root containers (UID 65532)
- Network policies enforce zero-trust
- Rate limiting on auth endpoints
- Audit logging for compliance
- Secrets managed via external systems

See DEPLOYMENT_SECURITY.md for detailed security architecture.
