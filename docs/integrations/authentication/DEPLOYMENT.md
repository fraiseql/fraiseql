# FraiseQL Authentication Deployment Guide

Production deployment guide for FraiseQL's authentication system.

## Prerequisites

**Required Knowledge:**

- OAuth 2.0 and OIDC protocols
- Kubernetes deployment and manifests
- Docker and containerization
- SSL/TLS certificate management
- Database administration and backups
- Linux system administration
- Load balancing and reverse proxy configuration
- Security best practices and compliance

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- Docker 20.10+ and Docker Compose 1.29+
- Kubernetes 1.24+ (kubectl configured)
- Helm 3+ (optional, for Kubernetes deployments)
- PostgreSQL 14+ database
- OpenSSL or cert management tool
- Nginx or reverse proxy (optional)
- Git for configuration management

**Required Infrastructure:**

- Kubernetes cluster or Docker host (for deployment)
- PostgreSQL 14+ database (primary + replica for HA)
- OAuth provider (Google Cloud, Auth0, Keycloak, etc.)
- Domain with DNS setup
- SSL/TLS certificate (Let's Encrypt, commercial CA, or internal)
- Load balancer or Ingress controller
- Network security groups/security groups properly configured
- Persistent storage for database
- Backup storage system

**Optional but Recommended:**

- Kubernetes cert-manager for automatic certificate renewal
- Helm charts for standardized deployments
- Container registry (Docker Hub, ECR, GCR, etc.)
- Secrets management system (HashiCorp Vault, AWS Secrets Manager)
- Monitoring and alerting infrastructure
- Log aggregation system
- Disaster recovery and backup testing
- Kubernetes autoscaling configuration

**Time Estimate:** 2-4 hours for Kubernetes deployment, 1-2 hours for Docker Compose setup

## Pre-Deployment Checklist

- [ ] OAuth provider credentials configured
- [ ] Database schema migrations applied
- [ ] SSL/TLS certificates installed
- [ ] Environment variables configured
- [ ] Monitoring and logging configured
- [ ] Backup strategy defined
- [ ] Security audit completed
- [ ] Load testing performed
- [ ] Runbook created

## Environment Configuration

### Production Environment Variables

```bash
# OAuth Provider (Google, Keycloak, Auth0, etc.)
OAUTH_PROVIDER=google
GOOGLE_CLIENT_ID=<prod-client-id>
GOOGLE_CLIENT_SECRET=<prod-secret>
OAUTH_REDIRECT_URI=https://api.yourdomain.com/auth/callback

# For Keycloak:
# KEYCLOAK_URL=https://keycloak.yourdomain.com
# KEYCLOAK_REALM=production
# KEYCLOAK_CLIENT_ID=fraiseql-prod
# KEYCLOAK_CLIENT_SECRET=<secret>

# JWT Configuration
JWT_ISSUER=https://accounts.google.com
JWT_ALGORITHM=RS256

# Database
DATABASE_URL=postgres://user:strong_password@prod-db.internal:5432/fraiseql
DATABASE_POOL_SIZE=20
DATABASE_MAX_LIFETIME=1800

# Security
RUST_LOG=info,fraiseql_server::auth=info
SESSION_TIMEOUT_MINUTES=60

# Server
PORT=8000
SERVER_HOST=0.0.0.0

# HTTPS (optional)
TLS_CERT_PATH=/etc/fraiseql/certs/server.crt
TLS_KEY_PATH=/etc/fraiseql/certs/server.key
```text

### .env.prod File

```bash
# Create in your deployment server
source /etc/fraiseql/auth.env

# Verify critical variables
echo "OAuth Provider: $OAUTH_PROVIDER"
echo "Database: $DATABASE_URL (hidden)"
echo "JWT Issuer: $JWT_ISSUER"
```text

## Database Setup

### 1. Create Database

```bash
# On PostgreSQL server
sudo -u postgres psql

CREATE DATABASE fraiseql;
CREATE USER fraiseql_app WITH PASSWORD 'strong_password_here';
ALTER ROLE fraiseql_app SET client_encoding TO 'utf8';
ALTER ROLE fraiseql_app SET default_transaction_isolation TO 'read committed';
ALTER ROLE fraiseql_app SET default_transaction_deferrable TO on;
ALTER ROLE fraiseql_app SET default_time_zone TO 'UTC';

GRANT ALL PRIVILEGES ON DATABASE fraiseql TO fraiseql_app;

\c fraiseql
GRANT ALL PRIVILEGES ON SCHEMA public TO fraiseql_app;
```text

### 2. Create Sessions Table

```sql
CREATE TABLE IF NOT EXISTS _system.sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_sessions_user_id ON _system.sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON _system.sessions(expires_at);
CREATE INDEX idx_sessions_revoked_at ON _system.sessions(revoked_at);

-- Grant permissions
GRANT ALL PRIVILEGES ON TABLE _system.sessions TO fraiseql_app;
GRANT ALL PRIVILEGES ON SEQUENCE _system.sessions_id_seq TO fraiseql_app;
```text

### 3. Verify Connection

```bash
export DATABASE_URL="postgres://fraiseql_app:strong_password_here@prod-db.internal:5432/fraiseql"
psql $DATABASE_URL -c "SELECT COUNT(*) FROM _system.sessions;"
```text

## Docker Deployment

### Dockerfile

```dockerfile
FROM rust:1.75 AS builder

WORKDIR /build
COPY . .

RUN cargo build --release -p fraiseql-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/fraiseql-server /usr/local/bin/

EXPOSE 8000

ENTRYPOINT ["fraiseql-server"]
```text

### Docker Compose Production

```yaml
version: '3.8'

services:
  fraiseql:
    image: fraiseql/server:latest
    container_name: fraiseql-auth
    restart: always
    environment:
      RUST_LOG: info
      DATABASE_URL: ${DATABASE_URL}
      GOOGLE_CLIENT_ID: ${GOOGLE_CLIENT_ID}
      GOOGLE_CLIENT_SECRET: ${GOOGLE_CLIENT_SECRET}
      OAUTH_REDIRECT_URI: ${OAUTH_REDIRECT_URI}
      JWT_ISSUER: ${JWT_ISSUER}
    ports:
      - "8000:8000"
    depends_on:
      - postgres
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health/auth"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

  postgres:
    image: postgres:15-alpine
    container_name: fraiseql-db
    restart: always
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_USER: fraiseql_app
      POSTGRES_PASSWORD: ${DATABASE_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U fraiseql_app"]
      interval: 10s
      timeout: 5s
      retries: 5

  nginx:
    image: nginx:alpine
    restart: always
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - /etc/letsencrypt:/etc/letsencrypt:ro
    depends_on:
      - fraiseql

volumes:
  postgres_data:
```text

## Nginx Configuration

### nginx.conf

```nginx
upstream fraiseql {
    server fraiseql:8000;
}

server {
    listen 80;
    server_name api.yourdomain.com;

    # Redirect HTTP to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name api.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/yourdomain.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-XSS-Protection "1; mode=block" always;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=10r/s;
    limit_req_zone $binary_remote_addr zone=auth_limit:10m rate=1r/s;

    location /auth/ {
        limit_req zone=auth_limit burst=5;
        proxy_pass http://fraiseql;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /graphql {
        limit_req zone=api_limit burst=20;
        proxy_pass http://fraiseql;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /health {
        access_log off;
        proxy_pass http://fraiseql;
    }
}
```text

## SSL/TLS Setup

### Using Let's Encrypt

```bash
# Install certbot
sudo apt-get install certbot python3-certbot-nginx

# Get certificate
sudo certbot certonly --standalone -d api.yourdomain.com

# Auto-renewal
sudo systemctl enable certbot.timer
sudo systemctl start certbot.timer

# Verify renewal
sudo certbot renew --dry-run
```text

## Kubernetes Deployment

### fraiseql-deployment.yaml

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  labels:
    app: fraiseql
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
    spec:
      containers:
      - name: fraiseql
        image: fraiseql/server:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        - name: GOOGLE_CLIENT_ID
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: google-client-id
        - name: GOOGLE_CLIENT_SECRET
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: google-client-secret
        livenessProbe:
          httpGet:
            path: /health/auth
            port: 8000
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health/auth
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 10
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql
spec:
  selector:
    app: fraiseql
  type: ClusterIP
  ports:
  - protocol: TCP
    port: 80
    targetPort: 8000
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
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
```text

## Monitoring Setup

### Prometheus Configuration

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'fraiseql'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
```text

### Grafana Dashboard

Import dashboard from `/docs/auth/grafana-dashboard.json`

## Backup Strategy

### Database Backups

```bash
#!/bin/bash
# backup.sh

BACKUP_DIR="/backups/fraiseql"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DB_NAME="fraiseql"

mkdir -p $BACKUP_DIR

# Full backup
pg_dump -h $DB_HOST -U fraiseql_app $DB_NAME | \
  gzip > $BACKUP_DIR/fraiseql_$TIMESTAMP.sql.gz

# Keep only last 30 days
find $BACKUP_DIR -name "fraiseql_*.sql.gz" -mtime +30 -delete

# Upload to S3
aws s3 cp $BACKUP_DIR/fraiseql_$TIMESTAMP.sql.gz \
  s3://fraiseql-backups/
```text

Schedule with cron:

```bash
# Run daily at 2 AM
0 2 * * * /scripts/backup.sh
```text

### Restore from Backup

```bash
gunzip -c fraiseql_20260121_020000.sql.gz | \
  psql -h prod-db.internal -U fraiseql_app fraiseql
```text

## Scaling

### Horizontal Scaling

- Run multiple FraiseQL instances behind load balancer
- Each instance connects to same database
- Sessions shared via PostgreSQL (or Redis)
- Stateless design allows easy scaling

### Vertical Scaling

Adjust resource limits:

```bash
# In Kubernetes
kubectl set resources deployment fraiseql \
  --limits=memory=1Gi,cpu=1000m \
  --requests=memory=512Mi,cpu=500m
```text

## Performance Tuning

### PostgreSQL Connection Pool

```bash
DATABASE_POOL_SIZE=50
DATABASE_MAX_LIFETIME=1800
```text

### Session Cache (if using Redis)

```bash
REDIS_URL=redis://redis.internal:6379
SESSION_CACHE_TTL=300
```text

## High Availability

### Multi-Region Setup

```text
Region 1: Primary database
Region 2: Read replica
Region 3: Standby replica

Failover: Automatic via RDS
```text

### Disaster Recovery

- RPO (Recovery Point Objective): 5 minutes
- RTO (Recovery Time Objective): 15 minutes
- Test failover monthly

## Cost Optimization

**Development**:

- Single instance
- Shared database
- ~$50/month

**Production**:

- 3x instances (HA)
- PostgreSQL managed service
- Monitoring and backups
- ~$500/month

## Monitoring Dashboard

Key metrics to monitor:

1. **Availability**: % uptime (target: 99.9%)
2. **Latency**: p50, p95, p99 (target: <100ms)
3. **Errors**: Error rate (target: <1%)
4. **Capacity**: CPU, memory, connections

## Troubleshooting

### Service Won't Start

```bash
# Check logs
docker logs fraiseql

# Check database connection
psql $DATABASE_URL -c "SELECT 1"

# Check OAuth provider
curl https://accounts.google.com/.well-known/openid-configuration
```text

### High Latency

```bash
# Check database slow queries
SELECT * FROM pg_stat_statements ORDER BY total_time DESC;

# Check OAuth provider latency
time curl https://accounts.google.com/.well-known/openid-configuration
```text

### Database Connection Pool Exhausted

```bash
# Increase pool size
DATABASE_POOL_SIZE=100

# Check active connections
psql -c "SELECT count(*) FROM pg_stat_activity;"
```text

## See Also

- [Monitoring Guide](./MONITORING.md)
- [Security Checklist](./SECURITY-CHECKLIST.md)
- [Troubleshooting](./TROUBLESHOOTING.md)

---

**Next Step**: Deploy to production and monitor performance.
