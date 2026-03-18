# FraiseQL Operational Runbooks

This directory contains operational runbooks for managing, troubleshooting, and maintaining FraiseQL in production. Each runbook follows a standard format with symptoms, investigation steps, mitigation, resolution, and prevention guidance.

## Quick Reference

| Runbook | Trigger | Severity |
|---------|---------|----------|
| [01 - Deployment](./01-deployment.md) | New deployment, rollback, verification | Standard |
| [02 - Database Failure](./02-database-failure.md) | PostgreSQL down or degraded | Critical |
| [03 - High Latency](./03-high-latency.md) | Response times > SLA | High |
| [04 - Memory Pressure](./04-memory-pressure.md) | OOM errors, memory > 85% | High |
| [05 - Authentication Issues](./05-authentication-issues.md) | Auth failures, JWT/OIDC errors | High |
| [06 - Rate Limiting Triggered](./06-rate-limiting-triggered.md) | Rate limits blocking requests | Medium |
| [07 - Connection Pool Exhaustion](./07-connection-pool-exhaustion.md) | DB connection pool full | High |
| [08 - Vault Unavailable](./08-vault-unavailable.md) | Secrets backend down | Critical |
| [09 - Redis Failure](./09-redis-failure.md) | Redis unavailable (cache/rate limiting) | Medium |
| [10 - Certificate Rotation](./10-certificate-rotation.md) | TLS cert expiry or renewal | Standard |
| [11 - Schema Migration](./11-schema-migration.md) | Update compiled schema | Standard |
| [12 - Incident Response](./12-incident-response.md) | General incident template | Variable |
| [13 - Schema Hot-Reload Failure](./13-schema-hot-reload-failure.md) | Schema reload cycle failing | Medium |
| [14 - Federation Circuit Breaker](./14-federation-circuit-breaker.md) | Circuit breaker tripped on federation entity | High |
| [15 - Tracing / OTLP](./15-tracing-otlp.md) | No traces, OTLP export failures | Medium |

## Using These Runbooks

### For On-Call Engineers

1. **Identify the issue** - Match the symptoms to a runbook
2. **Follow Investigation** - Execute diagnostic commands in order
3. **Apply Mitigation** - Immediate actions to stabilize service
4. **Execute Resolution** - Address root cause
5. **Document** - Record findings in incident ticket
6. **Escalate if needed** - Contact appropriate team

### Common Diagnostic Commands

```bash
# Check server status and health
curl http://localhost:8815/health || echo "Server unavailable"

# View recent logs
docker logs fraiseql-server | tail -50

# Check database connectivity
psql $DATABASE_URL -c "SELECT now(), version();"

# Monitor metrics
curl http://localhost:9090/metrics | grep fraiseql

# Check environment and configuration
env | grep -E "^(DB_|REDIS_|VAULT_|RUST_LOG)"

# Restart service
docker restart fraiseql-server

# View all running containers
docker ps | grep fraiseql
```

## Environment Variables

FraiseQL respects these standard configuration environment variables:

| Variable | Purpose | Example |
|----------|---------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://user:pass@host:5432/db` |
| `DB_PASSWORD` | Database password (alternative) | `securepassword` |
| `REDIS_URL` | Redis connection (optional) | `redis://localhost:6379` |
| `VAULT_ADDR` | HashiCorp Vault address | `https://vault.example.com:8200` |
| `VAULT_TOKEN` | Vault authentication token | `s.xxxxx` |
| `PORT` | HTTP server port | `8815` |
| `PROMETHEUS_PORT` | Metrics port | `9090` |
| `RUST_LOG` | Log level | `debug`, `info`, `warn`, `error` |
| `SCHEMA_PATH` | Path to compiled schema | `/etc/fraiseql/schema.compiled.json` |

## Health Checks

All runbooks assume FraiseQL server is running on `localhost:8815` (default). Adjust hostname/port as needed.

### Basic Health Check

```bash
curl -v http://localhost:8815/health
```

Expected response: `200 OK` with JSON containing health status.

### Detailed Metrics Check

```bash
curl http://localhost:8815/metrics
```

Returns Prometheus metrics including:

- Request rate, latency percentiles
- Database pool connections (active/idle)
- Authentication failures
- Rate limit triggers
- Cache hit/miss rates

## Escalation Contacts

Default escalation path:

1. **On-call engineer** - Initial response (you)
2. **Database team** - Database-specific issues (runbooks 02, 07)
3. **Security team** - Auth and Vault issues (runbooks 05, 08)
4. **Infrastructure team** - Deployment and networking issues
5. **Incident commander** - Major incidents affecting production

See individual runbooks for specific escalation contacts.

## Related Documentation

- [ARCHITECTURE_PRINCIPLES.md](../ARCHITECTURE_PRINCIPLES.md) - System design and principles
- [Deployment Guide](../deployment.md) - Standard deployment procedures
- [Configuration Reference](../configuration.md) - All FraiseQL configuration options
- [Troubleshooting Guide](../troubleshooting.md) - Common issues and solutions
- [Performance Tuning](../performance.md) - Optimization guidelines

## Contributing to Runbooks

When adding new runbooks:

1. Follow the standard format (Symptoms, Impact, Investigation, Mitigation, Resolution, Prevention, Escalation)
2. Include concrete commands, not just descriptions
3. Add environment-specific notes where needed
4. Link to related runbooks and documentation
5. Test commands in a staging environment first
6. Include timeouts and expected durations
7. Document any prerequisites or prerequisites

## Changelog

- **v2.2.0** (2026-03-18) - Added tracing/OTLP runbook
  - Runbook 15: OTLP export troubleshooting
- **v2.1.1** (2026-03-17) - Added federation circuit breaker runbook
  - Runbook 14: federation circuit breaker recovery and tuning
- **v2.1.0** (2026-03-16) - Added schema hot-reload failure runbook
  - Runbook 13: schema hot-reload failure diagnosis and recovery
- **v2.0.0** (2026-02-19) - Initial runbook suite for FraiseQL v2
  - 12 core operational runbooks covering critical scenarios
  - Standard diagnostic procedures
  - Incident response template
