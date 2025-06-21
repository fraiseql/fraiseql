# Docker Deployment Guide

This guide covers deploying FraiseQL using Docker for both development and production environments.

## Table of Contents
- [Quick Start](#quick-start)
- [Development Setup](#development-setup)
- [Production Deployment](#production-deployment)
- [Configuration](#configuration)
- [Monitoring Setup](#monitoring-setup)
- [Security Best Practices](#security-best-practices)
- [Troubleshooting](#troubleshooting)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql

# Start development environment
docker-compose up -d

# View logs
docker-compose logs -f fraiseql

# Access services
# - FraiseQL API: http://localhost:8000
# - GraphQL Playground: http://localhost:8000/playground
# - Prometheus: http://localhost:9090
# - Grafana: http://localhost:3000 (admin/admin)
# - Jaeger: http://localhost:16686
```

## Development Setup

### Prerequisites
- Docker Engine 20.10+
- Docker Compose 2.0+
- 4GB+ available memory

### Starting Development Environment

```bash
# Start all services
docker-compose up -d

# Start only core services (without monitoring)
docker-compose up -d postgres fraiseql

# Rebuild after code changes
docker-compose build fraiseql
docker-compose up -d fraiseql

# View real-time logs
docker-compose logs -f fraiseql

# Stop all services
docker-compose down

# Stop and remove volumes (careful - deletes data!)
docker-compose down -v
```

### Development Features

The development setup includes:
- PostgreSQL database with automatic initialization
- Hot-reload for code changes
- Full monitoring stack (Prometheus, Grafana, Jaeger)
- GraphQL Playground enabled
- Development authentication (admin/admin123)

### Customizing Development

Edit `docker-compose.yml` to customize:

```yaml
services:
  fraiseql:
    environment:
      # Change development credentials
      FRAISEQL_DEV_USERNAME: myuser
      FRAISEQL_DEV_PASSWORD: mypassword

      # Disable features
      FRAISEQL_ENABLE_PLAYGROUND: "false"

      # Custom database
      DATABASE_URL: postgresql://user:pass@host:5432/db
```

## Production Deployment

### Building Production Image

```bash
# Build optimized production image
docker build -t fraiseql:latest .

# Tag for registry
docker tag fraiseql:latest registry.example.com/fraiseql:v1.0.0

# Push to registry
docker push registry.example.com/fraiseql:v1.0.0
```

### Production Docker Compose

```bash
# Create .env file with production settings
cat > .env.prod << EOF
DATABASE_URL=postgresql://user:password@db.example.com:5432/production
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_API_IDENTIFIER=https://api.example.com
TRACING_ENDPOINT=http://jaeger:4317
APP_VERSION=1.0.0
REPLICAS=3
EOF

# Start production stack
docker-compose -f docker-compose.prod.yml --env-file .env.prod up -d

# Scale horizontally
docker-compose -f docker-compose.prod.yml up -d --scale fraiseql=4
```

### Production Health Checks

```bash
# Check service health
curl http://localhost:8000/health

# Check readiness (includes database connectivity)
curl http://localhost:8000/ready

# Check metrics
curl http://localhost:8000/metrics
```

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATABASE_URL` | PostgreSQL connection string | - | Yes |
| `FRAISEQL_PRODUCTION` | Enable production mode | false | No |
| `FRAISEQL_AUTO_CAMEL_CASE` | Auto convert snake_case | true | No |
| `FRAISEQL_AUTH_PROVIDER` | Auth provider (dev/auth0) | dev | No |
| `AUTH0_DOMAIN` | Auth0 domain | - | If auth0 |
| `AUTH0_API_IDENTIFIER` | Auth0 API identifier | - | If auth0 |
| `FRAISEQL_ENABLE_METRICS` | Enable Prometheus metrics | true | No |
| `FRAISEQL_ENABLE_TRACING` | Enable OpenTelemetry | true | No |
| `FRAISEQL_TRACING_ENDPOINT` | Tracing collector URL | - | If tracing |
| `FRAISEQL_TRACING_SAMPLE_RATE` | Trace sampling (0.0-1.0) | 1.0 | No |

### Resource Limits

Production deployment includes resource limits:

```yaml
deploy:
  resources:
    limits:
      cpus: '2'
      memory: 2G
    reservations:
      cpus: '0.5'
      memory: 512M
```

Adjust based on your workload:
- Small (< 100 req/s): 0.5 CPU, 512MB RAM
- Medium (< 1000 req/s): 1 CPU, 1GB RAM
- Large (< 10000 req/s): 2+ CPU, 2GB+ RAM

## Monitoring Setup

### Prometheus Configuration

Create `docker/prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['fraiseql:8000']
    metrics_path: '/metrics'
    scrape_interval: 5s

  - job_name: 'node'
    static_configs:
      - targets: ['prometheus-exporter:9100']
```

### Grafana Dashboards

Import pre-built dashboards:

1. Access Grafana at http://localhost:3000
2. Login with admin/admin
3. Import dashboards from `docker/grafana/dashboards/`
   - `fraiseql-overview.json` - Application metrics
   - `fraiseql-performance.json` - Performance metrics
   - `fraiseql-errors.json` - Error tracking

### Distributed Tracing

Access Jaeger UI at http://localhost:16686 to:
- View request traces
- Analyze performance bottlenecks
- Debug distributed transactions

## Security Best Practices

### 1. Image Security

```bash
# Scan image for vulnerabilities
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock \
  aquasec/trivy image fraiseql:latest

# Use specific versions, not latest
FROM python:3.11.7-slim  # Good
FROM python:latest       # Bad
```

### 2. Runtime Security

```yaml
# Add security options to docker-compose
services:
  fraiseql:
    security_opt:
      - no-new-privileges:true
    read_only: true
    tmpfs:
      - /tmp
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE
```

### 3. Secrets Management

Never hardcode secrets. Use:

```bash
# Docker secrets (Swarm mode)
echo "mypassword" | docker secret create db_password -
docker service create --secret db_password fraiseql

# Environment file (chmod 600)
echo "DATABASE_URL=postgresql://..." > .env
docker-compose --env-file .env up

# External secret management
# - HashiCorp Vault
# - AWS Secrets Manager
# - Kubernetes Secrets
```

### 4. Network Security

```yaml
# Isolate services with networks
networks:
  frontend:
  backend:
    internal: true

services:
  fraiseql:
    networks:
      - frontend
      - backend

  postgres:
    networks:
      - backend  # Only accessible internally
```

## Troubleshooting

### Common Issues

#### 1. Container fails to start

```bash
# Check logs
docker-compose logs fraiseql

# Common causes:
# - Missing environment variables
# - Database connection issues
# - Port already in use
```

#### 2. Database connection errors

```bash
# Test database connectivity
docker-compose exec fraiseql bash
apt-get update && apt-get install -y postgresql-client
psql $DATABASE_URL -c "SELECT 1"

# Check network
docker-compose exec fraiseql ping postgres
```

#### 3. Performance issues

```bash
# Check resource usage
docker stats fraiseql

# Increase resources
docker-compose up -d --scale fraiseql=4

# Check for N+1 queries
# Enable debug logging
FRAISEQL_LOG_LEVEL=DEBUG docker-compose up
```

#### 4. Memory leaks

```bash
# Monitor memory over time
docker stats --no-stream --format "table {{.Container}}\t{{.MemUsage}}"

# Set memory limits and restart policy
deploy:
  resources:
    limits:
      memory: 1G
  restart_policy:
    condition: any
    max_attempts: 3
```

### Debug Mode

Enable debug mode for troubleshooting:

```yaml
environment:
  FRAISEQL_DEBUG: "true"
  FRAISEQL_LOG_LEVEL: "DEBUG"
  PYTHONFAULTHANDLER: "1"
```

### Container Shell Access

```bash
# Access running container
docker-compose exec fraiseql bash

# Run one-off commands
docker-compose run --rm fraiseql python -c "import fraiseql; print(fraiseql.__version__)"

# Test GraphQL queries
docker-compose exec fraiseql curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name } } }"}'
```

## Next Steps

- [Kubernetes Deployment](./kubernetes.md) - For container orchestration
- [Cloud Deployment](./cloud.md) - AWS, GCP, Azure guides
- [Monitoring Setup](./monitoring.md) - Advanced monitoring configuration
- [Security Hardening](./security.md) - Production security checklist
