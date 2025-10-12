# Production Deployment

Deploy FraiseQL to production with Docker, monitoring, and security best practices.

## Overview

Production deployment checklist:
- Docker containerization
- Database migrations
- Environment configuration
- Performance optimization
- Monitoring and logging
- Security hardening

**Time**: 60-90 minutes

## Prerequisites

- Completed [Blog API Tutorial](./blog-api.md)
- Docker and Docker Compose installed
- Production database (PostgreSQL 14+)
- Domain name (for HTTPS)

## Project Structure

```
myapp/
├── src/
│   ├── app.py
│   ├── models.py
│   ├── queries.py
│   └── mutations.py
├── db/
│   └── migrations/
│       ├── 001_initial_schema.sql
│       ├── 002_views.sql
│       └── 003_functions.sql
├── deploy/
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── nginx.conf
├── .env.example
├── pyproject.toml
└── README.md
```

## Step 1: Dockerfile

```dockerfile
# deploy/Dockerfile
FROM python:3.11-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1000 app && \
    mkdir -p /app && \
    chown -R app:app /app

WORKDIR /app

# Install Python dependencies
COPY --chown=app:app pyproject.toml ./
RUN pip install --no-cache-dir -e .

# Copy application
COPY --chown=app:app src/ ./src/
COPY --chown=app:app db/ ./db/

# Switch to app user
USER app

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD python -c "import requests; requests.get('http://localhost:8000/health')"

# Run application
CMD ["uvicorn", "src.app:app", "--host", "0.0.0.0", "--port", "8000"]
```

## Step 2: Docker Compose

```yaml
# deploy/docker-compose.yml
version: '3.8'

services:
  db:
    image: postgres:14-alpine
    environment:
      POSTGRES_DB: ${DB_NAME}
      POSTGRES_USER: ${DB_USER}
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./db/migrations:/docker-entrypoint-initdb.d
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${DB_USER}"]
      interval: 10s
      timeout: 5s
      retries: 5

  api:
    build:
      context: ..
      dockerfile: deploy/Dockerfile
    environment:
      DATABASE_URL: postgresql://${DB_USER}:${DB_PASSWORD}@db:5432/${DB_NAME}
      ENV: production
      LOG_LEVEL: info
      RUST_ENABLED: "true"
      APQ_ENABLED: "true"
      APQ_STORAGE_BACKEND: postgresql
    ports:
      - "8000:8000"
    depends_on:
      db:
        condition: service_healthy
    restart: unless-stopped

  nginx:
    image: nginx:alpine
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./ssl:/etc/nginx/ssl:ro
    ports:
      - "80:80"
      - "443:443"
    depends_on:
      - api
    restart: unless-stopped

volumes:
  postgres_data:
```

## Step 3: Nginx Configuration

```nginx
# deploy/nginx.conf
events {
    worker_connections 1024;
}

http {
    upstream api {
        server api:8000;
    }

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=100r/m;

    server {
        listen 80;
        server_name yourdomain.com;

        # Redirect to HTTPS
        return 301 https://$host$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name yourdomain.com;

        # SSL configuration
        ssl_certificate /etc/nginx/ssl/fullchain.pem;
        ssl_certificate_key /etc/nginx/ssl/privkey.pem;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;

        # Security headers
        add_header X-Content-Type-Options nosniff;
        add_header X-Frame-Options DENY;
        add_header X-XSS-Protection "1; mode=block";
        add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

        # GraphQL endpoint
        location /graphql {
            limit_req zone=api_limit burst=20 nodelay;

            proxy_pass http://api;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # Timeouts
            proxy_connect_timeout 60s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;
        }

        # Health check
        location /health {
            proxy_pass http://api;
            access_log off;
        }
    }
}
```

## Step 4: Application Configuration

```python
# src/app.py
import os
from fraiseql import FraiseQL, FraiseQLConfig
from fraiseql.monitoring import setup_sentry, setup_prometheus
from psycopg_pool import AsyncConnectionPool

# Load environment
ENV = os.getenv("ENV", "development")
DATABASE_URL = os.getenv("DATABASE_URL")

# Configuration
config = FraiseQLConfig(
    database_url=DATABASE_URL,

    # Performance
    rust_enabled=os.getenv("RUST_ENABLED", "true").lower() == "true",
    apq_enabled=os.getenv("APQ_ENABLED", "true").lower() == "true",
    apq_storage_backend=os.getenv("APQ_STORAGE_BACKEND", "postgresql"),
    enable_turbo_router=True,
    json_passthrough_enabled=True,

    # Security
    enable_playground=(ENV != "production"),
    complexity_enabled=True,
    complexity_max_score=1000,
    query_depth_limit=10,

    # Monitoring
    enable_logging=True,
    log_level=os.getenv("LOG_LEVEL", "info"),
)

# Initialize app
app = FraiseQL(config=config)

# Connection pool
pool = AsyncConnectionPool(
    conninfo=DATABASE_URL,
    min_size=5,
    max_size=20,
    timeout=5.0
)

# Monitoring setup
if ENV == "production":
    setup_sentry(
        dsn=os.getenv("SENTRY_DSN"),
        environment=ENV,
        traces_sample_rate=0.1
    )

    setup_prometheus(app)

# Health check endpoint
@app.get("/health")
async def health_check():
    """Health check for load balancer."""
    async with pool.connection() as conn:
        await conn.execute("SELECT 1")
    return {"status": "healthy"}

# Graceful shutdown
@app.on_event("shutdown")
async def shutdown():
    await pool.close()
```

## Step 5: Environment Variables

```bash
# .env.example
# Database
DB_NAME=myapp_production
DB_USER=myapp
DB_PASSWORD=<secure-password>
DATABASE_URL=postgresql://${DB_USER}:${DB_PASSWORD}@db:5432/${DB_NAME}

# Application
ENV=production
LOG_LEVEL=info
SECRET_KEY=<generate-with-openssl-rand-hex-32>

# Performance
RUST_ENABLED=true
APQ_ENABLED=true
APQ_STORAGE_BACKEND=postgresql

# Monitoring
SENTRY_DSN=https://...@sentry.io/...

# Security
ALLOWED_HOSTS=yourdomain.com
```

## Step 6: Database Migrations

```bash
# db/migrations/001_initial_schema.sql
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

-- Tables
CREATE TABLE tb_user (...);
CREATE TABLE tb_post (...);

-- Indexes
CREATE INDEX idx_post_author ON tb_post(fk_author);
```

**Migration Script**:
```bash
#!/bin/bash
# scripts/migrate.sh

set -e

DATABASE_URL=${DATABASE_URL:-postgresql://localhost/myapp}

echo "Running migrations..."
for migration in db/migrations/*.sql; do
    echo "Applying $migration"
    psql "$DATABASE_URL" -f "$migration"
done

echo "Migrations complete!"
```

## Step 7: Deploy to Production

### Option A: Docker Compose

```bash
# 1. Clone repository
git clone https://github.com/yourorg/myapp.git
cd myapp

# 2. Configure environment
cp .env.example .env
nano .env  # Edit with production values

# 3. Start services
docker-compose -f deploy/docker-compose.yml up -d

# 4. Check health
curl https://yourdomain.com/health

# 5. View logs
docker-compose -f deploy/docker-compose.yml logs -f api
```

### Option B: Kubernetes

```yaml
# deploy/k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql-api
  template:
    metadata:
      labels:
        app: fraiseql-api
    spec:
      containers:
      - name: api
        image: yourorg/myapp:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-credentials
              key: url
        - name: ENV
          value: "production"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
```

## Step 8: Monitoring

### Prometheus Metrics

```python
# src/monitoring.py
from prometheus_client import Counter, Histogram, Gauge

# Request metrics
http_requests_total = Counter(
    'http_requests_total',
    'Total HTTP requests',
    ['method', 'endpoint', 'status']
)

query_duration_seconds = Histogram(
    'graphql_query_duration_seconds',
    'GraphQL query duration',
    ['operation']
)

db_pool_connections = Gauge(
    'db_pool_connections',
    'Active database connections'
)

# Middleware
@app.middleware("http")
async def metrics_middleware(request, call_next):
    start_time = time.time()
    response = await call_next(request)
    duration = time.time() - start_time

    query_duration_seconds.labels(
        operation=request.url.path
    ).observe(duration)

    http_requests_total.labels(
        method=request.method,
        endpoint=request.url.path,
        status=response.status_code
    ).inc()

    return response
```

### Grafana Dashboard

```json
{
  "dashboard": {
    "title": "FraiseQL Monitoring",
    "panels": [
      {
        "title": "Request Rate",
        "targets": [
          {
            "expr": "rate(http_requests_total[5m])"
          }
        ]
      },
      {
        "title": "Query Duration P95",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, graphql_query_duration_seconds)"
          }
        ]
      },
      {
        "title": "Database Connections",
        "targets": [
          {
            "expr": "db_pool_connections"
          }
        ]
      }
    ]
  }
}
```

## Step 9: Security Checklist

- [ ] Use HTTPS only (TLS 1.2+)
- [ ] Disable GraphQL Playground in production
- [ ] Implement rate limiting
- [ ] Set query complexity limits
- [ ] Use environment variables for secrets
- [ ] Enable CORS only for known origins
- [ ] Implement authentication middleware
- [ ] Add security headers (CSP, HSTS)
- [ ] Run database as non-root user
- [ ] Use prepared statements (automatic with FraiseQL)
- [ ] Enable audit logging
- [ ] Set up alerts for unusual activity

## Step 10: Performance Optimization

### Database Tuning

```sql
-- PostgreSQL configuration (postgresql.conf)
shared_buffers = 256MB
effective_cache_size = 1GB
work_mem = 16MB
maintenance_work_mem = 128MB
max_connections = 100

-- Connection pooling
max_pool_size = 20
min_pool_size = 5

-- Enable query logging
log_min_duration_statement = 100  # Log queries > 100ms
```

### Application Tuning

```python
config = FraiseQLConfig(
    # Layer 0: Rust (10-80x faster)
    rust_enabled=True,

    # Layer 1: APQ (5-10x faster)
    apq_enabled=True,
    apq_storage_backend="postgresql",

    # Layer 2: TurboRouter (3-5x faster)
    enable_turbo_router=True,
    turbo_router_cache_size=500,

    # Layer 3: JSON Passthrough (2-3x faster)
    json_passthrough_enabled=True,

    # Combined: 0.5-2ms cached responses
)
```

## Troubleshooting

### High Memory Usage
```bash
# Check connection pool
docker exec api python -c "
from src.app import pool
print(f'Pool size: {pool.get_stats()}')
"

# Adjust pool size
MAX_POOL_SIZE=10 docker-compose restart api
```

### Slow Queries
```bash
# Enable query logging
psql $DATABASE_URL -c "ALTER SYSTEM SET log_min_duration_statement = 100;"
psql $DATABASE_URL -c "SELECT pg_reload_conf();"

# View slow queries
docker-compose logs api | grep "duration:"
```

### Database Connection Errors
```bash
# Check database health
docker-compose exec db pg_isready

# Check connection string
docker-compose exec api env | grep DATABASE_URL
```

## Production Checklist

### Before Launch
- [ ] Run full test suite
- [ ] Load test with realistic traffic
- [ ] Set up monitoring alerts
- [ ] Configure backups
- [ ] Document rollback procedure
- [ ] Test health check endpoints
- [ ] Verify SSL certificates
- [ ] Review security settings

### After Launch
- [ ] Monitor error rates
- [ ] Check query performance
- [ ] Verify cache hit rates
- [ ] Monitor database connections
- [ ] Review security logs
- [ ] Test scaling

## See Also

- [Performance](../performance/index.md) - Optimization techniques
- [Monitoring](../production/monitoring.md) - Observability setup
- [Security](../production/security.md) - Security hardening
- [Database Patterns](../advanced/database-patterns.md) - Production patterns
