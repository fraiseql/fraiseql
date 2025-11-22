# Production Deployment Guide

## Overview

This guide covers the complete production deployment process for FraiseQL, including prerequisites, deployment steps, and post-deployment validation.

## Prerequisites

### Infrastructure Requirements

#### Minimum Hardware Specifications
- **CPU**: 4 cores (8 recommended for high traffic)
- **RAM**: 8GB minimum (16GB recommended)
- **Storage**: 50GB SSD minimum
- **Network**: 1Gbps connection

#### Supported Platforms
- **Container Orchestration**: Kubernetes 1.24+, Docker Compose
- **Cloud Providers**: AWS, GCP, Azure, DigitalOcean
- **Operating Systems**: Ubuntu 20.04+, CentOS 8+, RHEL 8+

### Software Dependencies

#### Required Software
- Docker 24.0+
- Docker Compose 2.0+
- PostgreSQL 16+ with pgvector extension
- Redis 7.0+
- Nginx 1.20+

#### Optional but Recommended
- certbot (for SSL certificates)
- Prometheus (monitoring)
- Grafana (dashboards)
- Loki (log aggregation)

### Network Configuration

#### Required Ports
```
80/tcp   - HTTP (redirect to HTTPS)
443/tcp  - HTTPS
5432/tcp - PostgreSQL (internal only)
6379/tcp - Redis (internal only)
9090/tcp - Prometheus (monitoring)
3000/tcp - Grafana (monitoring)
```

#### DNS Requirements
- Valid domain name with SSL certificate
- DNS A/AAAA records pointing to load balancer
- Reverse DNS for email deliverability (if applicable)

## Environment Setup

### 1. Clone Repository

```bash
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
git checkout main  # or specific tag
```

### 2. Environment Configuration

Create environment-specific configuration files:

```bash
# Production environment file
cp deploy/.env.example deploy/.env.prod

# Edit with production values
nano deploy/.env.prod
```

#### Required Environment Variables

```bash
# Database Configuration
DATABASE_URL=postgresql://fraiseql:secure_password@db.internal:5432/fraiseql_prod
DB_HOST=db.internal
DB_PORT=5432
DB_USER=fraiseql
DB_PASSWORD=secure_password
DB_SSL_MODE=require

# Redis Configuration
REDIS_URL=rediss://user:password@redis.internal:6379/0
REDIS_SSL_URL=rediss://user:password@redis.internal:6379/0

# Application Configuration
FRAISEQL_ENVIRONMENT=production
FRAISEQL_LOG_LEVEL=INFO
SECRET_KEY=your-256-bit-secret-key-here
JWT_SECRET_KEY=your-jwt-secret-key-here

# Monitoring
SENTRY_DSN=https://your-sentry-dsn@sentry.io/project-id
PROMETHEUS_METRICS_ENABLED=true

# Feature Flags
FEATURE_VECTOR_SEARCH=true
FEATURE_AUTH_NATIVE=true
FEATURE_CACHING=true
```

### 3. SSL Certificate Setup

#### Using Let's Encrypt (Recommended)

```bash
# Install certbot
sudo apt update
sudo apt install certbot python3-certbot-nginx

# Obtain certificate
sudo certbot certonly --nginx -d yourdomain.com

# Certificates will be stored in:
/etc/letsencrypt/live/yourdomain.com/
```

#### Using Custom Certificates

Place certificates in the appropriate directory:
```bash
deploy/ssl/
├── fullchain.pem
├── privkey.pem
└── dhparam.pem
```

## Deployment Steps

### Option 1: Docker Compose (Simple)

#### 1. Prepare Deployment Directory

```bash
# Create deployment directory
mkdir -p /opt/fraiseql
cd /opt/fraiseql

# Copy deployment files
cp -r /path/to/fraiseql/deploy/* ./

# Set proper permissions
sudo chown -R 1000:1000 data/
sudo chmod 600 .env.prod
```

#### 2. Configure Docker Compose

Edit `docker-compose.prod.yml` for your environment:

```yaml
version: '3.8'

services:
  fraiseql:
    image: ghcr.io/fraiseql/fraiseql:latest
    environment:
      - DATABASE_URL=${DATABASE_URL}
      - REDIS_URL=${REDIS_URL}
    env_file:
      - .env.prod
    volumes:
      - ./ssl:/app/ssl:ro
    depends_on:
      - db
      - redis
    restart: unless-stopped

  db:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: fraiseql_prod
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - db_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes --requirepass ${REDIS_PASSWORD}
    volumes:
      - redis_data:/data
    restart: unless-stopped

volumes:
  db_data:
  redis_data:
```

#### 3. Deploy Application

```bash
# Load environment variables
export $(cat .env.prod | xargs)

# Start services
docker-compose -f docker-compose.prod.yml up -d

# Verify deployment
docker-compose -f docker-compose.prod.yml ps
docker-compose -f docker-compose.prod.yml logs -f fraiseql
```

### Option 2: Kubernetes (Enterprise)

#### 1. Prepare Kubernetes Manifests

```bash
# Create namespace
kubectl create namespace fraiseql-prod

# Create secrets
kubectl create secret generic fraiseql-secrets \
  --from-env-file=.env.prod \
  --namespace=fraiseql-prod

# Apply manifests
kubectl apply -f k8s/ -n fraiseql-prod
```

#### 2. Configure Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: fraiseql-ingress
  namespace: fraiseql-prod
  annotations:
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  tls:
  - hosts:
    - yourdomain.com
    secretName: fraiseql-tls
  rules:
  - host: yourdomain.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: fraiseql-service
            port:
              number: 80
```

#### 3. Deploy and Verify

```bash
# Deploy application
kubectl apply -f k8s/fraiseql-deployment.yaml -n fraiseql-prod

# Check rollout status
kubectl rollout status deployment/fraiseql -n fraiseql-prod

# Verify services
kubectl get pods -n fraiseql-prod
kubectl get services -n fraiseql-prod
kubectl get ingress -n fraiseql-prod
```

## Post-Deployment Validation

### 1. Health Checks

#### Application Health

```bash
# Check application health endpoint
curl -k https://yourdomain.com/health

# Expected response:
{
  "status": "healthy",
  "version": "1.0.0",
  "database": "connected",
  "redis": "connected"
}
```

#### Database Connectivity

```bash
# Test database connection
docker-compose exec db pg_isready -U fraiseql -d fraiseql_prod

# Check database extensions
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "\dx"
```

#### Redis Connectivity

```bash
# Test Redis connection
docker-compose exec redis redis-cli ping
```

### 2. Functional Testing

#### API Endpoints

```bash
# Test GraphQL endpoint
curl -X POST https://yourdomain.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __typename }"}'

# Test introspection (should be disabled in production)
curl -X POST https://yourdomain.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name } } }"}'
```

#### Authentication (if enabled)

```bash
# Test authentication endpoints
curl -X POST https://yourdomain.com/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "test", "password": "test"}'
```

### 3. Performance Validation

#### Load Testing

```bash
# Install hey for load testing
go install github.com/rakyll/hey@latest

# Run load test
hey -n 1000 -c 10 https://yourdomain.com/health
```

#### Database Performance

```bash
# Check database performance
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "SELECT * FROM pg_stat_activity;"
```

## Monitoring Setup

### 1. Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['fraiseql:8000']
    metrics_path: '/metrics'
```

### 2. Grafana Dashboards

Import the provided dashboards:
- `grafana/performance_metrics.json`
- `grafana/cache_hit_rate.json`
- `grafana/database_pool.json`

### 3. Alerting Rules

Configure alerts for:
- High error rates (>5%)
- Database connection pool exhaustion
- High memory usage (>90%)
- Slow response times (>2s P95)

## Backup and Recovery

### Database Backup

```bash
# Create backup script
cat > backup.sh << 'EOF'
#!/bin/bash
BACKUP_DIR="/opt/fraiseql/backups"
DATE=$(date +%Y%m%d_%H%M%S)

# Create backup directory
mkdir -p $BACKUP_DIR

# Backup database
docker-compose exec -T db pg_dump -U fraiseql fraiseql_prod > $BACKUP_DIR/fraiseql_$DATE.sql

# Compress backup
gzip $BACKUP_DIR/fraiseql_$DATE.sql

# Clean old backups (keep last 7 days)
find $BACKUP_DIR -name "*.sql.gz" -mtime +7 -delete

echo "Backup completed: $BACKUP_DIR/fraiseql_$DATE.sql.gz"
EOF

# Make executable and schedule
chmod +x backup.sh
crontab -e
# Add: 0 2 * * * /opt/fraiseql/backup.sh
```

### Recovery Procedure

```bash
# Stop application
docker-compose down

# Restore database
gunzip fraiseql_backup.sql.gz
docker-compose exec -T db psql -U fraiseql fraiseql_prod < fraiseql_backup.sql

# Start application
docker-compose up -d

# Verify recovery
curl https://yourdomain.com/health
```

## Security Hardening

### 1. Network Security

```bash
# Configure firewall
sudo ufw enable
sudo ufw allow 22/tcp
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw --force reload
```

### 2. SSL/TLS Configuration

Ensure nginx configuration includes:
```nginx
# SSL Configuration
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;
ssl_session_cache shared:SSL:10m;
ssl_session_timeout 10m;

# Security headers
add_header X-Frame-Options DENY;
add_header X-Content-Type-Options nosniff;
add_header X-XSS-Protection "1; mode=block";
add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload";
```

### 3. Container Security

```bash
# Run containers as non-root user
# Use read-only root filesystem where possible
# Implement proper secrets management
# Regular security scanning with Trivy
```

## Troubleshooting

### Common Issues

#### Application Won't Start
```bash
# Check logs
docker-compose logs fraiseql

# Check environment variables
docker-compose exec fraiseql env | grep -E "(DATABASE|REDIS)"

# Test database connectivity
docker-compose exec fraiseql python -c "import psycopg; psycopg.connect(os.environ['DATABASE_URL'])"
```

#### Database Connection Issues
```bash
# Check database logs
docker-compose logs db

# Test connection from application container
docker-compose exec fraiseql nc -zv db 5432

# Check database credentials
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "SELECT version();"
```

#### High Memory Usage
```bash
# Check application memory usage
docker stats

# Check database memory usage
docker-compose exec db psql -U fraiseql -d fraiseql_prod -c "SELECT * FROM pg_stat_activity;"

# Review application configuration
# Consider increasing instance size or optimizing queries
```

### Log Analysis

```bash
# View recent logs
docker-compose logs --tail=100 fraiseql

# Follow logs in real-time
docker-compose logs -f fraiseql

# Search for specific errors
docker-compose logs fraiseql | grep ERROR
```

## Maintenance Procedures

### Regular Maintenance Tasks

#### Weekly
- Review error logs
- Check disk space usage
- Verify backup integrity
- Update SSL certificates

#### Monthly
- Security patching
- Performance optimization
- Log rotation
- Dependency updates

#### Quarterly
- Full security audit
- Performance benchmarking
- Disaster recovery testing
- Documentation review

### Updates and Upgrades

```bash
# Update application
docker-compose pull
docker-compose up -d

# Update database schema (if needed)
docker-compose exec fraiseql python manage.py migrate

# Verify update
curl https://yourdomain.com/health
```

## Support and Contact

For production support and issues:
- Check the troubleshooting guide
- Review application logs
- Contact the DevOps team
- Create GitHub issues for bugs

---

*This deployment guide is maintained alongside the codebase. Please check for updates before major deployments.*
