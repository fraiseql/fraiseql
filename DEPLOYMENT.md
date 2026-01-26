# FraiseQL v2 Deployment Guide

**Last Updated:** January 26, 2026
**Version:** 2.0.0-a1

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [System Requirements](#system-requirements)
3. [Configuration](#configuration)
4. [Database Setup](#database-setup)
5. [Security Hardening](#security-hardening)
6. [Running the Server](#running-the-server)
7. [Monitoring & Observability](#monitoring--observability)
8. [Troubleshooting](#troubleshooting)
9. [Production Checklist](#production-checklist)

---

## Quick Start

### Docker Deployment (Recommended)

```bash
# 1. Build the server image
docker build -t fraiseql-server:latest -f docker/Dockerfile.server .

# 2. Set up configuration
cp fraiseql-server/config.example.toml config.toml
# Edit config.toml with your settings

# 3. Run the server
docker run -p 8080:8080 \
  -v $(pwd)/config.toml:/etc/fraiseql/config.toml \
  -v $(pwd)/schema.compiled.json:/etc/fraiseql/schema.compiled.json \
  -e DATABASE_URL="postgresql://user:pass@db:5432/fraiseql" \
  fraiseql-server:latest
```

### Local Development

```bash
# 1. Compile schema
fraiseql compile schema.json -o schema.compiled.json

# 2. Start server
fraiseql-server -c fraiseql-server/config.toml

# Server will start on http://localhost:8080
# GraphQL endpoint: http://localhost:8080/graphql
# Playground: http://localhost:8080/playground
# Health check: http://localhost:8080/health
```

---

## System Requirements

### Minimum Requirements

- **CPU**: 2 cores (4+ cores recommended)
- **Memory**: 512 MB minimum (2+ GB recommended)
- **Disk**: 10 GB (depends on database size)
- **Network**: 100 Mbps minimum (1+ Gbps recommended)

### Database Support

FraiseQL v2 supports:

| Database | Version | Status | Notes |
|----------|---------|--------|-------|
| PostgreSQL | 12+ | ✅ Recommended | Full feature support, CDC via LISTEN/NOTIFY |
| MySQL | 8.0+ | ✅ Supported | Basic features, no CDC support |
| SQLite | 3.35+ | ✅ Supported | Local dev/testing only |
| SQL Server | 2019+ | ✅ Supported | Enterprise deployments |

### Required Software

- **Rust 1.75+** (for building from source)
- **Docker 20.10+** (for containerized deployment)
- **Kubernetes 1.24+** (for K8s deployments)

---

## Configuration

### Configuration File Structure

```toml
# fraiseql-server/config.toml

# Server
[server]
schema_path = "schema.compiled.json"
database_url = "postgresql://localhost/fraiseql"
bind_addr = "0.0.0.0:8080"

# Features
[features]
cors_enabled = true
cors_origins = ["https://app.example.com", "https://api.example.com"]
compression_enabled = true
tracing_enabled = true
apq_enabled = true

# Performance
[performance]
connection_pool_size = 20
query_timeout_ms = 30000
cache_enabled = true
cache_max_size = 10000

# Security
[security]
require_https = true
tls_cert = "/etc/fraiseql/certs/server.crt"
tls_key = "/etc/fraiseql/certs/server.key"
introspection_enabled = false  # Disable in production
rate_limit_enabled = true
rate_limit_requests = 1000
rate_limit_window_secs = 60

# Authentication
[auth]
jwt_secret = "${JWT_SECRET}"  # Use environment variable!
jwt_algorithms = ["RS256", "HS256"]
oauth2_enabled = false
# oauth2_provider = "github"  # Uncomment for OAuth2
```

### Environment Variables

```bash
# Database
export DATABASE_URL="postgresql://user:password@host:5432/fraiseql"

# Security
export JWT_SECRET="your-secret-key-here"  # Change this!

# Observability
export RUST_LOG="fraiseql=info,tower_http=debug"
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
```

### CORS Configuration (Important!)

⚠️ **Security Note**: CORS must be restricted to specific origins in production.

**Development** (localhost only):
```toml
cors_origins = ["http://localhost:3000"]
```

**Production** (restricted):
```toml
cors_origins = [
    "https://app.example.com",
    "https://subdomain.example.com"
]
```

**NOT Recommended** (wildcard - security risk):
```toml
cors_origins = ["*"]  # ❌ DO NOT USE IN PRODUCTION
```

---

## Database Setup

### PostgreSQL Setup

```bash
# 1. Create database
createdb fraiseql

# 2. Create user with password
psql fraiseql -c "CREATE USER fraiseql_user WITH PASSWORD 'strong_password';"

# 3. Grant privileges
psql fraiseql -c "GRANT ALL PRIVILEGES ON DATABASE fraiseql TO fraiseql_user;"

# 4. Run migrations (if any)
psql fraiseql -f migrations/001_initial_schema.sql

# 5. Verify connection
export DATABASE_URL="postgresql://fraiseql_user:strong_password@localhost/fraiseql"
psql $DATABASE_URL -c "SELECT version();"
```

### MySQL Setup

```bash
# 1. Create database
mysql -u root -p -e "CREATE DATABASE fraiseql;"

# 2. Create user
mysql -u root -p -e "CREATE USER 'fraiseql_user'@'localhost' IDENTIFIED BY 'strong_password';"

# 3. Grant privileges
mysql -u root -p -e "GRANT ALL PRIVILEGES ON fraiseql.* TO 'fraiseql_user'@'localhost';"
mysql -u root -p -e "FLUSH PRIVILEGES;"

# 4. Verify connection
export DATABASE_URL="mysql://fraiseql_user:strong_password@localhost/fraiseql"
```

### Connection Pooling

Configure connection pool for your database size:

```toml
[performance]
connection_pool_size = 20  # Default: 20
connection_max_lifetime_secs = 1800  # 30 minutes
connection_idle_timeout_secs = 60
```

**Sizing Guidelines**:
- Development: 5-10 connections
- Small production (<100 QPS): 20-50 connections
- Large production (100+ QPS): 50-200 connections

Formula: `pool_size = (cpu_cores * 2) + effective_spindle_count`

---

## Security Hardening

### TLS/SSL Configuration

```toml
[security]
require_https = true
tls_cert = "/etc/fraiseql/certs/server.crt"
tls_key = "/etc/fraiseql/certs/server.key"
tls_min_version = "TLSv1.2"
```

**Generate Self-Signed Certificate** (development only):
```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes
```

**Using Let's Encrypt** (production):
```bash
certbot certonly --standalone -d fraiseql.example.com
```

### Authentication & Authorization

**Enable JWT Validation**:
```toml
[auth]
jwt_secret = "${JWT_SECRET}"  # Must be >32 characters
jwt_algorithms = ["RS256"]     # Use RS256 for public key verification
jwt_expiration_secs = 3600
```

**Disable Introspection** (prevents schema enumeration):
```toml
[security]
introspection_enabled = false
```

### Rate Limiting

```toml
[security]
rate_limit_enabled = true
rate_limit_requests = 1000      # Requests per window
rate_limit_window_secs = 60     # Time window in seconds
rate_limit_per_ip = true        # Limit per source IP
```

### Input Validation

FraiseQL automatically validates:
- ✅ SQL injection prevention (parameterized queries)
- ✅ Query depth limits (prevents nested attack queries)
- ✅ Query complexity analysis (prevents expensive queries)
- ✅ Timeout enforcement (prevents long-running queries)

---

## Running the Server

### Using Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_USER: fraiseql_user
      POSTGRES_PASSWORD: strong_password
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

  fraiseql:
    build:
      context: .
      dockerfile: Dockerfile
    environment:
      DATABASE_URL: "postgresql://fraiseql_user:strong_password@postgres:5432/fraiseql"
      JWT_SECRET: "your-secret-key"
      RUST_LOG: "fraiseql=info"
    ports:
      - "8080:8080"
    depends_on:
      - postgres
    volumes:
      - ./config.toml:/etc/fraiseql/config.toml
      - ./schema.compiled.json:/etc/fraiseql/schema.compiled.json

volumes:
  postgres_data:
```

**Start services**:
```bash
docker-compose up -d
```

### Using Kubernetes

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql-server
  template:
    metadata:
      labels:
        app: fraiseql-server
    spec:
      containers:
      - name: fraiseql-server
        image: fraiseql-server:latest
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: jwt-secret
        resources:
          requests:
            cpu: "500m"
            memory: "512Mi"
          limits:
            cpu: "2000m"
            memory: "2Gi"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
```

**Deploy to K8s**:
```bash
kubectl apply -f k8s/deployment.yaml
```

---

## Monitoring & Observability

### Health Endpoint

```bash
curl http://localhost:8080/health

# Response:
# {
#   "status": "healthy",
#   "database": "connected",
#   "uptime_seconds": 3600,
#   "metrics": { ... }
# }
```

### Metrics Endpoint

```bash
curl http://localhost:8080/metrics

# Prometheus-format metrics:
# fraiseql_query_duration_ms{query="getUserById"} 45
# fraiseql_cache_hit_rate 0.87
# fraiseql_db_pool_active_connections 12
```

### Structured Logging

Control log level via `RUST_LOG`:

```bash
# Info level (default)
export RUST_LOG="fraiseql=info"

# Debug level (development)
export RUST_LOG="fraiseql=debug,tower_http=debug"

# Specific module
export RUST_LOG="fraiseql_core::executor=debug"
```

### Distributed Tracing

Enable OpenTelemetry:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317"
fraiseql-server -c config.toml
```

---

## Troubleshooting

### Server won't start

**Symptom**: "Address already in use"
```bash
# Find process using port
lsof -i :8080

# Kill the process
kill -9 <PID>
```

**Symptom**: "Cannot connect to database"
```bash
# Test database connection
psql $DATABASE_URL -c "SELECT 1;"

# Check DATABASE_URL format:
# postgresql://user:password@host:5432/dbname
```

### Queries returning errors

**Enable debug logging**:
```bash
export RUST_LOG="fraiseql=debug"
fraiseql-server -c config.toml
```

**Check query complexity** (if timeouts occur):
```
Query depth exceeded limit (max: 10)
Query complexity exceeded limit (max: 1000)
```

Adjust in config:
```toml
[security]
query_max_depth = 15
query_max_complexity = 2000
```

### High memory usage

**Reduce cache size**:
```toml
[performance]
cache_max_size = 5000  # Reduce from 10000
```

**Reduce pool size**:
```toml
[performance]
connection_pool_size = 10  # Reduce from 20
```

### Slow queries

**Enable query profiling**:
```bash
export RUST_LOG="fraiseql_core::executor=debug"
```

**Check database stats**:
```sql
-- PostgreSQL
SELECT query, calls, mean_exec_time
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;
```

---

## Production Checklist

Before deploying to production:

### Security
- [ ] CORS origins configured (not wildcard)
- [ ] TLS/HTTPS enabled
- [ ] JWT_SECRET is strong (>32 characters, random)
- [ ] Introspection disabled
- [ ] Rate limiting enabled
- [ ] Database credentials in secrets (not config files)
- [ ] All secrets use environment variables

### Configuration
- [ ] Database URL correct for production
- [ ] Connection pool size appropriate for load
- [ ] Query timeout set (recommend 30-60 seconds)
- [ ] Compression enabled
- [ ] APQ enabled (for repeated queries)

### Performance
- [ ] Database indexes created on frequently-filtered columns
- [ ] Cache enabled (unless not needed)
- [ ] Connection pooling configured
- [ ] Load testing completed
- [ ] Database connection limits reviewed

### Monitoring
- [ ] Health endpoint accessible
- [ ] Metrics endpoint configured
- [ ] Logging configured (RUST_LOG)
- [ ] Alerts configured on error rates
- [ ] Database monitoring in place

### Deployment
- [ ] Database backups configured
- [ ] Disaster recovery plan tested
- [ ] Rollback procedure documented
- [ ] Scaling plan in place
- [ ] SSL certificates renewed before expiry

### Testing
- [ ] All queries validated against schema
- [ ] Authorization rules tested
- [ ] Error cases handled gracefully
- [ ] Query complexity limits tested
- [ ] Load testing passed

---

## Support & Help

- **Documentation**: See [README.md](README.md)
- **Security**: See [SECURITY.md](SECURITY.md)
- **Troubleshooting**: See [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- **Issues**: Report at https://github.com/fraiseql/fraiseql/issues

---

**Remember**: Always test configuration changes in a staging environment before deploying to production.
