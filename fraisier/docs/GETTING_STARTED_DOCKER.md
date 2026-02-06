# Getting Started with Fraisier + Docker Compose

**Perfect For**: Complete local development, testing all components, pre-production validation

**Stack**: Fraisier + PostgreSQL + Prometheus + Grafana + NATS

**Time to Production**: 5 minutes (full stack)

---

## Quick Start

### One Command Setup

```bash
git clone https://github.com/your-org/fraisier.git
cd fraisier
docker-compose up -d
```

That's it! All services are running.

---

## What Gets Started

```
┌─────────────────────────────────────────┐
│         Full Fraisier Stack             │
├─────────────────────────────────────────┤
│ Fraisier API      | http://localhost:8000 │
│ PostgreSQL        | localhost:5432        │
│ Prometheus        | http://localhost:9090 │
│ Grafana           | http://localhost:3000 │
│ NATS JetStream    | nats://localhost:4222 │
│ pgAdmin (optional)| http://localhost:5050 │
└─────────────────────────────────────────┘
```

---

## Access Services

### Fraisier API

```bash
# Health check
curl http://localhost:8000/health

# Deploy a service
curl -X POST http://localhost:8000/api/v1/deployments/my_api/development \
  -H "Content-Type: application/json" \
  -d '{"version":"2.0.0"}'
```

### Grafana Dashboards

```bash
# Open browser
open http://localhost:3000

# Default credentials
# Username: admin
# Password: admin
```

Navigate to Dashboards → Fraisier to see deployment metrics.

### Prometheus Metrics

```bash
# Browse metrics
open http://localhost:9090

# Query deployment metrics
http://localhost:9090/graph?expr=fraisier_deployments_total
```

### NATS Event Bus

```bash
# Subscribe to events
nats sub "fraisier.>"

# Or from Docker
docker exec fraisier nats sub "fraisier.>"
```

### PostgreSQL Database

```bash
# Connect
docker exec -it fraisier-postgres psql -U fraisier -d fraisier

# Or via pgAdmin
open http://localhost:5050
# Email: pgadmin@example.com
# Password: admin
```

---

## Configuration

### Modify docker-compose.yml

**Change PostgreSQL password**:

```yaml
services:
  postgres:
    environment:
      POSTGRES_PASSWORD: your_secure_password
```

**Change Fraisier port**:

```yaml
services:
  fraisier:
    ports:
      - "8080:8000"  # Access at localhost:8080
```

**Add more replicas**:

```yaml
services:
  fraisier:
    deploy:
      replicas: 3
```

### Modify .env

```bash
# Database
POSTGRES_USER=fraisier
POSTGRES_PASSWORD=fraisier_password
POSTGRES_DB=fraisier

# Fraisier
FRAISIER_LOG_LEVEL=INFO
FRAISIER_WORKERS=4

# NATS
NATS_SERVERS=nats://nats:4222

# Grafana
GF_SECURITY_ADMIN_PASSWORD=admin
```

---

## Common Tasks

### Deploy a Service

```bash
# Using CLI
fraisier deploy my_api development

# Or using API
curl -X POST http://localhost:8000/api/v1/deployments/my_api/development
```

### View Logs

```bash
# Fraisier logs
docker-compose logs -f fraisier

# All services
docker-compose logs -f

# Specific service
docker-compose logs -f postgres
```

### Check Status

```bash
# All services
docker-compose ps

# Service health
fraisier health
```

### Execute Commands

```bash
# Run CLI command in container
docker-compose exec fraisier fraisier list

# Open shell
docker-compose exec fraisier bash

# Access database
docker-compose exec postgres psql -U fraisier -d fraisier
```

---

## Development Workflow

### 1. Start Stack

```bash
docker-compose up -d
docker-compose ps  # Verify all services started
```

### 2. Configure Service

Create `fraises.yaml`:

```yaml
fraises:
  my_api:
    type: api
    git_provider: github
    git_repo: your-org/my-api
    git_branch: main
    environments:
      development:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.yml
          service: api
```

### 3. Deploy

```bash
fraisier deploy my_api development

# Monitor
fraisier status my_api development --watch
```

### 4. View Metrics

```bash
# Real-time metrics
fraisier metrics

# Grafana dashboard
open http://localhost:3000
```

### 5. Troubleshoot

```bash
# View logs
docker-compose logs -f fraisier

# Check database
docker-compose exec postgres psql -U fraisier -d fraisier -c "SELECT * FROM tb_deployment;"

# Subscribe to events
docker-compose exec nats nats sub "fraisier.>"
```

---

## Scaling

### Run Multiple Instances

```bash
# Start 3 Fraisier instances
docker-compose up -d --scale fraisier=3

# Behind load balancer
# Add to docker-compose.yml:
services:
  nginx:
    image: nginx:latest
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
```

### Increase Database Connection Pool

```yaml
services:
  postgres:
    environment:
      POSTGRES_INIT_ARGS: "-c max_connections=500"
```

---

## Data Persistence

### Volume Mounts

Data persists in Docker volumes:

```bash
# View volumes
docker volume ls | grep fraisier

# Inspect volume
docker volume inspect fraisier_postgres_data

# Backup volume
docker run --rm -v fraisier_postgres_data:/data -v /backups:/backups \
  alpine tar czf /backups/postgres_data.tar.gz -C /data .
```

### Export Database

```bash
docker-compose exec postgres pg_dump -U fraisier fraisier > backup.sql
```

### Restore Database

```bash
cat backup.sql | docker-compose exec -T postgres psql -U fraisier -d fraisier
```

---

## Monitoring

### Real-time Dashboard

```bash
fraisier health --watch
```

### View Metrics

```bash
# In Grafana
http://localhost:3000/dashboards

# In Prometheus
http://localhost:9090/graph

# Via API
curl http://localhost:9090/api/v1/query?query=fraisier_deployments_total
```

### Export Logs

```bash
# View all logs
docker-compose logs > fraisier.log

# Export specific service
docker-compose logs fraisier > fraisier.log
```

---

## Production Preparation

### Before Deploying to Production

```bash
# 1. Test with realistic data
fraisier deploy my_api development
fraisier deploy my_api staging

# 2. Check backup strategy
docker-compose exec postgres pg_dump -U fraisier fraisier > /tmp/test.sql

# 3. Monitor resource usage
docker stats

# 4. Review logs
docker-compose logs fraisier | grep ERROR

# 5. Verify webhooks
fraisier webhook list

# 6. Test rollback
fraisier rollback my_api staging
```

### Migrate to Production

For production, use managed services:

```yaml
# Production setup
database:
  type: postgresql
  url: postgresql://user:password@prod.rds.example.com:5432/fraisier

cache:
  type: redis
  url: redis://prod.elasticache.example.com:6379

events:
  type: nats
  servers: nats-cluster.example.com:4222
```

---

## Troubleshooting

### Services Not Starting

```bash
# Check logs
docker-compose logs

# Verify all images downloaded
docker-compose pull

# Rebuild from scratch
docker-compose down -v
docker-compose up -d
```

### Port Already in Use

```bash
# Find what's using the port
lsof -i :8000

# Or change in docker-compose.yml
ports:
  - "8080:8000"  # Use 8080 instead of 8000
```

### Database Connection Issues

```bash
# Test connection
docker-compose exec fraisier psql postgresql://fraisier:password@postgres/fraisier -c "SELECT 1;"

# Check network
docker-compose exec fraisier ping postgres

# View network
docker network ls
```

### Out of Memory

```bash
# Check resource usage
docker stats

# Limit container resources
deploy:
  resources:
    limits:
      memory: 2G
    reservations:
      memory: 1G
```

### Container Keeps Restarting

```bash
# Check logs
docker-compose logs fraisier

# View restart count
docker-compose ps

# Debug
docker-compose run -it fraisier bash
```

---

## Cleanup

### Stop All Services

```bash
docker-compose down
```

### Remove Everything (Including Data)

```bash
docker-compose down -v
```

### Clean Up Unused Resources

```bash
docker system prune -a
docker volume prune
```

---

## Performance Tips

1. **Use host.docker.internal for Mac/Windows**:

   ```yaml
   services:
     fraisier:
       environment:
         POSTGRES_HOST: host.docker.internal
   ```

2. **Increase Docker Desktop Resources**:
   - Docker Desktop → Preferences → Resources
   - Memory: 4GB+
   - CPU: 2+ cores

3. **Use tmpfs for logs**:

   ```yaml
   services:
     fraisier:
       tmpfs: /tmp
   ```

4. **Enable BuildKit**:

   ```bash
   DOCKER_BUILDKIT=1 docker-compose build
   ```

---

## Advanced Configuration

### Add Custom Services

```yaml
services:
  my_service:
    build: ./services/my-service
    depends_on:
      - postgres
    environment:
      DATABASE_URL: postgresql://fraisier:password@postgres/fraisier
```

### Use External Networks

```yaml
services:
  fraisier:
    networks:
      - external_network

networks:
  external_network:
    external: true
```

### Mount Local Code

```yaml
services:
  fraisier:
    volumes:
      - ./fraisier:/app/fraisier  # Local development
      - /app/node_modules          # Exclude node_modules
```

---

## Reference

- [docker-compose.yml](../docker-compose.yml) - Complete configuration
- [CLI_REFERENCE.md](CLI_REFERENCE.md) - CLI commands
- [API_REFERENCE.md](API_REFERENCE.md) - API endpoints
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues

---

## Quick Reference

```bash
# Start all services
docker-compose up -d

# View status
docker-compose ps

# View logs
docker-compose logs -f

# Execute command
docker-compose exec fraisier fraisier deploy my_api development

# Stop all services
docker-compose down

# Remove everything
docker-compose down -v
```

---

**Ready to deploy?**

```bash
docker-compose up -d
fraisier deploy my_api development
```

Happy deploying! 🚀
