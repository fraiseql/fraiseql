# Ignored / DB-Backed Tests in FraiseQL

This document explains how FraiseQL's database-backed and `#[ignore]`-marked
tests are run. It is a guide to the model, not an exhaustive catalogue — the
authoritative list of what runs where is `.dagger/main.go` (the integration
legs).

## Execution model (current)

Integration tests obtain their backing services through the
`fraiseql-test-support` env-URL harness (`fraiseql_test_support::postgres()`,
`redis()`, …). In CI, Dagger binds the service as a sidecar and injects its URL
as `DATABASE_URL` (and `TLS_DATABASE_URL`, `REDIS_URL`, etc.); locally, the same
harness reads those env vars, or spawns a throwaway container behind the
`fraiseql-test-support/local-testcontainers` feature.

Two patterns coexist:

- **Self-skipping / env-gated** — most DB-backed integration tests live in
  `crates/*/tests/*.rs`. They run in the relevant Dagger integration leg (which
  injects `DATABASE_URL`) and are simply not executed by the non-DB `test` leg
  (which runs `--lib` only). The harness `expect`s the URL, so they are *not*
  run in environments without it.
- **`#[ignore]`-gated** — tests that need a service the default integration leg
  does not provision (a TLS-configured Postgres, an Apollo Router container,
  manual fuzz/load runs) carry `#[ignore]` and are reached with
  `--include-ignored` / `-- --ignored` in their dedicated leg or on demand.

There is **no** separate GitHub-Actions "Ignored Tests" job — that pre-Dagger
model is gone. Integration coverage is the Dagger `integration*` legs.

## Categories that still carry `#[ignore]`

### TLS integration (`crates/fraiseql-wire/tests/tls_integration.rs`)
Needs a Postgres configured for TLS. CI runs it in the Dagger wire leg with a
SCRAM/TLS-bound Postgres; the harness falls back from `TLS_DATABASE_URL` to
`DATABASE_URL`.

```bash
TLS_DATABASE_URL="postgres://user:pass@localhost/db" \
  cargo test -p fraiseql-wire --test tls_integration -- --ignored
```

### Manual / on-demand DB suites
Some `crates/*/src/**` unit modules and `crates/*/tests/*.rs` suites are
`#[ignore]`-marked because they are slow, need extra extensions, or are manual
load/fuzz checks. Each carries an in-file justification next to the attribute;
run them with `-- --ignored` against a `DATABASE_URL`-pointed database.

## Running DB-backed tests locally

Point the harness at any reachable Postgres and run the integration tests for a
crate (they self-skip the rest):

```bash
export DATABASE_URL="postgresql://user:pass@localhost:5432/test_fraiseql"
cargo test -p fraiseql-core --features test-postgres --test '*' -- --test-threads=1
```

Or let the harness spawn containers:

```bash
cargo test -p fraiseql-core \
  --features 'test-postgres,fraiseql-test-support/local-testcontainers' --test '*'
```

To include `#[ignore]`-marked tests, add `-- --include-ignored` (nextest) or
`-- --ignored` (libtest).

## Adding a new `#[ignore]` test

1. Mark it `#[ignore = "<reason>"]` with the reason inline (not just `#[ignore]`).
2. Obtain the service URL through `fraiseql_test_support`, not a hardcoded URL.
3. If it needs a service no integration leg provisions yet, wire that service in
   `.dagger/main.go` so it actually runs somewhere.

## Environment variables

| Variable | Used by | Example |
|----------|---------|---------|
| `DATABASE_URL` | the `fraiseql-test-support` harness (Postgres) | `postgresql://user:pass@host:5432/db` |
| `TLS_DATABASE_URL` | wire TLS tests (falls back to `DATABASE_URL`) | `postgresql://user:pass@host:5432/db` |
| `REDIS_URL` | Redis-backed APQ / observer queue tests | `redis://host:6379` |
| `RUST_LOG` | all tests (optional) | `debug`, `info` |
