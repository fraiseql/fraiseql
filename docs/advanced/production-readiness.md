---
← [Advanced Topics](index.md) | [Home](../index.md) | [Next: Monitoring](../deployment/monitoring.md) →
---

# Production Readiness Checklist

> **In this section:** Ensure your FraiseQL application is ready for production deployment
> **Prerequisites:** [Deployment setup](../deployment/index.md) completed
> **Time to complete:** 45 minutes

This comprehensive checklist ensures your FraiseQL application meets production standards for security, performance, reliability, and observability.

## Pre-Deployment Checklist

### Security ✅

#### Authentication & Authorization
- [ ] **Authentication implemented** - JWT, OAuth, or session-based auth
- [ ] **Authorization rules defined** - Field-level and operation-level permissions
- [ ] **Input validation comprehensive** - All user inputs validated and sanitized
- [ ] **Rate limiting configured** - Protection against abuse and DDoS
- [ ] **CORS properly configured** - Appropriate origins and methods allowed
- [ ] **SQL injection prevention verified** - Using parameterized queries only

#### Data Protection
- [ ] **Secrets externalized** - No hardcoded passwords or API keys
- [ ] **Environment variables secure** - Using secret management systems
- [ ] **Database access restricted** - Principle of least privilege applied
- [ ] **HTTPS enforced** - All traffic encrypted with valid SSL certificates
- [ ] **Security headers configured** - HSTS, CSP, X-Frame-Options, etc.

```python
# Security headers example
@app.middleware("http")
async def add_security_headers(request: Request, call_next):
    response = await call_next(request)

    response.headers.update({
        "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
        "X-Content-Type-Options": "nosniff",
        "X-Frame-Options": "DENY",
        "X-XSS-Protection": "1; mode=block",
        "Content-Security-Policy": "default-src 'self'",
        "Referrer-Policy": "strict-origin-when-cross-origin"
    })

    return response
```

### Database ✅

#### Connection Management
- [ ] **Connection pooling configured** - Appropriate min/max connections
- [ ] **Connection timeouts set** - Prevent hanging connections
- [ ] **Prepared statements used** - Better performance and security
- [ ] **Transaction isolation appropriate** - Based on consistency requirements
- [ ] **Dead connection handling** - Automatic reconnection on failures

```python
# Connection pool configuration
DATABASE_CONFIG = {
    "min_size": 10,
    "max_size": 50,
    "command_timeout": 30,
    "server_settings": {
        "jit": "off",  # For consistent performance
        "application_name": "fraiseql-prod"
    }
}
```

#### Performance Optimization
- [ ] **Indexes optimized** - All query paths covered by appropriate indexes
- [ ] **Query performance analyzed** - No N+1 queries, optimal execution plans
- [ ] **Connection limits appropriate** - Based on concurrent user load
- [ ] **Vacuum and analyze scheduled** - Regular maintenance tasks
- [ ] **Statistics updated** - Query planner has current statistics

#### Backup & Recovery
- [ ] **Automated backups configured** - Daily incremental, weekly full
- [ ] **Backup retention policy defined** - Legal/business requirements met
- [ ] **Recovery procedures tested** - RTO/RPO requirements verified
- [ ] **Point-in-time recovery available** - For data corruption scenarios
- [ ] **Cross-region backup replication** - For disaster recovery

### Application ✅

#### Configuration Management
- [ ] **Production environment set** - `FRAISEQL_MODE=production`
- [ ] **Debug mode disabled** - No debug information leaked to users
- [ ] **Logging configured** - Structured logs with appropriate levels
- [ ] **Error tracking integrated** - Sentry, Rollbar, or similar service
- [ ] **Configuration externalized** - 12-factor app principles followed

```bash
# Essential production environment variables
export FRAISEQL_MODE=production
export FRAISEQL_LOG_LEVEL=INFO
export FRAISEQL_QUERY_TIMEOUT=30
export FRAISEQL_MAX_QUERY_DEPTH=10
export FRAISEQL_MAX_QUERY_COMPLEXITY=1000
export DATABASE_URL="postgresql://..."
export REDIS_URL="redis://..."
export SECRET_KEY="..."
export SENTRY_DSN="..."
```

#### Error Handling
- [ ] **Comprehensive error handling** - All edge cases covered
- [ ] **User-friendly error messages** - No internal details exposed
- [ ] **Error logging complete** - All errors captured with context
- [ ] **Graceful degradation** - Service continues during partial failures
- [ ] **Circuit breakers implemented** - For external service dependencies

#### Performance
- [ ] **Response time targets met** - P95 < 200ms, P99 < 500ms typical
- [ ] **Memory usage optimized** - No memory leaks, appropriate limits
- [ ] **CPU usage efficient** - Proper async/await usage
- [ ] **Caching strategy implemented** - Redis, PostgreSQL, or application-level
- [ ] **Query complexity limits enforced** - Protection against expensive queries

### Infrastructure ✅

#### High Availability
- [ ] **Load balancer configured** - Multiple application instances
- [ ] **Health checks implemented** - `/health` and `/ready` endpoints
- [ ] **Auto-scaling configured** - Based on CPU, memory, or request rate
- [ ] **Rolling deployments supported** - Zero-downtime deployments
- [ ] **Database high availability** - Master-replica or cluster setup

```python
# Health check endpoints
@app.get("/health")
async def health_check():
    """Basic health check"""
    return {"status": "healthy", "timestamp": datetime.utcnow()}

@app.get("/ready")
async def readiness_check():
    """Readiness check with dependencies"""
    try:
        # Check database connection
        await check_database_connection()
        # Check Redis connection
        await check_redis_connection()

        return {"status": "ready", "checks": {"database": "ok", "redis": "ok"}}
    except Exception as e:
        raise HTTPException(503, f"Not ready: {str(e)}")
```

#### Resource Management
- [ ] **Resource limits configured** - CPU, memory, disk quotas
- [ ] **Disk space monitoring** - Alerts before space exhaustion
- [ ] **Log rotation configured** - Prevent disk space issues
- [ ] **Temporary file cleanup** - Regular cleanup of temp files
- [ ] **Network security groups** - Proper firewall rules

### Monitoring & Observability ✅

#### Metrics Collection
- [ ] **Application metrics exposed** - Prometheus format preferred
- [ ] **Database metrics monitored** - Connection count, query time, etc.
- [ ] **System metrics collected** - CPU, memory, disk, network
- [ ] **Business metrics tracked** - User activity, feature usage
- [ ] **Custom dashboards created** - Grafana or similar tool

```python
# Key metrics to monitor
from prometheus_client import Counter, Histogram, Gauge

# Application metrics
REQUEST_COUNT = Counter('fraiseql_requests_total', 'Total requests', ['method', 'endpoint'])
REQUEST_DURATION = Histogram('fraiseql_request_duration_seconds', 'Request duration')
ACTIVE_CONNECTIONS = Gauge('fraiseql_db_connections_active', 'Active DB connections')
QUERY_COMPLEXITY = Histogram('fraiseql_query_complexity', 'GraphQL query complexity')
CACHE_HITS = Counter('fraiseql_cache_hits_total', 'Cache hits', ['cache_type'])
```

#### Alerting
- [ ] **Alert rules configured** - For all critical conditions
- [ ] **Alert routing set up** - Appropriate escalation paths
- [ ] **Alert fatigue minimized** - Only actionable alerts enabled
- [ ] **Runbooks documented** - Clear resolution steps for each alert
- [ ] **On-call procedures defined** - Clear responsibility and escalation

#### Logging
- [ ] **Structured logging implemented** - JSON format with consistent fields
- [ ] **Log aggregation configured** - ELK stack, Loki, or cloud solution
- [ ] **Log retention policy** - Based on compliance requirements
- [ ] **Security event logging** - Authentication, authorization events
- [ ] **Performance logging** - Slow queries, high latency requests

```python
# Structured logging example
import structlog

logger = structlog.get_logger()

@fraiseql.query
async def users(info) -> list[User]:
    logger.info(
        "users_query_started",
        user_id=info.context.get("user", {}).get("id"),
        query_complexity=calculate_complexity(info)
    )

    start_time = time.time()
    try:
        result = await get_users()

        logger.info(
            "users_query_completed",
            duration=time.time() - start_time,
            result_count=len(result)
        )

        return result
    except Exception as e:
        logger.error(
            "users_query_failed",
            error=str(e),
            duration=time.time() - start_time
        )
        raise
```

## Performance Validation

### Load Testing
```bash
# Load testing with k6
k6 run --vus 100 --duration 5m load-test.js
```

```javascript
// load-test.js
import http from 'k6/http';
import { check } from 'k6';

const GRAPHQL_ENDPOINT = 'http://localhost:8000/graphql';

export let options = {
  stages: [
    { duration: '2m', target: 100 },  // Ramp up
    { duration: '5m', target: 100 },  // Stay at 100 users
    { duration: '2m', target: 200 },  // Scale up
    { duration: '5m', target: 200 },  // Stay at 200 users
    { duration: '2m', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<200'], // 95% of requests under 200ms
    http_req_failed: ['rate<0.01'],   // Error rate under 1%
  },
};

export default function() {
  const payload = JSON.stringify({
    query: `
      query {
        users(limit: 10) {
          id
          name
          email
        }
      }
    `
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
  };

  const response = http.post(GRAPHQL_ENDPOINT, payload, params);

  check(response, {
    'status is 200': (r) => r.status === 200,
    'response time < 200ms': (r) => r.timings.duration < 200,
    'no GraphQL errors': (r) => !JSON.parse(r.body).errors,
  });
}
```

### Performance Benchmarks

Target performance metrics for production:

| Metric | Target | Critical |
|--------|---------|----------|
| **Response Time P50** | < 50ms | < 100ms |
| **Response Time P95** | < 200ms | < 500ms |
| **Response Time P99** | < 500ms | < 1000ms |
| **Throughput** | > 1000 req/s | > 500 req/s |
| **Error Rate** | < 0.1% | < 1% |
| **Database Connections** | < 80% pool | < 95% pool |
| **Memory Usage** | < 80% available | < 95% available |
| **CPU Usage** | < 70% average | < 90% peak |

## Security Validation

### Security Testing
```bash
# OWASP ZAP security scan
zap-baseline.py -t http://localhost:8000/graphql

# SQL injection testing
sqlmap -u "http://localhost:8000/graphql" --data='{"query":"..."}' --level=5
```

### Security Checklist
- [ ] **Vulnerability scan passed** - No high/critical findings
- [ ] **Dependency scan clean** - All packages up to date
- [ ] **Penetration testing completed** - External security assessment
- [ ] **Compliance requirements met** - GDPR, HIPAA, PCI-DSS as applicable
- [ ] **Security incident response plan** - Clear procedures documented

## Deployment Validation

### Pre-Deployment Tests
```bash
# Run full test suite
pytest tests/ --cov=fraiseql --cov-report=html

# Database migration test
fraiseql migrate --dry-run

# Configuration validation
fraiseql check-config

# Load test against staging
k6 run --vus 50 --duration 2m staging-load-test.js
```

### Deployment Checklist
- [ ] **Staging environment identical** - Same configuration as production
- [ ] **Database migrations tested** - Forward and rollback procedures
- [ ] **Rollback plan prepared** - Quick recovery if issues arise
- [ ] **Feature flags configured** - Ability to disable features quickly
- [ ] **Monitoring alerts active** - Before traffic hits new deployment

### Post-Deployment Validation
- [ ] **Smoke tests passed** - Critical user flows working
- [ ] **Metrics within normal ranges** - No performance degradation
- [ ] **Error rates normal** - No spike in errors
- [ ] **Database performance stable** - No connection or query issues
- [ ] **External integrations working** - All APIs and services responding

## Maintenance Procedures

### Regular Maintenance
- [ ] **Database maintenance scheduled** - Weekly VACUUM, monthly REINDEX
- [ ] **Log rotation configured** - Daily rotation, 30-day retention
- [ ] **Certificate renewal automated** - SSL certificates auto-renew
- [ ] **Dependency updates scheduled** - Monthly security updates
- [ ] **Backup restoration tested** - Monthly recovery drills

### Disaster Recovery
- [ ] **RTO/RPO defined** - Recovery time and data loss objectives
- [ ] **DR procedures documented** - Step-by-step recovery guide
- [ ] **DR site maintained** - Secondary site ready if needed
- [ ] **Communication plan** - Incident notification procedures
- [ ] **Recovery testing scheduled** - Quarterly DR drills

## Production Launch Decision

### Go/No-Go Criteria

**✅ GO Criteria (all must be met):**
- All security requirements satisfied
- Performance benchmarks met in load testing
- All production checklist items completed
- Rollback plan tested and ready
- Monitoring and alerting fully operational
- Team trained on incident response

**❌ NO-GO Criteria (any blocks launch):**
- Critical security vulnerabilities unresolved
- Performance targets not met under load
- Database backup/recovery untested
- Essential monitoring not working
- Incident response procedures undefined

### Post-Launch Monitoring

**First 24 Hours:**
- [ ] Continuous monitoring dashboard active
- [ ] On-call engineer available
- [ ] Error rate and performance within targets
- [ ] User feedback channels monitored
- [ ] Ready to rollback if needed

**First Week:**
- [ ] Trend analysis of key metrics
- [ ] User adoption and engagement tracking
- [ ] Performance optimization based on real usage
- [ ] Documentation updates based on operational learnings

## See Also

### Production Guides
- [**Deployment Guide**](../deployment/index.md) - Step-by-step deployment
- [**Monitoring Setup**](../deployment/monitoring.md) - Observability implementation
- [**Security Guide**](security.md) - Comprehensive security practices
- [**Performance Tuning**](performance.md) - Optimization strategies

### Operations
- [**Troubleshooting**](../errors/troubleshooting.md) - Common production issues
- [**Testing Guide**](../testing/index.md) - Production testing strategies
- [**Configuration Reference**](configuration.md) - All configuration options
