# FraiseQL Test Matrix

This document describes the full test matrix, required infrastructure per feature flag,
and how to run each test category.

## Quick Reference

| Command | Infrastructure | Duration | Coverage |
|---------|---------------|----------|----------|
| `make test-unit` | None | ~30s | Unit tests (all crates, `--lib`) |
| `make test-integration` | Docker (db-up) | ~10 min | DB + observer + server integration |
| `make test-full` | Docker (db-up + federation-up) | ~30 min | Everything (8 steps) |
| `make test-all-ignored` | Docker (db-up) | ~15 min | All `#[ignore]` tests |
| `make test-federation` | Docker (federation-up) | ~5 min | Apollo Federation stack |
| `make test-parity` | uv, bun, go, mvn, php | ~10 min | Cross-SDK schema parity |

## Infrastructure Services

Started via `make db-up` (uses `docker/docker-compose.test.yml`):

| Service | Image | Port | Used By |
|---------|-------|------|---------|
| PostgreSQL | postgres:16-alpine | 5433 | Core DB tests, server, observers, federation (incl. `saga` integration tests) |
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

The comprehensive test target runs 8 steps in sequence, reporting a single pass/fail:

1. **Unit tests** -- `cargo test --lib --all-features` (no infrastructure)
2. **SQL snapshot tests** -- 34 snapshot tests for SQL generation correctness
3. **Database integration** -- PostgreSQL (4 threads)
4. **Redis tests** -- APQ storage, observer caching/queue/lease
5. **NATS + observer bridge** -- NATS transport, PostgreSQL+NATS bridge, PostgreSQL+Redis lease
6. **Vault secrets manager** -- HashiCorp Vault integration
7. **Server integration** -- Database queries, observer runtime, observer integration
8. **Federation** -- Apollo Router + subgraph pytest suite

## CI Workflows

### Dagger CI (`.dagger/main.go` — the leg definitions)

| Workflow / leg | What it tests |
|-----|---------------|
| `dagger-preflight.yml` | fmt (nightly rustfmt), ShellGates (incl. the no-orphan-suites and snapshot-pairing gates), rustdoc, clippy — on every push to every in-repo branch (required check) |
| `dagger-security.yml` | cargo-deny (licenses + advisories + bans + sources) + compliance gates (required check) |
| `dagger-test.yml` | Workspace test suite, stable + MSRV toolchains (push to dev + dispatch) |
| `dagger-integration.yml` | The integration shard matrix: postgres, server, redis, nats, observers, vault, tls, wire, storage, server-storage, federation, federation-compose, saml, quickstart, http-e2e |
| `dagger-feature-matrix.yml` | Feature-combination compilation and tests |

The legacy hosted `ci.yml` was retired in #951 — the Dagger legs had superseded
every job (its Clippy job had been unable to pass since the SAML feature landed,
because the hosted runner lacked xmlsec1). The one unique job (the
async-jobs-subgraph example clippy) moved into the preflight clippy leg.

### Feature Flags (`feature-flags.yml`) -- Runs on every PR

Tests server and database feature combinations to catch feature-flag-gated
compilation errors.

### Scheduled

| Workflow | Schedule | What |
|----------|----------|------|
| `fuzz.yml` | Weekly (Sun 3am UTC) | 6 fuzz targets, 1h each |
| `mutation.yml` | Manual | Mutation testing |
| `security.yml` | On push + weekly | Dependency vulnerability scan |
| `codeql.yml` | On push | SAST security analysis |

## Thread Concurrency Notes

- **PostgreSQL**: `--test-threads=4` (good connection pooling)
- **Redis**: `--test-threads=1` (shared state between tests)
- **NATS**: `--test-threads=1` (JetStream ordering)
