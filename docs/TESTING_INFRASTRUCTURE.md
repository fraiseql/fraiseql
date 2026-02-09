# FraiseQL Testing Infrastructure Guide

Comprehensive guide for running all tests in the FraiseQL v2 workspace, including the ~857 currently ignored tests across 41 files.

## Test Categories Overview

| Category | Count | % | Status | Action Required |
|----------|------:|--:|--------|-----------------|
| Incomplete/Placeholder | ~550 | 64% | Stubs awaiting implementation | Feature development |
| External Service Dependencies | ~165 | 19% | Implemented, need services | Docker + CI setup |
| Federation (Docker Compose) | ~61 | 7% | Implemented, need full stack | Docker Compose |
| Stress/Performance | ~41 | 5% | Intentionally excluded from CI | Nightly CI only |
| Server-Required E2E | ~47 | 5% | Need running FraiseQL server | E2E CI workflow |
| **Total** | **~857** | **100%** | | |

## Test Inventory by Crate

### fraiseql-server (744 tests, 86.8%)

#### Encryption Module (~480 tests)

Test stubs for field-level encryption features under development.

| File | Count | Reason |
|------|------:|--------|
| `src/encryption/query_builder_integration_tests.rs` | 62 | Requires query builder integration |
| `src/encryption/field_encryption_tests.rs` | 43 | Implemented in mod.rs basic tests |
| `src/encryption/mapper_integration_tests.rs` | 40 | Requires mapper integration |
| `src/encryption/rotation_tests.rs` | 39 | Requires rotation implementation |
| `src/encryption/database_adapter_tests.rs` | 39 | Requires database setup |
| `src/encryption/refresh_tests.rs` | 38 | Requires refresh implementation |
| `src/encryption/transaction_integration_tests.rs` | 37 | Requires transaction integration |
| `src/encryption/schema_detection_tests.rs` | 35 | Requires schema detection implementation |
| `src/encryption/rotation_api_tests.rs` | 34 | Requires API implementation |
| `src/encryption/audit_logging_tests.rs` | 33 | Requires audit logging integration |
| `src/encryption/error_recovery_tests.rs` | 32 | Requires error recovery implementation |
| `src/encryption/compliance_tests.rs` | 32 | Requires compliance implementation |
| `src/encryption/performance_tests.rs` | 31 | Requires batching implementation |
| `src/encryption/dashboard_tests.rs` | 30 | Requires dashboard implementation |
| `src/encryption/database_adapter.rs` | 5 | Inline tests, requires SecretsManager |

#### Auth Module (46 tests)

| File | Count | Reason |
|------|------:|--------|
| `src/auth/oauth_tests.rs` | 46 | Requires OAuth implementation |

#### Secrets Module (38 tests)

| File | Count | Reason |
|------|------:|--------|
| `src/secrets/schema_tests.rs` | 38 | Requires database implementation |

#### Secrets Manager Module (50 tests)

| File | Count | Reason |
|------|------:|--------|
| `src/secrets_manager/backends/vault_advanced_tests.rs` | 28 | Requires Vault running |
| `src/secrets_manager/backends/vault_integration_tests.rs` | 22 | Requires Vault + DB role |

#### Integration Test Files (80 tests)

| File | Count | Dependency |
|------|------:|------------|
| `tests/graphql_features_e2e_test.rs` | 32 | FraiseQL server |
| `tests/observer_runtime_integration_test.rs` | 14 | PostgreSQL |
| `tests/http_server_e2e_test.rs` | 14 | FraiseQL server (`FRAISEQL_TEST_URL`) |
| `tests/database_query_test.rs` | 11 | PostgreSQL (`DATABASE_URL`) |
| `tests/observer_e2e_test.rs` | 8 | PostgreSQL |
| `tests/concurrent_load_test.rs` | 1 | FraiseQL server on localhost:8000 |

### fraiseql-core (72 tests, 8.4%)

| File | Count | Dependency |
|------|------:|------------|
| `tests/federation_docker_compose_integration.rs` | 61 | Docker Compose federation stack |
| `tests/federation_saga_stress_test.rs` | 6 | Stress test |
| `tests/federation_observability_perf.rs` | 3 | Flaky under parallel execution |
| `tests/federation_saga_performance_test.rs` | 1 | Flaky under parallel execution |
| `src/federation/saga_store.rs` | 1 | PostgreSQL |

### fraiseql-observers (35 tests, 4.1%)

| File | Count | Dependency |
|------|------:|------------|
| `tests/nats_integration.rs` | 8 | NATS server on localhost:4222 |
| `tests/bridge_integration.rs` | 7 | PostgreSQL + NATS |
| `src/queue/redis.rs` | 6 | Redis on localhost:6379 |
| `tests/stress_tests.rs` | 5 | Stress test |
| `src/transport/postgres_notify.rs` | 5 | PostgreSQL (`TEST_DATABASE_URL`) |
| `tests/integration_test.rs` | 3 | Redis (`REDIS_URL`) |
| `src/storage.rs` | 1 | PostgreSQL |

### fraiseql-wire (6 tests, 0.7%)

| File | Count | Dependency |
|------|------:|------------|
| `tests/tls_integration.rs` | 5 | TLS PostgreSQL (`TLS_TEST_DB_URL`) |
| `src/connection/tls.rs` | 1 | PEM file on filesystem |

## Local Development Setup

### Quick Start: All Services

```bash
# Start all test infrastructure
docker compose -f docker-compose.test-all.yml up -d

# Wait for services to be healthy
docker compose -f docker-compose.test-all.yml ps

# Run standard tests (non-ignored)
cargo test --all-features

# Run infrastructure-dependent tests
REDIS_URL=redis://localhost:6379 \
DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
TLS_TEST_DB_URL="postgresql://fraiseql:fraiseql_test@localhost:5444/fraiseql_tls_test?sslmode=require" \
  cargo test --all-features -- --ignored --test-threads=1

# Tear down
docker compose -f docker-compose.test-all.yml down -v
```

### Individual Service Setup

#### Redis (9 tests)

```bash
docker run -d --name fraiseql-redis -p 6379:6379 redis:7-alpine

# Run Redis tests
REDIS_URL=redis://localhost:6379 \
  cargo test -p fraiseql-observers --features "dedup,caching,queue" -- --ignored redis

# Cleanup
docker rm -f fraiseql-redis
```

**Tests enabled:**
- `crates/fraiseql-observers/src/queue/redis.rs` (6 inline tests)
- `crates/fraiseql-observers/tests/integration_test.rs` (3 tests)

#### NATS with JetStream (8 tests)

```bash
docker run -d --name fraiseql-nats -p 4222:4222 -p 8222:8222 nats:2.10-alpine -js

# Run NATS tests
cargo test -p fraiseql-observers --features "nats,testing" --test nats_integration -- --ignored

# Cleanup
docker rm -f fraiseql-nats
```

**Tests enabled:**
- `crates/fraiseql-observers/tests/nats_integration.rs` (8 tests)

#### PostgreSQL (observer/bridge tests, 20 tests)

```bash
docker run -d --name fraiseql-postgres \
  -p 5433:5432 \
  -e POSTGRES_USER=fraiseql_test \
  -e POSTGRES_PASSWORD=fraiseql_test_password \
  -e POSTGRES_DB=test_fraiseql \
  postgres:16-alpine

# Initialize schema
psql -h localhost -p 5433 -U fraiseql_test -d test_fraiseql -f tests/sql/postgres/init.sql
psql -h localhost -p 5433 -U fraiseql_test -d test_fraiseql -f tests/sql/postgres/init-analytics.sql

# Run PostgreSQL-dependent tests
DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
  cargo test --all-features -- --ignored --test-threads=1

# Cleanup
docker rm -f fraiseql-postgres
```

#### TLS PostgreSQL (5 tests)

```bash
cd docker/tls-postgres
./generate-certs.sh
docker compose up -d

# Run TLS tests
TLS_TEST_DB_URL="postgresql://fraiseql:fraiseql_test@localhost:5433/fraiseql_tls_test?sslmode=require" \
  cargo test -p fraiseql-wire --test tls_integration -- --ignored

# Cleanup
docker compose down -v
```

**Tests enabled:**
- `crates/fraiseql-wire/tests/tls_integration.rs` (5 tests)

#### HashiCorp Vault (22+ tests)

```bash
docker run -d --name fraiseql-vault \
  -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=root \
  -e VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200 \
  --cap-add IPC_LOCK \
  hashicorp/vault:1.15

# Run Vault tests
VAULT_ADDR=http://localhost:8200 VAULT_TOKEN=root \
  cargo test -p fraiseql-server -- --ignored vault

# Cleanup
docker rm -f fraiseql-vault
```

**Tests enabled:**
- `crates/fraiseql-server/src/secrets_manager/backends/vault_integration_tests.rs` (22 tests)
- `crates/fraiseql-server/src/secrets_manager/backends/vault_advanced_tests.rs` (28 tests, but most are incomplete stubs)

#### Federation Stack (61 tests)

```bash
# Start full federation stack
docker compose -f tests/integration/docker-compose.yml up -d --build

# Wait for all services to be healthy (Apollo Router is last)
docker compose -f tests/integration/docker-compose.yml ps

# Run federation tests
cargo test -p fraiseql-core --test federation_docker_compose_integration -- --ignored

# Cleanup
docker compose -f tests/integration/docker-compose.yml down -v
```

**Tests enabled:**
- `crates/fraiseql-core/tests/federation_docker_compose_integration.rs` (61 tests)

### Bridge Tests (PostgreSQL + NATS, 7 tests)

```bash
# Requires both PostgreSQL and NATS running
DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
  cargo test -p fraiseql-observers --features "nats,testing,postgres" --test bridge_integration -- --ignored
```

## Environment Variables Reference

| Variable | Service | Default | Used By |
|----------|---------|---------|---------|
| `DATABASE_URL` | PostgreSQL | - | fraiseql-server, fraiseql-core, fraiseql-observers |
| `TEST_DATABASE_URL` | PostgreSQL | - | fraiseql-observers transport tests |
| `REDIS_URL` | Redis | `redis://localhost:6379` | fraiseql-observers queue/dedup/cache |
| `TLS_TEST_DB_URL` | TLS PostgreSQL | - | fraiseql-wire TLS tests |
| `VAULT_ADDR` | Vault | `http://localhost:8200` | fraiseql-server secrets manager |
| `VAULT_TOKEN` | Vault | - | fraiseql-server secrets manager |
| `MYSQL_URL` | MySQL | - | fraiseql-core MySQL tests |
| `SQLSERVER_URL` | SQL Server | - | fraiseql-core SQL Server tests |
| `FRAISEQL_TEST_URL` | FraiseQL Server | - | fraiseql-server E2E tests |

## Feature Flags for Testing

### fraiseql-observers

```bash
# All infrastructure features
cargo test -p fraiseql-observers --features "postgres,nats,dedup,caching,queue,testing"

# Redis-only features
cargo test -p fraiseql-observers --features "dedup,caching,queue"

# NATS features
cargo test -p fraiseql-observers --features "nats,testing"

# Full phase 8 features
cargo test -p fraiseql-observers --features "phase8"
```

### fraiseql-core

```bash
# PostgreSQL tests
cargo test -p fraiseql-core --features "test-postgres"

# MySQL tests
cargo test -p fraiseql-core --features "test-mysql"

# SQL Server tests
cargo test -p fraiseql-core --features "test-sqlserver"

# All features
cargo test -p fraiseql-core --all-features
```

### fraiseql-server

```bash
# All features (includes optional Redis, metrics, observability)
cargo test -p fraiseql-server --all-features
```

## Stress and Performance Tests

These tests are intentionally excluded from CI and should only be run manually or in nightly builds.

```bash
# Federation saga stress tests (6 tests)
cargo test -p fraiseql-core --test federation_saga_stress_test -- --ignored

# Federation observability performance (3 tests, run in isolation)
cargo test -p fraiseql-core --test federation_observability_perf -- --ignored --test-threads=1

# Federation saga performance (1 test, run in isolation)
cargo test -p fraiseql-core --test federation_saga_performance_test -- --ignored --test-threads=1

# Observer stress tests (5 tests)
cargo test -p fraiseql-observers --test stress_tests -- --ignored
```

## CI/CD Integration

### Existing CI (`.github/workflows/ci.yml`)

Already running:
- Format check (nightly rustfmt)
- Clippy linting (pedantic + deny warnings)
- Multi-OS test matrix (Linux, Windows, macOS) with MSRV 1.75
- PostgreSQL integration tests (port 5433)
- MySQL integration tests (port 3307)
- SQLite integration tests (no service needed)
- SQL Server integration tests (port 1434)
- Code coverage with Codecov
- Security audit (cargo-audit)
- Documentation build

### Extended CI (`.github/workflows/ci-extended.yml`)

Additional infrastructure tests:
- Redis integration tests
- NATS integration tests
- Vault integration tests
- TLS PostgreSQL tests
- Federation Docker Compose tests

Runs on:
- Push to `dev` and `release/*` branches
- Pull requests to `dev`
- Nightly schedule (2 AM UTC)

## Incomplete Test Implementation Status

The following test categories require feature implementation before they can run. They are not blocked by infrastructure.

| Module | Tests | Feature Required |
|--------|------:|-----------------|
| Encryption: query builder | 62 | Query builder encryption integration |
| Encryption: field encryption | 43 | Field-level encryption API |
| Encryption: mapper | 40 | Encryption mapper layer |
| Encryption: rotation | 39 | Key rotation infrastructure |
| Encryption: database adapter | 39 | Encryption database adapter |
| Encryption: refresh | 38 | Credential/key refresh |
| Encryption: transactions | 37 | Encryption in transactions |
| Encryption: schema detection | 35 | Encrypted field schema detection |
| Encryption: rotation API | 34 | Key rotation REST API |
| Encryption: audit logging | 33 | Audit logging integration |
| Encryption: error recovery | 32 | Error recovery implementation |
| Encryption: compliance | 32 | Compliance features |
| Encryption: performance | 31 | Batching/performance optimization |
| Encryption: dashboard | 30 | Dashboard UI |
| Auth: OAuth | 46 | OAuth2/OIDC provider integration |
| Secrets: schema | 38 | Secrets schema DB integration |
| Secrets: Vault advanced | 28 | Vault advanced features |
| **Total** | **~637** | |

These tests are deferred to post-alpha.3 feature development sprints.
