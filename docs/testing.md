# Testing Guide

FraiseQL has six distinct test categories with different semantics, infrastructure
requirements, and failure modes. This document describes the full taxonomy.

## Quick Reference

```bash
make test           # Unit + SQL snapshots + behavioral integration (PostgreSQL)
make test-full      # All categories: unit + snapshots + integration + federation
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
| `doc-only` | Pure documentation snapshot (e.g. basic SELECT without WHERE). No generator to call; the snapshot serves as a spec, not a regression test. Must include a comment explaining why. |

**A snapshot may never be left unregistered.** The `tools/check-snapshot-pairing.sh`
script (run in the preflight ShellGates leg, `make lint-snapshot-pairing`, and as a
pre-commit hook) enforces this **in both directions** — an unregistered snapshot
fails, and so does a registry row naming a snapshot that no longer exists.
When you add a new snapshot, register it:

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
DATABASE_URL="postgresql://..." cargo nextest run --features test-postgres -p fraiseql-core -- --ignored
```

**Infrastructure**: Docker (`make db-up` starts PostgreSQL, Redis, NATS, Vault).
**Blocks CI**: Yes (dedicated CI job).

---

### 4. Federation Integration Tests

**What**: End-to-end Apollo Federation v2 with real subgraphs.
Tests `@key` directives, entity resolution, and the federation gateway.
**Where**: the Dagger `integration (federation)` leg (`.dagger/main.go`,
`integrationFederation`), which stands up Apollo Router, the subgraphs and PostgreSQL in
containers.
**Run**:

```bash
# hermetic, no Docker: the committed subgraph SDL fixtures, composed with real
# Apollo Federation v2 composition
make federation-compose-check
```

**Infrastructure**: none for the local check; containers for the CI leg.
**Blocks CI**: the `Dagger — integration` leg is push-to-dev and dispatch, not a
required check on a branch.

> This section used to tell you to run `make test-federation` in `docker/federation-ci/`.
> Neither the target nor the directory existed — the target failed on its first line and
> the directory is in no commit reachable from `dev` (#1219).

---

### 5. Load Tests

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

### 6. Criterion Microbenchmarks

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
| Verifying Apollo Federation flow | Federation integration (category 4) |
| Verifying server throughput | Load test (category 5) |
| Measuring algorithm performance | Criterion benchmark (category 6) |

---

## Running Ignored Tests

Many tests are `#[ignore]` because they require live infrastructure (PostgreSQL,
Redis, NATS, Vault). Here is the complete procedure to run them all.

### Quick start

```bash
# 1. Start all required services (PostgreSQL, Redis, NATS, Vault)
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
| `REDIS_URL` | `redis://localhost:6379` | Redis for APQ and rate-limiting |
| `TEST_DATABASE_URL` | same as `DATABASE_URL` | Observer PostgreSQL transport |
| `SAGA_STORE_TEST_URL` | same as `DATABASE_URL` | Saga store integration (`saga` feature) |
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
# Start all test services (PostgreSQL, Redis, NATS, Vault)
make db-up

# Stop everything
make db-down

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
| Federation | `federation-tests` | Required |
| Load | `perf-baseline` | Advisory |
| Criterion | — | Manual only |
