# FraiseQL Test Matrix

This document describes the full test matrix, required infrastructure per feature flag,
and how to run each test category.

## Quick Reference

| Command | Infrastructure | Duration | Coverage |
|---------|---------------|----------|----------|
| `make test-unit` | None | ~30s | Unit tests (all crates, `--lib`) |
| `make test-integration` | Docker (db-up) | ~10 min | DB + observer + server integration |
| `make test-full` | Docker (db-up + federation-up) | ~30 min | Everything (9 steps) |
| `make test-all-ignored` | Docker (db-up) | ~15 min | All `#[ignore]` tests |
| `make test-federation` | Docker (federation-up) | ~5 min | Apollo Federation stack |
| `make test-parity` | uv, bun, go, mvn, php | ~10 min | Cross-SDK schema parity |

## Infrastructure Services

Started via `make db-up` (uses `docker/docker-compose.test.yml`):

| Service | Image | Port | Used By |
|---------|-------|------|---------|
| PostgreSQL | postgres:16-alpine | 5433 | Core DB tests, server, observers, federation saga |
| MySQL | mysql:8.0 | 3307 | Core DB tests, cross-DB parity |
| SQL Server | mssql/server:2022 | 1434 | Core DB tests |
| Redis | redis:7-alpine | 6379 | APQ, caching, queue, rate limiting, PKCE |
| NATS | nats:2.10-alpine | 4222 | Observer transport, bridge |
| Vault | hashicorp/vault:1.17 | 8200 | Secrets manager integration |

Started via `make federation-up` (uses `docker/docker-compose.federation.yml`):

| Service | Purpose |
|---------|---------|
| Apollo Router | Federation gateway |
| 3 subgraph services | Test subgraphs for entity resolution |

## Feature Flags Requiring Infrastructure

### fraiseql-core

| Feature | Requires | Tests |
|---------|----------|-------|
| `test-postgres` | PostgreSQL | DB integration (`--ignored`) |
| `test-mysql` | MySQL | DB integration (`--ignored`) |
| `test-sqlserver` | SQL Server | DB integration (`--ignored`) |
| `redis-apq` | Redis | APQ storage (`--ignored`) |

### fraiseql-observers

| Feature | Requires | Tests |
|---------|----------|-------|
| `postgres` | PostgreSQL | Observer PostgreSQL transport |
| `nats` | NATS | NATS transport, bridge integration |
| `caching` | Redis | Cache-backed observers |
| `dedup` | Redis | Deduplication |
| `queue` | Redis | Job queue |
| `redis-lease` | Redis + PostgreSQL | Distributed lease |

### fraiseql-server

| Feature | Requires | Tests |
|---------|----------|-------|
| `observers-nats` | NATS + PostgreSQL | Observer runtime integration |
| (default) | PostgreSQL | Database query tests |
| (default) | Vault | Secrets manager integration |

## `make test-full` Steps

The comprehensive test target runs 9 steps in sequence, reporting a single pass/fail:

1. **Unit tests** -- `cargo test --lib --all-features` (no infrastructure)
2. **SQL snapshot tests** -- 34 snapshot tests for SQL generation correctness
3. **Database integration** -- PostgreSQL (4 threads), MySQL (1 thread), SQL Server (1 thread)
4. **Cross-database parity** -- PostgreSQL vs MySQL query result comparison
5. **Redis tests** -- APQ storage, observer caching/queue/lease
6. **NATS + observer bridge** -- NATS transport, PostgreSQL+NATS bridge, PostgreSQL+Redis lease
7. **Vault secrets manager** -- HashiCorp Vault integration
8. **Server integration** -- Database queries, observer runtime, observer integration
9. **Federation** -- Apollo Router + subgraph pytest suite

## CI Workflows

### Main CI (`ci.yml`) -- Runs on every PR

| Job | What it tests |
|-----|---------------|
| `fmt` | Code formatting (nightly rustfmt) |
| `clippy` | Lint checks |
| `test` | Unit tests (stable + MSRV 1.92, ubuntu) |
| `integration-postgres` | PostgreSQL integration (testcontainers) |
| `integration-mysql` | MySQL integration (testcontainers) |
| `integration-sqlserver` | SQL Server integration (testcontainers) |
| `integration-arrow` | Arrow Flight protocol |
| `integration-observers` | Observer system (Redis + NATS + PostgreSQL) |
| `integration-server` | Server integration (PostgreSQL + Redis + Vault) |

### Feature Flags (`feature-flags.yml`) -- Runs on every PR

Tests 11 server feature combinations and 4 database feature combinations to catch
feature-flag-gated compilation errors.

### Scheduled

| Workflow | Schedule | What |
|----------|----------|------|
| `fuzz.yml` | Weekly (Sun 3am UTC) | 6 fuzz targets, 1h each |
| `mutation.yml` | Manual | Mutation testing |
| `security.yml` | On push + weekly | Dependency vulnerability scan |
| `codeql.yml` | On push | SAST security analysis |

## Thread Concurrency Notes

- **PostgreSQL**: `--test-threads=4` (good connection pooling)
- **MySQL**: `--test-threads=1` (concurrent connections fail)
- **SQL Server**: `--test-threads=1` (contention issues)
- **Redis**: `--test-threads=1` (shared state between tests)
- **NATS**: `--test-threads=1` (JetStream ordering)
