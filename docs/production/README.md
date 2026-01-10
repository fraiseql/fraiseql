# Production Documentation

Complete guides for deploying, monitoring, and running FraiseQL in production environments.

## Deployment

- **[Deployment Guide](deployment.md)** - Production deployment strategies
  - Docker and Docker Compose setup
  - Environment configuration
  - Database connection pooling (PgBouncer recommended)
  - Scaling strategies and best practices

## Monitoring & Observability

- **[Monitoring](monitoring.md)** - Built-in monitoring and error tracking
  - PostgreSQL-based error tracking (replaces Sentry)
  - Custom notification channels (Email, Slack, Webhook)
  - Error fingerprinting and grouping
  - OpenTelemetry integration
- **[Observability](observability.md)** - Logging, tracing, and metrics
  - Structured logging patterns
  - Distributed tracing with OpenTelemetry
  - Performance metrics collection
  - Grafana dashboard integration
- **[Health Checks](health-checks.md)** - Application health monitoring
  - Liveness and readiness probes
  - Database connection health
  - Custom health check endpoints

## Security

- **[Security Guide](security.md)** - Production security hardening
  - Row-Level Security (RLS) implementation
  - Authentication and authorization patterns
  - CORS configuration
  - SQL injection prevention
  - Cryptographic audit logging (SHA-256 + HMAC)
  - Rate limiting and DDoS protection
- **[Security Policy](../../SECURITY.md)** - Vulnerability reporting and security updates

## Cost Optimization

**Replace 4 Services with PostgreSQL** - Save $5,400-48,000/year:
- **Caching**: PostgreSQL UNLOGGED tables (replaces Redis)
- **Error Tracking**: Built-in monitoring (replaces Sentry)
- **Observability**: PostgreSQL-based metrics (replaces APM tools)
- **Centralized Storage**: One database to backup and monitor

See [Monitoring Guide](monitoring.md) for migration from Redis/Sentry.

## Production Checklist

Before deploying to production:

### Database
- [ ] Connection pooling configured (PgBouncer or pgpool-II)
- [ ] Row-Level Security policies created
- [ ] Audit logging enabled
- [ ] Backup strategy implemented
- [ ] PostgreSQL extensions installed (`uuid-ossp`, `ltree`, etc.)

### Application
- [ ] Environment variables secured (use secrets manager)
- [ ] CORS configured for production domains
- [ ] Rate limiting enabled
- [ ] Health check endpoints configured
- [ ] Error tracking initialized

### Monitoring
- [ ] Grafana dashboards imported
- [ ] Alert notifications configured
- [ ] OpenTelemetry traces enabled
- [ ] Log aggregation setup

### Security
- [ ] HTTPS/TLS configured
- [ ] SQL injection protection verified
- [ ] Authentication/authorization tested
- [ ] Sensitive data audit completed
- [ ] Security headers configured

## Performance & Scaling

- **[Performance Guide](../performance/index.md)** - Optimization strategies
- **[APQ Configuration](../performance/apq-optimization-guide.md)** - Automatic Persisted Queries
- **[Rust Pipeline](../performance/rust-pipeline-optimization.md)** - Rust acceleration setup

## Platform-Specific Guides

### Container Platforms
- **Docker**: See [Deployment Guide](deployment.md#docker-setup)
- **Kubernetes**: See [Deployment Guide](deployment.md#kubernetes-deployment)

### Cloud Providers
- **AWS**: ECS/Fargate + RDS PostgreSQL
- **GCP**: Cloud Run + Cloud SQL
- **Azure**: Container Instances + PostgreSQL Flexible Server

**Note**: Detailed Kubernetes manifests and cloud-specific configurations coming soon. For now, use Docker Compose template in [Deployment Guide](deployment.md).

## Quick Start - Production Deployment

```bash
# 1. Setup environment
cp .env.example .env.production
# Edit .env.production with production credentials

# 2. Run with Docker Compose
docker-compose -f docker-compose.prod.yml up -d

# 3. Verify health
curl http://localhost:8000/health

# 4. Import Grafana dashboards
# See monitoring.md for dashboard setup
```

## Support & Troubleshooting

- **[Troubleshooting Guide](../guides/troubleshooting.md)** - Common production issues
- **[Security Issues](../../SECURITY.md)** - Report security vulnerabilities
- **[GitHub Issues](../issues)** - Bug reports and feature requests
