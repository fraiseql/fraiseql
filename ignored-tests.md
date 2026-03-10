# Ignored Tests in FraiseQL

This document explains which tests are marked with `#[ignore]` and why, and how to run them.

## Overview

Certain tests in FraiseQL are marked with the `#[ignore]` attribute and are excluded from the default test run. These tests are:
- **Resource-intensive** - They require external services (TLS, databases)
- **Integration-only** - They test interaction with real systems
- **Manual trigger** - They should only run when explicitly requested

## Ignored Test Categories

### 1. TLS Integration Tests

**Location**: `crates/fraiseql-wire/tests/tls_integration.rs`

**Why ignored**: These tests require TLS certificate setup and PostgreSQL with specific configuration.

**Environment**: `TLS_TEST_DB_URL` environment variable must be set to a PostgreSQL connection string that supports TLS.

**How to run**:
```bash
TLS_TEST_DB_URL="postgres://user:password@localhost/db" cargo test --test tls_integration -- --ignored
```

### 2. PostgreSQL Observer Tests

**Location**: `crates/fraiseql-observers/src/transport/postgres_notify.rs`

**Why ignored**: These are integration tests for PostgreSQL NOTIFY/LISTEN functionality requiring a live database.

**Environment**: `TEST_DATABASE_URL` environment variable must be set to a PostgreSQL connection string.

**How to run**:
```bash
TEST_DATABASE_URL="postgres://user:password@localhost/db" cargo test --lib -- --ignored
```

### 3. Database Query Tests

**Location**: `crates/fraiseql-server/tests/database_query_test.rs`

**Why ignored**: These integration tests require a live database for end-to-end query execution testing.

**Environment**: `DATABASE_URL` environment variable must be set to a PostgreSQL connection string.

**How to run**:
```bash
DATABASE_URL="postgres://user:password@localhost/db" cargo test --test database_query_test -- --ignored
```

## Running All Ignored Tests

### CI/CD (Manual Trigger)

Use GitHub's workflow dispatch to manually trigger the `Ignored Tests` job:

1. Go to Actions → CI
2. Click "Run workflow" → trigger "Ignored Tests"
3. The job will run with a preconfigured PostgreSQL database

### Local Development

To run all ignored tests locally, you'll need:

1. **PostgreSQL running** (can use Docker):
   ```bash
   docker run -d \
     --name fraiseql-test-db \
     -e POSTGRES_PASSWORD=fraiseql_test_password \
     -e POSTGRES_USER=fraiseql_test \
     -e POSTGRES_DB=test_fraiseql \
     -p 5433:5432 \
     postgres:16
   ```

2. **Set environment variables**:
   ```bash
   export DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
   export TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
   export TLS_TEST_DB_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
   ```

3. **Run tests**:
   ```bash
   # All ignored tests
   cargo test -- --ignored --test-threads=1

   # Specific test file
   cargo test --test tls_integration -- --ignored
   ```

## Ignored Test Execution in CI

The main CI pipeline (`ci.yml`) runs selected ignored tests automatically:

- **PostgreSQL Integration Tests** - Run as part of `integration-postgres` job
- **Database Adapter Tests** - Run in `integration-postgres` job's dedicated step

The complete `Ignored Tests` job only runs on manual workflow dispatch (GitHub Actions → CI → "Run workflow").

### 4. Federation Integration Tests (Real Subgraph)

**Location**: `crates/fraiseql-server/tests/federation_integration_test.rs`

**Why ignored**: These tests run FraiseQL as a real Apollo Federation subgraph.
Tests 2 and 3 pull Docker images via testcontainers (PostgreSQL + optionally Apollo Router).
Tests 1 and 4 use `FailingAdapter` (no real DB) but are still `#[ignore]` to keep them out of the default run.

**Gate environment variable**: `FRAISEQL_DOCKER=1` (set in CI to signal Docker is available)

**How to run**:
```bash
# All 4 tests (Docker must be available):
cargo nextest run -p fraiseql-server --test federation_integration_test \
  -- --include-ignored

# SDL + null-entity tests only (no real DB needed, but still #[ignore]):
cargo nextest run -p fraiseql-server --test federation_integration_test \
  -E 'test(service_sdl) | test(entities_returns_null)' -- --include-ignored
```

**Tests**:
| Test | Requires | Purpose |
|------|----------|---------|
| `service_sdl_contains_federation_directives` | FailingAdapter | SDL has inline `@key` directives |
| `entities_resolves_user_by_id` | PostgreSQL container | `_entities` resolves real DB row |
| `apollo_router_routes_query_to_fraiseql_subgraph` | PostgreSQL + Apollo Router containers | Full gateway-to-subgraph flow |
| `entities_returns_null_for_missing_entity` | FailingAdapter | Missing entity → null (not error) |

## Adding New Ignored Tests

When adding a new ignored test:

1. Mark it with `#[ignore]`
2. Add documentation above the test explaining why it's ignored
3. Document environment variable requirements (if any)
4. Add a section to this file

Example:
```rust
/// Tests PostgreSQL NOTIFY/LISTEN functionality.
///
/// Requires TEST_DATABASE_URL environment variable pointing to a PostgreSQL database.
#[tokio::test]
#[ignore]
async fn test_postgres_notify() {
    let db_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run this test");
    // ...
}
```

## Environment Variables Reference

| Variable | Used By | Example |
|----------|---------|---------|
| `DATABASE_URL` | Server tests, integration tests | `postgresql://user:pass@localhost:5433/db` |
| `TEST_DATABASE_URL` | Observer tests | `postgresql://user:pass@localhost:5433/db` |
| `TLS_TEST_DB_URL` | Wire protocol TLS tests | `postgresql://user:pass@localhost:5433/db` |
| `RUST_LOG` | All tests (optional) | `debug`, `info` |

## Troubleshooting

### Tests fail with "database connection refused"
- Ensure PostgreSQL is running on the configured host/port
- Verify credentials in the connection string
- Check that the database exists and the user has access

### Tests timeout
- TLS tests may timeout if certificates aren't properly configured
- Increase test timeout: `cargo test -- --test-threads=1` (runs sequentially)

### Tests pass locally but fail in CI
- Verify environment variables are set in CI job (see `ci.yml` `env` sections)
- Check CI job logs for detailed error messages
- Some tests may require specific PostgreSQL versions/extensions

## Future Work

As FraiseQL evolves, we may:
- Add more comprehensive integration test coverage
- Move some ignored tests to non-ignored when infrastructure is standardized
- Add test fixtures/mocking to eliminate some external dependencies
