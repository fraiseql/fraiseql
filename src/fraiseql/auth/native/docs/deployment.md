# Deployment Guide - FraiseQL Native Authentication

Complete guide for deploying FraiseQL native authentication to production environments.

## Deployment Overview

### Production Requirements

- **Python 3.11+** (3.13 recommended for best performance)
- **PostgreSQL 15+** (16 recommended for latest features)
- **Redis** (optional, for distributed rate limiting)
- **SSL/TLS certificates** (mandatory for production)
- **Load balancer** (for high availability)
- **Monitoring system** (Prometheus, Grafana, or similar)

### Environment Types

1. **Development**: Local testing and development
2. **Staging**: Pre-production testing
3. **Production**: Live user-facing deployment
4. **DR (Disaster Recovery)**: Backup production environment

## Environment Configuration

### Environment Variables

```bash
# Required Configuration
export JWT_SECRET_KEY="$(openssl rand -base64 64)"
export DATABASE_URL="postgresql://user:pass@host:5432/db?sslmode=require"

# Security Configuration
export ENVIRONMENT="production"
export ENABLE_RATE_LIMITING=true
export ENABLE_SECURITY_HEADERS=true
export ENABLE_CSRF_PROTECTION=false  # Usually false for API-first apps

# Performance Configuration
export DATABASE_POOL_MIN_SIZE=10
export DATABASE_POOL_MAX_SIZE=20
export ACCESS_TOKEN_TTL_MINUTES=15
export REFRESH_TOKEN_TTL_DAYS=30

# Rate Limiting
export RATE_LIMIT_REQUESTS_PER_MINUTE=60
export RATE_LIMIT_AUTH_REQUESTS_PER_MINUTE=5
export REDIS_URL="redis://localhost:6379/0"  # Optional

# Logging
export LOG_LEVEL=INFO
export LOG_FORMAT=json
export SENTRY_DSN="https://your-sentry-dsn"  # Optional

# CORS (if needed)
export CORS_ALLOWED_ORIGINS="https://yourdomain.com,https://app.yourdomain.com"
export CORS_ALLOWED_METHODS="GET,POST,PUT,DELETE,OPTIONS"
export CORS_ALLOWED_HEADERS="Authorization,Content-Type,X-CSRF-Token"

# Multi-tenant (if applicable)
export DEFAULT_SCHEMA=public
export TENANT_RESOLUTION_METHOD=subdomain  # subdomain|header|token
```

### Production Secrets Management

**Never use plain environment variables for secrets in production!**

#### AWS Secrets Manager

```python
# production_secrets.py
import boto3
import json
import os

def load_secrets_from_aws():
    """Load secrets from AWS Secrets Manager"""
    client = boto3.client('secretsmanager', region_name='us-west-2')
    
    try:
        response = client.get_secret_value(SecretId='fraiseql/production')
        secrets = json.loads(response['SecretString'])
        
        # Set environment variables
        os.environ['JWT_SECRET_KEY'] = secrets['jwt_secret_key']
        os.environ['DATABASE_URL'] = secrets['database_url']
        
    except Exception as e:
        print(f"Error loading secrets: {e}")
        raise

# Call during application startup
load_secrets_from_aws()
```

#### Azure Key Vault

```python
# azure_secrets.py
from azure.keyvault.secrets import SecretClient
from azure.identity import DefaultAzureCredential

def load_secrets_from_azure():
    """Load secrets from Azure Key Vault"""
    credential = DefaultAzureCredential()
    client = SecretClient(
        vault_url="https://your-vault.vault.azure.net/", 
        credential=credential
    )
    
    # Load secrets
    os.environ['JWT_SECRET_KEY'] = client.get_secret("jwt-secret-key").value
    os.environ['DATABASE_URL'] = client.get_secret("database-url").value
```

#### Google Secret Manager

```python
# google_secrets.py
from google.cloud import secretmanager

def load_secrets_from_google():
    """Load secrets from Google Secret Manager"""
    client = secretmanager.SecretManagerServiceClient()
    project_id = "your-project-id"
    
    # Load JWT secret
    jwt_secret_name = f"projects/{project_id}/secrets/jwt-secret-key/versions/latest"
    jwt_response = client.access_secret_version(request={"name": jwt_secret_name})
    os.environ['JWT_SECRET_KEY'] = jwt_response.payload.data.decode("UTF-8")
```

## Database Setup

### Production PostgreSQL Configuration

```sql
-- PostgreSQL production settings
ALTER SYSTEM SET max_connections = 200;
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET work_mem = '4MB';

-- Security settings
ALTER SYSTEM SET ssl = on;
ALTER SYSTEM SET ssl_cert_file = 'server.crt';
ALTER SYSTEM SET ssl_key_file = 'server.key';

-- Logging
ALTER SYSTEM SET log_statement = 'mod';
ALTER SYSTEM SET log_connections = on;
ALTER SYSTEM SET log_disconnections = on;

-- Reload configuration
SELECT pg_reload_conf();
```

### Database Migration Script

```python
# migrate.py
import asyncio
import os
import sys
from psycopg_pool import AsyncConnectionPool
from fraiseql.auth.native import apply_native_auth_schema

async def migrate_production():
    """Apply database migrations safely"""
    database_url = os.environ.get("DATABASE_URL")
    if not database_url:
        print("❌ DATABASE_URL environment variable required")
        sys.exit(1)
    
    if not database_url.startswith("postgresql"):
        print("❌ DATABASE_URL must be a PostgreSQL connection string")
        sys.exit(1)
    
    print("🔄 Starting database migration...")
    
    try:
        pool = AsyncConnectionPool(
            database_url,
            min_size=1,
            max_size=5
        )
        
        # Apply native auth schema
        await apply_native_auth_schema(pool, schema=os.environ.get("DEFAULT_SCHEMA", "public"))
        
        print("✅ Database migration completed successfully")
        
    except Exception as e:
        print(f"❌ Migration failed: {e}")
        sys.exit(1)
    
    finally:
        await pool.close()

if __name__ == "__main__":
    asyncio.run(migrate_production())
```

### Database Backup Strategy

```bash
#!/bin/bash
# backup.sh - Production database backup

set -e

DB_URL="${DATABASE_URL}"
BACKUP_DIR="/backups"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="fraiseql_backup_${DATE}.sql"

# Create backup directory
mkdir -p "${BACKUP_DIR}"

# Perform backup
echo "🔄 Starting database backup..."
pg_dump "${DB_URL}" > "${BACKUP_DIR}/${BACKUP_FILE}"

# Compress backup
gzip "${BACKUP_DIR}/${BACKUP_FILE}"

# Upload to S3 (optional)
aws s3 cp "${BACKUP_DIR}/${BACKUP_FILE}.gz" "s3://your-backup-bucket/database/"

# Cleanup old backups (keep 30 days)
find "${BACKUP_DIR}" -name "fraiseql_backup_*.sql.gz" -mtime +30 -delete

echo "✅ Backup completed: ${BACKUP_FILE}.gz"
```

## Application Deployment

### Docker Deployment

#### Dockerfile

```dockerfile
# Dockerfile
FROM python:3.13-slim

# Set working directory
WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libpq-dev \
    gcc \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements first (for better caching)
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Create non-root user
RUN useradd -m -u 1000 appuser && chown -R appuser:appuser /app
USER appuser

# Health check
HEALTHCHECK --interval=30s --timeout=30s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Expose port
EXPOSE 8000

# Run application
CMD ["gunicorn", "main:app", "-w", "4", "-k", "uvicorn.workers.UvicornWorker", "--bind", "0.0.0.0:8000"]
```

#### Docker Compose (Production)

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "8000:8000"
    environment:
      - ENVIRONMENT=production
      - DATABASE_URL=postgresql://fraiseql:${POSTGRES_PASSWORD}@db:5432/fraiseql
      - JWT_SECRET_KEY=${JWT_SECRET_KEY}
      - REDIS_URL=redis://redis:6379/0
    depends_on:
      - db
      - redis
    restart: unless-stopped
    volumes:
      - ./logs:/app/logs
    networks:
      - fraiseql-network

  db:
    image: postgres:16
    environment:
      - POSTGRES_USER=fraiseql
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
      - POSTGRES_DB=fraiseql
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./postgresql.conf:/etc/postgresql/postgresql.conf
    ports:
      - "5432:5432"
    restart: unless-stopped
    networks:
      - fraiseql-network

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    restart: unless-stopped
    networks:
      - fraiseql-network

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
      - ./ssl:/etc/ssl/certs
    depends_on:
      - app
    restart: unless-stopped
    networks:
      - fraiseql-network

volumes:
  postgres_data:

networks:
  fraiseql-network:
    driver: bridge
```

### Kubernetes Deployment

#### Deployment YAML

```yaml
# k8s-deployment.yml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-auth
  labels:
    app: fraiseql-auth
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql-auth
  template:
    metadata:
      labels:
        app: fraiseql-auth
    spec:
      containers:
      - name: fraiseql-auth
        image: your-registry/fraiseql-auth:latest
        ports:
        - containerPort: 8000
        env:
        - name: ENVIRONMENT
          value: "production"
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        - name: JWT_SECRET_KEY
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: jwt-secret-key
        - name: REDIS_URL
          value: "redis://redis-service:6379/0"
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
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 15
          periodSeconds: 15

---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-auth-service
spec:
  selector:
    app: fraiseql-auth
  ports:
    - protocol: TCP
      port: 80
      targetPort: 8000
  type: ClusterIP
```

#### Secrets Configuration

```yaml
# k8s-secrets.yml
apiVersion: v1
kind: Secret
metadata:
  name: fraiseql-secrets
type: Opaque
data:
  database-url: <base64-encoded-database-url>
  jwt-secret-key: <base64-encoded-jwt-secret>
  redis-url: <base64-encoded-redis-url>
```

### Load Balancer Configuration

#### Nginx Configuration

```nginx
# nginx.conf
upstream fraiseql_backend {
    server app1:8000;
    server app2:8000;
    server app3:8000;
}

server {
    listen 443 ssl http2;
    server_name yourdomain.com;

    # SSL configuration
    ssl_certificate /etc/ssl/certs/yourdomain.com.pem;
    ssl_certificate_key /etc/ssl/certs/yourdomain.com.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options DENY always;
    add_header X-Content-Type-Options nosniff always;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=auth:10m rate=5r/m;

    # Auth endpoints (strict rate limiting)
    location /auth/ {
        limit_req zone=auth burst=10 nodelay;
        proxy_pass http://fraiseql_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # General endpoints
    location / {
        proxy_pass http://fraiseql_backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

# Redirect HTTP to HTTPS
server {
    listen 80;
    server_name yourdomain.com;
    return 301 https://$server_name$request_uri;
}
```

## Monitoring and Observability

### Health Checks

```python
# health.py
from fastapi import APIRouter, HTTPException
from psycopg_pool import AsyncConnectionPool

router = APIRouter()

@router.get("/health")
async def health_check():
    """Comprehensive health check endpoint"""
    health_status = {
        "status": "healthy",
        "timestamp": datetime.utcnow().isoformat(),
        "version": "1.0.0",
        "checks": {}
    }
    
    # Database connectivity
    try:
        async with app.state.db_pool.connection() as conn:
            await conn.execute("SELECT 1")
        health_status["checks"]["database"] = "healthy"
    except Exception as e:
        health_status["checks"]["database"] = f"unhealthy: {e}"
        health_status["status"] = "unhealthy"
    
    # Redis connectivity (if used)
    try:
        redis_client = app.state.redis_client
        await redis_client.ping()
        health_status["checks"]["redis"] = "healthy"
    except Exception as e:
        health_status["checks"]["redis"] = f"unhealthy: {e}"
        # Redis is optional, don't mark overall status as unhealthy
    
    # JWT secret key present
    jwt_secret = os.environ.get("JWT_SECRET_KEY")
    health_status["checks"]["jwt_config"] = "healthy" if jwt_secret else "unhealthy: missing secret"
    
    if health_status["status"] == "unhealthy":
        raise HTTPException(status_code=503, detail=health_status)
    
    return health_status

@router.get("/health/ready")
async def readiness_check():
    """Kubernetes readiness probe"""
    # Simplified check for readiness
    try:
        async with app.state.db_pool.connection() as conn:
            await conn.execute("SELECT 1")
        return {"status": "ready"}
    except Exception:
        raise HTTPException(status_code=503, detail={"status": "not ready"})

@router.get("/health/live")
async def liveness_check():
    """Kubernetes liveness probe"""
    # Basic liveness check
    return {"status": "alive", "timestamp": datetime.utcnow().isoformat()}
```

### Metrics Collection

```python
# metrics.py
from prometheus_client import Counter, Histogram, Gauge, make_asgi_app
import time

# Authentication metrics
auth_requests_total = Counter(
    'auth_requests_total', 
    'Total authentication requests',
    ['endpoint', 'status']
)

auth_duration = Histogram(
    'auth_duration_seconds',
    'Authentication request duration',
    ['endpoint']
)

active_sessions = Gauge(
    'active_sessions_total',
    'Number of active user sessions'
)

token_theft_detections = Counter(
    'token_theft_detections_total',
    'Number of token theft detections'
)

# Middleware to collect metrics
@app.middleware("http")
async def metrics_middleware(request: Request, call_next):
    start_time = time.time()
    
    response = await call_next(request)
    
    # Record metrics
    endpoint = request.url.path
    status = "success" if response.status_code < 400 else "error"
    duration = time.time() - start_time
    
    auth_requests_total.labels(endpoint=endpoint, status=status).inc()
    auth_duration.labels(endpoint=endpoint).observe(duration)
    
    return response

# Expose metrics endpoint
metrics_app = make_asgi_app()
app.mount("/metrics", metrics_app)
```

### Logging Configuration

```python
# logging_config.py
import logging
import sys
from pythonjsonlogger import jsonlogger

def setup_logging():
    """Configure structured logging for production"""
    
    # Create logger
    logger = logging.getLogger()
    logger.setLevel(logging.INFO)
    
    # Create handler
    handler = logging.StreamHandler(sys.stdout)
    
    # Create JSON formatter
    formatter = jsonlogger.JsonFormatter(
        fmt='%(asctime)s %(name)s %(levelname)s %(message)s',
        datefmt='%Y-%m-%d %H:%M:%S'
    )
    handler.setFormatter(formatter)
    
    # Add handler to logger
    logger.addHandler(handler)
    
    # Set log levels for specific modules
    logging.getLogger("uvicorn").setLevel(logging.INFO)
    logging.getLogger("fraiseql.auth").setLevel(logging.INFO)
    
    return logger

# Security event logging
def log_security_event(event_type: str, user_id: str = None, **kwargs):
    """Log security events with structured data"""
    logger = logging.getLogger("fraiseql.auth.security")
    
    event_data = {
        "event_type": event_type,
        "user_id": user_id,
        "timestamp": datetime.utcnow().isoformat(),
        **kwargs
    }
    
    logger.info("Security event", extra=event_data)
```

## Security Hardening

### Production Security Checklist

- [ ] **SSL/TLS certificates** properly configured
- [ ] **Security headers** enabled (HSTS, CSP, etc.)
- [ ] **Rate limiting** configured at load balancer and application level
- [ ] **Secrets management** using proper secret store (not environment variables)
- [ ] **Database encryption** at rest and in transit
- [ ] **Network segmentation** (VPC, security groups, firewalls)
- [ ] **Regular security updates** automated
- [ ] **Vulnerability scanning** integrated into CI/CD
- [ ] **Access logging** enabled and monitored
- [ ] **Intrusion detection** system configured

### Firewall Configuration

```bash
# Example iptables rules (adapt to your firewall)
# Allow SSH (from management network only)
iptables -A INPUT -p tcp -s 10.0.1.0/24 --dport 22 -j ACCEPT

# Allow HTTP/HTTPS
iptables -A INPUT -p tcp --dport 80 -j ACCEPT
iptables -A INPUT -p tcp --dport 443 -j ACCEPT

# Allow database (from app servers only)
iptables -A INPUT -p tcp -s 10.0.2.0/24 --dport 5432 -j ACCEPT

# Allow Redis (from app servers only)
iptables -A INPUT -p tcp -s 10.0.2.0/24 --dport 6379 -j ACCEPT

# Drop everything else
iptables -A INPUT -j DROP
```

## Scaling and Performance

### Horizontal Scaling

FraiseQL native auth is **stateless** and scales horizontally:

```yaml
# Auto-scaling configuration (Kubernetes)
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql-auth-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql-auth
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

### Database Scaling

```python
# Connection pool sizing
DATABASE_POOL_CONFIG = {
    "min_size": 10,          # Always-open connections
    "max_size": 50,          # Maximum concurrent connections
    "timeout": 30,           # Connection timeout
    "max_idle": 3600         # Max idle time (1 hour)
}

# For high load, consider read replicas
READ_REPLICA_URL = "postgresql://readonly-user:pass@replica-host:5432/db"
WRITE_DB_URL = "postgresql://user:pass@primary-host:5432/db"
```

### Caching Strategy

```python
# Redis caching for rate limiting and session storage
import redis.asyncio as redis

redis_client = redis.Redis.from_url("redis://localhost:6379/0")

# Cache user sessions
async def cache_user_session(session_id: str, user_data: dict):
    await redis_client.setex(
        f"session:{session_id}",
        3600,  # 1 hour TTL
        json.dumps(user_data)
    )

# Cache rate limit counters
async def check_rate_limit(ip_address: str) -> bool:
    key = f"rate_limit:{ip_address}"
    current = await redis_client.get(key)
    
    if current and int(current) >= 60:  # 60 requests/minute
        return False
    
    await redis_client.incr(key)
    await redis_client.expire(key, 60)  # 1 minute window
    return True
```

## Disaster Recovery

### Backup Strategy

```bash
# Automated backup script
#!/bin/bash
# backup.sh - Complete backup strategy

# Database backup
pg_dump $DATABASE_URL | gzip > /backups/db_$(date +%Y%m%d_%H%M%S).sql.gz

# Application logs backup
tar -czf /backups/logs_$(date +%Y%m%d_%H%M%S).tar.gz /app/logs/

# Configuration backup
tar -czf /backups/config_$(date +%Y%m%d_%H%M%S).tar.gz /app/config/

# Upload to cloud storage
aws s3 sync /backups/ s3://your-backup-bucket/fraiseql-backups/

# Cleanup old backups (keep 30 days)
find /backups -name "*.gz" -mtime +30 -delete
```

### Recovery Procedures

```bash
# Database recovery
# 1. Stop application
kubectl scale deployment fraiseql-auth --replicas=0

# 2. Restore database
gunzip -c /backups/db_20250122_120000.sql.gz | psql $DATABASE_URL

# 3. Verify data integrity
psql $DATABASE_URL -c "SELECT COUNT(*) FROM tb_user;"

# 4. Restart application
kubectl scale deployment fraiseql-auth --replicas=3
```

## Deployment Automation

### CI/CD Pipeline (GitHub Actions)

```yaml
# .github/workflows/deploy.yml
name: Deploy to Production

on:
  push:
    branches: [main]
    tags: ['v*']

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Set up Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.13'
    
    - name: Install dependencies
      run: |
        pip install -r requirements.txt
        pip install -e .
    
    - name: Run tests
      run: pytest tests/auth/native/ -v
      env:
        DATABASE_URL: postgresql://postgres:postgres@localhost:5432/postgres
        JWT_SECRET_KEY: test-secret-key

  deploy:
    needs: test
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Configure AWS credentials
      uses: aws-actions/configure-aws-credentials@v2
      with:
        aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
        aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
        aws-region: us-west-2
    
    - name: Build and push Docker image
      env:
        ECR_REGISTRY: ${{ steps.login-ecr.outputs.registry }}
        ECR_REPOSITORY: fraiseql-auth
        IMAGE_TAG: ${{ github.sha }}
      run: |
        docker build -t $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG .
        docker push $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG
    
    - name: Deploy to EKS
      run: |
        aws eks update-kubeconfig --name production-cluster
        kubectl set image deployment/fraiseql-auth fraiseql-auth=$ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG
        kubectl rollout status deployment/fraiseql-auth
```

### Blue-Green Deployment

```bash
# blue-green-deploy.sh
#!/bin/bash

set -e

# Configuration
BLUE_DEPLOYMENT="fraiseql-auth-blue"
GREEN_DEPLOYMENT="fraiseql-auth-green"
SERVICE="fraiseql-auth-service"
IMAGE_TAG=${1:-latest}

echo "🔄 Starting blue-green deployment with image tag: $IMAGE_TAG"

# Determine current active deployment
CURRENT=$(kubectl get service $SERVICE -o jsonpath='{.spec.selector.version}')
if [ "$CURRENT" == "blue" ]; then
    ACTIVE=$BLUE_DEPLOYMENT
    INACTIVE=$GREEN_DEPLOYMENT
    NEW_VERSION="green"
else
    ACTIVE=$GREEN_DEPLOYMENT
    INACTIVE=$BLUE_DEPLOYMENT
    NEW_VERSION="blue"
fi

echo "📋 Current active: $ACTIVE, deploying to: $INACTIVE"

# Update inactive deployment with new image
kubectl set image deployment/$INACTIVE fraiseql-auth=your-registry/fraiseql-auth:$IMAGE_TAG

# Wait for rollout to complete
kubectl rollout status deployment/$INACTIVE

# Health check on new deployment
NEW_POD=$(kubectl get pods -l app=fraiseql-auth,version=$NEW_VERSION -o jsonpath='{.items[0].metadata.name}')
kubectl exec $NEW_POD -- curl -f http://localhost:8000/health

# Switch traffic to new deployment
kubectl patch service $SERVICE -p '{"spec":{"selector":{"version":"'$NEW_VERSION'"}}}'

echo "✅ Traffic switched to $NEW_VERSION deployment"

# Optional: Scale down old deployment after verification period
echo "⏳ Waiting 5 minutes before scaling down old deployment..."
sleep 300

kubectl scale deployment $ACTIVE --replicas=0

echo "✅ Blue-green deployment completed successfully"
```

## Monitoring Alerts

### Prometheus Alerts

```yaml
# alerts.yml
groups:
- name: fraiseql-auth
  rules:
  - alert: HighAuthErrorRate
    expr: rate(auth_requests_total{status="error"}[5m]) > 0.1
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: "High authentication error rate"
      description: "Auth error rate is {{ $value }} requests/second"
  
  - alert: DatabaseDown
    expr: up{job="fraiseql-auth"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "FraiseQL Auth service is down"
      description: "FraiseQL Auth has been down for more than 1 minute"
  
  - alert: TokenTheftDetected
    expr: increase(token_theft_detections_total[1h]) > 0
    for: 0m
    labels:
      severity: critical
    annotations:
      summary: "Token theft detected"
      description: "{{ $value }} token theft events in the last hour"
```

## Post-Deployment Verification

### Deployment Checklist

After deployment, verify:

- [ ] **Health checks** return 200 OK
- [ ] **User registration** works correctly
- [ ] **User login** works correctly  
- [ ] **Token refresh** works correctly
- [ ] **Password reset** works correctly
- [ ] **Rate limiting** is functioning
- [ ] **Security headers** are present
- [ ] **SSL certificates** are valid
- [ ] **Database connections** are stable
- [ ] **Monitoring alerts** are configured
- [ ] **Backup system** is running
- [ ] **Log aggregation** is working

### Load Testing

```python
# load_test.py
import asyncio
import aiohttp
import time
from concurrent.futures import ThreadPoolExecutor

async def test_auth_endpoint(session, base_url):
    """Test authentication endpoint under load"""
    async with session.post(f"{base_url}/auth/login", 
                           json={"email": "test@example.com", "password": "TestPass123!"}) as resp:
        return resp.status

async def run_load_test(base_url, concurrent_users=100, duration_seconds=60):
    """Run load test against authentication endpoints"""
    
    async with aiohttp.ClientSession() as session:
        start_time = time.time()
        request_count = 0
        error_count = 0
        
        while time.time() - start_time < duration_seconds:
            tasks = []
            for _ in range(concurrent_users):
                task = asyncio.create_task(test_auth_endpoint(session, base_url))
                tasks.append(task)
            
            results = await asyncio.gather(*tasks, return_exceptions=True)
            
            for result in results:
                request_count += 1
                if isinstance(result, Exception) or result != 200:
                    error_count += 1
        
        # Calculate metrics
        total_time = time.time() - start_time
        rps = request_count / total_time
        error_rate = error_count / request_count * 100
        
        print(f"Load test results:")
        print(f"Duration: {total_time:.2f}s")
        print(f"Requests: {request_count}")
        print(f"RPS: {rps:.2f}")
        print(f"Error rate: {error_rate:.2f}%")

# Run the test
asyncio.run(run_load_test("https://yourdomain.com", concurrent_users=50, duration_seconds=60))
```

## Support and Troubleshooting

### Common Deployment Issues

1. **Database connection failures**
   - Check DATABASE_URL format
   - Verify PostgreSQL is running and accessible
   - Check firewall rules
   - Verify SSL configuration

2. **JWT secret key issues**
   - Ensure JWT_SECRET_KEY is set and secure
   - Verify secret is consistent across instances
   - Check secret manager integration

3. **SSL certificate problems**
   - Verify certificate validity
   - Check certificate chain
   - Ensure proper nginx configuration

4. **Performance issues**
   - Monitor database connection pool
   - Check for slow queries
   - Verify adequate resources (CPU, memory)
   - Review rate limiting configuration

### Getting Help

- **Documentation**: [FraiseQL Docs](https://fraiseql.dev/docs)
- **GitHub Issues**: [Report deployment issues](https://github.com/fraiseql/fraiseql/issues)
- **Community**: [Discord server](https://discord.gg/fraiseql)
- **Enterprise Support**: Contact for dedicated deployment assistance

---

**Congratulations!** You've successfully deployed FraiseQL native authentication to production. Your users now have secure, fast, and reliable authentication powered by your own infrastructure.