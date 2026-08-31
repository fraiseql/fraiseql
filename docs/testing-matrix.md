# FraiseQL Test Matrix

This document describes the full test matrix, required infrastructure per feature flag,
and how to run each test category.

## Quick Reference

| Command | Infrastructure | Duration | Coverage |
|---------|---------------|----------|----------|
| `make test-unit` | None | ~30s | Unit tests (all crates, `--lib`) |
| `make test-leg` | None | ~40 min | The `Dagger — test` workspace suite, exactly as CI runs it |
| `make test-integration-postgres` | Docker (db-up) | ~40 min | The `integration (postgres)` CI shard, exactly as CI runs it |
| `make test-integration` | Docker (db-up) | ~50 min | The above, plus the observer and server `#[ignore]` suites |
| `make test-full` | Docker (db-up) | ~65 min | Everything (8 steps) |
| `make test-all-ignored` | Docker (db-up) | ~15 min | All `#[ignore]` tests |
| `make federation-compose-check` | None | ~1 min | Subgraph SDL fixtures + real Apollo Federation v2 composition |
| `make test-parity` | uv, bun, go, mvn, php | ~10 min | Cross-SDK schema parity |

## Do not reach for plain `cargo test` on the DB-backed crates

The `fraiseql-core` and `fraiseql-db` integration suites provision their fixtures
with `CREATE TABLE` / `CREATE TYPE` in the **same `public` schema of the same
database**. Run in parallel they collide on PostgreSQL's catalog uniqueness:

```
duplicate key value violates unique constraint "pg_type_typname_nsp_index"
```

So with a `DATABASE_URL` bound, `cargo test -p fraiseql-core` gives **4257
passed, 38 failed** and `cargo test -p fraiseql-db` **457 passed, 5 failed** —
none of which are yours, and the failing *names* drift between runs, which reads
as flakiness in whatever you just changed. CI never uses default parallelism:
every one of these invocations carries `--test-threads=1`.

Use **`make test-integration-postgres`**. It is the line-for-line mirror of the
Dagger `integration (postgres)` shard, held to it in both directions by
`make lint-shard-parity`, so what passes there is what CI will run.

Two things this replaced, both of which misled (#1169):

- `cargo test -p <crate>` — red by construction, per above.
- `make test-integration`'s old PostgreSQL line, which passed `-- --ignored`.
  None of these suites are `#[ignore]`d — they self-skip on an absent
  `DATABASE_URL` — and `cargo test -- --ignored` runs *only* ignored tests, so
  it executed **1 test out of 2828** across 77 binaries and then printed "All
  integration tests passed."

Making the suites genuinely parallel-safe would cut the wall-clock a lot and is
worth doing; it is a separate, much larger change and is not a prerequisite for
anything here.

## Do not reach for plain `cargo test` before pushing, either

`cargo test` and `cargo test --workspace --all-features` are both smaller than
what the `Dagger — test` leg runs, in ways that do not announce themselves. The
leg is a `cargo build --all-features`, one `cargo test --workspace --exclude …`
sweep, twenty-six feature-scoped invocations and a doctest run: several crates are excluded from the sweep and run on their own
lines, several run **twice** (once with a wide `--features` list, once on the
default set — the only configuration that compiles a `#[cfg(not(feature = …))]`
arm at all), and eighty-one `tests/*.rs` binaries are named explicitly
because `--lib` does not reach `tests/`.

So a green `cargo test` says nothing about those eighty-one binaries. That is how
`config_coverage_manifest_test` — one of the explicitly named ones — reached
`dev` on `231c3a25c` with `make preflight` green and all sixteen branch legs
green, and reddened `Dagger — test` afterwards (#1257).

Use **`make test-leg`**. It is the line-for-line mirror of the `Dagger — test`
shard, held to it in both directions by `make lint-shard-parity`.

It refuses to start if `DATABASE_URL`, `REDIS_URL`, `NATS_URL`, `VAULT_ADDR` or
their siblings are exported: the leg binds none of them, so a suite that
self-skips in CI would otherwise run here against a live service and assert
something else — passing or failing for a reason CI cannot reproduce.

`make preflight` does **not** run any of this. It compiles the tests
(`--all-targets`) and never executes them.

## Infrastructure Services

Started via `make db-up` (uses `docker/docker-compose.test.yml`):

| Service | Image | Port | Used By |
|---------|-------|------|---------|
| PostgreSQL | postgres:16-alpine | 5433 | Core DB tests, server, observers, federation (incl. `saga` integration tests) |
| Redis | redis:7-alpine | 6379 | APQ, caching, queue, rate limiting, PKCE |
| NATS | nats:2.10-alpine | 4222 | Observer transport, bridge |
| Vault | hashicorp/vault:1.17 | 8200 | Secrets manager integration |

The federation stack — Apollo Router plus three subgraphs — is stood up by the Dagger
`integration (federation)` leg, not from the Makefile. There used to be a `make
federation-up` here, naming `docker/docker-compose.federation.yml`; neither that file nor
the `docker/federation-ci/` one the Makefile named is in the repository, so both entry
points failed on their first line while this table said they worked (#1219).

## Feature Flags Requiring Infrastructure

### fraiseql-core

| Feature | Requires | Tests |
|---------|----------|-------|
| `test-postgres` | PostgreSQL | DB integration (self-skips without `DATABASE_URL`; **not** `#[ignore]`d) |
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
| `dagger-test.yml` | Workspace test suite, stable + MSRV toolchains — on every push to every in-repo branch (both arms are required checks, #1257); mirrored locally by `make test-leg` |
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
