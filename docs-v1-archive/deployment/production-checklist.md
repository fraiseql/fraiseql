# Production Readiness Checklist

## Overview

This comprehensive checklist ensures your FraiseQL deployment is production-ready, secure, performant, and maintainable. Each item includes verification steps and recommended configurations.

## Pre-Deployment Assessment

### System Requirements ✓

- [ ] **Infrastructure sized correctly**

  - Minimum: 2 CPU cores, 4GB RAM
  - Recommended: 4+ CPU cores, 8GB+ RAM
  - PostgreSQL 14+ available
  - Redis cache configured

- [ ] **Load testing completed**
  ```bash
  # Example with k6
  k6 run --vus 100 --duration 30s load-test.js
  ```
  - Target: Handle expected peak load × 2
  - Response time P95 < 500ms
  - No memory leaks during sustained load

- [ ] **Capacity planning documented**

  - Expected requests/second
  - Database size projections
  - Storage requirements
  - Bandwidth estimates

## Security Checklist

### Authentication & Authorization

- [ ] **Authentication implemented**
  ```python
  # Verify JWT configuration
  JWT_SECRET = os.environ.get("JWT_SECRET")  # ✓ From environment
  JWT_EXPIRATION = 3600  # 1 hour
  JWT_ALGORITHM = "HS256"
  ```

- [ ] **Authorization rules defined**

  - Role-based access control (RBAC)
  - Field-level permissions
  - Query depth limiting

- [ ] **API keys managed securely**

  - Stored in secrets manager
  - Rotation policy defined
  - Audit logging enabled

### Network Security

- [ ] **HTTPS/TLS configured**
  ```nginx
  ssl_protocols TLSv1.2 TLSv1.3;
  ssl_ciphers HIGH:!aNULL:!MD5;
  ssl_prefer_server_ciphers on;
  ```

- [ ] **CORS properly configured**
  !!! note "CORS is disabled by default"
      Configure CORS at the reverse proxy level, or enable only if serving browsers directly:

  **Option 1: Reverse Proxy CORS (Recommended)**
  ```nginx
  # Nginx configuration
  add_header Access-Control-Allow-Origin "https://app.example.com";
  add_header Access-Control-Allow-Methods "GET, POST, OPTIONS";
  add_header Access-Control-Allow-Headers "Content-Type, Authorization";
  ```

  **Option 2: Application-level CORS (Only if needed)**
  ```python
  config = FraiseQLConfig(
      cors_enabled=True,  # Explicitly enable
      cors_origins=[
          "https://app.example.com",
          "https://www.example.com"
      ],
      cors_methods=["GET", "POST"],
      cors_headers=["Content-Type", "Authorization"]
  )
  ```

- [ ] **Rate limiting enabled**
  ```python
  RATE_LIMIT_PER_MINUTE = 100
  RATE_LIMIT_PER_HOUR = 1000
  RATE_LIMIT_BURST = 20
  ```

- [ ] **DDoS protection configured**

  - CloudFlare or AWS Shield
  - Rate limiting at load balancer
  - Connection limits set

### Data Security

- [ ] **Encryption at rest**
  ```sql
  -- PostgreSQL encryption
  CREATE TABLESPACE encrypted_tablespace
  LOCATION '/encrypted/data'
  WITH (encryption = true);
  ```

- [ ] **Encryption in transit**

  - Database SSL connections
  - Redis TLS enabled
  - Inter-service TLS

- [ ] **Secrets management**
  ```yaml
  # Never in code or config files
  DATABASE_URL: ${SECRET_MANAGER_DB_URL}
  JWT_SECRET: ${SECRET_MANAGER_JWT}
  API_KEYS: ${SECRET_MANAGER_API_KEYS}
  ```

- [ ] **SQL injection protection**
  ```python
  # Always use parameterized queries
  query = "SELECT * FROM users WHERE id = %s"
  cursor.execute(query, (user_id,))
  ```

- [ ] **Input validation**
  ```python
  # Validate all inputs
  from pydantic import BaseModel, validator

  class UserInput(BaseModel):
      email: EmailStr
      age: int = Field(ge=0, le=150)

      @validator('email')
      def validate_email(cls, v):
          # Custom validation logic
          return v
  ```

### Security Headers

- [ ] **Security headers configured**
  ```nginx
  add_header X-Frame-Options "SAMEORIGIN" always;
  add_header X-Content-Type-Options "nosniff" always;
  add_header X-XSS-Protection "1; mode=block" always;
  add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
  add_header Content-Security-Policy "default-src 'self'" always;
  ```

## Database Checklist

### Configuration

- [ ] **Connection pooling configured**
  ```python
  DATABASE_CONFIG = {
      "pool_size": 20,
      "max_overflow": 10,
      "pool_timeout": 30,
      "pool_recycle": 3600,
      "pool_pre_ping": True,
  }
  ```

- [ ] **Query optimization completed**
  ```sql
  -- Analyze query performance
  EXPLAIN ANALYZE SELECT ...;

  -- Create necessary indexes
  CREATE INDEX idx_users_email ON users(email);
  CREATE INDEX idx_posts_created_at ON posts(created_at DESC);
  ```

- [ ] **Database parameters tuned**
  ```sql
  -- postgresql.conf
  max_connections = 200
  shared_buffers = 256MB
  effective_cache_size = 1GB
  maintenance_work_mem = 128MB
  checkpoint_completion_target = 0.9
  wal_buffers = 16MB
  default_statistics_target = 100
  random_page_cost = 1.1
  effective_io_concurrency = 200
  ```

### Backup & Recovery

- [ ] **Backup strategy implemented**
  ```bash
  # Automated daily backups
  0 2 * * * pg_dump -h localhost -U fraiseql -d fraiseql_prod > /backup/fraiseql_$(date +%Y%m%d).sql
  ```

- [ ] **Point-in-time recovery tested**
  ```sql
  -- Enable WAL archiving
  archive_mode = on
  archive_command = 'cp %p /archive/%f'
  ```

- [ ] **Backup verification automated**
  ```bash
  # Verify backup integrity
  pg_restore --list backup.sql > /dev/null 2>&1
  if [ $? -eq 0 ]; then
      echo "Backup valid"
  fi
  ```

- [ ] **Recovery time objective (RTO) defined**

  - Target: < 1 hour
  - Documented recovery procedures
  - Regular recovery drills

### High Availability

- [ ] **Replication configured**
  ```sql
  -- Streaming replication
  primary_conninfo = 'host=primary port=5432 user=replicator'
  restore_command = 'cp /archive/%f %p'
  ```

- [ ] **Failover tested**

  - Automatic failover configured
  - Manual failover documented
  - Connection string updates automated

## Application Checklist

### Configuration

- [ ] **Production mode enabled**
  ```python
  FRAISEQL_MODE = "production"
  DEBUG = False
  TESTING = False
  ```

- [ ] **Environment variables set**
  ```bash
  # .env.production
  DATABASE_URL=postgresql://user:pass@host:5432/db
  REDIS_URL=redis://:password@redis:6379/0
  SECRET_KEY=$(openssl rand -hex 32)
  JWT_SECRET=$(openssl rand -hex 32)
  ```

- [ ] **Logging configured**
  ```python
  LOGGING = {
      'version': 1,
      'handlers': {
          'file': {
              'class': 'logging.handlers.RotatingFileHandler',
              'filename': '/var/log/fraiseql/app.log',
              'maxBytes': 10485760,  # 10MB
              'backupCount': 5,
              'formatter': 'json',
          },
      },
      'root': {
          'level': 'INFO',
          'handlers': ['file'],
      },
  }
  ```

### Error Handling

- [ ] **Comprehensive error handling**
  ```python
  @app.exception_handler(Exception)
  async def global_exception_handler(request, exc):
      logger.error(f"Unhandled exception: {exc}", exc_info=True)
      return JSONResponse(
          status_code=500,
          content={"error": "Internal server error"}
      )
  ```

- [ ] **Error tracking configured**
  ```python
  # Sentry configuration
  import sentry_sdk
  sentry_sdk.init(
      dsn=os.environ.get("SENTRY_DSN"),
      environment="production",
      traces_sample_rate=0.1,
  )
  ```

- [ ] **Graceful degradation**

  - Circuit breakers implemented
  - Fallback mechanisms ready
  - Cache serving during outages

### Health Checks

- [ ] **Health endpoints implemented**
  ```python
  @app.get("/health")
  async def health():
      return {"status": "healthy"}

  @app.get("/ready")
  async def ready():
      # Check database
      await db.execute("SELECT 1")
      # Check Redis
      await redis.ping()
      return {"status": "ready"}
  ```

- [ ] **Liveness probes configured**
  ```yaml
  livenessProbe:
    httpGet:
      path: /health
      port: 8000
    initialDelaySeconds: 30
    periodSeconds: 10
  ```

- [ ] **Readiness probes configured**
  ```yaml
  readinessProbe:
    httpGet:
      path: /ready
      port: 8000
    initialDelaySeconds: 5
    periodSeconds: 5
  ```

## Infrastructure Checklist

### Load Balancing

- [ ] **Load balancer configured**

  - Health checks enabled
  - SSL termination configured
  - Session affinity if needed
  - Connection draining enabled

- [ ] **Auto-scaling configured**
  ```yaml
  # HPA configuration
  metrics:

  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  ```

### Networking

- [ ] **CDN configured** (if applicable)

  - Static assets cached
  - Geographic distribution
  - DDoS protection enabled

- [ ] **DNS configured**

  - Multiple A records
  - TTL appropriately set
  - DNSSEC enabled

- [ ] **SSL certificates**

  - Valid certificates installed
  - Auto-renewal configured
  - Certificate monitoring enabled

### Storage

- [ ] **Storage provisioned**

  - Adequate disk space
  - Fast SSD storage
  - Backup storage separate

- [ ] **Log rotation configured**
  ```bash
  /var/log/fraiseql/*.log {
      daily
      rotate 30
      compress
      delaycompress
      notifempty
      create 0640 fraiseql fraiseql
  }
  ```

## Monitoring Checklist

### Metrics

- [ ] **Application metrics exposed**
  ```python
  # Prometheus metrics
  from prometheus_client import Counter, Histogram

  request_count = Counter('fraiseql_requests_total', 'Total requests')
  request_duration = Histogram('fraiseql_request_duration_seconds', 'Request duration')
  ```

- [ ] **System metrics collected**

  - CPU usage
  - Memory usage
  - Disk I/O
  - Network I/O

- [ ] **Business metrics tracked**

  - Request rate
  - Error rate
  - Response time
  - Active users

### Alerting

- [ ] **Alerts configured**
  ```yaml
  # Example Prometheus alert

  - alert: HighErrorRate
    expr: rate(fraiseql_errors_total[5m]) > 0.05
    for: 5m
    annotations:
      summary: "High error rate detected"
  ```

- [ ] **Alert channels set up**

  - Email notifications
  - Slack/Teams integration
  - PagerDuty for critical alerts

- [ ] **Runbooks created**

  - Common issues documented
  - Resolution steps defined
  - Escalation procedures clear

### Logging

- [ ] **Centralized logging**

  - Log aggregation configured
  - Log retention policy set
  - Search and analysis tools ready

- [ ] **Structured logging**
  ```python
  logger.info("Request processed", extra={
      "user_id": user_id,
      "endpoint": "/graphql",
      "duration": duration,
      "status": 200
  })
  ```

- [ ] **Audit logging**

  - Authentication events
  - Authorization failures
  - Data modifications
  - Administrative actions

## Performance Checklist

### Optimization

- [ ] **Query optimization**

  - N+1 queries eliminated
  - Batch loading implemented
  - Query complexity limits set

- [ ] **Caching strategy**
  ```python
  # Redis caching
  @cache(ttl=300)
  async def get_user(user_id: int):
      return await db.fetch_one(...)
  ```

- [ ] **Response compression**
  ```python
  from fastapi.middleware.gzip import GZipMiddleware
  app.add_middleware(GZipMiddleware, minimum_size=1000)
  ```

- [ ] **Connection pooling**

  - Database pool sized correctly
  - Redis pool configured
  - HTTP connection reuse

### Testing

- [ ] **Load testing passed**

  - Peak load handled
  - Sustained load stable
  - Graceful degradation verified

- [ ] **Performance benchmarks met**

  - P50 < 100ms
  - P95 < 500ms
  - P99 < 1000ms

## Disaster Recovery Checklist

### Backup Strategy

- [ ] **Backup schedule defined**

  - Daily full backups
  - Hourly incremental backups
  - Transaction log backups

- [ ] **Backup testing automated**

  - Weekly restore tests
  - Data integrity verification
  - Recovery time measurement

### Recovery Procedures

- [ ] **Runbooks documented**

  - Step-by-step procedures
  - Contact information
  - Decision trees

- [ ] **Recovery drills conducted**

  - Quarterly DR drills
  - Lessons learned documented
  - Procedures updated

### Business Continuity

- [ ] **RTO/RPO defined**

  - Recovery Time Objective: < 1 hour
  - Recovery Point Objective: < 15 minutes

- [ ] **Communication plan**

  - Stakeholder notifications
  - Status page updates
  - Customer communications

## Documentation Checklist

- [ ] **API documentation complete**
- [ ] **Deployment procedures documented**
- [ ] **Troubleshooting guide created**
- [ ] **Architecture diagrams updated**
- [ ] **Security policies documented**
- [ ] **SLA defined and published**

## Final Verification

### Pre-Launch

- [ ] **Security scan completed**
  ```bash
  # Run security scanner
  safety check
  bandit -r src/
  ```

- [ ] **Dependencies updated**
  ```bash
  pip list --outdated
  pip-audit
  ```

- [ ] **License compliance verified**
- [ ] **Performance baseline established**
- [ ] **Monitoring dashboard created**
- [ ] **Team trained on procedures**

### Post-Launch

- [ ] **Smoke tests passing**
- [ ] **Monitoring active**
- [ ] **Alerts verified**
- [ ] **Performance meeting targets**
- [ ] **No critical errors in logs**
- [ ] **Customer feedback positive**

## Sign-Off

| Role | Name | Date | Signature |
|------|------|------|-----------|
| Engineering Lead | | | |
| Security Lead | | | |
| Operations Lead | | | |
| Product Owner | | | |

## Notes

Remember: This checklist is a living document. Update it based on:

- Lessons learned from incidents
- New security requirements
- Performance optimizations
- Technology updates

**Last Updated**: [Current Date]
**Version**: 1.0.0
