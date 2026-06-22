# Production Security Checklist

A checklist for hardening FraiseQL deployments. Review each item before going live.

## Authentication

- [ ] Enable authentication (`[auth]` section in `fraiseql.toml`)
- [ ] Use OIDC/OAuth2 with PKCE for browser-based clients
- [ ] Configure API key authentication for service-to-service calls
- [ ] Set `__Host-` cookie prefix for session tokens (enabled by default)
- [ ] Rotate API keys on a regular schedule

## Authorization

- [ ] Enable Row-Level Security (RLS) in PostgreSQL for multi-tenant data isolation
- [ ] Use `requires_scope` on sensitive fields to enforce JWT scope checks
- [ ] Verify RLS is active when APQ caching is enabled (cache isolation depends on per-user WHERE clauses)

## Rate Limiting

- [ ] Enable rate limiting on auth endpoints (`[security.rate_limiting]`)
- [ ] Configure per-user limits appropriate for your traffic patterns
- [ ] Use Redis backend for rate limiting in multi-instance deployments

## Network

- [ ] Terminate TLS at a reverse proxy / load balancer — FraiseQL serves plaintext and **refuses to boot** if the `[tls]` section is set
- [ ] Configure trusted proxy headers for accurate client IP extraction
- [ ] Restrict admin endpoints to internal networks or VPN
- [ ] Set CORS origins explicitly (avoid `*` in production)

## Error Handling

- [ ] Enable error sanitization (enabled by default) -- prevents leaking SQL, file paths, or stack traces
- [ ] Review custom error messages for information disclosure
- [ ] Configure structured logging (`FRAISEQL_LOG_FORMAT=json`) for your log aggregator

## Secrets

- [ ] Store database credentials in environment variables, not config files
- [ ] Use HashiCorp Vault integration for secret management (`[secrets]`)
- [ ] Encrypt sensitive columns at the database/storage layer — field-level at-rest encryption is **not implemented** (the server refuses to boot if a field is marked for encryption)
- [ ] Ensure OTLP endpoint URLs do not contain embedded credentials

## Observability

- [ ] Enable Prometheus metrics (`[metrics]`) and configure alerting
- [ ] Capture auth events (login attempts, authorization denials) via `fraiseql-auth` — note: a generic compliance audit subsystem with file/PostgreSQL/syslog backends is **not** provided
- [ ] Configure OpenTelemetry tracing for distributed request tracing
- [ ] Monitor the Grafana dashboard (`GET /api/v1/admin/grafana-dashboard`)

## Schema

- [ ] Run `fraiseql-cli validate-documents` against your trusted document manifest
- [ ] Set `max_query_depth`, and configure per-tenant `cost_budget` / `@cost` weights (#379) to prevent abuse
- [ ] Review all mutation functions for proper input validation
- [ ] Ensure schema files are read-only in production (prevents tampering)

## Infrastructure

- [ ] Run multiple server instances behind a load balancer
- [ ] Configure connection pool sizes appropriate for your database
- [ ] Enable health check endpoint monitoring (`GET /health`)
- [ ] Set up automated credential rotation if using Vault
- [ ] Review SLA/SLO targets in `docs/sla.md`
