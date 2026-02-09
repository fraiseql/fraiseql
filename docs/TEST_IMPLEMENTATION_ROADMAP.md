# Test Implementation Roadmap

Step-by-step plan for enabling all ~857 ignored tests in FraiseQL v2.

## Summary

| Phase | Tests Enabled | Cumulative | Effort | Priority |
|-------|-------------:|----------:|--------|----------|
| 1: Redis + NATS | 24 | 24 | 1 sprint | High |
| 2: Vault + TLS | 27 | 51 | 1 sprint | High |
| 3: Federation | 61 | 112 | 1 sprint | High |
| 4: Encryption | ~480 | ~592 | 4-6 sprints | Medium |
| 5: OAuth + Secrets | ~134 | ~726 | 2 sprints | Medium |
| 6: E2E + Server | ~80 | ~806 | 1 sprint | Medium |
| 7: Stress/Perf | ~41 | ~847 | 1 sprint | Low |

## Phase 1: Redis + NATS (Sprint 1)

**Goal**: Enable 24 tests that have implementations but need Redis and NATS.

### Tasks

- [ ] Add Redis service to `ci-extended.yml`
- [ ] Enable Redis queue tests (`fraiseql-observers/src/queue/redis.rs`, 6 tests)
- [ ] Enable Redis integration tests (`fraiseql-observers/tests/integration_test.rs`, 3 tests)
- [ ] Add NATS service to `ci-extended.yml`
- [ ] Enable NATS integration tests (`fraiseql-observers/tests/nats_integration.rs`, 8 tests)
- [ ] Enable bridge tests (`fraiseql-observers/tests/bridge_integration.rs`, 7 tests)

### Verification

```bash
docker compose -f docker-compose.test-all.yml up -d redis nats postgres
REDIS_URL=redis://localhost:6379 \
DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
  cargo test -p fraiseql-observers --features "postgres,nats,dedup,caching,queue,testing" -- --ignored --test-threads=1
```

**Outcome**: 24 tests enabled

## Phase 2: Vault + TLS (Sprint 2)

**Goal**: Enable 27 tests requiring Vault and TLS PostgreSQL.

### Tasks

- [ ] Add Vault service to `ci-extended.yml`
- [ ] Enable Vault integration tests (`fraiseql-server/.../vault_integration_tests.rs`, 22 tests)
- [ ] Generate TLS certificates in CI
- [ ] Enable TLS integration tests (`fraiseql-wire/tests/tls_integration.rs`, 5 tests)

### Verification

```bash
docker compose -f docker-compose.test-all.yml up -d vault postgres-tls
cd docker/tls-postgres && ./generate-certs.sh && cd ../..

VAULT_ADDR=http://localhost:8200 VAULT_TOKEN=root \
  cargo test -p fraiseql-server -- --ignored vault --test-threads=1

TLS_TEST_DB_URL="postgresql://fraiseql:fraiseql_test@localhost:5444/fraiseql_tls_test?sslmode=require" \
  cargo test -p fraiseql-wire --test tls_integration -- --ignored --test-threads=1
```

**Outcome**: 51 tests enabled (cumulative)

## Phase 3: Federation (Sprint 3)

**Goal**: Enable 61 federation Docker Compose integration tests.

### Tasks

- [ ] Verify federation Docker Compose builds in CI
- [ ] Add federation job to `ci-extended.yml`
- [ ] Enable federation tests (`fraiseql-core/tests/federation_docker_compose_integration.rs`, 61 tests)

### Verification

```bash
docker compose -f tests/integration/docker-compose.yml up -d --build
# Wait for services to be healthy
cargo test -p fraiseql-core --test federation_docker_compose_integration -- --ignored --test-threads=1
docker compose -f tests/integration/docker-compose.yml down -v
```

**Outcome**: 112 tests enabled (cumulative)

## Phase 4: Encryption Features (Sprint 4-9)

**Goal**: Implement encryption features to enable ~480 test stubs.

**Complexity**: HIGH - requires actual feature implementation, not just infrastructure.

### Sprint 4-5: Core Encryption

- [ ] Implement field-level encryption API (`field_encryption_tests.rs`, 43 tests)
- [ ] Implement key rotation infrastructure (`rotation_tests.rs`, 39 tests)
- [ ] Implement key refresh logic (`refresh_tests.rs`, 38 tests)
- [ ] Implement encryption database adapter (`database_adapter_tests.rs`, 39 tests)

### Sprint 6-7: Encryption Integration

- [ ] Implement query builder integration (`query_builder_integration_tests.rs`, 62 tests)
- [ ] Implement mapper integration (`mapper_integration_tests.rs`, 40 tests)
- [ ] Implement transaction integration (`transaction_integration_tests.rs`, 37 tests)
- [ ] Implement schema detection (`schema_detection_tests.rs`, 35 tests)

### Sprint 8-9: Encryption Operations

- [ ] Implement rotation API (`rotation_api_tests.rs`, 34 tests)
- [ ] Implement audit logging integration (`audit_logging_tests.rs`, 33 tests)
- [ ] Implement error recovery (`error_recovery_tests.rs`, 32 tests)
- [ ] Implement compliance features (`compliance_tests.rs`, 32 tests)
- [ ] Implement performance optimization (`performance_tests.rs`, 31 tests)
- [ ] Implement dashboard (`dashboard_tests.rs`, 30 tests)

**Outcome**: ~592 tests enabled (cumulative)

## Phase 5: OAuth + Secrets (Sprint 10-11)

**Goal**: Implement OAuth and secrets management features.

### Sprint 10: OAuth/OIDC

- [ ] Implement OAuth2 provider integration (`auth/oauth_tests.rs`, 46 tests)
- [ ] Add mock OAuth provider for CI testing

### Sprint 11: Secrets Management

- [ ] Implement secrets schema DB integration (`secrets/schema_tests.rs`, 38 tests)
- [ ] Implement Vault advanced features (`vault_advanced_tests.rs`, 28 tests)
- [ ] Implement inline database adapter tests (`encryption/database_adapter.rs`, 5 tests)
- [ ] Implement TLS connection tests (`fraiseql-wire/src/connection/tls.rs`, 1 test)
- [ ] Implement saga store PostgreSQL test (`fraiseql-core/src/federation/saga_store.rs`, 1 test)
- [ ] Implement observer storage test (`fraiseql-observers/src/storage.rs`, 1 test)
- [ ] Implement observer postgres_notify tests (`fraiseql-observers/src/transport/postgres_notify.rs`, 5 tests)

**Outcome**: ~726 tests enabled (cumulative)

## Phase 6: E2E + Server Tests (Sprint 12)

**Goal**: Enable tests requiring a running FraiseQL server.

### Tasks

- [ ] Add FraiseQL server startup to CI
- [ ] Enable HTTP server E2E tests (`http_server_e2e_test.rs`, 14 tests)
- [ ] Enable GraphQL features E2E tests (`graphql_features_e2e_test.rs`, 32 tests)
- [ ] Enable observer runtime tests (`observer_runtime_integration_test.rs`, 14 tests)
- [ ] Enable database query tests (`database_query_test.rs`, 11 tests)
- [ ] Enable observer E2E tests (`observer_e2e_test.rs`, 8 tests)
- [ ] Enable concurrent load test (`concurrent_load_test.rs`, 1 test)

**Outcome**: ~806 tests enabled (cumulative)

## Phase 7: Stress and Performance Tests (Sprint 13)

**Goal**: Enable stress tests in nightly CI builds only.

### Tasks

- [ ] Add nightly CI workflow for stress tests
- [ ] Enable federation saga stress tests (6 tests)
- [ ] Enable federation observability performance tests (3 tests, sequential)
- [ ] Enable federation saga performance tests (1 test, sequential)
- [ ] Enable observer stress tests (5 tests)

### Nightly CI Configuration

```yaml
# Add to ci-extended.yml schedule or separate workflow
on:
  schedule:
    - cron: '0 4 * * *'  # 4 AM UTC, after extended CI

jobs:
  stress-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      # ... setup ...
      - name: Run stress tests
        run: |
          cargo test -p fraiseql-core --test federation_saga_stress_test -- --ignored --test-threads=1
          cargo test -p fraiseql-core --test federation_observability_perf -- --ignored --test-threads=1
          cargo test -p fraiseql-core --test federation_saga_performance_test -- --ignored --test-threads=1
          cargo test -p fraiseql-observers --test stress_tests -- --ignored --test-threads=1
```

**Outcome**: ~847 tests enabled (cumulative)

## Priority for v2.0.0-alpha.3 Release

### Immediate (this release)

1. Create `docs/TESTING_INFRASTRUCTURE.md`
2. Create `docker-compose.test-all.yml`
3. Create `.github/workflows/ci-extended.yml`
4. Create `docs/TEST_IMPLEMENTATION_ROADMAP.md`

### Post-Release (Phases 1-3)

Low risk, high value - enables 112 infrastructure-dependent tests:
- Phase 1: Redis + NATS (24 tests)
- Phase 2: Vault + TLS (27 tests)
- Phase 3: Federation (61 tests)

### Future Development (Phases 4-7)

Requires feature implementation - defer to roadmap:
- Phase 4: Encryption (~480 tests)
- Phase 5: OAuth + Secrets (~134 tests)
- Phase 6: E2E + Server (~80 tests)
- Phase 7: Stress/Performance (~41 tests)
