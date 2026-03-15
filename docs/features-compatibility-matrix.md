# Feature Compatibility Matrix

This document lists all Cargo feature flags across the FraiseQL workspace, their database backend support, inter-feature dependencies, and known constraints.

## Crate Dependency Graph

```
fraiseql (umbrella)
  -> fraiseql-core -> fraiseql-db -> fraiseql-error
                   -> fraiseql-federation -> fraiseql-db, fraiseql-error
  -> fraiseql-server -> fraiseql-core, fraiseql-error
                     -> fraiseql-auth (optional)
                     -> fraiseql-observers (optional)
                     -> fraiseql-secrets (optional)
                     -> fraiseql-webhooks (optional)
                     -> fraiseql-arrow (optional)
  -> fraiseql-cli -> fraiseql-core, fraiseql-server (optional)
  -> fraiseql-wire (standalone, PostgreSQL only)
```

## Feature Flags by Crate

### fraiseql (umbrella crate)

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `postgres` | yes | PostgreSQL backend | `fraiseql-core/postgres` |
| `mysql` | no | MySQL backend | `fraiseql-core/mysql` |
| `sqlite` | no | SQLite backend | `fraiseql-core/sqlite` |
| `sqlserver` | no | SQL Server backend | `fraiseql-core/sqlserver` |
| `server` | no | HTTP server | `dep:fraiseql-server` |
| `cli` | no | CLI binary | `dep:fraiseql-cli` |
| `observers` | no | Observer system | `dep:fraiseql-observers`, `fraiseql-server?/observers` |
| `arrow` | no | Arrow Flight | `dep:fraiseql-arrow`, `fraiseql-core/arrow`, `server` |
| `wire` | no | Wire protocol streaming | `dep:fraiseql-wire`, `fraiseql-core/wire-backend` |
| `full` | no | All optional components | `server`, `observers`, `arrow`, `wire`, `postgres`, `mysql`, `sqlite`, `cli` |
| `minimal` | no | Core only, no backends | (empty) |

### fraiseql-core

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `postgres` | yes | PostgreSQL backend | `fraiseql-db/postgres` |
| `mysql` | no | MySQL backend | `fraiseql-db/mysql` |
| `sqlite` | no | SQLite backend | `fraiseql-db/sqlite` |
| `sqlserver` | no | SQL Server backend | `fraiseql-db/sqlserver` |
| `rich-filters` | yes | Rich scalar type filters (44 types) | `fraiseql-db/rich-filters` |
| `arrow` | no | Arrow RecordBatch conversion | `dep:arrow`, `dep:arrow-array` |
| `kafka` | no | Kafka integration | `dep:rdkafka` |
| `redis-apq` | no | Redis-backed APQ cache | `dep:redis` |
| `schema-lint` | no | Anti-pattern detection, cost analysis | (empty) |
| `wire-backend` | no | Wire protocol backend | `fraiseql-db/wire-backend` |
| `test-utils` | no | Expose ManualClock for dependents | (empty) |
| `test-postgres` | no | PostgreSQL integration tests | `fraiseql-db/test-postgres` |
| `test-mysql` | no | MySQL integration tests | `mysql`, `fraiseql-db/test-mysql` |
| `test-sqlserver` | no | SQL Server integration tests | `sqlserver`, `fraiseql-db/test-sqlserver` |

### fraiseql-server

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `auth` | yes | Authentication middleware | `dep:fraiseql-auth` |
| `observers` | no | Observer system | `dep:fraiseql-observers` |
| `observers-nats` | no | NATS-backed observers | `observers`, `fraiseql-observers/nats` |
| `observers-enterprise` | no | HA observer features | `observers`, `fraiseql-observers/enterprise`, `fraiseql-observers/nats` |
| `arrow` | no | Arrow Flight server | `dep:fraiseql-arrow`, `dep:tonic` |
| `mcp` | no | Model Context Protocol server | `dep:rmcp`, `dep:schemars` |
| `metrics` | no | Prometheus metrics | `dep:metrics`, `dep:metrics-exporter-prometheus` |
| `tracing-opentelemetry` | no | OpenTelemetry tracing | `dep:opentelemetry`, `dep:opentelemetry_sdk`, `dep:opentelemetry-otlp`, `dep:tracing-opentelemetry` |
| `redis-apq` | no | Redis APQ (pass-through) | `fraiseql-core/redis-apq` |
| `redis-pkce` | no | Redis-backed PKCE state | `auth`, `fraiseql-auth/redis-pkce` |
| `redis-rate-limiting` | no | Redis-backed rate limiting | `dep:redis` |
| `secrets` | no | Secrets management | `dep:fraiseql-secrets` |
| `webhooks` | no | Webhook processing | `dep:fraiseql-webhooks` |
| `wire-backend` | no | Wire protocol backend | `fraiseql-core/wire-backend` |
| `aws-s3` | no | S3 storage | `dep:aws-sdk-s3`, `dep:aws-config` |
| `cors` | no | CORS support | (empty) |
| `database` | no | Database utilities | (empty) |
| `testing` | no | Test helpers | (empty) |

### fraiseql-db

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `postgres` | yes | PostgreSQL adapter | (empty) |
| `mysql` | no | MySQL adapter | `sqlx/mysql` |
| `sqlite` | no | SQLite adapter | `sqlx/sqlite` |
| `sqlserver` | no | SQL Server adapter | `dep:tiberius`, `dep:bb8`, `dep:bb8-tiberius` |
| `rich-filters` | no | Rich scalar type filters | (empty) |
| `wire-backend` | no | Wire protocol backend | `dep:fraiseql-wire` |
| `test-postgres` | no | PostgreSQL integration tests | (empty) |
| `test-mysql` | no | MySQL integration tests | `mysql` |
| `test-sqlserver` | no | SQL Server integration tests | `sqlserver` |

### fraiseql-error

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `axum-compat` | no | `IntoResponse` impl for errors | `dep:axum` |

### fraiseql-auth

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `redis-pkce` | no | Redis-backed PKCE state storage | `dep:redis` |
| `redis-rate-limiting` | no | Redis-backed rate limiting | `dep:redis` |

### fraiseql-observers

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `postgres` | yes | PostgreSQL backend | `sqlx/postgres` |
| `mysql` | no | MySQL backend | `sqlx/mysql` |
| `sqlite` | no | SQLite backend | `sqlx/sqlite` |
| `mssql` | no | SQL Server backend | `dep:tiberius`, `dep:tokio-util`, `dep:bb8`, `dep:bb8-tiberius` |
| `nats` | no | NATS messaging | `dep:async-nats` |
| `arrow` | no | Arrow integration | `dep:fraiseql-arrow`, `dep:arrow` |
| `analytics` | no | Analytics via Arrow | `arrow` |
| `caching` | no | Redis caching | `dep:redis` |
| `dedup` | no | Redis deduplication | `dep:redis` |
| `queue` | no | Redis queue | `dep:redis` |
| `redis-lease` | no | Redis lease management | `dep:redis` |
| `search` | no | Search capability | (empty) |
| `checkpoint` | no | Checkpoint support | (empty) |
| `metrics` | no | Prometheus metrics | `dep:prometheus` |
| `cli` | no | CLI interface | `dep:clap`, `dep:colored`, `dep:tabwriter` |
| `enterprise` | no | HA feature bundle | `checkpoint`, `dedup`, `caching`, `queue`, `search`, `metrics` |
| `multi-db` | no | PostgreSQL + MySQL | `postgres`, `mysql` |
| `all-db` | no | All SQL backends | `postgres`, `mysql`, `mssql` |
| `testing` | no | Test helpers | (empty) |

### fraiseql-arrow

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `clickhouse` | no | ClickHouse client | `dep:clickhouse` |
| `wire-backend` | no | Wire protocol support | (empty) |
| `testing` | no | Test helpers | (empty) |

### fraiseql-cli

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `run-server` | no | Embedded server for `serve` command | `dep:fraiseql-server` |

### fraiseql-wire

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `bench-with-postgres` | no | PostgreSQL benchmarks | (empty) |
| `bench-with-tokio-postgres` | no | tokio-postgres benchmarks | (empty) |

### fraiseql-federation

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `test-utils` | no | Test helper constructors | (empty) |
| `unstable` | no | Pre-release APIs | (empty) |

### fraiseql-webhooks

| Feature | Default | Description | Activates |
|---------|---------|-------------|-----------|
| `testing` | no | Test helpers | (empty) |

### fraiseql-secrets

No feature flags defined.

### fraiseql-test-utils

No feature flags defined. Internal crate (`publish = false`).

## Database Backend Support by Crate

| Crate | PostgreSQL | MySQL | SQLite | SQL Server |
|-------|:----------:|:-----:|:------:|:----------:|
| fraiseql-db | yes (default) | opt-in | opt-in | opt-in |
| fraiseql-core | yes (default) | opt-in | opt-in | opt-in |
| fraiseql (umbrella) | yes (default) | opt-in | opt-in | opt-in |
| fraiseql-observers | yes (default) | opt-in | opt-in | opt-in (as `mssql`) |
| fraiseql-server | PostgreSQL always compiled in via `fraiseql-core` | -- | -- | -- |
| fraiseql-wire | PostgreSQL only | -- | -- | -- |
| fraiseql-arrow | via `fraiseql-core` | via `fraiseql-core` | via `fraiseql-core` | via `fraiseql-core` |
| fraiseql-auth | PostgreSQL only (hardcoded sqlx) | -- | -- | -- |
| fraiseql-webhooks | PostgreSQL only (hardcoded sqlx) | -- | -- | -- |

## Feature Dependencies and Implied Activations

| When you enable... | You also get... |
|---|---|
| `fraiseql/arrow` | `fraiseql/server`, `fraiseql-core/arrow` |
| `fraiseql/wire` | `fraiseql-core/wire-backend` |
| `fraiseql/full` | `server`, `observers`, `arrow`, `wire`, `postgres`, `mysql`, `sqlite`, `cli` |
| `fraiseql-server/observers-enterprise` | `observers`, `fraiseql-observers/enterprise`, `fraiseql-observers/nats` |
| `fraiseql-server/observers-nats` | `observers`, `fraiseql-observers/nats` |
| `fraiseql-server/redis-pkce` | `auth`, `fraiseql-auth/redis-pkce` |
| `fraiseql-observers/enterprise` | `checkpoint`, `dedup`, `caching`, `queue`, `search`, `metrics` |
| `fraiseql-observers/all-db` | `postgres`, `mysql`, `mssql` |
| `fraiseql-core/test-mysql` | `mysql`, `fraiseql-db/test-mysql` |
| `fraiseql-core/test-sqlserver` | `sqlserver`, `fraiseql-db/test-sqlserver` |
| `fraiseql-db/sqlserver` | `dep:tiberius`, `dep:bb8`, `dep:bb8-tiberius` |
| `fraiseql-db/mysql` | `sqlx/mysql` |
| `fraiseql-db/sqlite` | `sqlx/sqlite` |

## Known Constraints and Incompatibilities

1. **`fraiseql-server` with `--no-default-features` does not compile.** Some code paths assume the `auth` feature is enabled. Tracked as a planned improvement.

2. **`fraiseql-server` always includes PostgreSQL.** The dependency on `fraiseql-core` is declared with `features = ["postgres", "schema-lint"]`. There is no way to build `fraiseql-server` without PostgreSQL support.

3. **`fraiseql-auth` and `fraiseql-webhooks` are PostgreSQL-only.** Both hardcode `sqlx` with the `postgres` feature. They cannot be used with MySQL, SQLite, or SQL Server databases.

4. **`fraiseql-wire` is PostgreSQL-only.** It implements the PostgreSQL wire protocol and has no support for other databases.

5. **`fraiseql-observers` uses `mssql` for SQL Server** (not `sqlserver`). This differs from the naming convention used in `fraiseql-db` and `fraiseql-core`, which use `sqlserver`.

6. **`fraiseql/full` does not include `sqlserver`.** The `full` feature bundle includes `postgres`, `mysql`, and `sqlite` but omits `sqlserver`.

7. **`fraiseql-observers/all-db` does not include `sqlite`.** It bundles `postgres`, `mysql`, and `mssql` only.

8. **Redis features are independent per subsystem.** `redis-apq` (APQ cache), `redis-pkce` (PKCE state), `redis-rate-limiting` (rate limiting), and observer `caching`/`dedup`/`queue` each bring their own Redis dependency. They do not share a single feature gate.

9. **`arrow` in `fraiseql` (umbrella) implies `server`.** You cannot use Arrow Flight without the HTTP server.

10. **`fraiseql-cli/run-server`** pulls in the full `fraiseql-server` crate. Without it, the CLI only provides compilation and linting tools.
