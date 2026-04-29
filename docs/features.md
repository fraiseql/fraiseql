# Feature Flags

FraiseQL uses Cargo feature flags to keep compile times low — you only pay for what you use.
This document answers two questions: **which features do I need?** and **which combinations are valid?**

## Quick-Start Decision Guide

| Use Case | Add these features |
|----------|--------------------|
| Local dev + PostgreSQL only | `default` (no changes) |
| Production with OIDC auth | `default` (auth is on by default) |
| Distributed rate limiting | `redis-rate-limiting` |
| Redis-backed persisted queries | `redis-apq` |
| Apollo Federation v2 | `federation` |
| Apache Arrow / analytics | `arrow` |
| Kafka subscription transport | `kafka` |
| HashiCorp Vault / AWS Secrets | `secrets` (on by default) |
| MySQL backend | `mysql` |
| SQLite (local dev/testing) | `sqlite` |
| SQL Server | `sqlserver` |
| Full observability (OTel) | `tracing-opentelemetry` |
| Full enterprise stack | `default,federation,redis-apq,redis-rate-limiting,arrow,kafka` |

To opt out of all defaults (minimal build): `--no-default-features --features postgres`.

## Feature Compatibility Matrix

FraiseQL server exposes optional capabilities via Cargo feature flags.
This document describes each feature, its dependencies, tested combinations, and known
invalid combinations.

## Feature Groups

Features are grouped by subsystem. Within a group, features are additive unless noted.
Across groups, all combinations are supported unless listed under **Invalid Combinations**.

### Default features

The default feature set for new projects:

| Feature | Description |
|---------|-------------|
| `auth` | OIDC/JWT authentication (required for `redis-pkce`) |
| `secrets` | Secrets management (HashiCorp Vault, environment, AWS SSM) |
| `webhooks` | Outbound webhook dispatch |

To opt out of all defaults: `--no-default-features`.

### Authentication and Session

| Feature | Depends on | Description |
|---------|-----------|-------------|
| `auth` | — | OIDC/JWT middleware and token validation |
| `redis-pkce` | `auth` | Redis-backed PKCE state storage for OAuth flows |

### Caching / APQ

| Feature | Depends on | Description |
|---------|-----------|-------------|
| `redis-apq` | — | Redis-backed Automatic Persisted Queries |

### Rate Limiting

| Feature | Depends on | Description |
|---------|-----------|-------------|
| *(default)* | — | In-memory token-bucket rate limiter (always available) |
| `redis-rate-limiting` | — | Redis-backed distributed rate limiter |

### Observers (Reactive Business Logic)

| Feature | Depends on | Description |
|---------|-----------|-------------|
| `observers` | — | Core event-driven runtime for reactive field observers |
| `observers-nats` | `observers` | NATS transport for cross-instance observer events |
| `observers-enterprise` | `observers` | HA coordinator, DLQ, lease management for multi-instance deployments |

> **Note**: `observers-enterprise` is not a paid or commercial tier.
> The name reflects the features needed for multi-instance high-availability deployments.
> Both `observers-nats` and `observers-enterprise` automatically enable `observers` as a base.

### Analytics / Export

| Feature | Depends on | Description |
|---------|-----------|-------------|
| `arrow` | — | Apache Arrow Flight gRPC server for analytics workloads |
| `wire-backend` | — | PostgreSQL wire protocol adapter (streaming JSON) |
| `aws-s3` | — | AWS S3 storage backend |

### Observability

| Feature | Depends on | Description |
|---------|-----------|-------------|
| `metrics` | — | Prometheus metrics endpoint (`/metrics`) |
| `tracing-opentelemetry` | — | OTLP trace export via OpenTelemetry |
| `mcp` | — | Model Context Protocol server (stdio or HTTP) |

### Other

| Feature | Description |
|---------|-------------|
| `cors` | CORS header injection (no external deps) |
| `testing` | Test helper utilities (do not enable in production builds) |

## Tested Combinations (CI-verified)

| Combination | CI Job / step |
|-------------|--------------|
| `--all-features` | `clippy`, `build`, `test`, `doc` steps |
| `postgres,mysql,sqlite,sqlserver,rich-filters` | `build` and `test` steps (workspace) |
| `observers-nats` | `ci-observers` — observer runtime integration |
| `observers/caching,queue,redis-lease` | `ci-observers` — Redis-backed observer |
| `observers/postgres,nats` | `ci-observers` — bridge integration |
| `redis-apq` | `ci-redis` — APQ cache integration |
| `test-postgres,test-mysql` | `ci-multi-db` — cross-database integration |
| `test-mysql` | `ci-mysql` |
| `sqlite` | `ci-sqlite` |
| `test-sqlserver` | `ci-sqlserver` |
| TLS integration | `ci-tls` — `fraiseql-wire` TLS tests |
| Secrets/Vault | `ci-vault` — `fraiseql-server` secrets integration |

## Invalid Combinations

| Combination | Why |
|-------------|-----|
| `observers-nats` without `observers` | Missing base runtime — compile error (Cargo.toml enforces dependency) |
| `observers-enterprise` without `observers` | Missing base runtime — compile error (Cargo.toml enforces dependency) |
| `redis-pkce` without `auth` | PKCE has no purpose without the auth middleware — Cargo.toml enforces dependency |
| `testing` in production | Enables test helpers that skip security checks |

## Minimum Feature Sets by Use Case

| Use Case | Recommended features |
|----------|---------------------|
| Local development | `--no-default-features` or `--no-default-features --features postgres` |
| Development with auth | `auth` (default) |
| Production (single-instance) | `auth,secrets,metrics` (default + `metrics`) |
| Production (multi-instance, HA) | `auth,secrets,metrics,observers,observers-enterprise` |
| Analytics workload | `arrow,metrics` |
| Wire protocol clients | `wire-backend` |
| Full feature build (CI) | `--all-features` |

## Adding a New Feature

1. Add the feature to `fraiseql-server/Cargo.toml` under `[features]`
2. If it depends on another feature, declare the dependency chain in the same `[features]` entry
3. Add a CI job or step that builds/tests with the new feature enabled
4. Update this file: add a row to the appropriate group table and the tested combinations table
5. Update `docs/architecture/overview.md` if the feature changes runtime behaviour
