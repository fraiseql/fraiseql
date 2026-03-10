# Docker Deployment Guide for Fraisier

This guide covers deploying Fraisier using Docker and docker-compose.

## Quick Start

### Development Environment

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f fraisier

# Stop all services
docker-compose down
```

### Access Services

- **Fraisier API**: http://localhost:8000
- **Fraisier Metrics**: http://localhost:8001/metrics
- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000 (admin/admin)
- **PostgreSQL**: localhost:5432

## Building the Docker Image

### Manual Build

```bash
# Build for development
docker build -t fraisier:latest .

# Build with specific version
docker build -t fraisier:v0.1.0 .

# Build without cache
docker build --no-cache -t fraisier:latest .
```

### Multi-Stage Build

The Dockerfile uses a multi-stage build for optimal image size:

1. **Builder Stage**: Installs build dependencies and compiles Python packages
2. **Runtime Stage**: Minimal image with only runtime dependencies

This results in:

- Smaller image size (~500MB vs 1.5GB)
- Better security (no build tools in production)
- Faster deployments

## Environment Configuration

### Required Environment Variables

```bash
# Database
DATABASE_URL=postgresql://fraisier:fraisier_password@postgres:5432/fraisier

# Logging
FRAISIER_LOG_LEVEL=INFO

# Metrics
PROMETHEUS_PORT=8001

# Webhooks
FRAISIER_WEBHOOK_SECRET=your-secret-key
```

### Optional Environment Variables

```bash
# Grafana
GF_SECURITY_ADMIN_PASSWORD=your-password
GF_SECURITY_ADMIN_USER=admin

# Redis
REDIS_PASSWORD=your-redis-password
```

## Production Deployment

### Using docker-compose with Production Overrides

```bash
# Deploy with production overrides and resource limits
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

# View status
docker-compose ps

# Check logs
docker-compose logs -f fraisier
```

### Scaling

```bash
# Scale Fraisier to 3 instances
docker-compose up -d --scale fraisier=3

# Note: This requires load balancer configuration
```

## Monitoring and Observability

### Prometheus Metrics

Fraisier exposes Prometheus metrics on port 8001:

```bash
# View metrics
curl http://localhost:8001/metrics

# Key metrics:
# - fraisier_deployment_total: Total deployments
# - fraisier_deployment_failed_total: Failed deployments
# - fraisier_deployment_duration_seconds: Deployment duration
# - fraisier_deployment_error_rate: Error rate during deployment
# - fraisier_health_check_failures_total: Health check failures
```

### Grafana Dashboards

Grafana is automatically configured with:

- **Fraisier Deployments Dashboard**: Shows deployment metrics, error rates, latency
- **System Metrics**: CPU, memory, disk usage

Access: http://localhost:3000
Default credentials: admin/admin

### Log Aggregation

Logs are stored in the `fraisier-logs` volume:

```bash
# View logs
docker volume inspect fraisier-logs

# Access logs directory
docker exec fraisier ls -la /var/log/fraisier/
```

## Health Checks

### Fraisier Health Endpoint

```bash
# Check service health
curl http://localhost:8001/metrics

# Healthy response: 200 with Prometheus metrics

# Docker compose uses this for service dependency
```

### PostgreSQL Health Check

```bash
# Check database connectivity
docker exec fraisier-postgres pg_isready -U fraisier -d fraisier
```

## Database Migrations

### Automatic Setup

The PostgreSQL initialization script (`scripts/postgres-init.sql`) automatically:

- Creates necessary extensions (uuid-ossp, pg_trgm)
- Creates schemas and tables
- Sets up indexes and permissions

### Manual Migration

If needed, run migrations manually:

```bash
docker exec fraisier-postgres psql -U fraisier -d fraisier -f /docker-entrypoint-initdb.d/init.sql
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker-compose logs fraisier

# Common issues:
# 1. Port already in use: Change port in docker-compose.yml
# 2. Database connection failed: Ensure postgres service is healthy
# 3. Out of memory: Increase Docker memory limits
```

### Database Connection Issues

```bash
# Test database connection
docker exec fraisier psql $DATABASE_URL

# Check PostgreSQL is running and healthy
docker-compose ps postgres

# View PostgreSQL logs
docker-compose logs postgres
```

### Metrics Not Showing in Prometheus

```bash
# Check Prometheus configuration
curl http://localhost:9090/api/targets

# Verify Fraisier metrics endpoint
curl http://localhost:8001/metrics

# Check Prometheus logs
docker-compose logs prometheus
```

### High Memory Usage

```bash
# Monitor resource usage
docker stats

# Reduce retention in prometheus.yml:
# --storage.tsdb.retention.time=7d  # Reduce from 15d

# Restart with reduced retention
docker-compose restart prometheus
```

## Advanced Configuration

### Custom Prometheus Configuration

Edit `monitoring/prometheus.yml` to:

- Add custom scrape targets
- Configure alerting
- Add custom dashboards

Restart Prometheus:

```bash
docker-compose restart prometheus
```

### Custom Grafana Dashboards

1. Create dashboard in Grafana UI
2. Export as JSON
3. Save to `monitoring/grafana/dashboards/`
4. Restart Grafana

```bash
docker-compose restart grafana
```

### Adding PostgreSQL Exporter

To monitor PostgreSQL metrics in Prometheus:

1. Add service to docker-compose.yml:

```yaml
postgres_exporter:
  image: prometheuscommunity/postgres-exporter
  environment:
    DATA_SOURCE_NAME: postgresql://fraisier:fraisier_password@postgres:5432/fraisier?sslmode=disable
  ports:
    - "9187:9187"
  depends_on:
    - postgres
```

2. Uncomment postgres job in `monitoring/prometheus.yml`

3. Restart services:

```bash
docker-compose up -d
```

## Backup and Restore

### Backup PostgreSQL Database

```bash
# Create backup
docker exec fraisier-postgres pg_dump -U fraisier fraisier > backup.sql

# Create compressed backup
docker exec fraisier-postgres pg_dump -U fraisier fraisier | gzip > backup.sql.gz
```

### Restore PostgreSQL Database

```bash
# From SQL file
docker exec -i fraisier-postgres psql -U fraisier fraisier < backup.sql

# From compressed file
gunzip -c backup.sql.gz | docker exec -i fraisier-postgres psql -U fraisier fraisier
```

### Backup Prometheus Data

```bash
# Prometheus data is in named volume
docker volume ls | grep prometheus

# Backup volume
docker run --rm -v fraisier_prometheus-data:/data -v $(pwd):/backup alpine tar czf /backup/prometheus-backup.tar.gz -C /data .
```

## Security Considerations

1. **Change Default Passwords**
   - PostgreSQL: Set POSTGRES_PASSWORD
   - Grafana: Set GF_SECURITY_ADMIN_PASSWORD
   - Redis: Set REDIS_PASSWORD

2. **Network Security**
   - Use custom bridge network (already configured)
   - Don't expose database port in production
   - Use TLS for external connections

3. **Secrets Management**
   - Use Docker secrets or environment files
   - Don't commit secrets to version control
   - Rotate webhook secrets regularly

4. **Access Control**
   - Restrict Prometheus/Grafana access
   - Implement authentication for Fraisier API
   - Use IP whitelisting in production

## Performance Tuning

### PostgreSQL

```yaml
postgres:
  environment:
    POSTGRES_INITDB_ARGS: "-c max_connections=200 -c shared_buffers=256MB"
```

### Prometheus

```bash
# Increase retention

--storage.tsdb.retention.time=30d

# Increase scrape parallelism

--query.max-concurrency=20
```

### Grafana

```yaml
grafana:
  environment:
    GF_SERVER_MAX_OPEN_CONNS: 200
```

## CI/CD Integration

See `.github/workflows/ci.yml` for GitHub Actions pipeline that:

- Runs tests on every PR
- Builds Docker image
- Pushes to registry
- Deploys to production

## Support and Documentation

- **Prometheus Docs**: https://prometheus.io/docs/
- **Grafana Docs**: https://grafana.com/docs/
- **PostgreSQL Docs**: https://www.postgresql.org/docs/
- **Docker Docs**: https://docs.docker.com/
