# Docker Deployment Guide

## Overview

Docker provides the simplest way to deploy FraiseQL in production. This guide covers everything from development setup to production-ready configurations.

## Prerequisites

- Docker 20.10+
- Docker Compose 2.0+
- 2GB+ RAM available
- PostgreSQL 14+ (or use provided Docker Compose)

## Quick Start

### Development Setup

```bash
# Clone the repository
git clone https://github.com/your-org/fraiseql.git
cd fraiseql

# Run with Docker Compose
docker-compose up -d

# Check logs
docker-compose logs -f app

# Access the application
curl http://localhost:8000/graphql
```

## Production Dockerfile

### Multi-Stage Build

```dockerfile
# Build stage
FROM python:3.11-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    g++ \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install Python dependencies
COPY pyproject.toml uv.lock ./
RUN pip install --no-cache-dir uv && \
    uv pip install --system --no-cache -r pyproject.toml

# Runtime stage
FROM python:3.11-slim

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1001 fraiseql && \
    mkdir -p /app && \
    chown -R fraiseql:fraiseql /app

WORKDIR /app

# Copy Python packages from builder
COPY --from=builder /usr/local/lib/python3.11/site-packages /usr/local/lib/python3.11/site-packages
COPY --from=builder /usr/local/bin /usr/local/bin

# Copy application code
COPY --chown=fraiseql:fraiseql . .

# Switch to non-root user
USER fraiseql

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8000/health || exit 1

# Environment configuration
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    FRAISEQL_MODE=production \
    FRAISEQL_PORT=8000 \
    FRAISEQL_HOST=0.0.0.0

EXPOSE 8000

# Run the application
CMD ["uvicorn", "src.fraiseql.main:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "4"]
```

## Docker Compose Configuration

### Development Configuration

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    container_name: fraiseql-postgres
    environment:
      POSTGRES_DB: fraiseql_dev
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: development_password
    volumes:

      - postgres_data:/var/lib/postgresql/data
      - ./scripts/init.sql:/docker-entrypoint-initdb.d/init.sql
    ports:

      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U fraiseql"]
      interval: 10s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    container_name: fraiseql-redis
    command: redis-server --appendonly yes
    volumes:

      - redis_data:/data
    ports:

      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  app:
    build:
      context: .
      dockerfile: deploy/docker/Dockerfile
    container_name: fraiseql-app
    environment:
      DATABASE_URL: postgresql://fraiseql:development_password@postgres:5432/fraiseql_dev
      REDIS_URL: redis://redis:6379/0
      SECRET_KEY: development-secret-key
      FRAISEQL_MODE: development
      LOG_LEVEL: INFO
    ports:

      - "8000:8000"
    volumes:

      - ./src:/app/src
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
    restart: unless-stopped

volumes:
  postgres_data:
  redis_data:
```

### Production Configuration

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    container_name: fraiseql-postgres-prod
    environment:
      POSTGRES_DB: ${POSTGRES_DB}
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_INITDB_ARGS: "--encoding=UTF8 --lc-collate=en_US.utf8 --lc-ctype=en_US.utf8"
    volumes:

      - postgres_data:/var/lib/postgresql/data
      - ./backups:/backups
    networks:

      - fraiseql-network
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER}"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: always
    deploy:
      resources:
        limits:
          memory: 2G
        reservations:
          memory: 1G

  redis:
    image: redis:7-alpine
    container_name: fraiseql-redis-prod
    command: redis-server --appendonly yes --requirepass ${REDIS_PASSWORD}
    volumes:

      - redis_data:/data
    networks:

      - fraiseql-network
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: always
    deploy:
      resources:
        limits:
          memory: 512M
        reservations:
          memory: 256M

  app:
    image: fraiseql:latest
    container_name: fraiseql-app-prod
    environment:
      DATABASE_URL: postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB}
      REDIS_URL: redis://:${REDIS_PASSWORD}@redis:6379/0
      SECRET_KEY: ${SECRET_KEY}
      FRAISEQL_MODE: production
      LOG_LEVEL: ${LOG_LEVEL:-INFO}
      CORS_ORIGINS: ${CORS_ORIGINS}
      MAX_CONNECTIONS: ${MAX_CONNECTIONS:-100}
      STATEMENT_TIMEOUT: ${STATEMENT_TIMEOUT:-30000}
    networks:

      - fraiseql-network
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
    restart: always
    deploy:
      replicas: 2
      resources:
        limits:
          memory: 1G
        reservations:
          memory: 512M
      update_config:
        parallelism: 1
        delay: 10s
        order: start-first

  nginx:
    image: nginx:alpine
    container_name: fraiseql-nginx
    ports:

      - "80:80"
      - "443:443"
    volumes:

      - ./nginx/nginx.conf:/etc/nginx/nginx.conf:ro
      - ./nginx/ssl:/etc/nginx/ssl:ro
      - nginx_cache:/var/cache/nginx
    networks:

      - fraiseql-network
    depends_on:

      - app
    restart: always
    deploy:
      resources:
        limits:
          memory: 256M
        reservations:
          memory: 128M

networks:
  fraiseql-network:
    driver: bridge

volumes:
  postgres_data:
    driver: local
  redis_data:
    driver: local
  nginx_cache:
    driver: local
```

## Nginx Configuration

```nginx
# nginx/nginx.conf
events {
    worker_connections 1024;
}

http {
    upstream fraiseql {
        least_conn;
        server app:8000 max_fails=3 fail_timeout=30s;
    }

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
    limit_req_status 429;

    # Cache settings
    proxy_cache_path /var/cache/nginx levels=1:2 keys_zone=api_cache:10m max_size=100m inactive=1h;

    server {
        listen 80;
        server_name your-domain.com;

        # Redirect to HTTPS
        return 301 https://$server_name$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name your-domain.com;

        # SSL Configuration
        ssl_certificate /etc/nginx/ssl/cert.pem;
        ssl_certificate_key /etc/nginx/ssl/key.pem;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;
        ssl_prefer_server_ciphers on;
        ssl_session_cache shared:SSL:10m;
        ssl_session_timeout 10m;

        # Security headers
        add_header X-Frame-Options "SAMEORIGIN" always;
        add_header X-Content-Type-Options "nosniff" always;
        add_header X-XSS-Protection "1; mode=block" always;
        add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

        # GraphQL endpoint
        location /graphql {
            limit_req zone=api burst=20 nodelay;

            proxy_pass http://fraiseql;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection 'upgrade';
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_cache_bypass $http_upgrade;

            # Timeouts
            proxy_connect_timeout 60s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;
        }

        # Health check endpoint
        location /health {
            proxy_pass http://fraiseql;
            access_log off;
        }

        # Metrics endpoint (internal only)
        location /metrics {
            proxy_pass http://fraiseql;
            allow 10.0.0.0/8;
            deny all;
        }
    }
}
```

## Environment Configuration

### .env File Template

```bash
# Database
POSTGRES_DB=fraiseql_prod
POSTGRES_USER=fraiseql_admin
POSTGRES_PASSWORD=strong_password_here
DATABASE_URL=postgresql://fraiseql_admin:strong_password_here@postgres:5432/fraiseql_prod

# Redis
REDIS_PASSWORD=redis_password_here
REDIS_URL=redis://:redis_password_here@redis:6379/0

# Application
SECRET_KEY=your-secret-key-here-use-openssl-rand-hex-32
FRAISEQL_MODE=production
LOG_LEVEL=INFO

# Security
CORS_ORIGINS=https://your-domain.com,https://www.your-domain.com
JWT_SECRET=your-jwt-secret-here
JWT_EXPIRATION=3600

# Performance
MAX_CONNECTIONS=100
STATEMENT_TIMEOUT=30000
QUERY_COMPLEXITY_LIMIT=1000

# Monitoring
PROMETHEUS_ENABLED=true
METRICS_PORT=9090
SENTRY_DSN=https://your-sentry-dsn-here
```

## Building and Running

### Build the Image

```bash
# Build for production
docker build -t fraiseql:latest -f deploy/docker/Dockerfile .

# Build with specific version
docker build -t fraiseql:v1.0.0 -f deploy/docker/Dockerfile .

# Multi-platform build
docker buildx build --platform linux/amd64,linux/arm64 \
  -t fraiseql:latest -f deploy/docker/Dockerfile .
```

### Run with Docker

```bash
# Run standalone container
docker run -d \
  --name fraiseql \
  -p 8000:8000 \
  -e DATABASE_URL="postgresql://user:pass@host:5432/db" \
  -e SECRET_KEY="your-secret-key" \
  -e FRAISEQL_MODE="production" \
  fraiseql:latest

# Run with Docker Compose
docker-compose -f docker-compose.prod.yml up -d

# Scale the application
docker-compose -f docker-compose.prod.yml up -d --scale app=3
```

## Database Migrations

```bash
# Run migrations
docker-compose exec app python -m fraiseql migrate

# Create migration
docker-compose exec app python -m fraiseql makemigrations

# Rollback migration
docker-compose exec app python -m fraiseql migrate --rollback
```

## Backup and Restore

### Database Backup

```bash
#!/bin/bash
# backup.sh
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="/backups/fraiseql_${TIMESTAMP}.sql"

docker-compose exec -T postgres pg_dump \
  -U ${POSTGRES_USER} \
  -d ${POSTGRES_DB} \
  > ${BACKUP_FILE}

# Compress the backup
gzip ${BACKUP_FILE}

# Upload to S3 (optional)
aws s3 cp ${BACKUP_FILE}.gz s3://your-bucket/backups/
```

### Database Restore

```bash
#!/bin/bash
# restore.sh
BACKUP_FILE=$1

# Decompress if needed
gunzip ${BACKUP_FILE}.gz

# Restore the database
docker-compose exec -T postgres psql \
  -U ${POSTGRES_USER} \
  -d ${POSTGRES_DB} \
  < ${BACKUP_FILE}
```

## Monitoring

### Health Checks

```python
# src/fraiseql/health.py
from fastapi import FastAPI, Response
from sqlalchemy import text
import asyncpg

app = FastAPI()

@app.get("/health")
async def health_check():
    """Basic health check"""
    return {"status": "healthy"}

@app.get("/ready")
async def readiness_check():
    """Readiness check with database connection"""
    try:
        # Check database connection
        async with get_db_connection() as conn:
            await conn.execute(text("SELECT 1"))

        # Check Redis connection
        redis_client = get_redis_client()
        await redis_client.ping()

        return {"status": "ready"}
    except Exception as e:
        return Response(
            content={"status": "not ready", "error": str(e)},
            status_code=503
        )
```

### Docker Logs

```bash
# View logs
docker-compose logs -f app

# View last 100 lines
docker-compose logs --tail=100 app

# Export logs
docker-compose logs app > app.log

# Log rotation configuration
cat > /etc/logrotate.d/fraiseql << EOF
/var/log/fraiseql/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0640 fraiseql fraiseql
    sharedscripts
    postrotate
        docker-compose restart app
    endscript
}
EOF
```

## Performance Optimization

### Docker Build Cache

```dockerfile
# Optimize layer caching
FROM python:3.11-slim as builder

# Install dependencies first (changes less frequently)
COPY pyproject.toml uv.lock ./
RUN pip install --no-cache-dir uv && \
    uv pip install --system --no-cache

# Then copy source code (changes more frequently)
COPY . .
```

### Resource Limits

```yaml
# docker-compose.yml
services:
  app:
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 2G
        reservations:
          cpus: '1.0'
          memory: 1G
```

### Connection Pooling

```python
# Database connection pool
DATABASE_CONFIG = {
    "pool_size": 20,
    "max_overflow": 10,
    "pool_timeout": 30,
    "pool_recycle": 3600,
    "pool_pre_ping": True,
}
```

## Security Best Practices

### 1. Use Non-Root User
```dockerfile
RUN useradd -m -u 1001 fraiseql
USER fraiseql
```

### 2. Minimal Base Image
```dockerfile
FROM python:3.11-slim  # Not python:3.11
```

### 3. Security Scanning
```bash
# Scan for vulnerabilities
docker scan fraiseql:latest

# Use Trivy for comprehensive scanning
trivy image fraiseql:latest
```

### 4. Secrets Management
```bash
# Use Docker secrets
echo "my_secret_password" | docker secret create db_password -

# Reference in compose
services:
  app:
    secrets:

      - db_password
    environment:
      DATABASE_PASSWORD_FILE: /run/secrets/db_password
```

## Troubleshooting

### Common Issues

#### Container Exits Immediately
```bash
# Check logs
docker logs fraiseql

# Debug with shell
docker run -it --entrypoint /bin/bash fraiseql:latest
```

#### Database Connection Failed
```bash
# Check network
docker network ls
docker network inspect fraiseql-network

# Test connection
docker-compose exec app python -c "
import psycopg2
conn = psycopg2.connect('${DATABASE_URL}')
print('Connected!')
"
```

#### Permission Denied
```bash
# Fix permissions
docker-compose exec -u root app chown -R fraiseql:fraiseql /app
```

### Debug Mode
```yaml
# docker-compose.debug.yml
services:
  app:
    environment:
      FRAISEQL_MODE: development
      LOG_LEVEL: DEBUG
    stdin_open: true
    tty: true
```

## Production Checklist

- [ ] Use multi-stage builds
- [ ] Run as non-root user
- [ ] Enable health checks
- [ ] Configure resource limits
- [ ] Set up logging
- [ ] Use secrets management
- [ ] Enable SSL/TLS
- [ ] Configure backups
- [ ] Set up monitoring
- [ ] Test disaster recovery

## Next Steps

1. Set up [Kubernetes deployment](./kubernetes.md) for orchestration
2. Configure [monitoring](./monitoring.md) and alerting
3. Implement [scaling strategies](./scaling.md)
4. Deploy to [cloud platforms](./aws.md)
