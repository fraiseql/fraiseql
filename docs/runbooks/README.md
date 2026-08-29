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
| [10 - Certificate Rotation](./10-certificate-rotation.md) | TLS cert renewal at the reverse proxy / load balancer | Standard |
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
curl http://localhost:8000/health || echo "Server unavailable"

# View recent logs
docker logs fraiseql-server | tail -50

# Check database connectivity
psql $DATABASE_URL -c "SELECT now(), version();"

# Monitor metrics (served on the main port; bearer token required when configured)
curl -H "Authorization: Bearer $FRAISEQL_METRICS_TOKEN" http://localhost:8000/metrics | grep fraiseql

# Check environment and configuration
env | grep -E "^(DATABASE_URL|FRAISEQL_|VAULT_|RUST_LOG)"

# Restart service
docker restart fraiseql-server

# View all running containers
docker ps | grep fraiseql
```

## Environment Variables

The server reads an **explicit** set of environment variables — there is no generic
`FRAISEQL_*` mapping onto config keys. The operator-relevant ones (the authoritative list
is `fraiseql-server --help`):

| Variable | Purpose | Example |
|----------|---------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://user:pass@host:5432/db` |
| `FRAISEQL_BIND_ADDR` | HTTP bind address (default `127.0.0.1:8000`) | `0.0.0.0:8000` |
| `FRAISEQL_SCHEMA_PATH` | Path to compiled schema | `/etc/fraiseql/schema.compiled.json` |
| `FRAISEQL_CONFIG` | Path to the server TOML config | `/etc/fraiseql/server.toml` |
| `FRAISEQL_ENV` | Deployment posture (`production` fail-closes CORS etc.) | `production` |
| `FRAISEQL_METRICS_ENABLED` / `FRAISEQL_METRICS_TOKEN` | Metrics endpoint + its bearer token | `true` / `<token>` |
| `FRAISEQL_ADMIN_API_ENABLED` / `FRAISEQL_ADMIN_TOKEN` | Admin API + its bearer token | `true` / `<token>` |
| `FRAISEQL_RATE_LIMITING_ENABLED`, `FRAISEQL_RATE_LIMIT_RPS_PER_IP`, `FRAISEQL_RATE_LIMIT_RPS_PER_USER`, `FRAISEQL_RATE_LIMIT_BURST_SIZE`, `FRAISEQL_RATE_LIMIT_MAX_BUCKETS` | Per-field rate-limit overrides (win over file and compiled schema). `MAX_BUCKETS` is a memory ceiling per tracking map, not a throttle | `true` / `100` / `100000` |
| `FRAISEQL_LOG_FORMAT` | `json` or `pretty` log output | `json` |
| `RUST_LOG` | Log filter | `debug`, `info`, `warn`, `error` |
| `FRAISEQL_SECRETS_BACKEND` | Secrets backend selection (`env`, `file`, `vault`) | `vault` |
| `VAULT_ADDR`, `VAULT_TOKEN`, `VAULT_ROLE_ID`, `VAULT_NAMESPACE`, `VAULT_TLS_VERIFY` | Vault address + authentication | `s.xxxxx` |
| `FRAISEQL_REQUIRE_REDIS` | Refuse to boot if the Redis PKCE store is unavailable | `1` |

Every other knob (pool sizing, cache toggle, timeouts, …) lives in the `--config` TOML
file: **edit the file and restart** — exporting an invented `FRAISEQL_*` variable does
nothing, and CI now rejects runbooks that name a variable no code reads (#838).

`/metrics` requires `Authorization: Bearer $FRAISEQL_METRICS_TOKEN` when a metrics token
is configured (it should be, in production). The admin API lives under `/api/v1/admin/…`
and requires `Authorization: Bearer $FRAISEQL_ADMIN_TOKEN`.

## Health Checks

All runbooks assume FraiseQL server is running on `localhost:8000` (default). Adjust hostname/port as needed.

### Basic Health Check

```bash
curl -v http://localhost:8000/health
```

Expected response: `200 OK` with JSON containing health status.

### Detailed Metrics Check

```bash
curl http://localhost:8000/metrics
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

- [Architecture Overview](../architecture/overview.md) - System design and principles
- [Config vs Settings](../architecture/config-vs-settings.md) - Configuration model and the real env-var override list
- [Troubleshooting Guide](../operations/troubleshooting.md) - Common issues and solutions
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
