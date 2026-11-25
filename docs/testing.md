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

## Guard-Site Mutation

Every place an authorization, session or tenancy rule is applied is a *guard site*, and
each one must have a test that fails when the guard is removed. A guard can be
reachable only through a second door its suite never opens; a suite that "covers" the
route then passes with the guard gone. So each site is proven by mutating it.

`tools/guard-sites.toml` is the inventory. A `[[site]]` names the file, the function the
guard sits in (`Type::method`, or `::name` for the free fn, when the file defines the name
twice; `impl Type::method` or `impl Trait for Type::method` when the type has both; or a
`const` that holds a shared SQL predicate), with `cfg` naming the build when the function
is defined once per build (`cfg = 'feature = "admin-sql"'` picks the definition under
`#[cfg(feature = "admin-sql")]`), an exact `anchor` of its code, the `mutant` that replaces it, the
`killers` (tests that must fail on the mutant) and the `shard` (test command) they run
in. Two sites that each cover the other share a `joint` and are mutated together;
selecting one (`--site`, `--shard`, `--changed-since`) runs the whole joint.
`[[held]]` paths are not inventoried yet; `[[exempt]]` functions match a guard pattern
without being one, with the reason and the issue deciding them. An exempt row that names
an `anchor` exempts that statement alone, so a guard added beside it is still reported; a
`.sql` file has no functions, so its exempt rows always name one. A policy that restricts
nothing (`USING (true)`, `WITH CHECK (true)`) is such a match: it is refused unless an
exempt row names what guards its table instead, the grants.

Two tools read it:

- `make lint-guard-sites` (`tools/check-guard-sites.py`, every PR, in ShellGates): each
  site resolves (its function defined once, its anchor once inside it, its killers
  defined as tests), and every call matching a `[[family]]` pattern in production code
  is inside a site's anchor, a held path or an exempt function. An unrowed call is
  reported at its file, line and column. A family scans code by default; `scan =
  "strings"` scans inside string literals instead, for guards that are SQL predicates,
  and a strings family with `sql = true` also scans the `.sql` files under `crates/` and
  `examples/` (migrations, and example schemas a test applies), SQL comments blanked. A
  site in a `.sql` file names no function: its anchor occurs once in the file.
  Self-test: `make test-guard-sites-gate`.
- `tools/run-guard-mutants.py`: runs each selected shard once unmutated, then applies
  each mutant, runs its shard and restores the file. A shard that already fails is
  reported BASELINE-RED and none of its mutants run, since a red shard would "kill"
  every mutant; one whose test binary runs no test is BASELINE-EMPTY, since it would
  let every mutant "survive" untested. An untracked file a shard run leaves behind (a
  proptest seed of a failure a mutant caused) fails the run as LEFT-BEHIND: committed, it
  would pin the mutant's failure as a case to re-run. A mutant is killed only when one of its named killers fails; one that
  survives, is failed only by an unnamed test, or does not build fails the run.
  An interrupted run leaves no mutant: a signal restores each file, and a run killed
  outright is repaired by the next start, from saved bytes recorded under the git dir,
  which then refuses to run until each recorded file matches HEAD.
  `--site`, `--shard` and `--changed-since REF` (or `--changed-files PATH`, a list of
  the changed paths) select a subset; `--list` prints the selection; `--output DIR`
  keeps each mutant's test output. Self-test: `make test-guard-mutants-gate`.
- `dagger call guard-mutants --source=.` runs the runner in CI shape: every site, in
  `--lanes` lanes side by side (2 by default). `--lane I/N` gives each lane the shards of
  the cargo builds it compiles, so a build is compiled in one lane; each lane has its own
  target volume and its own Postgres (pgvector), Redis and NATS, and the call fails unless
  every mutant is killed by a test its site names. `--changed="<paths>"` runs only the
  sites in those files (the per-change run: `git diff --name-only` against the merge
  base), and `--shard NAME` one shard's. The whole inventory takes about 3h10m on two
  lanes. `.github/workflows/dagger-guard-mutants.yml` runs it on a schedule, and on
  dispatch for the commit a release will tag; `tools/release.sh` refuses to tag a tree no
  green sweep has covered (`SKIP_GUARD_MUTANTS=1` skips the check).

**Adding a guard.** Add a `[[site]]` row, then run its mutant:

```bash
python3 tools/run-guard-mutants.py --site <id>
```

The e2e shards need `DATABASE_URL` on the image CI uses, `ghcr.io/fraiseql/pgvector:pg18`,
and a fresh database. A server without the `vector` extension fails a shard unmutated, so
the runner checks for it first and stops with a RIG error (exit 2) rather than report
BASELINE-RED. The database is fresh because a table another branch left behind in a
shared one can fail a shard before any mutant is applied. Every shard command
sets `FRAISEQL_TEST_REQUIRE_POSTGRES=1`, and the gate refuses one that does not: a killer
that finds no database would skip and pass, so its baseline would read green and its
mutant as survived, and the variable makes a missing database fail the baseline instead. The observer
transport shard also reads `TEST_DATABASE_URL`, and the NATS shard `NATS_URL` (a
JetStream broker); the NATS shard's command sets the development flags the CI leg sets,
since a plaintext local broker is refused in production. A mutant that survives means the rule has no test:
write the test first (the surviving mutant is its RED), then add the row.

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
| `SAGA_STORE_TEST_URL` | same as `DATABASE_URL` | Saga store integration (`fraiseql-saga`) |
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

---

## Where a Gate Runs

Each list of gates is written once, in the `Makefile`, and CI runs that list.

- **Shell gates.** Every `shell-gates: <target>` line adds a target to the list. The
  Dagger `ShellGates` function runs `make -k shell-gates`, and `make preflight` runs
  the same target. `-k` reports every failing gate in one run; the exit code is still
  non-zero if any one of them fails. To add a gate, write its target and add one
  `shell-gates:` line, with a comment saying what it catches.
- **Rust gates.** Each of the other `Preflight` functions (`Fmt`, `Rustdoc`,
  `RustdocDefault`, `Clippy`, `CheckDefault`, `CheckFuzz`, `CheckExampleCrates`,
  `CheckRExamples`) runs one `make` command in its own container. `make preflight`
  depends on the same targets.
- **Test shards.** `make test-leg` and `make test-integration-postgres` run the lines
  of the `Test` and `integrationPostgres` shards. `tools/leg-lines.py` reads them out
  of `.dagger/main.go` when the target runs; `python3 tools/leg-lines.py Test` prints
  them.

`tools/check-gate-reachability.py` (in `shell-gates`) holds this together. It fails
when:

- a `tools/check-*` script, a `tools/tests/*_test.sh` self-test, or a `lint-*` or
  `test-*-gate` target is run by no CI leg. A gate that is run by no CI leg on
  purpose gets a row with a `reason` in `tools/gate-reachability-exemptions.toml`;
- a `Preflight` function's `make` command differs from the one the script pins;
- a target a CI leg reaches can carry on past a failure: a `-` recipe prefix, a
  command ending `|| true`, `|| :` or `; true`, `make -i`, an `.IGNORE:` line, or
  `-i` in `MAKEFLAGS`. A recipe line that says why, with `# Reason:`, may carry on
  past a failure; `.IGNORE:` and `MAKEFLAGS` have no such exception.
