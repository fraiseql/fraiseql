# Deploying to Docker Compose with Fraisier

**Perfect For**: Development environments, staging servers, containerized deployments, microservices

**Components**: Docker + docker-compose

**Setup Time**: 10-15 minutes

---

## Overview

The Docker Compose provider deploys services using docker-compose for orchestration. This is ideal for:

- ✅ Development and testing environments
- ✅ Staging servers
- ✅ Microservices deployments
- ✅ Rapid prototyping
- ✅ Container-based infrastructure

---

## Prerequisites

### Server Requirements

- Docker 20.10+ installed
- docker-compose 2.0+ installed
- Git installed
- 2GB+ available disk space

### Client Requirements

- Fraisier CLI installed
- SSH access to server (optional, for remote deployments)
- Docker compose file in repository

---

## Step 1: Prepare Docker Compose File

### Basic docker-compose.yml

```yaml
version: '3.9'

services:
  api:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: my-api
    ports:
      - "8000:8000"
    environment:
      - NODE_ENV=production
      - DATABASE_URL=postgresql://postgres:password@db:5432/app
      - REDIS_URL=redis://redis:6379
    depends_on:
      - db
      - redis
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 10s
    restart: unless-stopped
    volumes:
      - ./logs:/app/logs
    networks:
      - app-network

  db:
    image: postgres:15-alpine
    container_name: my-db
    environment:
      - POSTGRES_DB=app
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=password
    volumes:
      - db-data:/var/lib/postgresql/data
    networks:
      - app-network
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    container_name: my-redis
    networks:
      - app-network
    restart: unless-stopped

volumes:
  db-data:

networks:
  app-network:
    driver: bridge
```

### Key Configuration

| Setting | Purpose | Example |
|---------|---------|---------|
| `build` | Build image from Dockerfile | See below |
| `environment` | Environment variables | DATABASE_URL=... |
| `ports` | Port mappings | "8000:8000" |
| `healthcheck` | Health check definition | curl http://localhost:8000/health |
| `depends_on` | Service dependencies | [db, redis] |
| `volumes` | Data persistence | db-data:/var/lib/postgresql/data |
| `networks` | Service networking | app-network |

---

## Step 2: Create Dockerfile

### Python Flask Example

```dockerfile
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application
COPY . .

# Health check
HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
  CMD curl -f http://localhost:8000/health || exit 1

# Run application
CMD ["python", "-m", "flask", "run", "--host=0.0.0.0"]
```

### Node.js Express Example

```dockerfile
FROM node:18-alpine

WORKDIR /app

# Install dependencies
COPY package*.json ./
RUN npm ci --only=production

# Copy application
COPY . .

# Health check
HEALTHCHECK --interval=10s --timeout=5s --retries=3 \
  CMD node -e "require('http').get('http://localhost:8000/health', (r) => {if (r.statusCode !== 200) throw new Error()})"

# Run application
CMD ["npm", "start"]
```

---

## Step 3: Configure Fraisier Provider

### In fraises.yaml

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

          # Health check configuration
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
            max_retries: 3

          # Deployment options
          deployment_strategy: rolling
          build_cache: true  # Use Docker build cache

      staging:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.staging.yml
          service: api

      production:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.prod.yml
          service: api
          deployment_strategy: blue_green
```

### Configuration Reference

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `docker_compose_file` | string | ./docker-compose.yml | Path to docker-compose file |
| `service` | string | Required | Service name in docker-compose |
| `working_directory` | string | . | Working directory for docker-compose |
| `health_check.type` | string | http | Check type (http, tcp, docker) |
| `health_check.url` | string | http://localhost:8000/health | Health check URL |
| `build_cache` | boolean | true | Use Docker build cache |
| `pull_images` | boolean | true | Pull latest images |
| `deployment_strategy` | string | rolling | Strategy (rolling, blue_green) |

---

## Step 4: Deploy

### Local Development Deployment

```bash
# Start all services
docker-compose up -d

# Verify services are running
docker-compose ps

# Check logs
docker-compose logs -f api

# Deploy via Fraisier
fraisier deploy my_api development
```

### Output Example

```
Starting deployment...

✓ Code pulled (3 commits)
✓ Docker image built (2 commits)
✓ Docker images pulled
✓ Service updated (8.5s)
✓ Health checks passed (50ms)

✅ Deployment successful in 15 seconds

Service: my_api
Status: healthy
Version: 2.0.0
Port: 8000
```

### Remote Server Deployment

For staging/production on remote servers, use SSH:

```yaml
provider_config:
  remote:
    enabled: true
    ssh_host: staging.example.com
    ssh_user: deploy
    ssh_key_path: ~/.ssh/fraisier
    working_directory: /opt/my-api
```

Then deploy:

```bash
fraisier deploy my_api staging
# Will SSH to staging server and run docker-compose commands
```

---

## Step 5: Manage Services

### View Service Status

```bash
# Check all services
docker-compose ps

# Check specific service
docker-compose ps api

# Detailed information
docker inspect my-api

# Service logs
docker-compose logs api -f
```

### Scale Services

```yaml
# docker-compose.yml with scaling
services:
  api:
    deploy:
      replicas: 3  # Run 3 instances
    ports:
      - "8000-8002:8000"  # Port range

  worker:
    deploy:
      replicas: 5  # Run 5 worker instances
```

Scale at runtime:

```bash
# Scale using docker-compose
docker-compose up -d --scale api=3

# Or manually
docker-compose up -d && docker-compose up -d --scale api=2
```

### Update Single Service

```bash
# Update and restart specific service
docker-compose up -d --build api

# Without rebuild
docker-compose up -d api

# Pull new image and restart
docker-compose pull api && docker-compose up -d api
```

---

## Data Persistence

### Volume Configuration

```yaml
services:
  api:
    volumes:
      - ./data:/app/data           # Mount local directory
      - api-cache:/app/cache        # Named volume
      - /etc/ssl/certs:/certs:ro   # Read-only mount

  db:
    volumes:
      - postgres-data:/var/lib/postgresql/data  # Data persistence

volumes:
  postgres-data:
  api-cache:
```

### Backup Volumes

```bash
# Backup database volume
docker run --rm -v postgres-data:/data -v /backups:/backups \
  alpine tar czf /backups/db-backup-$(date +%Y-%m-%d).tar.gz -C /data .

# Backup all data
docker-compose exec db pg_dump -U postgres app > backup.sql
```

### Restore Volumes

```bash
# Restore database
docker-compose exec db psql -U postgres app < backup.sql

# Or restore volume
docker run --rm -v postgres-data:/data -v /backups:/backups \
  alpine tar xzf /backups/db-backup-2024-01-22.tar.gz -C /data
```

---

## Networking

### Service Communication

Services can communicate using service names as hostnames:

```python
# Python example
import os
db_url = os.environ.get('DATABASE_URL', 'postgresql://postgres:password@db:5432/app')

# Python code
import psycopg
conn = psycopg.connect(db_url)
```

Services are on network `app-network`, so use:

- `db` instead of `localhost`
- `redis` instead of `127.0.0.1`

### External Network

Connect multiple docker-compose files:

```yaml
networks:
  app-network:
    external: true
```

Create network:

```bash
docker network create app-network
```

---

## Monitoring & Logging

### View Logs

```bash
# Real-time logs for one service
docker-compose logs -f api

# Last 100 lines
docker-compose logs --tail=100 api

# Logs from last hour
docker-compose logs --timestamps api | grep $(date -d '1 hour ago')

# All services
docker-compose logs -f
```

### Container Statistics

```bash
# Real-time stats
docker stats

# JSON format for parsing
docker stats --no-stream --format "{{json .}}"
```

### Health Check Status

```bash
# View health status
docker inspect my-api | grep -A 10 "Health"

# Manual health check
curl http://localhost:8000/health

# Inside container
docker-compose exec api curl http://localhost:8000/health
```

---

## Deployment Strategies

### Rolling Deployment

Deploy one instance at a time:

```yaml
provider_config:
  deployment_strategy: rolling
```

**Flow**:

1. Build new image
2. Stop current service
3. Start new service
4. Run health checks
5. If healthy, complete

### Blue-Green Deployment

```yaml
provider_config:
  deployment_strategy: blue_green
```

**Flow**:

1. Build new image as `api-green`
2. Start green service
3. Run health checks on green
4. If healthy, switch traffic to green
5. Keep blue as fallback

### Canary Deployment

```yaml
provider_config:
  deployment_strategy: canary
  canary_percentage: 10  # 10% traffic to new version
```

---

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker-compose logs api

# Check image
docker images | grep my-api

# Rebuild
docker-compose build --no-cache api

# Try running manually
docker run -it my-api bash
```

### Health Check Failing

```bash
# Test endpoint manually
docker-compose exec api curl -v http://localhost:8000/health

# Check if port is open
docker-compose exec api netstat -tlnp | grep 8000

# Check application logs
docker-compose logs -f api

# Run interactive shell
docker-compose exec api bash
```

### Out of Memory

```bash
# Check memory usage
docker stats api

# Limit container memory
services:
  api:
    deploy:
      resources:
        limits:
          memory: 512M
        reservations:
          memory: 256M
```

### Network Issues

```bash
# Check network
docker network ls

# Inspect network
docker network inspect app-network

# Check service on network
docker-compose exec api ping db  # Should work
```

---

## Production Best Practices

### 1. Environment Separation

```
.
├── docker-compose.yml          # Development
├── docker-compose.staging.yml
├── docker-compose.prod.yml
├── .env.example
├── .env.staging
└── .env.prod
```

### 2. Resource Limits

```yaml
services:
  api:
    deploy:
      resources:
        limits:
          cpus: '1'
          memory: 1G
        reservations:
          cpus: '0.5'
          memory: 512M
```

### 3. Restart Policy

```yaml
restart_policy:
  condition: unless-stopped
  delay: 5s
  max_attempts: 5
  window: 120s
```

### 4. Logging Configuration

```yaml
services:
  api:
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"
```

### 5. Security

```yaml
services:
  api:
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE
```

---

## Advanced Configuration

### Multi-Stage Docker Build

```dockerfile
# Build stage
FROM node:18 AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci && npm run build

# Runtime stage
FROM node:18-alpine
WORKDIR /app
COPY --from=builder /app/dist ./dist
COPY package*.json ./
RUN npm ci --only=production
HEALTHCHECK --interval=10s CMD node -e "require('http').get('http://localhost:8000/health', (r) => {if (r.statusCode !== 200) throw new Error()})"
CMD ["node", "dist/index.js"]
```

### Conditional Services

```yaml
services:
  # Only in development
  mailhog:
    image: mailhog/mailhog:latest
    profiles:
      - dev
    ports:
      - "8025:8025"

# Run with dev profile
# docker-compose --profile dev up
```

### Health Check with Custom Script

```yaml
healthcheck:
  test:
    - CMD
    - /bin/sh
    - -c
    - |
      curl -f http://localhost:8000/health && \
      pg_isready -h db -U postgres || exit 1
  interval: 10s
  timeout: 5s
  retries: 3
```

---

## Production Checklist

- [ ] Dockerfile created and tested
- [ ] docker-compose.yml configured
- [ ] Environment variables configured
- [ ] Health check implemented
- [ ] Volumes configured for data persistence
- [ ] Resource limits set
- [ ] Logging configured
- [ ] Restart policies configured
- [ ] Security options applied
- [ ] Backup strategy defined
- [ ] Monitoring configured
- [ ] First deployment successful

---

## Reference

- [Docker Documentation](https://docs.docker.com/)
- [docker-compose Reference](https://docs.docker.com/compose/compose-file/)
- [PROVIDER_BARE_METAL.md](PROVIDER_BARE_METAL.md) - Bare Metal provider
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues
