# Testing Guide

FraiseQL has seven distinct test categories with different semantics, infrastructure
requirements, and failure modes. This document describes the full taxonomy.

## Quick Reference

```bash
make test           # Unit + SQL snapshots + behavioral integration (PostgreSQL)
make test-full      # All categories: unit + snapshots + integration + cross-db + federation
make test-load      # Load testing (requires running server + k6)
```

---

## Test Categories

### 1. Unit Tests

**What**: Pure logic with no I/O, no database.
**Where**: `mod tests { ... }` embedded in each source file.
**Run**: `cargo nextest run` or `cargo test --lib`
**Infrastructure**: None.
**Blocks CI**: Yes.

These are the default tests — fast, dependency-free, run on every `cargo test`.

---

### 2. SQL Snapshot Tests

**What**: Verify that the SQL compiler generates specific SQL strings.
These are regression tests for SQL generation, not execution correctness.
A passing snapshot test means the SQL has not changed; it does NOT mean
the SQL is correct. Behavioral tests (category 3) verify actual execution.
**Where**: `crates/fraiseql-core/tests/sql_snapshots.rs`
**Run**: `cargo nextest run --test sql_snapshots`
**Infrastructure**: None.
**Blocks CI**: Yes.

#### Updating Snapshots

When you change the SQL compiler, existing snapshots will fail. To update them:

```bash
# 1. Accept all changes
INSTA_UPDATE=accept cargo nextest run --test sql_snapshots

# 2. Review each change interactively
cargo insta review

# 3. Commit the updated .snap files
git add crates/fraiseql-core/tests/snapshots/
git commit -m "test(sql): update SQL snapshots after compiler change"
```

**Important**: Review every changed snapshot to verify the new SQL is correct,
not just different.

#### Snapshot Pairing Policy

Every snapshot in `crates/fraiseql-core/tests/snapshots/` must be registered in
`tests/snapshot-pairs.md` and must have one of the following coverage statuses:

| Status | Meaning |
|--------|---------|
| `generator` | The snapshot is produced by calling a real generator (e.g. `PostgresWhereGenerator`) in the snapshot test itself. Changes to the generator will cause the test to fail, providing true regression protection. |
| `behavioral` | A separate test in `tests/sql_behavioral.rs` calls the same generator with identical inputs and asserts `==` (not a snapshot). Best for WHERE-clause operators and projection logic. |
| `db-integration` | The snapshot's correctness is verified by a `#[ignore]` integration test that executes against a real database. Required for mutations, RLS, and aggregate queries. |
| `cross-db-parity` | Covered by `cross_database_test.rs` — the identical logical query is executed on ≥2 databases and results are compared. |
| `doc-only` | Pure documentation snapshot (e.g. basic SELECT without WHERE). No generator to call; the snapshot serves as a spec, not a regression test. Must include a comment explaining why. |

**A snapshot may never be left unregistered.** The `tools/check-snapshot-pairing.sh`
script (run as a pre-commit hook) enforces this. When you add a new snapshot, register it:

```bash
# 1. Add entry to tests/snapshot-pairs.md
# 2. Confirm the pairing script passes
./tools/check-snapshot-pairing.sh
```

---

### 3. Behavioral Integration Tests

**What**: Execute real queries against a real database. Verify result correctness
(not just SQL shape).
**Where**: `crates/*/tests/*_test.rs`
**Run**:

```bash
# PostgreSQL
DATABASE_URL="postgresql://..." cargo nextest run --features test-postgres -p fraiseql-core -- --ignored

# MySQL
DATABASE_URL="mysql://..." cargo nextest run --features test-mysql -p fraiseql-core -- --ignored

# SQL Server
DATABASE_URL="server=...;..." cargo nextest run --features test-sqlserver -p fraiseql-core -- --ignored
```

**Infrastructure**: Docker (`make db-up` starts PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault).
**Blocks CI**: Yes (dedicated CI job per database).

---

### 4. Cross-Database Parity Tests

**What**: Execute identical WHERE clauses on PostgreSQL AND MySQL simultaneously.
Verify that both databases return identical results for the same query.
**Where**: `crates/fraiseql-core/tests/cross_database_test.rs`
**Run**:

```bash
DATABASE_URL="postgresql://..." \
MYSQL_URL="mysql://..." \
  cargo nextest run \
    --features test-postgres,test-mysql \
    -p fraiseql-core \
    --test cross_database_test -- --ignored --test-threads=1
```

**Infrastructure**: Both PostgreSQL AND MySQL running simultaneously.
**Blocks CI**: Currently advisory (see [Issue #09](../plans/issue-09-ci-gate.md)).
**Why `#[ignore]`**: Requires two databases in parallel — too heavy for the standard CI slot.

These tests are the authoritative check that adding MySQL/SQL Server support has not
introduced silent behavioral divergence. Run them whenever touching SQL generation code.

---

### 5. Federation Integration Tests

**What**: End-to-end Apollo Federation v2 with real subgraphs.
Tests `@key` directives, entity resolution, and the federation gateway.
**Where**: `docker/federation-ci/`
**Run**:

```bash
make test-federation
# or manually:
cd docker/federation-ci && pytest -q --tb=short
```

**Infrastructure**: Docker Compose — Apollo Router + 3 Flask subgraphs + PostgreSQL.
**Blocks CI**: Yes (dedicated `federation-tests` job).

---

### 6. Load Tests

**What**: Performance and throughput validation — P99 latency, error rate, request volume.
**Where**: `benchmarks/load/` (k6 scripts)
**Run**:

```bash
make test-load
# or manually:
k6 run benchmarks/load/basic.js
k6 run benchmarks/load/mutations.js
```

**Infrastructure**: Running `fraiseql-server` with a connected database.
**Blocks CI**: Advisory — CI records results but does not fail on threshold breaches.
Thresholds: P99 < 500ms, error rate < 1%.

---

### 7. Criterion Microbenchmarks

**What**: Algorithm-level benchmarks for hot paths (SQL generation, cache lookups, etc.).
**Where**: `crates/*/benches/`
**Run**:

```bash
cargo bench
# Run a specific benchmark:
cargo bench --bench sql_generation
```

**Infrastructure**: Optional database for some benchmarks.
**Blocks CI**: No — manual only.

---

## Decision Guide: Which Test Should I Write?

| Scenario | Write this |
|----------|-----------|
| Testing a pure function or algorithm | Unit test (category 1) |
| Verifying the compiler generates specific SQL | Snapshot test (category 2) |
| Verifying a query returns correct rows | Behavioral integration (category 3) |
| Verifying MySQL and PostgreSQL agree | Cross-database parity (category 4) |
| Verifying Apollo Federation flow | Federation integration (category 5) |
| Verifying server throughput | Load test (category 6) |
| Measuring algorithm performance | Criterion benchmark (category 7) |

---

## Running Ignored Tests

Many tests are `#[ignore]` because they require live infrastructure (PostgreSQL,
Redis, NATS, Vault). Here is the complete procedure to run them all.

### Quick start

```bash
# 1. Start all required services (PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault)
make db-up

# 2. Run every #[ignore] test suite
make test-all-ignored

# 3. Tear down when done
make db-down
```

`make test-all-ignored` sets all required environment variables internally. To run
a specific ignored suite manually, export the vars below first:

### Required environment variables

| Variable | Default used by `make test-*` | Purpose |
|---|---|---|
| `DATABASE_URL` | `postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql` | PostgreSQL connection |
| `MYSQL_URL` | `mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql` | MySQL for cross-DB parity |
| `REDIS_URL` | `redis://localhost:6379` | Redis for APQ and rate-limiting |
| `TEST_DATABASE_URL` | same as `DATABASE_URL` | Observer PostgreSQL transport |
| `SAGA_STORE_TEST_URL` | same as `DATABASE_URL` | Saga store integration |
| `VAULT_ADDR` | `http://localhost:8200` | HashiCorp Vault secrets tests |
| `VAULT_TOKEN` | `fraiseql-test-token` | Vault auth token |

### Running a single ignored suite

```bash
# Redis APQ tests
REDIS_URL=redis://localhost:6379 \
  cargo nextest run -p fraiseql-core --features redis-apq --lib redis -- --ignored

# Observer NATS transport
cargo nextest run -p fraiseql-observers --features nats --test nats_integration -- --ignored

# Vault secrets
VAULT_ADDR=http://localhost:8200 VAULT_TOKEN=fraiseql-test-token \
  cargo nextest run -p fraiseql-server --test secrets_manager_integration_test -- --ignored
```

> **Note**: `cargo nextest run` uses `--ignored` to run only ignored tests.
> Standard `cargo test` uses `-- --ignored` (double dash).

---

## Infrastructure Setup

```bash
# Start all test databases (PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault)
make db-up

# Start only the federation stack
make federation-up

# Stop everything
make db-down
make federation-down

# Reset database volumes (useful after schema changes)
make db-reset
```

The `docker/docker-compose.test.yml` defines all services with the correct ports and
credentials that the Makefile targets use.

---

## CI Coverage

| Category | CI Job | Failure Policy |
|----------|--------|---------------|
| Unit | `test` | Required |
| SQL snapshots | `test` | Required |
| Integration (PostgreSQL) | `integration-postgres` | Required |
| Integration (MySQL) | `integration-mysql` | Required |
| Integration (SQL Server) | `integration-sqlserver` | Required |
| Cross-database parity | (advisory, see Issue #09) | Advisory |
| Federation | `federation-tests` | Required |
| Load | `perf-baseline` | Advisory |
| Criterion | — | Manual only |
