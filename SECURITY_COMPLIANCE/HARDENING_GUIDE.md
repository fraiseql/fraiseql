# FraiseQL Hardening Guide

> **Status:** Template - Customize for your environment
> **Last Updated:** 2025-11-22
> **Applies To:** FraiseQL v1.5.0+

## 1. Overview

This guide provides security hardening procedures for FraiseQL deployments in sensitive environments.

## 2. Application Hardening

### 2.1 Configuration Hardening

```python
# config.py - Production security settings

from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    # Disable introspection in production
    introspection_enabled=False,

    # Query complexity limits
    max_query_depth=10,
    max_query_complexity=1000,
    max_aliases=10,

    # Rate limiting
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=100,
    rate_limit_burst=20,

    # CSRF protection
    csrf_protection=True,

    # Secure cookies
    cookie_secure=True,
    cookie_httponly=True,
    cookie_samesite="strict",

    # Disable debug features
    debug=False,
    show_graphql_errors=False,
)
```

### 2.2 Environment Variables

```bash
# Required security settings
FRAISEQL_ENVIRONMENT=production
DEBUG=false
SHOW_GRAPHQL_ERRORS=false

# Strong secret key (256-bit minimum)
SECRET_KEY=$(openssl rand -hex 32)

# JWT configuration
JWT_SECRET_KEY=$(openssl rand -hex 32)
JWT_ALGORITHM=HS256
JWT_ACCESS_TOKEN_EXPIRE_MINUTES=15

# Strict CORS
ALLOWED_ORIGINS=https://app.yourdomain.com

# Database with SSL
DATABASE_URL=postgresql://user:pass@host:5432/db?sslmode=verify-full
```

### 2.3 Logging Configuration

```python
import structlog

structlog.configure(
    processors=[
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_logger_name,
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)

# Log security events
logger = structlog.get_logger()
logger.info("auth_success", user_id=user.id, ip=request.client.host)
logger.warning("auth_failure", attempted_user=username, ip=request.client.host)
```

## 3. Database Hardening

### 3.1 PostgreSQL Security

```sql
-- Create application user with minimal privileges
CREATE USER fraiseql_app WITH PASSWORD 'strong_password';

-- Grant only necessary permissions
GRANT CONNECT ON DATABASE fraiseql TO fraiseql_app;
GRANT USAGE ON SCHEMA public TO fraiseql_app;

-- Read-only for queries
GRANT SELECT ON ALL TABLES IN SCHEMA public TO fraiseql_app;
GRANT SELECT ON ALL SEQUENCES IN SCHEMA public TO fraiseql_app;

-- Execute for stored procedures
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO fraiseql_app;

-- Deny direct table modifications (use functions)
REVOKE INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public FROM fraiseql_app;

-- Create read-only user for reporting
CREATE USER fraiseql_readonly WITH PASSWORD 'readonly_password';
GRANT CONNECT ON DATABASE fraiseql TO fraiseql_readonly;
GRANT USAGE ON SCHEMA public TO fraiseql_readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO fraiseql_readonly;
```

### 3.2 Connection Security

```ini
# postgresql.conf hardening
ssl = on
ssl_cert_file = '/etc/postgresql/server.crt'
ssl_key_file = '/etc/postgresql/server.key'
ssl_ca_file = '/etc/postgresql/root.crt'
ssl_min_protocol_version = 'TLSv1.2'
ssl_ciphers = 'HIGH:!aNULL:!MD5'

# pg_hba.conf - require SSL
hostssl all all 0.0.0.0/0 scram-sha-256
hostssl all all ::0/0 scram-sha-256
```

### 3.3 Row-Level Security

```sql
-- Enable RLS
ALTER TABLE tb_data ENABLE ROW LEVEL SECURITY;

-- Create policy for tenant isolation
CREATE POLICY tenant_isolation ON tb_data
    USING (tenant_id = current_setting('app.tenant_id')::uuid);

-- Set tenant context in application
SET app.tenant_id = 'tenant-uuid-here';
```

## 4. Container Hardening

### 4.1 Dockerfile Best Practices

```dockerfile
# Use specific version, not latest
FROM python:3.13-slim-bookworm@sha256:SPECIFIC_DIGEST

# Create non-root user
RUN groupadd -r fraiseql && useradd -r -g fraiseql fraiseql

# Set working directory
WORKDIR /app

# Copy only necessary files
COPY --chown=fraiseql:fraiseql requirements.lock ./
COPY --chown=fraiseql:fraiseql src/ ./src/

# Install dependencies with hash verification
RUN pip install --no-cache-dir --require-hashes -r requirements.lock

# Remove unnecessary packages
RUN apt-get purge -y --auto-remove && rm -rf /var/lib/apt/lists/*

# Switch to non-root user
USER fraiseql

# Read-only filesystem marker
LABEL org.opencontainers.image.read-only="true"

# Health check
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Expose only necessary port
EXPOSE 8000

# Run with minimal privileges
CMD ["uvicorn", "fraiseql.main:app", "--host", "0.0.0.0", "--port", "8000"]
```

### 4.2 Docker Compose Security

```yaml
version: '3.8'

services:
  app:
    image: fraiseql:${VERSION}
    security_opt:
      - no-new-privileges:true
    read_only: true
    tmpfs:
      - /tmp:noexec,nosuid,size=100M
    cap_drop:
      - ALL
    cap_add:
      - NET_BIND_SERVICE
    user: "1000:1000"
    deploy:
      resources:
        limits:
          cpus: '1'
          memory: 512M
        reservations:
          cpus: '0.5'
          memory: 256M
```

## 5. Kubernetes Hardening

### 5.1 Pod Security Standards

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: fraiseql
spec:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    runAsGroup: 1000
    fsGroup: 1000
    seccompProfile:
      type: RuntimeDefault

  containers:
  - name: fraiseql
    image: fraiseql:v1.5.0
    securityContext:
      allowPrivilegeEscalation: false
      readOnlyRootFilesystem: true
      capabilities:
        drop:
          - ALL
    resources:
      limits:
        cpu: "1"
        memory: "512Mi"
      requests:
        cpu: "500m"
        memory: "256Mi"
    volumeMounts:
    - name: tmp
      mountPath: /tmp
    - name: cache
      mountPath: /app/.cache

  volumes:
  - name: tmp
    emptyDir:
      sizeLimit: 100Mi
  - name: cache
    emptyDir:
      sizeLimit: 50Mi
```

### 5.2 Network Policies

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: fraiseql-network-policy
  namespace: fraiseql
spec:
  podSelector:
    matchLabels:
      app: fraiseql
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 8000
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          name: database
    ports:
    - protocol: TCP
      port: 5432
  - to:
    - namespaceSelector: {}
      podSelector:
        matchLabels:
          k8s-app: kube-dns
    ports:
    - protocol: UDP
      port: 53
```

### 5.3 Secrets Management

```yaml
# Use external secrets operator
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: fraiseql-secrets
  namespace: fraiseql
spec:
  refreshInterval: 1h
  secretStoreRef:
    kind: ClusterSecretStore
    name: vault-backend
  target:
    name: fraiseql-secrets
  data:
  - secretKey: database-url
    remoteRef:
      key: fraiseql/database
      property: url
  - secretKey: jwt-secret
    remoteRef:
      key: fraiseql/jwt
      property: secret
```

## 6. Network Hardening

### 6.1 Nginx Configuration

```nginx
# nginx.conf - Security hardening

# Hide version
server_tokens off;

# Security headers
add_header X-Frame-Options "DENY" always;
add_header X-Content-Type-Options "nosniff" always;
add_header X-XSS-Protection "1; mode=block" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
add_header Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';" always;
add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;
add_header Permissions-Policy "geolocation=(), microphone=(), camera=()" always;

# SSL configuration
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
ssl_prefer_server_ciphers off;
ssl_session_timeout 1d;
ssl_session_cache shared:SSL:50m;
ssl_stapling on;
ssl_stapling_verify on;

# Rate limiting
limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
limit_req zone=api burst=20 nodelay;

# Request size limits
client_max_body_size 1m;
client_body_buffer_size 16k;

# Timeouts
client_body_timeout 12s;
client_header_timeout 12s;
keepalive_timeout 65s;
send_timeout 10s;

server {
    listen 443 ssl http2;
    server_name api.example.com;

    location /graphql {
        limit_req zone=api burst=50;
        proxy_pass http://fraiseql:8000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # Block introspection in production
    location ~ /graphql.*(__schema|__type) {
        return 403;
    }
}
```

## 7. Monitoring and Detection

### 7.1 Security Monitoring

```yaml
# prometheus-rules.yaml
groups:
- name: fraiseql-security
  rules:
  - alert: HighAuthFailureRate
    expr: rate(fraiseql_auth_failures_total[5m]) > 10
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: High authentication failure rate

  - alert: SuspiciousQueryComplexity
    expr: fraiseql_query_complexity > 800
    for: 1m
    labels:
      severity: warning
    annotations:
      summary: High query complexity detected

  - alert: RateLimitExceeded
    expr: rate(fraiseql_rate_limit_exceeded_total[5m]) > 50
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: Possible DDoS attack
```

### 7.2 Audit Log Analysis

```sql
-- Detect suspicious patterns
SELECT
    user_id,
    action,
    COUNT(*) as count,
    MAX(created_at) as last_occurrence
FROM audit.events
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY user_id, action
HAVING COUNT(*) > 100
ORDER BY count DESC;

-- Verify audit chain integrity
SELECT
    id,
    event_hash,
    previous_hash,
    signature,
    CASE
        WHEN verify_chain_integrity(id) THEN 'VALID'
        ELSE 'TAMPERED'
    END as integrity
FROM audit.events
ORDER BY id DESC
LIMIT 100;
```

## 8. Pre-Deployment Checklist

### 8.1 Application Security

- [ ] Introspection disabled
- [ ] Query complexity limits configured
- [ ] Rate limiting enabled
- [ ] CSRF protection enabled
- [ ] Debug mode disabled
- [ ] Error details hidden
- [ ] Secure cookies configured
- [ ] CORS properly restricted

### 8.2 Infrastructure Security

- [ ] TLS 1.2+ only
- [ ] Strong cipher suites
- [ ] Security headers configured
- [ ] Non-root containers
- [ ] Read-only filesystems
- [ ] Resource limits set
- [ ] Network policies applied
- [ ] Secrets encrypted

### 8.3 Database Security

- [ ] SSL connections required
- [ ] Least privilege users
- [ ] Row-level security enabled
- [ ] Audit logging enabled
- [ ] Encryption at rest
- [ ] Backup encryption

### 8.4 Monitoring

- [ ] Security alerts configured
- [ ] Audit log retention
- [ ] Log aggregation enabled
- [ ] Intrusion detection active

---

**Classification:** INTERNAL
**Distribution:** DevOps/Security Teams
