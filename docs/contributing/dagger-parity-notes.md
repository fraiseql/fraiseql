<!-- Published from `.phases/dagger-adoption/parity-notes.md` (#1210). -->

# Dagger parity notes

This is the record of what changed when this repository's CI moved from
GitHub-hosted YAML jobs to Dagger functions: what was ported, what was
deliberately left behind, and why each gate is shaped the way it is.

**It is published because `.dagger/*.go` defers to it.** Sixteen comments across
`.dagger/main.go`, `.dagger/security.go`, `.dagger/feature-combos.go`, three
workflow files and two crates say "see parity-notes.md" for the reasoning behind
a decision. Until #1210 that file lived only in the working copy that wrote it —
`.phases/` is gitignored — so a contributor in a fresh clone got the conclusion
with the argument amputated. `docker-build.yml` was the concrete cost: its header
explained that image builds were tag-only "until Dagger Ph06 reproduces
image-build" and pointed at a plan nobody could read, and two jobs that could
never run sat under that comment for three months (#1206).

The rest of the `.phases/dagger-adoption/` plan is **not** published. It is a
migration plan that has been executed; this file is the part that is still load
bearing, because it is the part other files cite.

⚠ It is a historical record, written as the migration happened. Statements in it
are true as of their dated entry, not necessarily today — the authority for what
CI does now is `.dagger/` and `.github/workflows/`. Where the two disagree, the
code is right and this file is the reason the code looks like that.

---

Running record of differences between legacy GitHub-hosted YAML jobs and their Dagger replacements. Populate as Phases 01-08 progress.

## Format

For each ported job, record:

- **Legacy job name** (from `.github/workflows/`)
- **Dagger function** (e.g. `dagger call preflight`)
- **First 3 PRs** of parallel-run validation (PR #, run IDs, outcome)
- **Documented differences** (intentional or accepted)
- **Wall time** before vs after, warm + cold cache

## Phase 01 — Foundation

| Legacy job | Dagger fn | Validation | Notes |
|---|---|---|---|
| `axum-route-syntax-check` (ci.yml, `bash tools/check-route-syntax.sh` on ubuntu-latest) | `dagger call lint-routes` | local `dagger call` + 1 self-hosted **push/dispatch** run (Day-0: no 3-PR strangler race; public repo → push-only) | runs the script verbatim in a pinned Ubuntu container; `git init` to satisfy `cd $(git rev-parse --show-toplevel)` |

**Intended behaviour vs old YAML** (read from `ci.yml` + `tools/check-route-syntax.sh`,
not raced): both fail iff a `:param` axum-0.7 capture exists in `crates/` or `examples/`,
single-line (grep) or multi-line (awk) form. Exit non-zero on violation, prints the same
ERROR + migration hint, exit 0 + `OK: ...` otherwise.

**Accepted divergences (Phase 01):**
- **gawk, not mawk.** Ubuntu's default `awk` is mawk, which does not honour `\s` — the
  char class the load-bearing multi-line pass relies on. The Dagger container installs
  `gawk`, so the multi-line gate actually fires. (If ubuntu-latest's mawk silently skipped
  that pass, the Dagger port is *stricter/correct*, not a regression.)
- **`lint-routes-selftest` instead of a static `testdata/bad-route/` tree.** The plan's
  standalone fixture dir can't work with the verbatim script (no `tools/` + single git
  toplevel). The selftest overlays a synthetic multi-line `:param` file via `WithNewFile`
  and asserts non-zero — proves the gate fires without duplicating the script.
- **No strangler-fig parallel run.** Day-0 stripped the hosted `axum-route-syntax-check`
  trigger, so there is no legacy job to race for 3 PRs. Verified by local `dagger call`
  + one self-hosted push/dispatch run.
- **Push-only triggers (public repo).** `fraiseql/fraiseql` is public; a self-hosted
  runner must never run fork `pull_request` code, so `dagger-lint-routes.yml` (and all
  future Dagger workflows) trigger on `push` + `workflow_dispatch` only — never
  `pull_request`/`pull_request_target`.
- **`+ignore` directive on the `source` param (not server-side `WithoutDirectory`).** The
  local working copy carries a 277 GB / ~450k-file `target/`; Dagger's context upload walks
  it before any server-side `WithoutDirectory("target")` can run, so `dagger init` raced a
  vanishing build artifact and crashed, and every `dagger call --source=.` would re-walk
  450k files. Fix: a client-side `// +ignore=["target", "**/target", ".git"]` directive on
  the `LintRoutes`/`LintRoutesSelftest` `source` parameters prunes those paths at upload
  time. The script reads neither (`crates/`+`examples/`, `*.rs`, excludes `*/target/*`), so
  this is a strict parity improvement — it makes a local call behave like the legacy job's
  clean checkout. (`dagger init`/`develop`, which sync the module context rather than the
  `source` param, were run with `target/` parked aside via an instant same-device rename,
  then restored; the self-hosted runner does a fresh `actions/checkout` and never has a
  `target/`, so CI is unaffected either way.)

**Status (2026-06-01): local GREEN.** Engine boots (kernel rebooted to 7.0.9). Module
scaffolded (`dagger init --sdk=go --source=.dagger --name=fraiseql-ci`; `go.mod` module
`dagger/fraiseql-ci`, receiver `FraiseqlCi` — matched the draft, no import/type reconcile).
`dagger functions` lists `lint-routes` + `lint-routes-selftest`. Verified locally:

| Function | Result | Wall (cold base / cached) |
|---|---|---|
| `dagger call lint-routes --source=.` | exit 0 — `OK: no axum 0.7-style :param captures in crates/ or examples/.` | 18.5s cold apt / ~2s cached |
| `dagger call lint-routes-selftest --source=.` | exit 0 — `lint-routes selftest OK: injected :param route was correctly flagged` | 1.4s |

Re-ran `lint-routes` with the full 277 GB `target/` restored: exit 0, CACHED (the `+ignore`
exclude made the source hash identical to the target-parked run) — confirms the directive
works under real local conditions.

**Self-hosted run (remote GREEN):** push of `86fbc368e` → `dev` triggered
`dagger-lint-routes.yml` run **26770879052** on `fraiseql-8core` — conclusion **success**,
`dagger v0.21.3`, `OK: no axum 0.7-style :param captures in crates/ or examples/.` The
`dagger call` step ran in ~5s (engine + `ubuntu:24.04` + apt layers warm: the runner shares
this box's Docker/Dagger engine). Day-0 (`3e32421f2`) landed in the same push, so the only
billable-class workflow that fired was this self-hosted gate (~$0); the "Graph Update:
go_modules in /.dagger" run is GitHub's free dependency-graph, not an Actions workflow.

## Phase 02 — Fast Gates

Ports ci.yml's cheap-but-frequent gates into `.dagger` functions + a `preflight`
umbrella, backed by `.github/workflows/dagger-preflight.yml` (self-hosted,
push(dev)+dispatch only).

| Legacy ci.yml job/step | Dagger fn | Notes |
|---|---|---|
| `fmt` — `cargo +nightly fmt --all -- --check` | `dagger call fmt` | nightly rustfmt installed in rustBase (toolchain file pins stable to MSRV 1.92) |
| `clippy` — `cargo clippy --workspace --all-features --all-targets -- -D warnings` | `dagger call clippy` | the full-compile pole; uses cache volumes |
| `docs` — `RUSTDOCFLAGS=-D warnings cargo doc --workspace --all-features --no-deps` | `dagger call rustdoc` | |
| `clippy` step `make lint-tests-layout` | `dagger call shell-gates` | grep/wc only — no Rust toolchain |
| `clippy` step `make lint-unwrap UNWRAP_ALLOW_LIMIT=3` | `shell-gates` | limit threaded as a module const |
| `clippy` step `make lint-expect` | `shell-gates` | |
| `clippy` step `make lint-async-trait` | `shell-gates` | ASYNC_TRAIT_LIMIT=180 (Makefile default) |
| `clippy` step `make lint-gate-db` | `shell-gates` | |
| `clippy` step `make lint-gate-core` | `shell-gates` | |
| `check-test-imports` — `bash tools/check-test-imports.sh` | `shell-gates` | |
| `axum-route-syntax-check` — `bash tools/check-route-syntax.sh` | `shell-gates` (+ standalone `lint-routes`, Phase 01) | run once inside shell-gates; the Phase-01 `lint-routes` fn is now redundant with preflight |

**rustBase design (Cycle 1):** `rust:1.92` (non-slim → gcc/perl/curl/git present) +
apt `mold clang pkg-config libssl-dev cmake protobuf-compiler python3 libsasl2-dev
zlib1g-dev` (native deps for the `--all-features` compile: openssl→tiberius,
cmake/sasl/zlib→rdkafka, protoc→tonic, python3→deno_core/v8). Components: clippy +
rustfmt + rust-analyzer on stable (matches rust-toolchain.toml), plus a minimal
nightly with only rustfmt. sccache (prebuilt, pinned) as `RUSTC_WRAPPER`.
Env: `CARGO_INCREMENTAL=0` (sccache requirement), `CARGO_BUILD_JOBS=16` (mirrors
.cargo/config.toml's 31 GiB-RAM cap), `RUSTFLAGS=-C linker=clang -C
link-arg=-fuse-ld=mold`. Cache volumes: `fraiseql-cargo-registry`,
`fraiseql-cargo-git`, `fraiseql-rust-target`, `fraiseql-sccache`.

**Accepted divergences (Phase 02):**
- **mold/clang linking is ON** in the Dagger container, where the committed
  `.cargo/config.toml` keeps it OFF (the comment explains: GitHub-hosted runners lack
  mold). The self-hosted Dagger image ships mold, so we enable it via `RUSTFLAGS` env
  (env overrides config; the config block stays commented, untouched). Faster linking,
  same lint result.
- **sccache + persistent cache volumes** replace the GitHub `sccache-action` (GHA
  cache backend) and `actions/cache`. The runner shares the engine's cache volumes
  with local `dagger call` runs, so a `dagger call clippy` locally warms the same
  cache the CI preflight run reuses.
- **`shell-gates` bundles the grep/wc policy checks + check-test-imports +
  route-syntax** into one minimal (no-toolchain) container, instead of being steps of
  the clippy job. The Phase-01 `lint-routes` fn is now subsumed by preflight's
  shell-gates (kept for now; candidate to retire).
- **No strangler-fig** (Day-0 killed legacy ci.yml) — verified by local `dagger call`
  + one self-hosted push run, not a 3-PR race.
- **Out of scope for this phase (still only on dispatch-only ci.yml, no PR guard):**
  `example-async-jobs-subgraph` (detached-crate clippy), `security` (cargo-deny →
  Phase 06), `dependency-audit`, semver-checks. Flagged so the coverage gap is visible.

**Status (2026-06-01): local GREEN.** All gates verified locally via `dagger call`:

| Function | Result | Wall (this box, i7-13700K/jobs=16) |
|---|---|---|
| `shell-gates` | exit 0 (tests-layout, unwrap 1/3, expect, async_trait 180≤180, db 4/40, core 14/20, test-imports, route-syntax all OK) | ~14s |
| `fmt` | exit 0 (`cargo +nightly fmt --all -- --check` clean) | ~42s incl. cold rustBase build |
| `clippy` | exit 0 (`--workspace --all-features --all-targets -- -D warnings`, no warnings); **all native deps resolved** — no missing-lib failure | 2m13s cold full compile |
| `rustdoc` | exit 0 (no doc warnings) | (fresh compile inside preflight) |
| `preflight` | exit 0 — `preflight OK: all fast gates passed` | 57.2s (rustdoc fresh, rest cached) |

`target/` parked aside (instant same-device rename) for `dagger develop`/`functions`
only; the gates compile into the `fraiseql-rust-target` cache volume, not the host
tree, so parking doesn't affect them. Cold clippy was fast (2m13s) thanks to jobs=16
+ sparse registry + the v8 prebuilt download (not source build).

**Self-hosted run (remote GREEN):** push of `ed76badf4` → `dev` triggered
`dagger-preflight.yml` run **26772144183** on `fraiseql-8core` — conclusion **success**;
log: shell-gates → fmt → rustdoc → clippy → `preflight OK: all fast gates passed`. Ran
**warm** off the cache volumes the local runs populated. The standalone
`dagger-lint-routes.yml` was retired in the same push (route-syntax now via preflight's
shell-gates). Benign annotation: an `actions/checkout` post-cleanup `.gitconfig` lock
warning on the self-hosted runner (job exit 0; route-syntax ran fine) — watch, non-blocking.

## Phase 03 — Test Suite

Ports ci.yml's `test` job (Linux path) into `dagger call test --rust=<stable|msrv>`,
backed by `.github/workflows/dagger-test.yml` (self-hosted matrix, push(dev)+dispatch).

| Legacy ci.yml `test` step (non-Windows) | Dagger `test` equivalent | Notes |
|---|---|---|
| `cargo build --all-features --verbose` | same | full-codegen compile of every crate/feature |
| `cargo test --workspace --exclude {core,db,arrow,observers,server} --all-features` | same **+ `--exclude fraiseql-wire --exclude fraiseql-functions`** + `-- --skip metadata::tests --skip migrations::tests --skip routes::tests` | testcontainers tests skipped; wire+functions run separately (see below) |
| _(wire was inside the workspace step)_ | `cargo test -p fraiseql-wire --lib --all-features` | wire's `tests/*` are all testcontainers → run lib unit tests only |
| _(functions was inside the workspace step, `--all-features`)_ | `cargo test -p fraiseql-functions --features "runtime-wasm,host-live,host-storage" -- --skip migrations::tests` | all features **except runtime-deno** (v8 crash, see below); migrations::tests skipped (testcontainers) |
| `cargo test -p fraiseql-core --features "<SYNC:CORE_FEATURES>"` | same | no `test-postgres` → infra-free |
| `cargo test -p fraiseql-db --features "<SYNC:DB_FEATURES>"` | same | infra-free |
| `cargo test -p fraiseql-server --lib --features "<SYNC:SERVER_FEATURES>"` | same | `--lib`, infra-free |
| `cargo test --doc --all-features --verbose` | same | |
| `RUSTDOCFLAGS=-D warnings cargo doc --workspace --all-features --no-deps` | _omitted_ | already the Phase-02 `rustdoc` gate (preflight) — not duplicated |

**Toolchain (Cycle 3):** `--rust=msrv` (default) = `RUSTUP_TOOLCHAIN=1.92` (the base
image's toolchain == rust-toolchain.toml pin == Cargo.toml `rust-version`); `--rust=stable`
installs latest stable and sets `RUSTUP_TOOLCHAIN=stable` (env overrides the toolchain
file). Separate target cache volume per toolchain (`fraiseql-rust-target-test-1-92`,
`fraiseql-rust-target-test-stable`) — incompatible artifacts must not share a target dir.

**Accepted divergences (Phase 03):**
- **✓ RESTORED in Phase 04 (Increment 4).** Every testcontainers test below now runs as a
  real enforcing gate against a Dagger-bound service via the harness (storage→`storage`
  suite, functions→`postgres` suite, wire→`wire` suite). The skip below was Phase-03-only.
- **Testcontainers tests skipped (interim — Phase 04 restores).** The Dagger engine has
  no Docker socket, so tests that boot their own Postgres via testcontainers can't run.
  Affected (all non-`#[ignore]`d, so they DO run on the bare-metal runner's Docker in
  legacy CI): `fraiseql-storage` `metadata::tests`/`migrations::tests`/`routes::tests`
  (lib), `fraiseql-functions` `migrations::tests` (lib), and **all** `fraiseql-wire`
  `tests/*` integration binaries (testcontainer_auth, load_tests, typed_streaming,
  stress_tests, integration_operators — via `tests/common`). Skipped via `--skip` +
  running wire `--lib` only; the run logs an explicit `### skipped …` line. They fail
  *cleanly* in-engine (no Docker = no container leak), and Phase 04 wires Dagger-native
  Postgres so they run again. Coverage gap is logged, not silent.
- **runtime-deno (v8) tests excluded (interim).** `fraiseql-functions` has 23 deno
  tests (+ `observer::tests::test_function_observer_dispatches_ts_to_deno`) that each
  boot an embedded V8 isolate (`JsRuntime::new`). Embedded V8 **SIGSEGVs inside the
  Dagger exec sandbox even single-threaded** (`signal: 11`) — it works on the bare-metal
  runner. All v8 tests are `#[cfg(feature = "runtime-deno")]`, so functions runs with
  `--features "runtime-wasm,host-live,host-storage"` (all-features-minus-deno) — the v8
  tests cfg out cleanly with no compile error, and the `cargo build --all-features` step
  still compiles the deno path (so deno code is type-checked). Follow-up: run the deno
  tests on the bare-metal host runner, or under a relaxed exec sandbox
  (`InsecureRootCapabilities`), in a later phase. Logged, not silent.
  - Same constraint for the fraiseql-server V8 tests: the `sources` suite skips
    `fires_a_model_b_connector_end_to_end` by name, and the P30 #573 gate
    `ingests_across_schedule_windows_and_a_restart` is additionally `#[ignore]`d
    (real V8 guest + real multi-minute schedule windows, ~5 min wall clock). Run it
    locally with `DATABASE_URL` set:
    `cargo test -p fraiseql-server --features sources --lib
     ingests_across_schedule_windows_and_a_restart -- --ignored --test-threads=1`.
    Its CI-runnable halves: the #796 window simulator + cross-restart cron-state
    guard (functions leg) and the cursor round-trip (sources leg).
- **mold/sccache/cache-volumes** as in Phase 02 (shared `rustBase`).
- **No strangler-fig** (Day-0) — verify via local `dagger call` + self-hosted run.

**Status (2026-06-01): `--rust=msrv` local GREEN** (4 runs to converge — the legacy
`test` job leaned on the runner's Docker far more than the plan assumed). Final run:
`dagger call test --rust=msrv --source=.` → exit 0, `test OK: workspace suite passed
(toolchain 1.92, testcontainers tests skipped)`, all 8 steps green (build → workspace
→ wire --lib → functions sans-deno → core --lib → db --lib → server --lib → doctests).
`--rust=stable` (rustc 1.96) verified GREEN too (own `fraiseql-rust-target-test-stable`
cache). Note: the stable *cold* 40-min run hit a dagger return-value timeout
(`client session attachables: context deadline exceeded` + telemetry backlog) AFTER the
suite passed — infra, not code; hardened by dropping `--verbose`. `target/` parked for
develop/functions; gates compile into the cache volume.

**Self-hosted run (remote GREEN):** push of `00d567f40` → `dev` triggered
`dagger-test.yml` run **26781616484** on `fraiseql-8core` — **run=success**,
`test (stable): success` + `test (msrv): success`, warm (no timeout). `dagger-preflight`
(run 26781616071) also success on the same push.

Diagnosis trail (each interim failure fixed forward): (1) functions SIGSEGV — v8
multi-isolate? no — v8 crashes in the exec sandbox even single-threaded → exclude
runtime-deno; (2) core SIGSEGV/panics at tests/common/testcontainer.rs — core tests/*
boot Postgres → core/db scoped to --lib (integration → Phase 04). storage/functions/wire
testcontainer tests handled as above.

## Phase 04 — Integration Matrix

**Architecture (user-directed 2026-06-02): converge on ONE provisioning model.** Dagger is
the sole service orchestrator (local==CI); tests are pure env-URL readers routed through a
single dependency-light harness crate `fraiseql-test-support`. NO DinD. Tier-B hardcoded
consts and Tier-C in-test `Postgres::default().start()` are DELETED (not preserved as
fallbacks) so each increment lands in final form. testcontainers ends up in exactly one
crate behind one feature (`local-testcontainers`); CI never enables it → leak-proof by
construction. Sequence: harness + Dagger service defs + Tier-A/B first (clear the gate),
then Tier-C crate-by-crate (rows below). Per-test DB isolation is explicitly out of scope.

### Increment 1 — `fraiseql-test-support` harness crate (DONE 2026-06-02, local GREEN)

New crate `crates/fraiseql-test-support` (publish=false, dependency-light: only optional
`testcontainers`/`testcontainers-modules` behind `local-testcontainers`; dev-dep tokio for
the doctest). Single provisioning policy: read env URL → use it; unset + feature → spawn
local container; else `None` (caller skips). API: `postgres()/mysql()/redis()/sqlserver()/
nats()` → `Option<Service>` (async, spawnable family), `vault()` → `Option<Vault>` (sync,
env-only). Canonical env vars: `DATABASE_URL`/`MYSQL_URL`/`REDIS_URL`/`SQLSERVER_URL`/
`NATS_URL`/`VAULT_ADDR`+`VAULT_TOKEN`. `fraiseql-test-utils::db` now re-exports
`database_url`/`try_database_url` from here (one policy location; its 23 callers unchanged;
old `db/tests.rs` deleted, canonical `normalize` tests live in test-support).

Local spawn implemented for **postgres only** so far (the dominant inner-loop service);
mysql/redis/sqlserver/nats are env-only until their Tier-C slice wires a spawn path (logged,
not silent). Verified: `cargo clippy -p fraiseql-test-support --all-targets --all-features
-- -D warnings` clean; 4 unit + 1 doctest pass; `cargo check -p fraiseql-test-utils` clean.

### Increment 2 — Dagger service binding + first Tier-B migration (DONE 2026-06-02, local GREEN)

`.dagger/main.go` adds `TestIntegration(--suite=…)` + `pgService()` (`postgres:16`, env creds,
fixtures `tests/sql/postgres/{init,init-analytics}.sql` mounted as `00-`/`01-` in
`/docker-entrypoint-initdb.d` for load order, `WithExposedPort(5432)`, `AsService()`) +
`integrationBase()` (rustBaseFor + dedicated `fraiseql-rust-target-integration-<tc>` cache +
RUST_LOG=debug). `integrationPostgres` binds the service as alias `postgres`, injects
`DATABASE_URL=postgresql://fraiseql_test:…@postgres:5432/test_fraiseql`.

Tier-B migration: `aggregation_integration.rs` hardcoded `localhost:5433` const DELETED →
`fraiseql_test_support::postgres()` (skip-on-None, holds the `Service` guard for the test
lifetime). `fraiseql-core` dev-deps `fraiseql-test-support` (no cycle: harness has no
workspace deps).

**`dagger call test-integration --suite=postgres` → exit 0, GREEN.** The DB-backed
`test_end_to_end_aggregate_query` connected to the bound `postgres:5432`, queried `tf_sales`
(from the initdb fixtures), passed (7 passed / 41 filtered / 0 failed). **37s total / 19s
cargo** — fresh integration target volume but the shared `fraiseql-sccache` volume served the
compile; mold linked fast. Dagger's exposed-port wait handled readiness (no pg_isready loop
needed). Validates the whole architecture end-to-end: env-URL harness → Dagger-bound seeded
service, local==CI, no testcontainers, no DinD.

### Increment 3a — fact_table migration + sqlite suite (DONE 2026-06-02, local GREEN)

`fact_table_integration.rs` (10 tests) migrated off its `localhost:5433` const → harness
(`create_test_introspector` now returns `Option<(Service, PostgresIntrospector)>`; each test
let-else-skips and holds the Service guard). `sqlite` suite added (`integrationSqlite`,
in-process `SqliteAdapter::in_memory`, no service). **`dagger call test-integration
--suite=postgres` → 17 passed / 0 failed** (7 aggregation + 10 fact_table) in 19.7s warm;
**`--suite=sqlite` → 9 passed / 0 failed** (~42s incl. one-time sqlite-feature compile in the
shared integration cache). Note: legacy ran fact_table/aggregation only via the broad
`--test '*'` step (the dedicated `--ignored` step was a no-op — 0 ignored tests); Dagger runs
them as a named, real suite.

| Legacy job | Dagger suite | Tier | Status |
|---|---|---|---|
| (harness foundation) | — | — | Inc 1 DONE (local GREEN) |
| integration-postgres (aggregation + fact_table) | `--suite=postgres` | B | Inc 2+3a DONE (17 passed) |
| integration-sqlite | `--suite=sqlite` | A | Inc 3a DONE (9 passed) |
| integration-postgres (broad core/db `--test '*'` sweep) | `--suite=postgres` | C | Inc 4 |
| integration-vault | `--suite=vault` | A | Inc 3 (next) — vaultService(hashicorp/vault:1.17 dev) + VAULT_ADDR/VAULT_TOKEN; tests `#[ignore="requires vault"]` → `--ignored` |
| integration-server | `--suite=server` | A | Inc 3 (next) — pgService; database_query_test uses try_database_url() skip-on-None (NO `--ignored`) |
| integration-redis | `--suite=redis` | A | Inc 3 — redisService+pgService; core apq + observers queue/lease lib `#[ignore]` → `--ignored` |
| integration-mysql | `--suite=mysql` | A | Inc 3 — mysqlService + init.sql; procedures.sql needs DELIMITER wrap; tests read MYSQL_URL |
| integration-nats | `--suite=nats` | B | Inc 3 — natsService + migrate hardcoded nats://localhost:4222 → NATS_URL |
| integration-sqlserver | `--suite=sqlserver` | B | Inc 3 — sqlserverService (mssql 2022, heavy/continue-on-error) + migrate SQLSERVER_URL const |
| integration-observers | `--suite=observers` | A | Inc 3 — pg+redis+nats multi-bind |
| integration-http-e2e | `--suite=http-e2e` | A | Inc 3 — build+run server binary as a bound service |
| integration-tls | `--suite=tls` | A | Inc 3 — TLS pg + pre-generated CA cert injected to test ctr (SAN = bind alias) |
| integration-cross-db | `--suite=cross-db` | C | Inc 4 (testcontainers → harness) |

### Increment 3b — server / vault / redis / mysql suites (2026-06-02)

Service factories added: `redisService` (redis:7-alpine), `vaultService` (hashicorp/vault:1.17,
`AsService` Args `server -dev`, dev-token), `mysqlService` (mysql:8.3, initdb 00-init.sql +
01-procedures.sql DELIMITER-wrapped). New suites: server, redis, vault, mysql.

- **server** — GREEN, **11 passed**. Tier-A (`try_database_url` skip-on-None). Surfaced + fixed a
  pre-existing flake: `test_pool_size_limits` asserted `num_idle()>=1` after a single `yield_now()`
  (sqlx returns conns async) → replaced with a bounded poll. (This test never ran in legacy CI: the
  `integration-server` job used `--ignored` but the tests aren't `#[ignore]d` → 0 ran.)
- **vault** — GREEN, **2 passed**. Tier-B migration: 2 `#[ignore]` tests' hardcoded
  `127.0.0.1:8200` → harness `vault()` (addr+token), skip-on-None. (Config-error tests
  `namespace_configuration`/`tls_verification_disabled` keep a deliberately-unreachable addr —
  they assert get_secret errors; not service-backed, left as-is.)
- **redis** — GREEN, **4 + 6 passed** (core APQ + observers queue). Core APQ already Tier-A
  (`REDIS_URL`). Observers Tier-B migration: `queue/tests::setup_test_queue` hardcoded
  `redis://localhost:6379` → harness `redis()` (1 helper, 6 call sites skip-on-None). listener/
  lease tests already Tier-A (env+skip).
- **mysql** — Tier-A tests read `MYSQL_URL`. First run: 28 passed / 2 failed → fixed: (a) seed
  `procedures.sql` (DELIMITER-wrapped) for `fn_create_tag` (mutation test); (b) the missing-view
  error test inline-hardcoded `localhost:3307` (bypassed env) → read `MYSQL_URL`. Also deleted the
  4 `mysql_url()` helpers' `localhost:3307` fallbacks → `env::var("MYSQL_URL").expect(...)`
  (env-only, no const). Re-running. NOTE: mysql tests read the canonical `MYSQL_URL` via `env::var`
  rather than the harness fn (no spawn/skip needed for these always-require-mysql tests) — a minor
  consistency cleanup, not a service-bypass.

- **nats** — GREEN, **8 passed**. Service `nats:2.10-alpine` `-js -m 8222` (AsService Args). Tier-B
  migration: the `#[ignore]` tests built `NatsConfig { .., ..Default::default() }` (url default
  `nats://localhost:4222`); added `with_nats_url(config)` (overrides url from `NATS_URL`) and wrapped
  the `.expect("Should (re)connect…")` connect lines. The non-ignored `test_nats_connection_error`
  keeps its deliberately-bad url. No new dev-dep (sync `env::var`).

**7 suites green: postgres (17) · sqlite (9) · server (11) · vault (2) · redis (4+6) · mysql (30) ·
nats (8).** Remaining Inc-3 suites: sqlserver (heavy mssql, Tier-B const), observers (pg+redis+nats
multi-bind), http-e2e (server binary as bound svc), tls (pre-gen CA cert). Then Inc4 (Tier-C) + Inc5
(`dagger-integration.yml`).

**Process lesson (logged):** do NOT edit `.dagger/main.go` while a `dagger call` is in flight — a
mid-edit snapshot caused a transient `integrationMysql undefined` Go build error on one redis run
(re-run was clean). Rust-test edits during a run are safe (the source is snapshotted at call start).

### Increment 4 (Tier-C) + Increment 5 (matrix) — COMPLETE (2026-06-02, all suites local GREEN)

**Zero testcontainers leak.** `grep -rl '^testcontainers ' crates/*/Cargo.toml | grep -v test-support`
is EMPTY. testcontainers lives in exactly one crate (`fraiseql-test-support`) behind one feature
(`local-testcontainers`) that CI never enables → the container-leak class cannot compile into a CI
artifact. All 7 crates with a real dev-dep migrated: core, db, functions, wire, storage, server.

**16 suites, all enforcing (no continue-on-error, no best-effort, no deferred):**

| Suite | Backing services | Result |
|---|---|---|
| postgres | postgres:16 | broad `core --test '*'` + `db --test '*'` + functions migrations — 178 test bins ok |
| sqlite | (in-process) | 9 |
| mysql | mysql:8.3 | 30 |
| sqlserver | mssql:2022 | 27 (init.sql via sqlcmd behind readiness loop) |
| cross-db | postgres + mysql | 5 (FEDERATION_TESTS=1; legacy left it unset → was a no-op) |
| redis | redis:7 + postgres | 4 + 6 |
| nats | nats:2.10 | 8 |
| observers | postgres + redis + nats | notify4 + bridge7 + runtime14 |
| tls | postgres-tls (pre-gen CA) | 7 |
| vault | hashicorp/vault:1.17 | 2 |
| server | postgres | database 11 + usage 5 + observer 8 + pipeline 1 |
| server-storage | minio | 4 (S3 round-trip) |
| http-e2e | server binary + e2e postgres | 27 + 22 |
| wire | scram postgres | 18 binaries |
| storage | postgres + azurite + fake-gcs | lib 25 + azure 1 + gcs 1 |
| federation | 2 subgraph servers + Apollo Router + 2 postgres | 15 (incl. router routing + cross-subgraph) |

**Production bugs fixed (all surfaced by making never-run tests actually run):**
- `fraiseql-db` `row_to_map_test`: enum INSERT/SELECT casts + NULL-as-Option (was `--no-run`-only).
- `fraiseql-server` `observer_test_helpers`: `tb_observer_log.entity_id` VARCHAR→UUID.
- `fraiseql-server` `s3::exists()`: typed `HeadObjectError::is_not_found` (Display is "service error").
- `fraiseql-federation` `database_resolver`: quote the entity table (`FROM "user"`, reserved word).
- `fraiseql-federation` `selection_parser`: strip `... on Type` inline-fragment tokens from the SELECT.
- `fraiseql-core` cross_database_test: split multi-statement MySQL schema for `sqlx::query`.
- `fraiseql-core` federation mock: strip quotes from the parsed table name (matches the resolver).

**Accepted divergences (Increment 4/5):**
- **Per-test isolation = TRUNCATE/DROP + `--test-threads=1` on a shared bound DB**, NOT per-test
  ephemeral databases (explicitly out of scope). Each suite's pg/mysql service is fresh per `dagger
  call`, so cross-run state never accumulates.
- **Federation uses a dedicated target cache volume** (`fraiseql-rust-target-fed-1-92`) for its
  `--features federation` build; the rest share `fraiseql-rust-target-integ2-*` (bumped from
  `-integration-*` to flush a stale federation-resolver artifact — Dagger normalizes source mtimes,
  so a warm cache does not reliably recompile an edited dependency crate; documented in the memory).
- **Cross-db parity is now enforcing** (FEDERATION_TESTS=1); the legacy job left that unset, so its
  parity assertions never executed.
- **No strangler-fig** (Day-0 killed legacy ci.yml) — validated by per-suite local `dagger call`.

### sqlserver — self-initializing service (2026-06-03 fix)

The sqlserver suite regressed to RED on dev (`fe49d6916` push) AFTER the 2026-06-02 disk migration
(its first run since). Symptom: a deterministic `21×30s ≈ 630s` wall of `bb8: Timed out` panics; 6
master/error-path tests passed, the 21 needing app databases failed. Isolation proved init.sql + mssql
are BOTH fine standalone (applied cleanly to a plain `docker run` of the same pinned image; mssql ran 90s
idle, no OOM, swap active). The original design applied init.sql from a SEPARATE init container bound to
the same `*dagger.Service` object as the test container — and **Dagger does not guarantee that a second
container binding the same service reuses the first's running instance**, so the test container connected
to an UNINITIALIZED mssql (no databases); bb8 retried each connect to its 30s timeout. An explicit
`svc.Start(ctx)` did NOT fix it.

**Fix (divergence from legacy + from the other suites): make the service self-initializing.**
`sqlserverService` bakes init.sql into the service's own startup via `AsService(Args: ["bash","-c", …])`
— launch `sqlservr`, wait for it to accept connections, apply `/init.sql`, then `wait` on sqlservr to hold
it in the foreground. Every instance is therefore initialized regardless of Dagger's service-instance
lifecycle (init.sql is idempotent, so a re-applied instance is harmless). `integrationSQLServer` then
polls a `dbo.init_done` sentinel (written last by init.sql) as a readiness gate before cargo test, and
holds one instance via `.Start()`. The two near-identically named test databases (`test_fraiseql` +
`fraiseql_test`) were consolidated into one `fraiseql_test` (the split was incidental — only `v_user`
lived in `test_fraiseql`); `multi_database_integration.rs`'s one `sqlserver_conn("test_fraiseql")` call
site moved to `fraiseql_test`. Local `dagger call test-integration --suite=sqlserver` → 27 passed / 0
failed in 0.59s (was 630s). **General lesson: for a stateful Dagger service consumed by more than one
container, make it self-initializing — do NOT rely on a separate init container + service-binding reuse.**

## Phase 05 — Feature Matrix

Ports `feature-flags.yml`'s `cargo check --features …` matrix into ONE parameterized
Dagger fn (`feature-check --combo=X`) + an aggregate `feature-matrix`, with the combo
list as typed Go data in `.dagger/feature-combos.go`. Backed by
`.github/workflows/dagger-feature-matrix.yml` (self-hosted, push(dev)+dispatch). The
workflow's `check` matrix is GENERATED from the Go list via `dagger call list-combos`
→ `fromJSON`, so adding a combo is a one-line Go struct literal — no YAML edit.

**Why this phase is rate-limit-immune:** every combo is `cargo check` (or `cargo
clippy` for functions) — no test binaries, so NO backing services. It only ever pulls
the already-cached `rust:1.92` base, unaffected by the Docker Hub anonymous pull
rate-limit that gates the integration suites.

| Legacy `feature-flags.yml` job | Dagger combos | Command shape |
|---|---|---|
| `feature-matrix` (fraiseql-server, 17) | `server-*` | `cargo check -p fraiseql-server [--no-default-features] [--features …]` — crate defaults stay ON except the explicit `server-no-default` case |
| `database-matrix` (fraiseql-core, 7) | `core-*` | `cargo check -p fraiseql-core --no-default-features --features …` |
| `storage-matrix` (fraiseql-storage, 3) | `storage-*` | `cargo check -p fraiseql-storage --no-default-features --features …` |
| `functions-matrix` (fraiseql-functions, 5) | `functions-*` | `cargo clippy -p fraiseql-functions --no-default-features --features … --all-targets -- -D warnings` |

32 combos total. Names are crate-prefixed so they're unique + legible as GH status
rows (e.g. `server-gcs` vs `storage-gcs`). `dagger call feature-check --combo=<name>`
runs one; `dagger call feature-matrix` runs all serially with a pass/fail summary;
`dagger call list-combos` emits the JSON name array the workflow expands.

**Accepted divergences (Phase 05):**
- **`feature-matrix` runs SERIALLY, not the plan's errgroup/`--max-parallel`.**
  Deliberate: (1) cost-over-speed is a hard project rule (the self-hosted runner is
  ~$0/min); (2) cargo holds a per-target build lock, so combos sharing one target
  volume serialize on it anyway — real parallelism would need a target volume per
  worker, multiplying disk on a disk-pressured box (the repo `target/` is already
  277 GB); (3) CI runs one job per combo at `max-parallel: 1` regardless (RAM-bound
  box). `fail-fast` is OFF (every combo runs even after a failure), matching the
  legacy `fail-fast: false`.
- **functions combos run `cargo clippy` ONLY (covers `check`), not check-then-clippy.**
  The legacy `functions-matrix` ran `cargo check` *then* `cargo clippy`; clippy is a
  superset of check, so running clippy alone is one compile instead of two (cost over
  speed) with strictly-greater coverage (the lints). server/core/storage combos run
  `cargo check` only, matching their legacy jobs.
- **`--no-default-features` is a no-op for `functions-*`** (fraiseql-functions defines
  no `default` feature) — kept for verbatim parity with the legacy command, harmless.
  It IS load-bearing for core/storage (both have `default = ["postgres"]`) and for the
  one `server-no-default` combo.
- **Single shared target cache volume** (`fraiseql-rust-target-features-1-92`), kept
  apart from the Phase-02 gate / Phase-03 unit-test / Phase-04 integration caches
  (those hold `--all-features`/test artifacts; the combos compile narrow
  `--no-default-features` slices). Different feature-sets for the same crate churn that
  crate's artifacts, but sccache backs the unchanged upstream dependency graph, so the
  warm reuse combo-to-combo is high.
- **Combo-feature validity is checked by cargo itself**, not a separate Go Cargo.toml
  parser (the plan's Cycle-1 REFACTOR). A bad feature fails the `cargo check` loudly;
  a one-time manual cross-check of all 32 combos against the four `[features]` sections
  confirmed zero typos at authoring time. `lookupCombo` still gives a clean fail-fast
  error (with the known names) for an unknown `--combo=` *name* (Cycle-2 CLEANUP).
- **No strangler-fig** (Day-0 killed hosted CI; `feature-flags.yml` is dispatch-only) —
  validated by local `dagger call` per combo, not a 3-PR legacy race, as in Phases 01–04.

**Coverage gap logged (NOT silently dropped):** `feature-flags.yml`'s
`feature-integration-tests` job (4 combos: `mcp`, `metrics`, `apq-memory`, `tracing`)
runs `cargo test` against test binaries (`mcp_integration_test`,
`metrics_integration_test`, `redis_apq_integration_test`, `tracing_integration_test`),
some service-backed. Those belong to the integration matrix (Phase 04), not this
check-only matrix, and are NOT yet ported by any Dagger suite — a real gap, deferred as
a Phase-04 follow-up increment.

**Status (2026-06-02): 32/32 validated GREEN locally** via `dagger call feature-check
--combo=<name>` — the exact per-combo path CI uses (per-combo check, list-combos JSON,
the workflow's fromJSON matrix proven end-to-end). Reached in two passes plus one
fix-forward:

| Combos | Result |
|---|---|
| all 17 `server-*` | ✅ PASS (11–70s; `server-no-default` 70s primed the dep graph) |
| all 7 `core-*` | ✅ PASS (14–61s) |
| all 3 `storage-*` | ✅ PASS (19–24s) |
| all 5 `functions-*` (clippy) | ✅ PASS (21–42s) after the clippy-component fix below |

**Two findings surfaced and fixed during validation:**

1. **Docker Hub 429 (infra) blocked 13 combos on the first pass** — every one failed
   at `rust:1.92` image resolution with `429 Too Many Requests`, zero rustc errors.
   *Corrects the plan's "no services → rate-limit-immune" assumption:* `cargo check`
   pulls no service images, but Dagger still re-resolves the `rust:1.92` **base tag →
   manifest** per fresh container build, and those registry GETs count against the
   anonymous 100-pulls/6h budget (already spent on the 16 integration suites + the 19
   combos before it). Box-wide → the self-hosted runner hits it identically. Cleared by
   `docker login` on the box (Dagger's engine reads the host docker config for registry
   auth); re-running the 13 then gave 8 green immediately. **Durable fix candidate
   (deferred, roadmapped "Later: pin by digest"):** pin `rustImage` to
   `rust:1.92@sha256:6ca5ad232312…` so buildkit resolves the base from local content
   without a registry manifest GET — touches the shared base, so do it when all phases
   can be re-validated.

2. **`cargo clippy` missing for the `RUSTUP_TOOLCHAIN=1.92` toolchain (the 5
   `functions-*` combos).** `rustBase` installs clippy on the base image's *default*
   toolchain, but `rustBaseFor(msrv)` pins `RUSTUP_TOOLCHAIN=1.92`, which selects a
   toolchain instance (`1.92-x86_64-unknown-linux-gnu`) that ships only rustc →
   `error: 'cargo-clippy' is not installed for the toolchain`. Only Phase 05 exposes
   this: the Phase-02 clippy/fmt gates use `rustBase()` directly (no RUSTUP_TOOLCHAIN)
   and Phase-03/04 use `rustBaseFor` but run `cargo build`/`test`, not clippy. **Fix:**
   `FeatureCheck` prepends `rustup component add clippy` (idempotent) for the clippy
   combos only — scoped to feature-check, so the shared base and Phases 02–04 are
   untouched. With clippy present, all 5 functions combos run `cargo clippy … -D
   warnings` clean (no real lints, incl. the runtime-deno/v8 path).

## Security & Compliance Gates (out-of-phase port, DONE 2026-06-03, local GREEN)

NOT a numbered phase — a logged PR-gap (Day-0 stripped `security-compliance.yml` +
`security.yml` to `workflow_dispatch`, leaving cargo-deny/compliance/dependency-audit
unguarded on `dev`). Ports the **portable subset** onto ONE umbrella `dagger call
security` (`.dagger/security.go`), backed by `.github/workflows/dagger-security.yml`
(self-hosted, push(dev)+dispatch, no pull_request).

| Legacy `security-compliance.yml` job | Dagger fn | Command shape |
|---|---|---|
| `license-scan` + `dependency-audit` | `CargoDeny` | `cargo deny check` (licenses+advisories+bans+sources; `[graph] all-features=true` in deny.toml) |
| `compliance-check` | `Compliance` | required-file gate (SECURITY.md/LICENSE/CODE_OF_CONDUCT.md, hard-fail) + nginx-header & hardcoded-secret greps (warn-only), verbatim from the job |
| (umbrella) | `Security` | runs `compliance` then `cargo-deny`, cheap-first fail-fast, mirrors `Preflight` |

`dependency-audit` (legacy `cargo deny check advisories`) is SUBSUMED by `CargoDeny`'s
full `cargo deny check` (advisories ⊂ full check). Single shared advisory-db cache
volume `fraiseql-advisory-db`; the cargo registry cache is shared with rustBase.

**NOT ported — stay GitHub-native / dispatch-only (same precedent as CodeQL, README
"Out of scope"):**
- `secrets-scan` (TruffleHog) — scans the PR *diff* via `github.event.pull_request.{base,head}.sha`; PR-shaped, no push(dev) analogue. Remains the authoritative secret gate (compliance's grep is warn-only).
- `container-security` (Trivy) — 45-min image scan + SARIF upload to GH Code Scanning (needs the GH security-events API); was a flagged PR-time cost driver. Overlaps the Phase-06 image build; revisit there if wanted.
- `dependency-review` (`security.yml`) — GitHub Dependency-Graph API, PR-only.

**Findings surfaced during the port:**

1. **The gate immediately caught a real Phase-04 regression.** `cargo deny check`
   FAILED (`bans`): the new `fraiseql-test-support` crate dev-deps `testcontainers
   0.27.3`, pulling `etcetera 0.11.0` — a duplicate of `etcetera 0.8.0` (via
   sqlx-postgres). With `bans.multiple-versions = "deny"` that's a hard fail, sitting
   unguarded on `dev` since Phase 04 because cargo-deny wasn't running. FIXED with a
   `[[bans.skip]] etcetera = =0.11.0` entry (idiomatic — matches ~30 existing transitive
   skips; skipping the one newly-introduced version leaves 0.8.0 as the single counted
   version). This is the concrete payoff of restoring the gate.

2. **cargo-deny needs `cargo` on PATH** despite parsing Cargo.lock — `cargo deny check`
   shells out to `cargo metadata` to resolve the graph (`failed to run cargo: No such
   file or directory` on a bare-ubuntu base). So `denyBase` is built `From(rustImage)`
   (cargo + git/curl from buildpack-deps), NOT bare ubuntu — but it skips rustBase's
   mold/clang/sccache/native-dep layers since nothing compiles. The legacy job confirms
   this (it installed `dtolnay/rust-toolchain` alongside cargo-deny).

3. **`lost+found` at the repo root breaks local `dagger call --source=.`** — a
   side-effect of the 2026-06-02 disk migration: the repo is now mounted at the root of
   its own ext4 volume (`lv_work`), and ext4 puts a root-owned mode-700 `lost+found`
   there. fsutil's host-walk hits `open …/lost+found: permission denied` before any
   container runs. Added `"lost+found"` to the `+ignore` exclude on the security fns'
   `source` params (fsutil skips excluded paths; no-op on a clean checkout). **CI is
   unaffected** — `actions/checkout` lands in the runner's `_work` dir on a different
   filesystem (no root-level lost+found), which is why the post-migration test+integration
   re-runs were GREEN. NOTE: the same condition breaks LOCAL `dagger call` for the
   `main.go`/`feature-combos.go` fns too (their `+ignore` lacks `lost+found`) — fix
   pending (host chmod/rmdir vs. a repo-wide `+ignore` sweep; user to decide).

**Warn-only secret-scan behaviour preserved:** compliance's grep flags benign false
positives (dep version pins like `jsonwebtoken = "10"`, doc-comment examples, the
`__Host-access_token=` cookie-prefix literals) and does NOT fail — verbatim with the
legacy `exit 0` (TruffleHog is the real gate). deny.toml also emits several non-failing
`unmatched/unnecessary-skip` + `license-not-encountered (OpenSSL)` warnings (pre-existing
drift; OpenSSL allow is load-bearing for `ring`'s clarify, so it stays) — left as-is, a
cleanup candidate.

**Status (2026-06-03): `dagger call security --source=.` GREEN locally** — compliance ✅
(files present, headers found, secret-scan warns benign), cargo-deny ✅ (`advisories ok,
bans ok, licenses ok, sources ok`). NOT pushed (stop-and-ask).

## Phase 06 — Release Pipeline (minimal scope: local pre-tag validation)

**Decision (2026-06-04):** the full release pipeline was deliberately NOT ported.
The Dagger cost win lives in the high-frequency per-PR/push gates (Phases 01–05);
the release path runs only on `v*` tags, so its hosted-CI cost is negligible and
re-implementing a working crates.io publisher is the highest-risk, lowest-reward
port. `release.yml` and `docker-build.yml` stay GitHub-native and **remain live on
`v*` tags** — the v2.4.0 cut runs on them. Phase 06 is therefore NOT a blocker for
releasing.

What was ported = **local pre-tag validation only**, so a release can be checked
before the tag instead of discovering breakage during the release run:

| Dagger fn | Mirrors | Notes |
|---|---|---|
| `publish-order` | release.yml crate order | Canonical 16-crate topological order. |
| `publish-order-selftest` | (new) | `cargo metadata` → asserts publishable set + topological validity vs the embedded order. Catches a new/removed crate or a new cross-crate edge. |
| `publish-dry-run` | release.yml "Dry-run publish for every publishable crate" | Tokenless; collect-all-then-fail. **Strict** (see divergence below). |
| `semver-named` | ci.yml "API Semver Compatibility" | 5 named crates, advisory/non-gating (`\|\| true`). |
| `semver-workspace` | semver.yml | gating `--workspace --exclude fraiseql-test-utils`. |

Convenience: `make release-validate [VERSION=x.y.z]` and
`make release-validate-semver [BASELINE=...]`.

**NOT ported (stay GitHub-native, by design):** the actual `cargo publish`, image
build/push (`docker-build.yml`), GitHub Release creation, binary builds
(`build-binaries`), the release smoke (`release-smoke.yml`), and the `v*`-tag
trigger itself. SDK publish is Phase 07. Full decommission is Phase 08 (not a goal
for the release path yet).

**Toolchain note:** `semverBase` installs the pinned prebuilt cargo-semver-checks
v0.48.0 (musl) rather than legacy `cargo install ... --locked` (no recompile per
run). It runs on the pinned **stable** 1.92 toolchain — cargo-semver-checks unlocks
the otherwise-nightly rustdoc JSON via `RUSTC_BOOTSTRAP` internally (verified: 223
checks pass on fraiseql-error, baseline cloned from HEAD~1, no nightly needed).

**fraiseql-codegen finding (2026-06-04):** `fraiseql-codegen` (new in #291) has
never been published (crates.io 404) and `fraiseql-cli` depends on it, so
`cargo publish --dry-run -p fraiseql-cli` fails with `no matching package named
fraiseql-codegen`. The legacy release.yml dry-run gate had the identical blind
spot and would have blocked the v2.4.0 release on `fraiseql-cli` — even though the
real ordered publish succeeds (codegen ships before cli; codegen@2.4.0 satisfies
cli's `^2.3.0`). Fixed in `release.yml` (commit on this branch); see the divergence.

## Phase 07 — SDK Pipelines

(Add rows as ported, one per language.)

## Phase 08 — Decommission

| Workflow | Deletion PR # | Date | Cost saved/PR |
|---|---|---|---|

## Accepted divergences

Cases where Dagger and legacy YAML differ on purpose. Examples might include:

- Different parallelism (Dagger uses Go errgroup; YAML uses matrix)
- Container base image differs from `ubuntu-latest` (Dagger uses pinned digest)
- Cache strategy differs (Dagger volumes vs `actions/cache`)

Document each so future contributors don't chase phantom regressions.

**Phase 06 — dry-run gate strictness (intentional, 2026-06-04).** The local Dagger
`publish-dry-run` is **strict**: a crate that fails because a sibling it depends on
is not yet on crates.io (a first-publish, e.g. fraiseql-codegen) is a hard failure,
so a human running it before a tag sees every gap. The GitHub `release.yml` gate was
**patched to tolerate** that one case (a single "failed to prepare local package for
uploading" whose only missing packages are our own publishable crates) so an
unattended tag release does not false-block on a first-publish ordering artifact the
real ordered publish resolves. Same intent, different audience: local diagnostic vs
unattended release. Everything else (gitignored files / build.rs artifacts in the
tarball, a genuinely missing external dep, verify-build compile errors) hard-fails in
both.
