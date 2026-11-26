.PHONY: help build test test-unit test-integration federation-compose-check test-full test-all-ignored clippy fmt check clean clean-test-containers install dev doc bench memory-profile db-up db-down db-logs db-reset db-failover-reset db-status e2e e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-velocitybench e2e-clean e2e-status test-parity test-parity-strict security audit test-count lint-gate lint-gate-db lint-gate-wire lint-gate-core lint-unwrap lint-expect lint-tests-layout lint-guard-parity lint-guard-test-lock test-guard-test-lock-gate lint-mount-ledger test-mount-ledger-gate lint-guard-sites test-guard-sites-gate test-guard-mutants-gate release release-validate release-validate-semver load-test load-test-all chart-deploy compose-stack changelog changelog-full

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run unit + integration tests (PostgreSQL) — ~50 min, serialized"
	@echo "  make test-unit          - Run unit tests only (fast, no database)"
	@echo "  make test-leg           - The 'Dagger — test' workspace suite, exactly as CI runs it (required check)"
	@echo "  make test-integration   - Run integration tests (requires Docker)"
	@echo "  make test-integration-postgres - The 'integration (postgres)' CI shard, exactly as CI runs it"
	@echo "  make test-full          - Run ALL categories: unit + snapshots + DBs + Redis/NATS/Vault + server + federation"
	@echo "  make federation-compose-check - Compose the subgraph SDL fixtures (no Docker)"
	@echo "  make test-all-ignored   - Run ALL #[ignore] tests (requires full infra: db-up)"
	@echo "  make test-parity        - Cross-SDK schema parity (absent toolchains named, not gated)"
	@echo "  make test-parity-strict - Cross-SDK schema parity exactly as CI runs it"
	@echo "  make coverage           - Generate test coverage report"
	@echo "  make load-test          - Run k6 mixed-workload load test (requires running server)"
	@echo "  make load-test-all      - Run all k6 load test scenarios"
	@echo ""
	@echo "Database (Docker):"
	@echo "  make db-up              - Start all test infrastructure (PostgreSQL, Redis, NATS, Vault)"
	@echo "  make db-down            - Stop test infrastructure"
	@echo "  make db-logs            - View infrastructure logs"
	@echo "  make db-reset           - Reset test infrastructure (remove volumes)"
	@echo "  make db-status          - Check infrastructure health"
	@echo ""
	@echo "Code Quality:"
	@echo "  make build              - Build all crates"
	@echo "  make clippy             - Run Clippy linter"
	@echo "  make fmt                - Format code with rustfmt"
	@echo "  make check              - Run all checks (fmt + clippy + test)"
	@echo "  make preflight          - Run the Dagger preflight gate locally before pushing (fmt+clippy+rustdoc + policy lint gates)"
	@echo "  make changelog          - Preview unreleased changelog entries (git-cliff)"
	@echo "  make changelog-full     - Generate full changelog (overwrites CHANGELOG.md)"
	@echo "  make clean              - Clean build artifacts"
	@echo "  make clean-test-containers - Remove leaked testcontainers postgres instances"
	@echo ""
	@echo "Development:"
	@echo "  make dev                - Run development server"
	@echo "  make doc                - Build documentation"
	@echo "  make bench              - Run benchmarks"
	@echo "  make install            - Install CLI tool"
	@echo ""
	@echo "Delivery artifacts (what an operator consumes):"
	@echo "  make compose-stack      - Bring up docker-compose.yml on this branch's image and query it"
	@echo "  make chart-deploy       - Deploy the Helm chart into a throwaway k3s cluster and query it"
	@echo "  make image-boot         - Boot each server image on its own CMD and query it"
	@echo "  make image-properties   - Assert what the built image IS (linkage, user, labels, size)"
	@echo ""

# Build all crates
build:
	cargo build --all-features

# Build release
build-release:
	cargo build --release --all-features

# Prepare a release: bump version, update CHANGELOG and README, commit, tag.
# Usage: make release VERSION=2.2.0
release:
	@test -n "$(VERSION)" || (echo "Usage: make release VERSION=x.y.z" && exit 1)
	bash tools/release.sh $(VERSION)

# Pre-tag release validation via Dagger (same engine locally and on the runner).
# Tokenless and read-only — never publishes. Run before cutting a release tag, after
# the version bump (release.sh does both), to catch the "tag shipped but publish
# failed" class (v2.3.0/v2.3.1) up front. At a version already on crates.io it fails
# on a checksum mismatch: see PublishDryRun.
#   make release-validate                 # package and verify every publishable crate
#   make release-validate VERSION=2.4.0   # also assert Cargo.toml is on 2.4.0
release-validate:
	dagger call publish-dry-run --source=. $(if $(VERSION),--expect-version=$(VERSION),)

# Heavier API-compatibility gate (recompiles rustdoc for the whole workspace + its
# baseline); run separately when an API-breaking review is wanted before a tag.
#   make release-validate-semver                 # baseline = HEAD~1
#   make release-validate-semver BASELINE=v2.3.2 # baseline = a published tag
release-validate-semver:
	dagger call semver-workspace --source=. --baseline-rev=$(or $(BASELINE),HEAD~1)

# Run all tests (unit + integration)
test: test-unit test-integration

# Run the full test suite: unit + snapshots + all DBs + Redis/NATS/Vault + server
# Requires full infrastructure: Docker with PostgreSQL, Redis, NATS, Vault
# Reports a single pass/fail at the end.
test-full: db-up
	@echo "=== Running full test suite (9 steps) ==="
	@echo ""
	@echo "[1/9] Unit tests..."
	@cargo test --lib --all-features
	@echo ""
	@echo "[2/9] SQL snapshot tests..."
	@cargo nextest run --test sql_snapshots 2>/dev/null || cargo test --test sql_snapshots
	@echo ""
	@echo "[3/9] Database integration tests (PostgreSQL)..."
# Was the same `-p fraiseql-core … -- --ignored` line as test-integration's, with
# the same result: 1 test of 2828, under a banner claiming the database
# integration tests had run (#1169). A second corrected copy would drift from the
# first, so both call the one mirror target.
	@$(MAKE) --no-print-directory test-integration-postgres
	@echo ""
	@echo "[4/9] (retired — cross-database parity removed with the non-PostgreSQL backends, G2/#374)"
	@echo ""
	@echo "[5/9] Redis tests (APQ + observer cache/lease)..."
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-core --features "redis-apq" --lib redis -- --ignored --test-threads=1
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-observers --features "caching,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[6/9] NATS + observer PostgreSQL tests..."
	cargo test -p fraiseql-observers --features "nats" --test nats_integration -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features redis-lease --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[7/9] Vault secrets manager tests..."
	VAULT_ADDR="http://localhost:8200" \
	VAULT_TOKEN="fraiseql-test-token" \
		cargo test -p fraiseql-server --test secrets_manager_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "[8/9] Server integration tests (database queries + observers)..."
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --test database_query_test -- --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --features "observers-nats" --test observer_runtime_integration_test -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "caching,testing" -p fraiseql-observers --test integration_test
	@echo ""
	@echo "[9/9] Doctests per feature set..."
	@$(MAKE) --no-print-directory test-doc-feature-sets
	@echo ""
	@echo "=== Full test suite complete (all 9 steps passed) ==="

# Run each workspace library's doctests on its own with its default features, and the
# umbrella crate's once per feature its documentation is gated on. The `--all-features`
# doctest pass compiles an example that needs a feature, and `--workspace` unifies
# features across members, so neither sees an example that fails for a crate built
# with its defaults. CI runs it in the Doctests leg (dagger-doctests.yml). Libraries
# with no doctest are skipped; tools/check-doctest-skip.py holds that list.
.PHONY: test-doc-feature-sets
test-doc-feature-sets:
	bash tools/check-doctest-feature-sets.sh

# Run unit tests only (no database required)
#
# NOT `--all-features`: `test-postgres` is an opt-in marker feature meaning "a live
# database is available", and `--all-features` switches it on, which pulled 77
# DB-backed tests in fraiseql-db/src into this target and made it unpassable (#1169
# neighbourhood). fraiseql-db is therefore given an explicit feature list, mirroring
# the per-crate enumeration in .github/workflows/coverage.yml.
test-unit:
	@echo "Running unit tests..."
	@cargo test --lib --workspace --all-features

# Run integration tests (requires Docker databases)
# Runs each suite with the correct feature flags and env vars.
#
# The PostgreSQL section is `test-integration-postgres` — the mirror of the
# Dagger shard. It used to be one `-p fraiseql-core … -- --ignored` line, which
# ran 1 test out of 2828 across 77 binaries and then printed "All integration
# tests passed": none of those suites are `#[ignore]`d, and `--ignored` runs
# ONLY ignored tests (#1169).
#
# Nor do they self-skip on an absent DATABASE_URL: fraiseql-test-support's
# `resolve_or_panic` fails loudly by design, so a DB-backed test invoked without
# a URL is a hard failure, not a skip. That is why `test-unit` must not enable
# the `test-postgres` marker feature -- see the note on that target above.
test-integration: test-integration-postgres
	@echo ""
	@echo "=== fraiseql-observers integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "caching,testing" -p fraiseql-observers --test integration_test
	@echo ""
	@echo "=== fraiseql-server integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --lib --tests -- --ignored
	@echo ""
	@echo "All integration tests passed."

# ============================================================================
# `integration (postgres)` — the local mirror of the Dagger shard (#1169)
# ============================================================================
#
# Every suite below provisions its fixtures in the same `public` schema of the
# same database, so they collide on Postgres' catalog uniqueness when run in
# parallel: `cargo test -p fraiseql-core` with a DATABASE_URL bound gives 38
# failures that have nothing to do with your change, and the failing NAMES drift
# between runs, so it reads as flakiness in whatever you just touched. CI never
# uses default parallelism — `.dagger/main.go`'s integrationPostgres runs every
# line with `--test-threads=1` — but that knowledge lived only in that file.
# This target is the command that was missing.
#
# The lines are the shard's own: tools/leg-lines.py reads them out of
# `.dagger/main.go`'s integrationPostgres at run time, so they are written once,
# with their rationale, there. `python3 tools/leg-lines.py integrationPostgres`
# prints them; copy one to run it alone (with the exports below).
#
# Serialized on purpose: expect this to take a while. What is NOT mirrored is
# the shard's build environment (sccache, mold, CARGO_INCREMENTAL=0, a pinned
# toolchain image) — that is the local box's `.cargo/config.toml` to decide, and
# none of it changes what the tests assert.
test-integration-postgres: export DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql
test-integration-postgres: export STANDBY_DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5436/test_fraiseql
test-integration-postgres: export FAILOVER_STANDBY_DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5437/test_fraiseql
test-integration-postgres: export BELOW_FLOOR_DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5440/test_fraiseql
# As the leg sets it: a test that finds no database here fails instead of skipping.
test-integration-postgres: export FRAISEQL_TEST_REQUIRE_POSTGRES := 1
# Mirrors integrationBase/rustBase: RUST_LOG=debug is what CI's assertions run
# under, so a test that only fails at that level must fail here too.
test-integration-postgres: export RUST_LOG := debug
test-integration-postgres: export RUST_BACKTRACE := 1
.PHONY: test-integration-postgres
# db-failover-reset first: the #957 test calls pg_promote() on the failover
# standby and a promoted standby never goes back. CI starts from fresh
# containers every run; locally the second run would assert against a plain
# writable server unless it is re-cloned.
test-integration-postgres: db-up db-failover-reset
	@echo ""
	@echo "=== integration (postgres) — the lines of .dagger/main.go integrationPostgres ==="
	@script="$$(python3 tools/leg-lines.py integrationPostgres)" && bash -ec "$$script"
	@echo "test-integration OK: postgres suite passed"

# ============================================================================
# `test` — the local mirror of the Dagger workspace-suite leg (#1257)
# ============================================================================
#
# The workspace suite is not `cargo test`. It is one `cargo test --workspace
# --exclude …` sweep, twenty-six feature-scoped invocations carrying the
# `SYNC:*` lists, several naming explicit `--test` binaries because `--lib` does
# not reach `tests/*`. A developer running `cargo test` before pushing runs a
# different, smaller thing — which is how `config_coverage_manifest_test` went
# to `dev` on `231c3a25c` with `make preflight` green and all sixteen branch
# legs green, and reddened `Dagger — test` afterwards. `make preflight` compiles
# these tests (`--all-targets`) and never runs them.
#
# The lines are the shard's own: tools/leg-lines.py reads them out of
# `.dagger/main.go`'s `Test` at run time, so they are written once, with their
# rationale, there. `python3 tools/leg-lines.py Test` prints them; copy one to run
# it alone.
#
# What is NOT mirrored is the leg's build environment (sccache, mold,
# CARGO_INCREMENTAL=0, a pinned toolchain image) — that is the local box's
# `.cargo/config.toml` to decide, and none of it changes what the tests assert.
# `CARGO_BUILD_JOBS` is the exception: it is set for the same reason the leg sets
# it (#615 — V8 in `--all-features` peaks over this box's RAM at full
# parallelism, and the OOM killer takes rustc with a bare exit-101).
#
# Docker-free by construction, like the leg: the testcontainers-backed suites are
# skipped by name and the DB-backed ones self-skip. That only holds if no
# backing-service URL is exported, which the guard below enforces — the leg binds
# none, so a suite that self-skips there must self-skip here.
test-leg: export CARGO_BUILD_JOBS := 8
# Mirrors rustBase, which sets it for every leg: a panic in CI prints a backtrace,
# so one here should too. Diagnostic only — it changes no assertion.
test-leg: export RUST_BACKTRACE := 1
.PHONY: test-leg
test-leg:
	@leaked="$$(env | grep -E '^(DATABASE_URL|TEST_DATABASE_URL|STANDBY_DATABASE_URL|FAILOVER_STANDBY_DATABASE_URL|BELOW_FLOOR_DATABASE_URL|REDIS_URL|NATS_URL|VAULT_ADDR|VAULT_TOKEN)=' || true)"; \
	if [ -n "$$leaked" ]; then \
		echo "test-leg: refusing to run — a backing-service URL is exported:"; \
		printf '%s\n' "$$leaked" | sed 's/^/    /'; \
		echo ""; \
		echo "The Dagger test leg binds none of these, so suites that self-skip there"; \
		echo "would run here against a live service and assert something else."; \
		echo "Unset them, or use 'make test-integration-postgres' for the DB suites."; \
		exit 1; \
	fi
	@echo ""
	@echo "=== test — the lines of .dagger/main.go Test ==="
# A cold full run grew target/ by ~200 GB here — 138 GB of debug/deps plus 94 GB of
# debug/incremental (which CI never creates: rustBase sets CARGO_INCREMENTAL=0). Out of
# space, rustc and sccache report "failed to parse process output", which reads as a
# toolchain fault 40 minutes in; printing the figure up front makes the log say
# otherwise. `rm -rf target/debug/incremental` reclaims the larger half without
# invalidating anything already built.
	@echo "### free on this filesystem: $$(df -Ph . | tail -1 | awk '{print $$4}') (a cold run adds ~200 GB to target/)"
	@script="$$(python3 tools/leg-lines.py Test)" && bash -ec "$$script"

# The two mirror targets above read their lines through tools/leg-lines.py.
.PHONY: lint-leg-lines
lint-leg-lines:
	@python3 tools/leg-lines.py Test >/dev/null
	@python3 tools/leg-lines.py integrationPostgres >/dev/null
	@python3 tools/leg-lines.py --crates fraiseql-core,fraiseql-db Test integrationPostgres >/dev/null
	@python3 tools/leg-lines.py --coverage
	@echo "leg-lines: OK — Test and integrationPostgres read, in agreement with check-suite-coverage.py; CoveragePostgres has lines for core and db, and coverage.yml measures neither"

# Red-capability pin for the reader: a shard shape it cannot read, an unresolvable
# piece, a canary line of neither form and a disagreement with the coverage gate
# must each fail, by name.
.PHONY: test-leg-lines-gate
test-leg-lines-gate:
	@bash tools/tests/leg_lines_test.sh

# Run ALL #[ignore] tests — requires full test infrastructure (make db-up first).
# Covers: Redis APQ, NATS transport, Vault secrets, the observer runtime.
# Stress tests (60s+ each) are excluded; run them separately with:
#   cargo test -p fraiseql-observers --test stress_tests -- --ignored
test-all-ignored: db-up
	@echo ""
	@echo "=== Redis tests (APQ + observer cache/lease) ==="
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-core --features "redis-apq" --lib redis -- --ignored --test-threads=1
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-observers --features "caching,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "=== NATS transport tests ==="
	cargo test -p fraiseql-observers --features "nats" --test nats_integration -- --ignored --test-threads=1
	@echo ""
	@echo "=== Observer PostgreSQL transport + lease tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features redis-lease --lib -- --ignored --test-threads=1
	@echo ""
	@echo "=== Vault secrets manager tests ==="
	VAULT_ADDR="http://localhost:8200" \
	VAULT_TOKEN="fraiseql-test-token" \
		cargo test -p fraiseql-server --test secrets_manager_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "=== Observer server runtime tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --features "observers-nats" --test observer_runtime_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "All ignored tests passed."

# ============================================================================
# Changelog (git-cliff)
# ============================================================================

## Preview unreleased changelog entries
changelog:
	git cliff --unreleased --strip header

## Generate full changelog (overwrites CHANGELOG.md)
changelog-full:
	git cliff --output CHANGELOG.md

# The Rust gates of the Dagger Preflight. Each of its sibling functions (Rustdoc,
# RustdocDefault, Clippy, CheckDefault, CheckFuzz, CheckExampleCrates, CheckRExamples,
# Fmt) runs its target here in its own container, and `make preflight` runs the same
# targets, so each command is written once.

# Clippy over the workspace, and over examples/async-jobs-subgraph/subgraph, a
# standalone crate (its own [workspace]) that `--workspace` never reaches. Each run goes
# through tools/clippy-config-paths-gate.sh: a path in .clippy.toml that clippy cannot
# resolve is only a warning `-D warnings` lets through, and the ban it names is dropped.
.PHONY: clippy
clippy:
	bash tools/clippy-config-paths-gate.sh cargo clippy --workspace --all-features --all-targets -- -D warnings
	cd examples/async-jobs-subgraph/subgraph && bash ../../../tools/clippy-config-paths-gate.sh cargo clippy --all-targets -- -D warnings

# Rustdoc with every feature, warnings denied.
.PHONY: rustdoc
rustdoc: export RUSTDOCFLAGS := -D warnings
rustdoc:
	cargo doc --workspace --all-features --no-deps

# Rustdoc with default features (the links `--all-features` resolves), then
# fraiseql-server once per module-gating feature and fraiseql-functions once per runtime.
.PHONY: rustdoc-default
rustdoc-default: export RUSTDOCFLAGS := -D warnings
rustdoc-default:
	cargo doc --workspace --no-deps --keep-going
	bash tools/check-rustdoc-feature-sets.sh

# The DEFAULT feature set, which no other Rust gate here compiles. `clippy` and
# `rustdoc` are both `--all-features`, and `--all-features` can never compile a
# `cfg(not(feature = …))` arm — so the configuration a plain `cargo build` and the
# slim image actually produce is the one preflight never built (#1101).
.PHONY: check-default
check-default:
	cargo check --workspace --all-targets

# Secondary gate: count #[allow(clippy::unwrap_used)] annotations in production source files.
# Primary enforcement: clippy::unwrap_used = "deny" in workspace lints — any new .unwrap() in
# production code fails `cargo clippy --workspace -- -D warnings` before this gate runs.
# This secondary gate limits annotation proliferation (each annotation is a deliberate exception).
# Excludes lines containing "test" (covers #![allow] in test modules and test-only src files).
# `#![allow]` inside `#[cfg(test)]` modules are excluded via `grep -v '#!\[allow'`.
# The budget is set here once: `shell-gates` (the Dagger ShellGates leg) and a bare
# `make lint-unwrap` both check this value.
# Raise UNWRAP_ALLOW_LIMIT only with a PR comment justifying each new addition.
UNWRAP_ALLOW_LIMIT ?= 2
.PHONY: lint-unwrap
lint-unwrap:
	@echo "=== Counting unwrap allows in production code ==="
	@count=$$(grep -rn 'allow.*unwrap_used' crates/*/src/ --include="*.rs" \
		| grep -v "test" | grep -v '#!\[allow' | wc -l); \
	echo "Current count: $$count / $(UNWRAP_ALLOW_LIMIT)"; \
	if [ "$$count" -gt "$(UNWRAP_ALLOW_LIMIT)" ]; then \
		echo "ERROR: $$count production unwrap allows exceeds limit of $(UNWRAP_ALLOW_LIMIT)"; \
		echo "Review new additions or raise UNWRAP_ALLOW_LIMIT with justification."; \
		exit 1; \
	fi; \
	echo "OK: $$count <= $(UNWRAP_ALLOW_LIMIT)"

# Check for empty or placeholder .expect() messages in production code.
# .expect("") or .expect("TODO") is functionally equivalent to .unwrap().
.PHONY: lint-expect
lint-expect:
	@echo "=== Checking for empty/placeholder .expect() calls ==="
	@count=$$(grep -rn '\.expect("")\|\.expect("TODO")\|\.expect("todo")\|\.expect("FIXME")\|\.expect("fixme")' \
		crates/*/src/ --include="*.rs" | grep -v test | wc -l); \
	if [ "$$count" -gt "0" ]; then \
		echo "ERROR: $$count .expect() calls with empty/placeholder messages in production code:"; \
		grep -rn '\.expect("")\|\.expect("TODO")\|\.expect("todo")\|\.expect("FIXME")\|\.expect("fixme")' \
			crates/*/src/ --include="*.rs" | grep -v test; \
		exit 1; \
	fi; \
	echo "OK: no empty .expect() calls"

# Gate: ensure the number of #[async_trait] usages has not grown above the baseline.
# async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425).
# Phase 0 baseline: 128 (crates/*/src/ only, matching the convention used by lint-unwrap/lint-expect).
# Run `make lint-async-trait` to detect regressions (e.g. a new dyn-dispatch trait added without tracking comment).
# 189 → 190: the construction-parity fixture adapter in
# `fraiseql-server/src/server/parity_tests.rs` implements `DatabaseAdapter`, which is
# itself declared `#[async_trait]`, so the impl has no choice. It lives in `src/`
# rather than `tests/` because it asserts on private `Server` fields.
# 190 → 192: `SamlIdpStore` (#947) and its `PgSamlIdpStore` impl. The registry holds the
# store as `Arc<dyn SamlIdpStore>` so a deployment can back it with something other than
# Postgres and so the hot-reload path is testable without a database — dyn dispatch is the
# point, which is exactly the case this baseline exempts.
# 192 → 195: `ScimStore` (#946), its `PgScimStore` impl, and the `AccountStore` impl the
# SCIM tests exercise. Same reason: the SCIM router holds the store dyn-dispatched so the
# provisioning surface is backend-agnostic and testable.
# 195 → 197: two `DatabaseAdapter` test doubles for the streaming read surface (#958) —
# the trait-default adapter in `fraiseql-db/src/traits/tests.rs` and the forwarding spy in
# `fraiseql-core/src/cache/adapter/tests.rs`. Both implement a trait that is itself
# declared with the macro, so the impls have no choice; both live in `src/` because they
# test module-private behaviour (the trait's own defaults, and which method the cache
# wrapper calls on its inner adapter).
# 197 → 198: one shared `ArrowDatabaseAdapter` test double for the Flight surface
# (#1036, #1039). It implements a trait that is itself declared with the macro, so the
# impl has no choice, and it lives in `src/` because the handlers it drives —
# `execute_bulk_export` and `handle_refresh_schema_registry` — are `pub(crate)` and
# unreachable from `tests/`. This is +1, not +3: the work began with three separate
# doubles (N rows / one row / no rows) and collapsed them into one parameterised mock
# rather than raising this budget by three for test code.
# 198 → 199: one `JobQueue` test double for the worker-pool lifecycle tests (#1063). It
# implements a trait that is itself declared with the macro, so the impl has no choice,
# and it lives in `src/` because `mod worker_tests` sits in `queue/tests.rs` alongside the
# module it covers — that module was an empty `mod worker_tests {}`, which is how a pool
# whose `stop()` hung forever shipped. This is +1, not +4: one idle queue drives all four
# lifecycle tests, since shutdown is observable on the no-work path.
# 199 → 202: three test doubles for the `before:mutation` enforcement tests (#1327) —
# a `DatabaseAdapter` that logs which SQL function each write called (the existing
# capturing adapter overwrites its capture and drops the function name, so it cannot
# answer "did `guarded` write?" for a document that also ran `harmless`), and two
# `BeforeMutationGate` doubles. All three implement traits that are themselves declared
# with the macro, so the impls have no choice, and they live in `src/` because the suite
# sits in `runners/mutation/tests.rs` beside the chokepoint it covers.
#
# 202 → 204: two more test doubles for the `before:mutation` read bridge (#1328), in the
# same suite and for the same reason — a `DatabaseAdapter` whose reads answer with the
# RLS filter they were handed (so "what did this hook see?" is a fact about the identity
# the read ran under), and a `BeforeMutationGate` that reads through the request's bridge.
# Both implement traits declared with the macro, so the impls have no choice.
#
# Note the grep sees only the bare `#[async_trait]` spelling; the tree also carries ~62
# `#[async_trait::async_trait]` sites this ratchet has never counted, so the number is a
# trend line for one spelling rather than a census.
#
# 204 → 205: `RecordingAdapter`, the test double the gRPC mutation arm converges against
# (#1330). One real impl — a `DatabaseAdapter`, whose signatures the macro fixes — plus
# the convention comment that justifies it, because this grep counts prose as well as
# attributes. #1330 was measured with `make test-leg` and clippy but never with
# `make preflight`, which is why the increment surfaced a phase later.
# 205 → 206: `GatedEntityAdapter`, the test double behind the field-authorizer pin for
# #1331/#1352. Its mutation returns a `User` whose `email` is declared `authorize`, which is
# what makes the anonymous-vs-authenticated pair discriminating — with an empty selection set
# the authorizer takes zero calls and the value is served. One real impl, of a
# `DatabaseAdapter` whose signatures the macro fixes, so the impl has no choice.
# 206 → 207: `MarkerWithoutOverride`, the test double that carries the `SupportsMutations`
# marker and deliberately never overrides `supports_mutations()` — the pairing the marker's
# own doc calls "stated rather than enforced". It is the witness that the typed write entries
# consult the runtime gate; no adapter already in the tree has that shape, because every one
# of them states the pairing correctly. One real impl, of a `DatabaseAdapter` whose
# signatures the macro fixes, so the impl has no choice.
# 207 → 215: the write API (ruling AA 6). `Writer`, the one write capability, is a new
# dyn-dispatched trait (`Arc<dyn Writer>` is the executor's write handle, as
# `Arc<dyn DatabaseAdapter>` is its read handle), so the trait and every impl carry the
# macro: the trait, `PostgresAdapter`, `CachedDatabaseAdapter`, `FailingAdapter`, the two
# `test_support` mocks and two tenancy doubles (+9), less `MarkerWithoutOverride` and one
# double whose only write method moved (-2). Stated so it is not mistaken for the whole
# cost: the migration also gave 34 test doubles a `Writer` impl spelled
# `#[async_trait::async_trait]`, which this grep has never counted (see above) — each one
# replaces write methods that sat in that double's `DatabaseAdapter` impl, so no double
# gained a dyn-dispatch trait it did not already implement.
ASYNC_TRAIT_LIMIT := 212
.PHONY: lint-async-trait
lint-async-trait:
	@count=$$(grep -rn "#\[async_trait\]" crates/*/src/ --include="*.rs" | wc -l); \
	if [ "$$count" -gt "$(ASYNC_TRAIT_LIMIT)" ]; then \
	  echo "ERROR: async_trait count $$count exceeds baseline $(ASYNC_TRAIT_LIMIT)"; \
	  echo "New dyn-dispatch traits must add:"; \
	  echo "  // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)"; \
	  exit 1; \
	fi; \
	echo "async_trait count OK ($$count ≤ $(ASYNC_TRAIT_LIMIT))"

# Gate: ensure the number of crate-level clippy allows in fraiseql-core has not grown.
# Target: ≤20 allows (currently 16 after B1 remediation).
# Run `make lint-gate` in CI to detect regressions.
lint-gate:
	@ALLOW_COUNT=$$(grep -c '^#!\[allow(clippy::' crates/fraiseql-core/src/lib.rs); \
	echo "fraiseql-core lib.rs crate-level allow count: $$ALLOW_COUNT"; \
	if [ "$$ALLOW_COUNT" -gt 20 ]; then \
	  echo "ERROR: too many crate-level clippy allows ($$ALLOW_COUNT > 20)"; \
	  echo "Fix the underlying code or justify each allow with a Reason: comment."; \
	  exit 1; \
	fi; \
	echo "OK: $$ALLOW_COUNT allows (≤20 threshold)"

# Gate: ensure HIGH-risk cast allows are not re-added to fraiseql-db crate level.
# cast_possible_truncation, cast_precision_loss, cast_sign_loss must not be global.
# Current crate-level allows: 37 (target ≤40 after removing the 3 cast allows).
FRAISEQL_DB_LIB_ALLOWS_MAX ?= 40
.PHONY: lint-gate-db
lint-gate-db:
	@count=$$(grep -c '^#!\[allow(clippy' crates/fraiseql-db/src/lib.rs); \
	echo "fraiseql-db lib.rs crate-level allows: $$count (max: $(FRAISEQL_DB_LIB_ALLOWS_MAX))"; \
	for lint in cast_possible_truncation cast_precision_loss cast_sign_loss; do \
	  if grep -q "allow.*$$lint" crates/fraiseql-db/src/lib.rs; then \
	    echo "ERROR: HIGH-risk cast lint $$lint must not be allowed at crate level"; \
	    exit 1; \
	  fi; \
	done; \
	if [ "$$count" -gt "$(FRAISEQL_DB_LIB_ALLOWS_MAX)" ]; then \
	  echo "ERROR: too many crate-level clippy allows in fraiseql-db ($$count > $(FRAISEQL_DB_LIB_ALLOWS_MAX))"; \
	  exit 1; \
	fi; \
	echo "OK: $$count allows (≤$(FRAISEQL_DB_LIB_ALLOWS_MAX)), no HIGH-risk cast lints at crate level"

# Gate: ensure crate-level clippy allows in fraiseql-wire/src/lib.rs do not grow.
# Target: ≤15 allows (post-F053 reorganization: 8 wire-protocol casts + 7 style prefs).
# Test-bleed allows (unreadable_literal, explicit_iter_loop) live in mod tests blocks
# and must not return to crate level.
# The three crate-level counts match `#![allow` only at the start of a line: a `//`
# comment quoting one is prose, and counting it put this gate at 16 over 15 real
# allows while no leg ran it.
FRAISEQL_WIRE_LIB_ALLOWS_MAX ?= 15
.PHONY: lint-gate-wire
lint-gate-wire:
	@count=$$(grep -c '^#!\[allow(clippy::' crates/fraiseql-wire/src/lib.rs); \
	echo "fraiseql-wire lib.rs crate-level allow count: $$count (max: $(FRAISEQL_WIRE_LIB_ALLOWS_MAX))"; \
	for lint in unreadable_literal map_unwrap_or explicit_iter_loop range_plus_one; do \
	  if grep -q "^#!\[allow(clippy::$$lint" crates/fraiseql-wire/src/lib.rs; then \
	    echo "ERROR: test-bleed lint $$lint must not be allowed at crate level (move to mod tests)"; \
	    exit 1; \
	  fi; \
	done; \
	if [ "$$count" -gt "$(FRAISEQL_WIRE_LIB_ALLOWS_MAX)" ]; then \
	  echo "ERROR: too many crate-level clippy allows in fraiseql-wire ($$count > $(FRAISEQL_WIRE_LIB_ALLOWS_MAX))"; \
	  echo "Fix the underlying code or scope the allow to a specific module."; \
	  exit 1; \
	fi; \
	echo "OK: $$count allows (≤$(FRAISEQL_WIRE_LIB_ALLOWS_MAX)), no test-bleed lints at crate level"

# Gate: ensure narrow cast allows in fraiseql-core do not proliferate beyond threshold.
# Only narrow per-site #[allow(clippy::cast_*)] annotations are counted (not crate-level //!).
FRAISEQL_CORE_CAST_ALLOWS_MAX ?= 20
.PHONY: lint-gate-core
lint-gate-core:
	@count=$$(grep -r '#\[allow(clippy::cast' crates/fraiseql-core/src/ | wc -l); \
	echo "fraiseql-core narrow cast allows: $$count (max: $(FRAISEQL_CORE_CAST_ALLOWS_MAX))"; \
	for lint in cast_possible_truncation cast_precision_loss cast_sign_loss; do \
	  if grep -r "^#!\[allow.*$$lint" crates/fraiseql-core/src/lib.rs 2>/dev/null | grep -q .; then \
	    echo "ERROR: HIGH-risk cast lint $$lint must not be allowed at crate level in fraiseql-core"; \
	    exit 1; \
	  fi; \
	done; \
	if [ "$$count" -gt "$(FRAISEQL_CORE_CAST_ALLOWS_MAX)" ]; then \
	  echo "ERROR: too many narrow cast allows in fraiseql-core ($$count > $(FRAISEQL_CORE_CAST_ALLOWS_MAX))"; \
	  exit 1; \
	fi; \
	echo "OK: $$count narrow cast allows (≤$(FRAISEQL_CORE_CAST_ALLOWS_MAX)), no HIGH-risk cast lints at crate level"

# Gate: ensure error-documentation coverage does not regress across all crates.
# Counts "# Errors" doc sections; floors raised as coverage grows.
FRAISEQL_CORE_RUNTIME_ERRORS_DOC_MIN ?= 56
FRAISEQL_CORE_ERRORS_DOC_MIN         ?= 140
FRAISEQL_DB_ERRORS_DOC_MIN           ?= 75
FRAISEQL_SERVER_ERRORS_DOC_MIN       ?= 95

.PHONY: lint-gate-errors-doc lint-gate-errors-doc-core-runtime lint-gate-errors-doc-core lint-gate-errors-doc-db lint-gate-errors-doc-server
lint-gate-errors-doc: lint-gate-errors-doc-core-runtime lint-gate-errors-doc-core lint-gate-errors-doc-db lint-gate-errors-doc-server

lint-gate-errors-doc-core-runtime:
	@count=$$(grep -r "# Errors" crates/fraiseql-core/src/runtime/ | wc -l); \
	[ "$$count" -ge "$(FRAISEQL_CORE_RUNTIME_ERRORS_DOC_MIN)" ] || \
	  (echo "ERROR: fraiseql-core/runtime # Errors regressed ($$count < $(FRAISEQL_CORE_RUNTIME_ERRORS_DOC_MIN))"; exit 1); \
	echo "OK fraiseql-core/runtime: $$count (≥$(FRAISEQL_CORE_RUNTIME_ERRORS_DOC_MIN))"

lint-gate-errors-doc-core:
	@count=$$(grep -r "# Errors" crates/fraiseql-core/src/ | wc -l); \
	[ "$$count" -ge "$(FRAISEQL_CORE_ERRORS_DOC_MIN)" ] || \
	  (echo "ERROR: fraiseql-core # Errors regressed ($$count < $(FRAISEQL_CORE_ERRORS_DOC_MIN))"; exit 1); \
	echo "OK fraiseql-core: $$count (≥$(FRAISEQL_CORE_ERRORS_DOC_MIN))"

lint-gate-errors-doc-db:
	@count=$$(grep -r "# Errors" crates/fraiseql-db/src/ | wc -l); \
	[ "$$count" -ge "$(FRAISEQL_DB_ERRORS_DOC_MIN)" ] || \
	  (echo "ERROR: fraiseql-db # Errors regressed ($$count < $(FRAISEQL_DB_ERRORS_DOC_MIN))"; exit 1); \
	echo "OK fraiseql-db: $$count (≥$(FRAISEQL_DB_ERRORS_DOC_MIN))"

lint-gate-errors-doc-server:
	@count=$$(grep -r "# Errors" crates/fraiseql-server/src/ | wc -l); \
	[ "$$count" -ge "$(FRAISEQL_SERVER_ERRORS_DOC_MIN)" ] || \
	  (echo "ERROR: fraiseql-server # Errors regressed ($$count < $(FRAISEQL_SERVER_ERRORS_DOC_MIN))"; exit 1); \
	echo "OK fraiseql-server: $$count (≥$(FRAISEQL_SERVER_ERRORS_DOC_MIN))"

# Gate: ensure no inline #[cfg(test)] blocks in any source file (workspace-wide).
# The correct pattern is a sibling tests.rs file declared via `#[cfg(test)] mod tests;`.
# Inline blocks (those with an opening `{`) are prohibited; declarations (`;`) are fine.
.PHONY: lint-tests-layout
lint-tests-layout:
	@echo "=== Checking for inline test blocks in src/ (workspace-wide) ==="
	@violations=$$(grep -rn "^mod tests {" \
		crates/*/src/ --include="*.rs" \
		| grep -v "/tests\.rs:" || true); \
	if [ -n "$$violations" ]; then \
		echo "ERROR: inline test blocks found — extract to tests.rs:"; \
		echo "$$violations"; \
		exit 1; \
	fi; \
	echo "OK: no inline test blocks in workspace src/"

# Unit tests for the release tooling (tools/release.sh shell libs). Pure bash, no
# toolchain — wired into the Dagger ShellGates so the release-cut helpers (tag-note
# extraction, internal floor bump, the derived crate publish order, the check of what a
# release published) cannot silently bit-rot.
.PHONY: test-release-tooling
test-release-tooling:
	@bash tools/tests/release_helpers_test.sh
	@bash tools/tests/publish_order_test.sh
	@bash tools/tests/verify_published_test.sh
	@bash tools/tests/wait_for_workflow_runs_test.sh

# Unit tests for the advisory-deadline gate. Its boundary behaviour is worth
# pinning: an off-by-one is a day on which every open branch is blocked by a
# required check, and a file it forgets to scan is a risk acceptance that
# expires invisibly — both of which had already happened (#1103).
.PHONY: test-deadline-gate
.PHONY: test-audit-ledger-gate
.PHONY: test-docs-env-vars-gate
.PHONY: test-doc-claims-gate
.PHONY: test-release-validation-gate
test-deadline-gate:
	@bash tools/tests/check_deadlines_test.sh

# Unit tests for the audit-ledger gate: every finding in the tracked 2026-06-11 audit
# must have a ledger row with evidence, and an audit that yields no findings is red.
test-audit-ledger-gate:
	@bash tools/tests/check_audit_ledger_test.sh

# Unit tests for the docs env-var gate: the audit-record exception (2026-09-21) is pinned
# in both directions so it cannot widen into a runbook exemption.
test-docs-env-vars-gate:
	@bash tools/tests/check_docs_env_vars_test.sh

# Unit tests for the doc-claims gate: each rule red on the claim it exists for, green on
# the historical or negative form, red when it has nothing to scan.
test-doc-claims-gate:
	@bash tools/tests/check_doc_claims_test.sh

# Unit tests for the release-validation gate: each post-publish validation must be able
# to fail, install the release version, load the package and compare the version.
test-release-validation-gate:
	@bash tools/tests/release_validation_gate_test.sh

# Unit tests for the rustc-wrapper gate: a job that runs cargo under a RUSTC_WRAPPER it
# never installs is red, including through a local composite action.
.PHONY: test-rustc-wrapper-gate
test-rustc-wrapper-gate:
	@bash tools/tests/rustc_wrapper_gate_test.sh

# Unit tests for the changelog-completeness gate (#1127). The gate ITSELF cannot
# run in ShellGates — it needs real git history, and `.dagger/main.go` ignores
# `.git` and runs `git init -q .` in the container, so it would find zero
# `Closes #N` and pass vacuously. This self-test builds its own fixture
# repositories in a temp dir, so it belongs here alongside test-deadline-gate.
.PHONY: test-changelog-gate
test-changelog-gate:
	@bash tools/tests/changelog_gate_test.sh

# Gate: ensure no axum 0.7-style `:param` captures slip back into `.route()` calls.
# Issue #316 prevention — see `tools/check-route-syntax.sh` for the load-bearing multi-line awk.
.PHONY: lint-routes
lint-routes:
	@bash tools/check-route-syntax.sh

# Gate: no route answers 501 through `not_implemented(` except revoke's conditional one
# (ADR-0007 §5, check 8). Positional: file and line; the allowance is a file and a count.
.PHONY: lint-not-implemented
lint-not-implemented:
	@bash tools/check-not-implemented.sh

# Red-capability pin for the gate above: every branch proved on a synthetic tree,
# including that a missing helper exits 2 rather than scanning nothing.
.PHONY: test-not-implemented-gate
test-not-implemented-gate:
	@bash tools/tests/not_implemented_test.sh

.PHONY: lint-guard-parity
lint-guard-parity:
	@bash tools/check-guard-parity.sh

# Gate: the test side of lint-guard-parity. A test asserting a guard's behaviour must
# take the temp_env lock; a lock-free reader races the sibling that sets the bypass and
# reads the guard as disabled (#1272). Entry points are discovered from the chokepoint
# on each run, so a new guard is covered when it is written.
.PHONY: lint-guard-test-lock
lint-guard-test-lock:
	@python3 tools/check-guard-test-lock.py

# Red-capability pin for the gate above. It now passes over the tree it just cleaned,
# so green is equally consistent with "the scan matches nothing" — every branch is
# proved on a synthetic tree, including that an empty scan exits 2 rather than 0.
.PHONY: test-guard-test-lock-gate
test-guard-test-lock-gate:
	@bash tools/tests/guard_test_lock_test.sh

# Gate: every router the server mounts goes through its mount ledger, so the transport
# refusal matrix sees every transport (#1844). Positional: each unrecorded `.merge(` /
# `.nest(` / `.nest_service(` is reported at its own site.
.PHONY: lint-mount-ledger
lint-mount-ledger:
	@python3 tools/check-mount-ledger.py

# Red-capability pin for the gate above, on a synthetic tree.
.PHONY: test-mount-ledger-gate
test-mount-ledger-gate:
	@bash tools/tests/mount_ledger_test.sh

# Gate: every authorization, session and tenancy guard site in tools/guard-sites.toml
# resolves, and every guard-shaped call of an enabled family is an inventoried site
# (#1846). Positional: each unrowed call is reported at its own site.
.PHONY: lint-guard-sites
lint-guard-sites:
	@python3 tools/check-guard-sites.py

# Red-capability pins for the gate above and for the mutant runner's verdicts.
.PHONY: test-guard-sites-gate
test-guard-sites-gate:
	@bash tools/tests/guard_sites_test.sh

.PHONY: test-guard-mutants-gate
test-guard-mutants-gate:
	@bash tools/tests/guard_mutants_test.sh

# Gate: every test suite (tests/*.rs binary and feature-gated lib test module)
# maps to a CI leg that actually executes it — execution coverage is a checked
# artifact, not an inference. Exemptions: tools/suite-coverage-exemptions.toml.
.PHONY: lint-suite-coverage
lint-suite-coverage:
	@python3 tools/check-suite-coverage.py

# Gate: every `-- --ignored` line in this Makefile, .dagger/main.go and the workflows
# names a target that holds an `#[ignore]` test (#2103). On a target with none, the
# line runs 0 tests and exits 0, as five lines here did.
.PHONY: lint-ignored-lines
lint-ignored-lines:
	@python3 tools/check-ignored-lines.py

# Red-capability pin for the check above, over a fixture crate.
.PHONY: test-ignored-lines-gate
test-ignored-lines-gate:
	@bash tools/tests/ignored_lines_test.sh

# Gate: no pipe into `grep -q` where `pipefail` is on (#1870, #1961). grep exits at its
# first match, the writer takes SIGPIPE, and the pipeline reports 141: a match reads as
# a miss. Write `grep -c … >/dev/null`, which reads all its input.
.PHONY: lint-grep-q-pipefail
lint-grep-q-pipefail:
	@python3 tools/check-grep-q-pipefail.py

# Red-capability pin for the check above, over fixture trees.
.PHONY: test-grep-q-pipefail-gate
test-grep-q-pipefail-gate:
	@bash tools/tests/grep_q_pipefail_test.sh

# Red-capability pin for tools/clippy-config-paths-gate.sh, which `clippy` runs through.
.PHONY: test-clippy-config-paths-gate
test-clippy-config-paths-gate:
	@bash tools/tests/clippy_config_paths_test.sh

# Gate: every crate the doctest leg skips holds no doctest. The list is a claim about
# the source; the check refuses it when a skipped crate's docs hold anything rustdoc
# could compile.
.PHONY: lint-doctest-skip
lint-doctest-skip:
	@python3 tools/check-doctest-skip.py >/dev/null

# Red-capability pin for the check above, over fixture trees.
.PHONY: test-doctest-skip-gate
test-doctest-skip-gate:
	@bash tools/tests/doctest_skip_test.sh

# Gate: snapshot pairing, both directions — every .snap registered in
# tests/snapshot-pairs.md, and no registry row naming a deleted snapshot (#986).
.PHONY: lint-snapshot-pairing
lint-snapshot-pairing:
	@bash tools/check-snapshot-pairing.sh

# Gate: a local red suite must be a *classified* red suite. Without a reachable
# Postgres ~80 tests fail, and "known-red plus grep for your own error strings"
# is how the eighty-first — a real regression — gets waved through.
.PHONY: test-baseline
test-baseline:
	@bash tools/check-test-baseline.sh

# Gate: no #[test] whose body is only comments — write the assertion or delete
# the test (#895; the #748 empty-test class).
.PHONY: lint-empty-tests
lint-empty-tests:
	@bash tools/check-empty-tests.sh

# Gate: an integration test binary must reference the crate it tests. The rung above
# lint-empty-tests: not a comment-only body but a whole file asserting against mocks it
# defined itself — 23 of them, 9,633 lines, 273 tests, fourteen on the required leg
# (#1269, #1270). See tools/check-test-subject.py.
.PHONY: lint-test-subject
lint-test-subject:
	@python3 tools/check-test-subject.py

# Red-capability pin for the gate above. Deleting 273 always-green tests cannot be
# proved by a green run, so every branch is proved on a synthetic tree instead.
.PHONY: test-test-subject-gate
test-test-subject-gate:
	@bash tools/tests/test_subject_test.sh

# Gate: every crates/*/src/**/*.rs is reached by a `mod` declaration from a target
# root. A file nothing declares is never compiled, linted or tested, yet reads as
# code to every reviewer and every line count: 11 files, 2,652 lines, sat in the
# workspace that way (#1640). Walks the filesystem, not git, so it holds in the
# preflight container. See tools/check-module-reachability.py.
.PHONY: lint-module-reachability
lint-module-reachability:
	@python3 tools/check-module-reachability.py

# Red-capability pin for the gate above: each resolution rule (#[path], inline
# modules, non-mod-rs stems, cfg'd and commented-out declarations) on a synthetic crate.
.PHONY: test-module-reachability-gate
test-module-reachability-gate:
	@bash tools/tests/module_reachability_test.sh

# Gate: every public module of a library is used by a binary, through non-test code,
# or is allowlisted (test-support, api, or pending #N) in
# tools/pub-mod-callers-allowlist.txt. A `pub` module is exempt from rustc's dead_code
# lint, so one whose last caller went stayed green, kept alive by its own tests (#1640).
# Walks the filesystem, not git. `--why <module>` prints the chain that makes one used.
.PHONY: lint-pub-mod-callers
lint-pub-mod-callers:
	@python3 tools/check-pub-mod-callers.py

# Red-capability pin for the gate above: each resolution and liveness rule, and each
# allowlist rule, on a synthetic two-crate workspace.
.PHONY: test-pub-mod-callers-gate
test-pub-mod-callers-gate:
	@bash tools/tests/pub_mod_callers_test.sh

# Gate: issue references (#NNN) in non-test code comments may only go down, per crate,
# against tools/prose-ratchet-baseline.txt. Doc comments are API contract and are not
# counted. A comment says what the code does; the commit cites the issue. A drop must be
# locked into the baseline. Walks the filesystem, not git. `--print` gives current counts.
.PHONY: lint-prose-ratchet
lint-prose-ratchet:
	@python3 tools/check-prose-ratchet.py

# Gate: every doc with a ```rust block is a doctest of the `fraiseql` crate (its
# `#[cfg(doctest)] mod docs_examples` includes it, and `cargo test --doc` compiles it),
# and no doc fence is bare (rustdoc compiles a bare fence as Rust).
.PHONY: lint-docs-rust-examples
lint-docs-rust-examples:
	@python3 tools/check-docs-rust-examples.py

# Red-capability pin for the gate above.
.PHONY: test-docs-rust-examples-gate
test-docs-rust-examples-gate:
	@bash tools/tests/docs_rust_examples_test.sh

# Gate: no `fraiseql_query` call under docs/, examples/, tests/ or a crate's tests/
# interpolates a value into the query text; values go through the variables argument,
# which no value can escape.
.PHONY: lint-query-variables
lint-query-variables:
	@python3 tools/check-query-variables.py

# Red-capability pin for the gate above.
.PHONY: test-query-variables-gate
test-query-variables-gate:
	@bash tools/tests/query_variables_test.sh

# Red-capability pin for the performance gate's comparator (tools/check-perf-instructions.py):
# rises and drops past the threshold, the mean over runs, and every vacuous shape (no
# summaries, a 0 count, an unmatched bench or baseline line). The gate itself runs in
# the `perf (instructions)` leg: `dagger call perf-instructions --source=.`.
.PHONY: test-perf-instructions-gate
test-perf-instructions-gate:
	@bash tools/tests/perf_instructions_test.sh

# Red-capability pin for the ratchet: each counted shape, each ignored one (doc comments,
# literals, raw strings, test code) and each baseline rule, on a one-crate tree.
.PHONY: test-prose-ratchet-gate
test-prose-ratchet-gate:
	@bash tools/tests/prose_ratchet_test.sh

# Gate: every test binary that finds a database before it loads its document carries a
# database-free test that loads it. Those binaries self-skip without DATABASE_URL, so a
# load-time refusal of their document reports as a pass in every run without one —
# 2b843cd27, 12 tests red under a green preflight. preflight runs no other tests, so
# this runs the guards themselves, not only a scan for them.
.PHONY: test-e2e-documents
test-e2e-documents:
	@python3 tools/check-e2e-documents.py

# The same gate's static half alone: every such binary defines the guard, and the guard
# cannot skip. It needs no toolchain, so it runs in the Dagger ShellGates leg; the guards
# themselves run in the legs that run their binaries (#1945).
.PHONY: lint-e2e-documents
lint-e2e-documents:
	@python3 tools/check-e2e-documents.py --static-only

# Red-capability pin for the gate above: its static half, on synthetic trees.
.PHONY: test-e2e-documents-gate
test-e2e-documents-gate:
	@bash tools/tests/e2e_documents_test.sh

# Gate: deployment artifacts must not re-expose backing services (H46) or regress the
# Phase-13 sweep — loopback-only ports, authenticated Redis, fail-loud secrets, no
# :latest pins, no readOnlyRootFilesystem: false. See tools/check-deploy-security.sh.
.PHONY: lint-deploy-security
lint-deploy-security:
	@bash tools/check-deploy-security.sh

# Gate: the Dockerfile's OCI version label, the Helm chart's version/appVersion and
# values.yaml's image.tag must equal the workspace version, and the chart's default
# image must be one this project publishes. They sat at 2.1.1 / 2.1.0 / 2.8.0 against a
# 2.14.1 product, and `repository: fraiseql` resolved to an image that does not exist
# (#1129) — tools/release.sh now bumps them, and this is what says so.
.PHONY: lint-deploy-versions
lint-deploy-versions:
	@bash tools/check-deploy-versions.sh

# Gate: every compiled schema a workflow BOOTS must name the version this tree
# builds. Since #1304 the server refuses an artifact its own build did not
# produce, and `release-smoke.yml` is `release/*`/`v*`-triggered — so a stale
# `docker/e2e/*.compiled.json` is invisible on dev and fails at the tag.
.PHONY: lint-compiled-schema-stamp
lint-compiled-schema-stamp:
	@bash tools/check-compiled-schema-stamp.sh

# Unit tests for the gate above. It discovers its own subjects from the workflows,
# so "found nothing" must be exit 2 (cannot run) rather than success — a vacuous
# pass here would read as "the artifacts are current" over a tree never examined.
.PHONY: test-compiled-schema-stamp-gate
test-compiled-schema-stamp-gate:
	@bash tools/tests/compiled_schema_stamp_test.sh

# Gate: fuzz.yml's matrix may only name targets that exist in the crate it names.
# `{crate: fraiseql-core, target: toml_config}` named a target that had MOVED to
# fraiseql-server, so `cargo fuzz build` failed and every scheduled run was red from at
# least 2026-05-17 (#1128). And the other way: every target on disk has a row, or a line
# in tools/fuzz-unscheduled-targets.txt saying why not (#1847).
.PHONY: lint-fuzz-targets
lint-fuzz-targets:
	@bash tools/check-fuzz-targets.sh

# The red capability of the gate above: an unscheduled target, an exemption with no
# reason or naming a scheduled or missing target, a row naming the wrong crate.
.PHONY: test-fuzz-targets-gate
test-fuzz-targets-gate:
	@bash tools/tests/fuzz_targets_test.sh

# Gate: every Compose file a tracked file hands to `docker compose -f` exists.
#
# `make federation-up`, `make federation-down` and `make test-federation` all drove
# `docker/federation-ci/docker-compose.yml`, which is in no commit reachable from `dev`.
# All three failed on their first line while `make help` and two docs pages advertised
# them (#1219). `check-examples-integrity.sh` cannot see this: it discovers compose files
# with `find` and checks what is inside them, so a file that is only *named* is outside
# every check in the repository.
.PHONY: lint-compose-references
lint-compose-references:
	@bash tools/check-compose-references.sh

# Unit tests for the gate above: a dead reference fails, a live one passes, a path quoted
# in a comment is prose rather than an entry point, and a scan that matches nothing fails
# instead of reporting OK over every entry point in the repository.
.PHONY: test-compose-references-gate
test-compose-references-gate:
	@bash tools/tests/compose_references_test.sh

# First-party sessions are signed by `[auth_issuer]` (ES256) and validated by OidcValidator;
# the shared-secret path was removed. This gate keeps it from coming back: a file
# outside its EXEMPT and PENDING lists that names the path fails, and PENDING only shrinks.
.PHONY: lint-no-first-party-hs256
lint-no-first-party-hs256:
	@bash tools/check-no-first-party-hs256.sh

# Unit tests for the gate above: an unlisted file fails, a comment is not a hit, a listed
# file that stopped matching fails, the directory entry covers its own directory only, and
# a tree where nothing matches fails.
.PHONY: test-no-first-party-hs256-gate
test-no-first-party-hs256-gate:
	@bash tools/tests/no_first_party_hs256_test.sh

# Gate: every apt install in the Dagger module refreshes its package index in the same exec
# (`aptInstall`). An install layered on a separately cached `apt-get update` 404'd preflight
# on every run once Ubuntu replaced a package the cached index still named (2026-10-10).
# Gate: every pg_tviews version the repository names is the one docker/pg-tviews builds
# (#1391). The pin lives in that Dockerfile; docs and compose follow it.
.PHONY: lint-pg-tviews-pin
lint-pg-tviews-pin:
	@bash tools/check-pg-tviews-pin.sh

# Unit tests for the gate above: another version fails, the pin passes, no pin fails.
.PHONY: test-pg-tviews-pin-gate
test-pg-tviews-pin-gate:
	@bash tools/tests/pg_tviews_pin_test.sh

# The release benchmark's quiet-window check, its baseline host guard, and the check
# that a run measured something.
.PHONY: test-bench-quiet-window-gate test-detect-regression-host-gate test-bench-require-measured-gate
test-bench-quiet-window-gate:
	@bash tools/tests/bench_quiet_window_test.sh
test-detect-regression-host-gate:
	@bash tools/tests/detect_regression_host_test.sh
test-bench-require-measured-gate:
	@bash tools/tests/bench_require_measured_test.sh

# The pg_tviews suites (#1391), locally: builds the pinned image on first use (a pgrx
# compile, ~2.5 min cold), starts it on 127.0.0.1:5441, and runs them against it. Their
# runtime-absent case reads DATABASE_URL, a database without pg_tviews. Its `/metrics` case
# without the preload reads the database tools/pg-tviews-unpreloaded.sh builds (with the
# same entrypoint as the leg's pgTviewsUnpreloadedService), required, then removed.
.PHONY: test-pg-tviews
test-pg-tviews:
	docker compose -f docker/docker-compose.test.yml up -d --wait postgres-tviews-test
	url=$$(bash tools/pg-tviews-unpreloaded.sh up) && \
	TVIEWS_UNPRELOADED_DATABASE_URL="$$url" FRAISEQL_TEST_REQUIRE_TVIEWS_UNPRELOADED=1 \
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	DATABASE_URL=$${DATABASE_URL:-postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql} \
	cargo test -p fraiseql-server --features metrics --test tviews_cascade_e2e_pg -- --test-threads=1; \
	status=$$?; bash tools/pg-tviews-unpreloaded.sh down; exit $$status
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	cargo test -p fraiseql-server --test cache_embedding_eviction_e2e_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	cargo test -p fraiseql-server --test cache_by_id_eviction_e2e_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	cargo test -p fraiseql-server --features rest --test cache_tviews_feed_e2e_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	cargo test -p fraiseql-server --features rest --test rest_embedding_native_join_e2e_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	cargo test -p fraiseql-server --test graphql_nested_native_join_e2e_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	DATABASE_URL=$${DATABASE_URL:-postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql} \
	cargo test -p fraiseql-cli --features test-postgres --test cascade_source_compile_test --test doctor_pg_tviews_pg --test tview_index_advice_pg -- --test-threads=1
	TVIEWS_DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5441/test_fraiseql \
	DATABASE_URL=$${DATABASE_URL:-postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql} \
	cargo test -p fraiseql-cli --features test-postgres --test temporal_key_advice_pg -- --include-ignored --test-threads=1

.PHONY: lint-dagger-apt
lint-dagger-apt:
	@bash tools/check-dagger-apt.sh

# Unit tests for the gate above: each argument-vector form fails, the helper passes, and a
# module the gate cannot see fails rather than reporting OK.
.PHONY: test-dagger-apt-gate
test-dagger-apt-gate:
	@bash tools/tests/dagger_apt_test.sh

# Gate: CI names a version for every tool it installs.
#
# `.dagger/main.go` installed the #946 SCIM conformance client with `pip install
# scim2-tester httpx`. On 2026-09-20 two upstream releases nine hours apart reddened
# `integration (saml)` on a branch nobody had touched — first an `AttributeError` before
# any check ran, then a conformant DELETE→204→GET→404 reported as a defect. A build that
# can redden because someone else shipped is a build whose reds stop being believed.
.PHONY: lint-ci-install-pins
lint-ci-install-pins:
	@bash tools/check-ci-install-pins.sh

.PHONY: test-ci-install-pins-gate
test-ci-install-pins-gate:
	@bash tools/tests/ci_install_pins_test.sh

# The #1421 stale-artifact check (run by tools/ci-target-canary.sh in every Dagger Rust
# leg) proved on synthetic cargo logs: it must name an artifact older than the leg's source
# stamp, and leave this leg's own, freshly compiled and registry artifacts alone.
.PHONY: test-ci-stale-artifacts-gate
test-ci-stale-artifacts-gate:
	@bash tools/tests/ci_stale_artifacts_test.sh

# #1596: a Rust leg stamps its sources inside each exec, once it holds its target volume
# (BASH_ENV sources tools/ci-stamp-sources.sh); the gate refuses the three ways to lose it,
# and its self-test proves the stamp catches another leg's later artifact in a tree with no
# .git, the shape a leg and ShellGates have.
.PHONY: lint-ci-source-stamp
lint-ci-source-stamp:
	@bash tools/check-ci-source-stamp.sh

.PHONY: test-ci-source-stamp-gate
test-ci-source-stamp-gate:
	@bash tools/tests/ci_source_stamp_test.sh

# Gate: a FraiseQL image named in documentation must be pullable.
#
# `deploy/deployment-security-guide.md` showed `image: fraiseql:1.8.0-hardened` — a bare
# name, which Docker resolves to `docker.io/library/fraiseql`, the official-images
# namespace this project cannot publish to. #1129 was the same defect in a real manifest;
# this was it in a fenced code block, where `check-deploy-versions.sh`,
# `check-deploy-security.sh` and `check-image-parity.py` do not look (#1220).
.PHONY: lint-doc-image-refs
lint-doc-image-refs:
	@bash tools/check-doc-image-refs.sh

# Gate: no shipped file sends a reader to `.phases/`, which is gitignored and so
# does not exist in a clone. 27 tracked files did, including 16 comments in
# `.dagger/*.go` that deferred their reasoning to `parity-notes.md` — published as
# docs/contributing/dagger-parity-notes.md (#1210).
.PHONY: lint-phases-citations
lint-phases-citations:
	@bash tools/check-phases-citations.sh

# Unit tests for the gate above. Four ways to go red, including an exemption list
# that has grown to cover everything.
.PHONY: test-phases-citations-gate
test-phases-citations-gate:
	@bash tools/tests/phases_citations_test.sh

# Gate: every path the image Dockerfiles COPY — and each Dockerfile itself —
# survives the `+ignore` filter on `.dagger/image.go`'s functions. The filter was
# narrowed so a docs-only push stops rebuilding every layer (#1215), and that makes
# it a second, invisible copy of what the build needs. Add a COPY for a path it
# drops and the build runs against a context missing it.
.PHONY: lint-image-context
lint-image-context:
	@bash tools/check-image-context.sh

# Unit tests for the gate above. Seven ways to go red, including the two that got
# through by hand: a second Dockerfile, and the Dockerfile itself.
.PHONY: test-image-context-gate
test-image-context-gate:
	@bash tools/tests/image_context_test.sh

# Unit tests for the gate above. Both spellings must be reachable: written as one regex,
# the `--image=` form matched nothing while the gate reported OK.
.PHONY: test-doc-image-refs-gate
test-doc-image-refs-gate:
	@bash tools/tests/doc_image_refs_test.sh

# Gate: every crates/*/fuzz crate COMPILES, and every fuzz target on disk is a [[bin]].
#
# The gate above is existence-only by design — pure bash, no toolchain, so it can run in
# the Dagger ShellGates container — and can therefore never see a type error. Nothing
# else compiles a fuzz crate either: each declares its own `[workspace]`, so it is
# outside `cargo check --workspace` and every clippy/test leg. `570baf9b1` changed
# `WhereClause::from_graphql_json`'s signature on 2026-08-20 and two fraiseql-db targets
# sat at error[E0308] through two red scheduled runs before anyone looked (#1254).
#
# Not in ShellGates: this one runs cargo. It is inline in `preflight` and a sibling gate
# of the Dagger `Preflight` function, the same shape as clippy and check-default.
.PHONY: check-fuzz
check-fuzz:
	@bash tools/check-fuzz-compiles.sh

# Unit tests for the gate above. Every half has to be able to go red: a fuzz crate
# that does not compile, a target file with no [[bin]] — the gate's own blind spot,
# since `cargo check` only builds the bins the manifest declares — and a Cargo.lock
# that is stale or missing.
.PHONY: test-fuzz-compiles-gate
test-fuzz-compiles-gate:
	@bash tools/tests/fuzz_compiles_test.sh

# Gate: every standalone Cargo project under examples/ compiles. They declare their
# own [workspace], so nothing else in this repository builds them — the same shape
# as the fuzz crates above. `examples/rust/flight_client` is the one that motivated
# it (#1200): a client a reader is invited to `cargo run` could rot unnoticed.
#
# Not in ShellGates: this one runs cargo.
.PHONY: check-example-crates
check-example-crates:
	@bash tools/check-example-crates-compile.sh

# Unit tests for the gate above. Four ways to go red, none of them observable from
# a passing run of the real tree.
.PHONY: test-example-crates-gate
test-example-crates-gate:
	@bash tools/tests/example_crates_compile_test.sh

# Gate: every secret a publish job reads is one release.yml's prerequisite step checks
# (#1518: the Rust SDK read CARGO_REGISTRY_TOKEN, which the repository never had, and
# failed at the upload). See tools/check-publish-secrets.py.
.PHONY: lint-publish-secrets
lint-publish-secrets:
	@python3 tools/check-publish-secrets.py

# Red-capability pin for the gate above.
.PHONY: test-publish-secrets-gate
test-publish-secrets-gate:
	@bash tools/tests/publish_secrets_test.sh

# Gate: pin the set of production files that READ TypeDefinition.internal (#665), so a
# property-named flag cannot silently grow new consumers. See tools/check-internal-flag-sites.sh.
.PHONY: lint-internal-flag
lint-internal-flag:
	@bash tools/check-internal-flag-sites.sh

# Regenerate the capability table of docs/features-compatibility-matrix.md from
# fraiseql_server::capability::REGISTRY. The same test, without the variable, fails
# when the table is stale.
.PHONY: gen-matrix
gen-matrix:
	@FRAISEQL_GEN_MATRIX=1 cargo test -p fraiseql-server --lib --all-features \
		capability::manifest_tests::the_capability_matrix_is_generated_from_the_registry

# Gate: no production file builds a write statement and hands it to `execute_raw_query`
# (#1327). Every gate on a write — the Authorizer, requires_role, requires_actor, argument
# and selection validation, the change-log write and the before:mutation chain — is
# enforced in `execute_mutation_impl`; a raw write skips all of them. A call to the
# adapter's write itself, what the gRPC arm did until #1330, is banned by clippy
# (`disallowed-methods` in .clippy.toml). The KNOWN_RAW list is empty — an entry there is
# a named defect with an issue, never a resting place. See tools/check-mutation-dispatch-sites.sh.
.PHONY: lint-mutation-dispatch
lint-mutation-dispatch:
	@bash tools/check-mutation-dispatch-sites.sh

# Gate: a write's SELECTION SET is derived, never invented (#1331, #1352). An empty
# selection set is the permissive shape — `project_entity` returns the whole entity and
# the #423 field authorizer short-circuits with zero calls — so REST's anonymous arm
# served gated fields an authenticated caller is refused, and gRPC's own derivation
# reached the same empty set through `unwrap_or_default()`. The third rule bans the other
# half: a GraphQL document rebuilt out of arguments by `format!`, which is why an
# authenticated REST write could not carry a nested body at all.
.PHONY: lint-write-selections
lint-write-selections:
	@python3 tools/check-write-selection-sources.py

# The red capability of the gate above: each rule goes red on the shape it names, and
# stays green on the adjacent shape that is correct — an empty `inline_arguments`, an
# error message opening with the word "mutation", a `#[cfg(test)]` module, a doc comment
# quoting the defect. The twins are what stop it being a blunt ban on `&[]`.
.PHONY: test-write-selections-gate
test-write-selections-gate:
	@bash tools/tests/write_selection_sources_test.sh

# Gate: first-party auth's tables live in schema `auth`, which a separate role owns. An
# inventory test proves where the DDL creates them; this gate catches the query that
# still reads the old location, which fails only at runtime on the path that runs it.
# Positional: string literals in code, never comments or prose.
.PHONY: lint-auth-schema
lint-auth-schema:
	@python3 tools/check-auth-schema-qualification.py

# The red capability of the gate above: each rule reddens on its shape and stays green on
# its twins — a comment naming the old location, an engine table in `core`,
# `_system.session_state`, an application `tb_user` outside the auth crate.
.PHONY: test-auth-schema-gate
test-auth-schema-gate:
	@bash tools/tests/auth_schema_qualification_test.sh

# Gate: SQL text built at runtime reaches sqlx only through `sqlx::AssertSqlSafe`, and each
# one carries a `// Reason:` saying what the text is built from, beside the call.
.PHONY: lint-sql-safe
lint-sql-safe:
	@python3 tools/check-sql-safe-assertions.py

# The red capability of the gate above: a call with no reason, a neighbour's reason, a
# reason cut off by a blank line, an import or alias of the type.
.PHONY: test-sql-safe-gate
test-sql-safe-gate:
	@bash tools/tests/sql_safe_assertions_test.sh

# Gate: every DDL script FraiseQL runs takes the workspace's one DDL lock
# (`fraiseql_guard::ddl::lock_prefixed` / `locked_statement`), and every advisory lock
# statement binds its key or says why it is not the DDL lock.
.PHONY: lint-ddl-lock
lint-ddl-lock:
	@python3 tools/check-ddl-lock.py

# The red capability of the gate above: DDL from a literal, a constant, a `format!`, a
# `concat!`, an `include_str!`, a writer or a wrapper, reaching an executor unlocked.
.PHONY: test-ddl-lock-gate
test-ddl-lock-gate:
	@bash tools/tests/ddl_lock_test.sh

# Gate: every site that turns a credential into a principal also resolves that
# principal's DB identity (#1336). Three transports produced one and dispatched it
# unresolved for three releases, and nothing could see it: a transport that skips the
# resolve answers every request reading no enriched field exactly as one that runs it.
.PHONY: lint-principal-producers
lint-principal-producers:
	@bash tools/check-principal-producers.sh

# Red-capability pin for the gate above, on a synthetic tree.
.PHONY: test-principal-producers-gate
test-principal-producers-gate:
	@bash tools/tests/principal_producers_test.sh

# Gate: every config struct reachable from `ServerConfig` refuses unknown keys (#1337).
# serde does not propagate `deny_unknown_fields` into nested structs, so a mistyped key
# in a section was discarded in silence — leaving security switches at their defaults.
# The sections are DISCOVERED, and the count asserted, so a new one cannot join unchecked.
# A second walk does the same from `TomlSchema`, the type `fraiseql.toml` is read into.
.PHONY: lint-config-deny-unknown
lint-config-deny-unknown:
	@python3 tools/check-config-deny-unknown.py

# Unit tests for the gate above, for a hand-written `Deserialize`: each way its allow-list
# entry can pin nothing (no entry, a missing or ignored test, a stale or malformed entry)
# must fail by name. For the walk from `TomlSchema`: an open table, a flatten, a stale or
# malformed open entry, a type read through another and a name two crates define.
.PHONY: test-config-deny-unknown-gate
test-config-deny-unknown-gate:
	@bash tools/tests/config_deny_unknown_test.sh

# Gate: every `bail!` / `anyhow!` on the fraiseql.toml load path is a value check listed with
# the key it checks and why it carries no code; a refused key is `refusal(...)`, so it exits 2
# and names its code, table and key.
.PHONY: lint-config-refusals
lint-config-refusals:
	@python3 tools/check-config-refusals.py

# Unit tests for the gate above: an unlisted site, a stale or malformed entry, two sites one
# entry would stand for, and an empty scope each fail by name.
.PHONY: test-config-refusals-gate
test-config-refusals-gate:
	@bash tools/tests/config_refusals_test.sh

# Gate: the `value_json` seam has one owner (#719). Hand-rolled JSON escaping, in-band
# `$`-prefix variable detection and silent `.ok()` fallbacks on an argument parse are all
# refused — a dropped `where:` argument widens a result set instead of narrowing it.
.PHONY: lint-value-json
lint-value-json:
	@bash tools/check-value-json-seam.sh

# Gate: every FRAISEQL_* env var named in docs/, examples/*.md or README.md has a reader
# in the workspace (#838) — a runbook step that exports an inert variable is an
# instruction to do nothing, applied during a live incident.
.PHONY: lint-docs-env-vars
lint-docs-env-vars:
	@bash tools/check-docs-env-vars.sh

# Gate: docs status lines claiming "vX.Y.Z released" must match the workspace version
# (#735 — overview.md once carried three different versions at once).
.PHONY: lint-docs-version
lint-docs-version:
	@bash tools/check-docs-version.sh

# Unit tests for the gate above: a status line, the README install pin, the fraiseql.toml
# JSON Schema's `$id` and a documented schema URL each naming another version fail.
.PHONY: test-docs-version-gate
test-docs-version-gate:
	@bash tools/tests/docs_version_test.sh

# Regenerate docs/schemas/fraiseql.toml.schema.json from the grammar the compiler reads.
.PHONY: bless-toml-schema
bless-toml-schema:
	@FRAISEQL_BLESS_TOML_SCHEMA=1 cargo test -q -p fraiseql-cli --lib \
		json_schema::tests::the_committed_schema_is_the_generated_one

# Regenerate docs/schemas/fraiseql-server.schema.json from the type the server reads its
# configuration file into, built with every feature so the schema names every key.
.PHONY: bless-server-config-schema
bless-server-config-schema:
	@FRAISEQL_BLESS_SERVER_CONFIG_SCHEMA=1 FRAISEQL_TEST_REQUIRE_ALL_FEATURES=1 cargo test -q \
		-p fraiseql-server --all-features --lib server_config::json_schema::

# Gate: every typed TOML config loader has a coverage manifest naming each key's consumer
# (#909 — `fraiseql_core::config` accepted a whole `[server]`/`[database]`/`[cache]` tree
# that no code outside its own module ever read).
.PHONY: lint-config-loaders
lint-config-loaders:
	@bash tools/check-config-loaders.sh

# Gate: a published crate's public API must not name a third-party type the crate does
# not re-export (#1198). `JwtValidator::new` took a `jsonwebtoken::Algorithm` that
# `fraiseql-auth` re-exported nowhere, so the crate's own documented first line did not
# compile: a caller had to add `jsonwebtoken` and guess the major this workspace builds
# against, and a mismatch is a type error in code they never wrote.
.PHONY: lint-public-api-reexports
lint-public-api-reexports:
	@python3 tools/check-public-api-reexports.py

.PHONY: lint-sdk-publication-claims
lint-sdk-publication-claims:
	@python3 tools/check-sdk-publication-claims.py

.PHONY: test-sdk-publication-claims-gate
test-sdk-publication-claims-gate:
	@bash tools/tests/sdk_publication_claims_gate_test.sh

# Red-capability pin for the gate above. Its subjects come from release.yml's `cargo
# publish --package` steps, so a rename there would leave it checking nothing and
# reporting OK — the shape #1206 shipped. Fixture workspaces in a temp dir; this
# repository is never mutated, so an interrupted run leaves nothing half-edited.
.PHONY: test-public-api-reexports-gate
test-public-api-reexports-gate:
	@bash tools/tests/public_api_reexports_gate_test.sh

# Gate: no example provisions or points at a database backend #374 removed. The
# PostgreSQL-only de-scope covered crates/; examples/federation/saga-basic kept a
# running MySQL topology for three phases afterwards (#940), demonstrating in
# working form a shape the engine refuses at boot.
.PHONY: lint-examples-postgres-only
lint-examples-postgres-only:
	@bash tools/check-examples-postgres-only.sh

# Gate: a shipped example must be able to run at all — compose mounts resolve, COPY
# sources exist in the build context, no `|| true` around a build step, no health
# grep that also matches "unhealthy", and every documented `cd` lands somewhere.
# The static tier of the examples gate (#1050-#1054, #1071-#1073); the two executing
# tiers need a toolchain and a database and live in the `examples` integration suite.
.PHONY: lint-examples-integrity
lint-examples-integrity:
	@bash tools/check-examples-integrity.sh

# Gate: every R file under examples/ parses. Nothing in this repository ran,
# linted or even parsed `examples/r/fraiseql_client.R` (#1260) — the example gates
# reach authoring artifacts, compose files and Cargo projects, and none of them
# reaches R. #1200 rewrote that client on a machine with no R installed, so the
# rewrite shipped unverified in the one dimension it was most exposed to.
#
# Parsing is not running: it says nothing about whether the Flight handshake
# works. That is Level 3 of #1260 and needs a live server. Uses Rscript when it is
# on PATH and a pinned r-base image otherwise, and refuses to skip when it has
# neither.
.PHONY: lint-r-examples
lint-r-examples:
	@bash tools/check-r-examples-parse.sh

# Unit tests for the gate above. Five ways to go red, none of them observable from
# a passing run of the real tree — which holds exactly one R file, and it parses.
.PHONY: test-r-examples-gate
test-r-examples-gate:
	@bash tools/tests/r_examples_parse_test.sh

# Manual probe: run examples/r/fraiseql_client.R against a live Arrow Flight
# server that enforces the same handshake and authorization header FraiseQL's
# does. Not a merge gate and not in preflight — the first build compiles libarrow
# from source (~15 min). `lint-r-examples` above is the gate; this is what you run
# when the R client itself changes, because parsing it is not running it (#1260).
.PHONY: probe-r-flight
probe-r-flight:
	@bash tools/r-flight-probe/run.sh

# Gate: an SDK authoring surface removed for having no compiler consumer stays
# removed (#926). The compiled-schema seam denies unknown fields, but a surface that
# never reaches the wire at all — Java's registry-only dispatch config, Dart's
# unreflected annotation — is invisible to it. Only a grep sees those.
.PHONY: lint-sdk-dead-surface
lint-sdk-dead-surface:
	@bash tools/check-sdk-dead-surface.sh

# Gate: a declared-but-unread `feature = []` is a promise the build cannot keep —
# enabling it changes nothing, so the capability it names is either absent or
# reachable another way. Ran in the Dagger leg only until #1135.
.PHONY: lint-feature-chains
lint-feature-chains:
	@bash tools/check-feature-chains.sh

# The feature-check matrix, run natively, exactly as `Dagger — feature matrix` runs it.
#
# #1227: that leg is `push: branches: [dev]`, so it cannot gate a branch, and preflight
# is structurally unable to find what it finds — preflight's clippy pass is
# `--all-features` (a feature-OFF arm is not compiled at all) and its narrow-feature
# pass is `cargo check`, which runs no clippy lints. `94e7b5558` went to `dev` with
# `make preflight` exit 0 and reddened 4 of 47 combos on one nursery lint.
#
# The combo list is DERIVED from .dagger/feature-combos.go, never copied: a literal the
# parser cannot read, or a field it does not model, is fatal, so this can never cover
# fewer combos than the leg declares.
#
# Deliberately NOT in `preflight`: a cold run compiles 47 feature sets (~25 min warm on
# the 8-core box, much longer cold), and a target that slow is a target nobody runs,
# which would weaken every other gate preflight carries. Run it before pushing anything
# under a `#[cfg(feature = ...)]`. `--clippy-only` narrows to the 11 combos the leg
# clippies; any narrowing is printed next to the declared total.
.PHONY: lint-feature-matrix
lint-feature-matrix:
	@bash tools/lint-feature-matrix.sh $(FEATURE_MATRIX_ARGS)

# Red-capability pin for the runner above. Stubs `cargo`, so it compiles nothing and
# needs no toolchain — it pins the one property that matters: the runner cannot
# silently cover fewer combos than .dagger/feature-combos.go declares and still print a
# green summary.
.PHONY: test-feature-matrix-gate
test-feature-matrix-gate:
	@bash tools/tests/feature_matrix_local_test.sh

# Each combo above builds exactly the features it names (#2096). A combo that builds its
# test targets also gets what its crate's dev-dependencies enable, and five combos named
# "no auth" compiled with `auth` that way. `cargo metadata` and `cargo tree` only, in
# about ten seconds. CI runs it at the start of the `Dagger — feature matrix` leg: the
# ShellGates container has no cargo.
.PHONY: lint-feature-combo-features
lint-feature-combo-features:
	@python3 tools/check-feature-combo-features.py

# Red-capability pin for the check above, over a stubbed `cargo`: no toolchain needed.
.PHONY: test-feature-combo-features-gate
test-feature-combo-features-gate:
	@bash tools/tests/feature_combo_features_test.sh

# The cross-SDK conformance harness's own properties, over synthetic dicts — no
# language toolchain, no CLI, no network, so it runs here rather than in the
# eleven-runtime sdk-conformance.yml job. project.py cited a `selftest.py` that had
# never existed (#1118); this is that file.
.PHONY: test-conformance-selftest
test-conformance-selftest:
	@python3 sdks/official/conformance/selftest.py

# Red-capability pin for the INNER feature-gate side of check-suite-coverage.py.
# The gate read feature gates only off the `mod tests;` declaration chain, so a test
# fn behind `#[cfg(feature = "x")]` inside an ungated module was invisible — counted
# covered while compiled out (#1179). Pure text; no cargo, no toolchain.
.PHONY: test-suite-coverage-inner-gates
test-suite-coverage-inner-gates:
	@bash tools/tests/suite_coverage_inner_gates_test.sh

# Red-capability pin for the FILTER side of check-suite-coverage.py. Thirty leg
# invocations carry a positional `--lib`/`--test` filter and nothing checked any
# still matched: cargo prints `running 0 tests` and exits 0, so a rename retires a
# suite silently (#1300). Pure text; no cargo, no toolchain.
# Red-capability pin for the DEFINED-feature side of check-suite-coverage.py.
# `--all-features` turns on only the features a crate defines, so a suite gated on any
# other name is built by no leg; the gate counted it covered (#2619). Pure text.
.PHONY: test-suite-coverage-undefined-feature
test-suite-coverage-undefined-feature:
	@bash tools/tests/suite_coverage_undefined_feature_test.sh

.PHONY: test-suite-coverage-filters
test-suite-coverage-filters:
	@bash tools/tests/suite_coverage_filters_test.sh

# Gate: a runaway-growth ratchet on each crate's production lines, and a check that
# every crate HAS a budget. Unit tests, docs and tests/ are reported, not gated
# (#1882). Ran in no leg at all until #1055/#990 — its only caller was the orphaned
# tools/lint.sh — by which time four crates were over budget and five had no row.
.PHONY: lint-crate-sizes
lint-crate-sizes:
	@bash tools/check-crate-sizes.sh

# Red-capability pin for the gate above: one fixture crate per counting rule.
.PHONY: test-crate-sizes-gate
test-crate-sizes-gate:
	@bash tools/tests/crate_sizes_test.sh

# Red-capability pin for tools/check-crate-growth.py, the per-commit growth bound. The
# check needs real history, so it runs in the Changelog Completeness job, not here;
# locally: `python3 tools/check-crate-growth.py` (the commits since origin/dev).
.PHONY: test-crate-growth-gate
test-crate-growth-gate:
	@bash tools/tests/crate_growth_test.sh

# No release file keeps its own list of the workspace's crates: the crates a release
# publishes come from tools/publish-order.py. A deliberate subset says why with a
# `crate-subset: <reason>` comment. The gate, then its red-capability pin.
.PHONY: lint-crate-lists test-crate-lists-gate
lint-crate-lists:
	@python3 tools/check-crate-lists.py

test-crate-lists-gate:
	@bash tools/tests/crate_lists_test.sh

# Every official SDK's own test suite and linters (#1346): SDK conformance checks what
# each SDK emits, not its unit tests, and no other local target runs those. A missing
# toolchain falls back to the SDK's container; anything that could not run fails the
# target, by name. One SDK: `make test-sdks SDKS=python`.
.PHONY: test-sdks
test-sdks:
	@bash tools/test-sdks.sh $(SDKS)

# Gate: every official SDK is gated by a workflow that runs on a BRANCH push. Four of
# the eleven were not — two declared `tags` with no `branches` (which suppresses every
# branch push), one was post-merge-only, and the official Ruby SDK's tests ran nowhere
# because the workflow named for it watches the community copy (#1119).
.PHONY: lint-sdk-workflows
lint-sdk-workflows:
	@python3 tools/check-sdk-workflow-coverage.py

# Red-capability pin for the gate above, rule by rule, and for tools/sdk-suites-matrix.py,
# which decides what the required `SDK suites` check runs (#1467).
.PHONY: test-sdk-workflows-gate
test-sdk-workflows-gate:
	@bash tools/tests/sdk_workflow_coverage_test.sh

# Gate: the Go SDK's floor is go.mod's `go` line, and the SDK matrix's oldest Go, every
# literal setup-go pin, the conformance container and the docs agree with it. Four files
# pinned Go independently, and the README promised 1.22 against go.mod's 1.23 (#2278).
.PHONY: lint-go-floor
lint-go-floor:
	@python3 tools/check-go-floor.py

# Red-capability pin for the gate above: one mutation per rule.
.PHONY: test-go-floor-gate
test-go-floor-gate:
	@bash tools/tests/go_floor_test.sh

# Gate: `make preflight` must run everything the Dagger ShellGates leg runs, or
# its "Safe to push" line is false. Two lists maintained by hand in two files
# drift silently, and did twice (#1135).
# Gate: no job may be gated on an event or a ref its own workflow cannot receive.
# The 2026-05-31 Dagger migration stripped push-to-branch and pull_request triggers
# and left the job conditions that referenced them behind, so docker-build.yml kept
# two jobs that read as image coverage and had never once run — an unreachable job
# is absent from the checks list, not reported as skipped (#1206).
.PHONY: lint-workflow-reachability
lint-workflow-reachability:
	@python3 tools/check-workflow-job-reachability.py

# Red-capability pin for the gate above, in both directions: it must flag the
# shapes that cannot run, and must NOT flag any trigger form this repo uses — a
# false positive there costs a real job.
.PHONY: test-workflow-reachability-gate
test-workflow-reachability-gate:
	@bash tools/tests/workflow_job_reachability_test.sh

# Gate: every self-hosted job a push can start routes through already-passed.yml,
# so a fast-forward to dev skips a SHA that already passed the leg. The guard was
# opt-in, and `Dagger — image` reran ~36 min on every dev push without it.
.PHONY: lint-already-passed-guard
lint-already-passed-guard:
	@python3 tools/check-already-passed-guard.py

.PHONY: test-already-passed-guard-gate
test-already-passed-guard-gate:
	@bash tools/tests/already_passed_guard_test.sh

# Gate: every ghcr.io/fraiseql image the Dagger module pulls is a destination of
# mirror-base-images.yml, and every mirror row is pinned (#1453: the minio row's
# `:latest` source stopped resolving and the weekly mirror was red for weeks).
.PHONY: lint-image-mirror
lint-image-mirror:
	@python3 tools/check-image-mirror.py

.PHONY: test-image-mirror-gate
test-image-mirror-gate:
	@bash tools/tests/image_mirror_test.sh

# Gate: no two SQL fixtures declare the same `public` relation. Two did, with
# incompatible shapes and both under `CREATE TABLE IF NOT EXISTS`, so whichever
# loaded second was a silent no-op and its dependent objects failed to build —
# the state #1229 reported and could not attribute (#1281).
.PHONY: lint-fixture-collisions
lint-fixture-collisions:
	@python3 tools/check-fixture-relation-collisions.py

# Red-capability pin for the gate above, in both directions.
.PHONY: test-fixture-collisions-gate
test-fixture-collisions-gate:
	@bash tools/tests/fixture_relation_collisions_test.sh

# Gate: GitHub's push ref-filter rule has ONE implementation. Four gates needed it
# and four gates wrote it; two were wrong, in opposite directions, and the two that
# were right were no help because nothing connected them (#1301).
.PHONY: lint-trigger-rule-copies
lint-trigger-rule-copies:
	@python3 tools/check-trigger-rule-copies.py

# Acceptance test for that rule: one table of trigger shapes, driven through the
# rule and through all four consumers, plus a mutation of every branch of it.
.PHONY: test-workflow-trigger-rule
test-workflow-trigger-rule:
	@bash tools/tests/workflow_trigger_rule_test.sh

# The Dagger image leg must build exactly the images docker-build.yml publishes.
# Static (python3 only, builds nothing), so it is safe in preflight — the image
# BUILD itself is deliberately not, see .dagger/image.go.
.PHONY: lint-image-parity
lint-image-parity:
	@python3 tools/check-image-parity.py

# Red-capability pin for the gate above, in both directions: a variant published
# but not built before the tag, and one built by the leg that nothing publishes.
.PHONY: test-image-parity-gate
test-image-parity-gate:
	@bash tools/tests/image_parity_test.sh

# Every recipe for the platform artifacts (the `-full` tarball's two binaries, the
# platform image) compiles the server features docs/releases.md lists (#1621).
# Static (python3 only), so it is safe in preflight.
.PHONY: lint-artifact-features
lint-artifact-features:
	@python3 tools/check-artifact-features.py

# Red-capability pin for the gate above: one recipe at a time drops or adds a feature.
.PHONY: test-artifact-features-gate
test-artifact-features-gate:
	@bash tools/tests/artifact_features_test.sh

# Every artifact this repository ships maps to a leg that EXECUTES it, or to an
# exemption naming the issue that owns the gap. The ledger is
# tools/delivery-artifacts.toml. Static (python3 only, ships nothing), so it belongs
# in preflight; the legs that do the executing are the heavy image trigger.
.PHONY: lint-delivery-coverage
lint-delivery-coverage:
	@python3 tools/check-delivery-coverage.py

# Red-capability pin for the gate above: a new artifact arriving with no row, a row
# outliving its artifact, a leg that exists but no workflow calls, and — the two that
# matter — a `dagger call` that appears only in a COMMENT, and an exemption whose gap
# has closed.
.PHONY: test-delivery-coverage-gate
test-delivery-coverage-gate:
	@bash tools/tests/delivery_coverage_test.sh

# A published SDK's lockfile may not pin a version its own manifest no longer claims.
# #1225: the 2.15.0 bump edited the SDK manifests and left fraiseql-python's uv.lock and
# fraiseql-rust's Cargo.lock at 2.14.1, while fraiseql-typescript stayed correct only
# because `npm ci` refuses a disagreeing lockfile. Pure text (python3, stdlib), so it
# runs everywhere on every push; dependency drift is the `--locked` flags on the SDK legs.
.PHONY: lint-sdk-lockfile-freshness
lint-sdk-lockfile-freshness:
	@python3 tools/check-sdk-lockfile-freshness.py

# Red-capability pin for the gate above. Load-bearing: package-lock.json records the root
# version TWICE and only one site drifting must still go red, and an unclassified lock
# format must be FATAL rather than skipped — which is not hypothetical, since writing the
# gate turned up a tracked bun.lock a hand-written format list had missed.
.PHONY: test-sdk-lockfile-freshness-gate
test-sdk-lockfile-freshness-gate:
	@bash tools/tests/sdk_lockfile_freshness_test.sh

# Every Cargo.lock records each path package at the version its manifest declares. The
# 2.16.0 cut left the seven fuzz lockfiles at 2.15.0 and fraiseql-client's at 2.3.0: none
# of those workspaces builds `--locked`, so nothing noticed. The general rule beneath
# lint-sdk-lockfile-freshness, over the whole tree. Pure text (python3, stdlib).
.PHONY: lint-cargo-lock-path-versions
lint-cargo-lock-path-versions:
	@python3 tools/check-cargo-lock-path-versions.py

# Red-capability pin for the gate above: the two shapes 2.16.0 shipped must go red, a
# registry record or a target/ lockfile must not, and a record the gate cannot match to
# exactly one manifest must be FATAL rather than skipped.
.PHONY: test-cargo-lock-path-versions-gate
test-cargo-lock-path-versions-gate:
	@bash tools/tests/cargo_lock_path_versions_test.sh

# Boot each shipped server image against a real Postgres and require an answer
# only a working engine can give: /health reporting the database connected, a
# GraphQL query resolved THROUGH SQL to rows, and — the assertion that matters —
# a row inserted behind the engine's back coming back in a re-query.
#
# Deliberately NOT in `preflight`: it builds images. Same reason as the build
# itself (see .dagger/image.go); it runs on dagger-image.yml's trigger.
#
# RUN_ID defaults to a fresh timestamp because Dagger caches a module function
# call on its arguments — with a constant id, a second run replays the first
# run's output in ~2s without starting anything. Override it only to deliberately
# re-read a previous run's result.
#   make image-boot                 # boot both server variants, for real
#   make image-boot RUN_ID=abc123   # replay/pin a specific run id
.PHONY: image-boot
image-boot:
	dagger call image-boots --source=. --run-id=$(or $(RUN_ID),local-$(shell date +%s%N))

# Assert what each shipped server image IS, on the built artifact rather than on
# the Dockerfile that describes it: the dynamic linkage #1133 measured by hand,
# no libpq package, uid 65532 non-root, the OCI version label and the binary's
# own reported version both equal to the workspace version, an image-size budget
# whose failure names the delta — and the image's own HEALTHCHECK executed,
# required to fail before the server starts, pass while it serves, and fail again
# once it is killed.
#
# Deliberately NOT in `preflight`, for the same reason as image-boot: it builds
# images. It runs as dagger-image.yml's third step, where the images are already
# in the engine cache.
#
# RUN_ID as for image-boot: this tier executes the healthcheck, and a replayed
# execution claim is the class of green this program exists to delete.
#   make image-properties
#   make image-properties RUN_ID=abc123
.PHONY: image-properties
image-properties:
	dagger call image-properties-all --source=. --run-id=$(or $(RUN_ID),local-$(shell date +%s%N))

# Red-capability pin for .gitleaks.toml, the config behind the repository's only
# executing secret scanner (#1208). Runs inside the Dagger `secret-scan` gate on
# every push; this target is the local half, and needs `gitleaks` on PATH.
# Deliberately NOT in `preflight`: preflight must not require a tool that only the
# security leg's container installs.
.PHONY: test-secret-scan-gate
test-secret-scan-gate:
	@bash tools/tests/gitleaks_allowlist_test.sh

# The postgres and observers integration legs build each package with one feature
# list; a line that needs features OFF names the flag and the suite it keeps running
# (`// feature-off:`). Every extra list rebuilt fraiseql-server and fraiseql-core under
# it (#1886, #1902).
.PHONY: lint-leg-feature-sets
lint-leg-feature-sets:
	@python3 tools/check-leg-feature-sets.py

# Red-capability pin for the leg feature-set gate: a second list, a stale, misplaced or
# dangling annotation, a vanished suite, and a flag still on through another feature
# must each be rejected, for its own reason.
.PHONY: test-leg-feature-sets-gate
test-leg-feature-sets-gate:
	@bash tools/tests/leg_feature_sets_test.sh

# Gate: every gate script under tools/ (check-*, tests/*_test.sh) is run by a CI leg,
# or exempted in tools/gate-reachability-exemptions.toml with the reason it runs in
# none. It reads from the files, so a gate added to `preflight` alone is named too.
.PHONY: lint-gate-reachability
lint-gate-reachability:
	@python3 tools/check-gate-reachability.py

# Red-capability pin for the gate-reachability check: a script nothing runs, one only
# mentioned, one run only by its self-test or by a Dagger function no workflow calls,
# and a stale or missing exemption must each be named.
.PHONY: test-gate-reachability-gate
test-gate-reachability-gate:
	@bash tools/tests/gate_reachability_test.sh

# Red-capability pin for the bare-DATABASE_URL gate. It ran in preflight and in the
# required CI leg for its whole life without ever rejecting anything (#1075), so every
# assertion here is a shape it must reject — starting with the literal text its
# BRE-escaped pattern could not see.
.PHONY: test-imports-gate
test-imports-gate:
	@bash tools/tests/test_imports_gate_test.sh

# Red-capability pin for the GitHub Actions side of the suite-coverage gate
# (#1120). Reading `run:` blocks removed four exemptions; the risk it introduced
# is that a workflow can look like coverage and provide none — dispatch-only, a
# working-directory in another workspace, `--bench`, a paths filter that never
# fires. Each is a fixture here, and each must be reported.
.PHONY: test-suite-coverage-workflows
test-suite-coverage-workflows:
	@bash tools/tests/suite_coverage_workflows_test.sh

# The red-capability pin for the gate's GATING side (#1289). Coverage and gating
# are different questions: `Dagger — integration` ran every `*_e2e_pg` suite and
# could not fail a merge, so `dev` stayed red in it for two weeks under four green
# required checks. Each fixture is one link in the chain the gate now resolves —
# job runs leg, job reports context, context is or is not required — broken in the
# direction that reads safe.
.PHONY: test-suite-coverage-gating
test-suite-coverage-gating:
	@bash tools/tests/suite_coverage_gating_test.sh

# The red-capability pin for .dagger/main.go's `suiteCountPrelude` (#1276). The
# prelude buffers a suite's output so SUITE-RAN can quote its counts, and the
# buffering is what went wrong: under the leg's `set -e` a failing cargo killed the
# shell before the output was printed, so `integration (server)` exited 101 twice
# with nothing in its log to say why. Reads the constant out of main.go, fakes
# `cargo` on PATH — no toolchain, so it belongs here rather than in a heavy leg.
.PHONY: test-suite-marker-prelude
test-suite-marker-prelude:
	@bash tools/tests/suite_marker_prelude_test.sh

# Diff tools/required-checks.toml against the live `dev` ruleset. NOT in preflight:
# it needs a network and a token, which the offline Dagger legs have neither of.
# Run it whenever the mirror or the ruleset changes.
.PHONY: lint-required-checks
lint-required-checks:
	@bash tools/check-required-checks.sh

# Gate: a test reads DATABASE_URL through the test-support helpers, never inline,
# so a missing URL fails the test instead of skipping it.
.PHONY: lint-test-imports
lint-test-imports:
	@bash tools/check-test-imports.sh

# Gate: deny.toml, .cargo/audit.toml and docs/dependency-risk-policy.md accept the
# same entries.
.PHONY: lint-audit-lockstep
lint-audit-lockstep:
	@bash tools/check-audit-lockstep.sh

# ============================================================================
# shell-gates — every gate the Dagger ShellGates leg runs, written once
# ============================================================================
#
# `.dagger/main.go`'s ShellGates runs `make -k shell-gates` and nothing else, and
# `preflight` depends on this target, so CI and the local run read one list.
# Each prerequisite is a gate target; a rule per line keeps each gate's reason
# beside it, and make runs them in the order written. Every gate here runs in
# the ShellGates container: make, git, gawk, findutils, grep and python3, no Rust
# toolchain and no `.git` (the leg runs `git init -q .` first). A gate that needs
# cargo or another toolchain runs in its own Dagger function instead.
#
# `make -k shell-gates` reports every failing gate in one run; make still exits
# non-zero if any failed. One gate by hand: `make <target>`.
.PHONY: shell-gates
shell-gates: lint-tests-layout
shell-gates: lint-unwrap
shell-gates: lint-expect
shell-gates: lint-async-trait
shell-gates: lint-gate-db
shell-gates: lint-gate-core

# The crate-level allow budgets of fraiseql-core's lib.rs and fraiseql-wire, and the
# `# Errors` doc floors per crate. They ran in no leg until the reachability check
# below named them.
shell-gates: lint-gate
shell-gates: lint-gate-wire
shell-gates: lint-gate-errors-doc
shell-gates: test-release-tooling
shell-gates: test-deadline-gate
shell-gates: test-audit-ledger-gate
shell-gates: test-docs-env-vars-gate
shell-gates: test-doc-claims-gate
shell-gates: test-release-validation-gate
shell-gates: test-rustc-wrapper-gate

# The changelog gate's SELF-TEST only. The gate itself needs real git
# history and would pass vacuously here — the ShellGates leg mounts the tree
# without `.git` and runs `git init -q .` first, so `Closes #N` over the release range
# returns nothing. The gate runs in changelog-check.yml and in
# release.yml's validate-release, both with fetch-depth: 0 (#1127).
shell-gates: test-changelog-gate

# The nightly rustfmt is pinned and pinned once: a bare `nightly` makes the
# Fmt gate depend on the day it runs (2026-09-25 red dev this way).
shell-gates: lint-fmt-toolchain
shell-gates: lint-test-imports
shell-gates: lint-routes

# No route answers 501 through `not_implemented(` but one route's conditional one
# (ADR-0007 §5, check 8): a route that exists and declines is a capability the
# registry reports served and the deployment does not serve.
shell-gates: lint-not-implemented
shell-gates: test-not-implemented-gate
shell-gates: lint-guard-parity

# The test side of the same rule (#1272): a test that asserts a guard's
# behaviour must take the temp_env lock, or it races the sibling that sets
# the guard's opt-out and reads the guard as disabled. Four vault tests did; the
# three asserting a refusal reddened the ShellGates leg at random, and the one
# asserting Ok passed with the outbound-address guard refusing every address.
shell-gates: lint-guard-test-lock
shell-gates: test-guard-test-lock-gate

# Every router the server mounts is recorded, so the transport refusal matrix
# sees every transport (#1844).
shell-gates: lint-mount-ledger
shell-gates: test-mount-ledger-gate

# Every guard site has a mutant and a named killer, and every guard-shaped call
# of an enabled family is an inventoried site (#1846).
shell-gates: lint-guard-sites
shell-gates: test-guard-sites-gate
shell-gates: test-guard-mutants-gate
shell-gates: lint-deploy-security

# Deploy artifacts must name the version being released, and the chart's
# default image must be one this project publishes (#1129).
shell-gates: lint-deploy-versions

# A compiled schema a workflow boots must name the build this tree makes:
# the server refuses any other since #1304, and release-smoke.yml only runs
# on release/* and v*, so a stale one first shows up at the tag.
shell-gates: lint-compiled-schema-stamp
shell-gates: test-compiled-schema-stamp-gate

# Every fuzz.yml matrix row must name a target that exists in the crate it
# names (#1128), and every target on disk has a row or a stated exemption (#1847).
shell-gates: lint-fuzz-targets
shell-gates: test-fuzz-targets-gate

# A Compose file merely NAMED by a make target, and absent, is outside
# check-examples-integrity.sh — that gate discovers compose files with `find` and
# checks what is inside them. Three make targets drove a path in no commit
# reachable from dev and failed on their first line (#1219).
shell-gates: lint-compose-references
shell-gates: test-compose-references-gate

# First-party sessions are ES256 ([auth_issuer]) and validated by OidcValidator.
# The shared-secret path is being removed; a file outside the gate's lists that
# names it fails, and the PENDING list only shrinks.
shell-gates: lint-no-first-party-hs256
shell-gates: test-no-first-party-hs256-gate

# Every apt install refreshes its index in the same exec: one layered on a
# separately cached `apt-get update` 404'd the ShellGates leg on every run once
# Ubuntu replaced a package the cached index still named (2026-10-10).
shell-gates: lint-dagger-apt
shell-gates: test-dagger-apt-gate

# One pg_tviews pin (docker/pg-tviews/Dockerfile); the docs and compose name it (#1391).
shell-gates: lint-pg-tviews-pin
shell-gates: test-pg-tviews-pin-gate

# The quiet-window check the release benchmark waits on before measuring on forge:
# another CI job, a cargo or rustc, or a load over the limit keeps it from measuring.
shell-gates: test-bench-quiet-window-gate

# A velocitybench baseline compares only on the host that measured it.
shell-gates: test-detect-regression-host-gate

# A velocitybench run that measured nothing fails the release benchmark.
shell-gates: test-bench-require-measured-gate

# A bare `image: fraiseql:...` in a fenced code block resolves to
# docker.io/library/fraiseql, which this project cannot publish to — #1129's
# defect, in the one place the file-level gates do not read (#1220).
shell-gates: lint-doc-image-refs

# Every tool CI installs names a version. `pip install scim2-tester httpx` in
# .dagger/main.go let two PyPI releases redden `integration (saml)` twice in nine
# hours on an untouched branch (2026-09-20) — once before a single check ran.
shell-gates: lint-ci-install-pins
shell-gates: test-ci-install-pins-gate

# The #1421 stale-artifact check the canary runs in every Rust leg, proved RED and
# GREEN on synthetic cargo logs (a green leg cannot show it would ever fire).
shell-gates: test-ci-stale-artifacts-gate

# #1596: the stamp is taken inside each exec of a Rust leg, after its target volume
# is mounted, by tools/ci-stamp-sources.sh (BASH_ENV); proved in a tree with no .git.
shell-gates: lint-ci-source-stamp
shell-gates: test-ci-source-stamp-gate
shell-gates: lint-phases-citations
shell-gates: test-phases-citations-gate
shell-gates: lint-image-context
shell-gates: test-image-context-gate

# Every gate script under tools/ is run by some CI leg or exempted with a
# reason. The five self-tests above and in CheckFuzz, CheckExampleCrates and
# CheckRExamples ran only in `make preflight` until this gate named them.
shell-gates: lint-gate-reachability
shell-gates: test-gate-reachability-gate
shell-gates: test-doc-image-refs-gate

# Every secret a publish job reads is checked by release.yml's prerequisite
# step before anything is published. The Rust SDK read CARGO_REGISTRY_TOKEN, a
# secret the repository never had: empty at the upload, after every other
# package of v2.16.0 was already public (#1518).
shell-gates: lint-publish-secrets
shell-gates: test-publish-secrets-gate

# The pre-tag image leg must build exactly what docker-build.yml publishes,
# in both directions and across both of its matrices. Static — it reads the
# two lists; building the images is the heavy leg's job (#1205).
shell-gates: lint-image-parity
shell-gates: test-image-parity-gate

# #1621: every platform-artifact recipe compiles the server features
# docs/releases.md lists.
shell-gates: lint-artifact-features
shell-gates: test-artifact-features-gate

# One level up from check-suite-coverage.py: every artifact this repository
# SHIPS maps to a leg that executes it, or to an exemption naming the issue
# that owns the gap. Discovery reads the same sources the four per-class
# parity gates read, so a new image variant, crate or SDK arrives here too;
# what this adds is that a row may not claim coverage that does not exist —
# `dagger:PublishDryRun` is a real function no workflow calls, and a leg is
# only coverage if some run: step invokes it (#1205's hole, one level up).
shell-gates: lint-delivery-coverage
shell-gates: test-delivery-coverage-gate

# #1640: every crates/*/src/**/*.rs is declared by some target. Walks the
# filesystem, not `git ls-files`, which is empty in the ShellGates container.
shell-gates: lint-module-reachability
shell-gates: test-module-reachability-gate

# #1640: every public library module is used by a binary or allowlisted.
shell-gates: lint-pub-mod-callers
shell-gates: test-pub-mod-callers-gate

# #1640: issue numbers in non-test code comments only go down, per crate.
shell-gates: lint-prose-ratchet
shell-gates: test-prose-ratchet-gate

# #1636: the perf comparator's self-test (the gate runs in PerfInstructions).
shell-gates: test-perf-instructions-gate

# #1640: docs with a ```rust block are doctests, and no doc fence is bare.
shell-gates: lint-docs-rust-examples
shell-gates: test-docs-rust-examples-gate

# #1859: a `fraiseql_query` example binds its values as variables.
shell-gates: lint-query-variables
shell-gates: test-query-variables-gate

# A published SDK's lockfile may not pin a version its manifest no longer
# claims (#1225): the 2.15.0 bump edited the SDK manifests and left
# fraiseql-python's uv.lock and fraiseql-rust's Cargo.lock at 2.14.1.
# fraiseql-typescript stayed correct only because its suite runs `npm ci`,
# which refuses a disagreeing lockfile — one SDK gated by accident of tooling
# and two not. Pure text, so it runs here on every push, whichever SDKs the
# push touched; dependency drift is covered by the `--locked` flags in
# tools/sdk-suite.sh, and neither subsumes the other.
shell-gates: lint-sdk-lockfile-freshness
shell-gates: test-sdk-lockfile-freshness-gate

# The general rule beneath it, over every Cargo.lock in the tree: a path package
# is recorded at its manifest's version. The 2.16.0 cut left the seven fuzz
# lockfiles at 2.15.0 and fraiseql-client's at 2.3.0, because no build of those
# workspaces runs --locked.
shell-gates: lint-cargo-lock-path-versions
shell-gates: test-cargo-lock-path-versions-gate

# The local feature-matrix runner's red-capability pin (#1227). The RUNNER is
# local-only — the leg it mirrors is this repo's `Dagger — feature matrix`, so
# running it here would be running the matrix twice. What belongs here is the
# pin: it stubs `cargo`, compiles nothing, needs no toolchain, and asserts the
# one property that makes the local gate worth trusting — it cannot silently
# cover fewer combos than .dagger/feature-combos.go declares.
shell-gates: test-feature-matrix-gate

# The pin for tools/check-feature-combo-features.py (#2096), over a stubbed
# `cargo`. The check itself needs cargo, so it runs in the feature-matrix leg.
shell-gates: test-feature-combo-features-gate
shell-gates: lint-internal-flag
shell-gates: lint-value-json

# The mutation chokepoint is only a chokepoint while nothing reaches the
# database's write path on its own (#1327). Clippy refuses a call to the
# adapter's write (.clippy.toml), which is how the gRPC arm did it until #1330;
# this gate refuses the other way, write SQL handed to `execute_raw_query`.
# Either skips every gate on a write at once, invisibly to the transport's tests.
shell-gates: lint-mutation-dispatch

# One layer in from the chokepoint (#1331, #1352): the selection set a write is
# projected through. `&[]` is the PERMISSIVE shape — project_entity returns the
# whole entity and the #423 field authorizer takes zero calls — so REST's
# anonymous arm served gated fields the authenticated arm is refused, and gRPC's
# own copy of the derivation reached the same empty set via unwrap_or_default().
# Rule 3 bans the other half: a document rebuilt out of arguments by format!,
# which quotes object keys as JSON and so refused every nested REST body.
shell-gates: lint-write-selections
shell-gates: test-write-selections-gate

# First-party auth's tables live in schema `auth`, owned by its own role. The
# inventory test proves where the DDL puts them; this catches a query left on
# the old location, which fails only at runtime on the path that runs it.
shell-gates: lint-auth-schema
shell-gates: test-auth-schema-gate

# SQL text built at runtime reaches sqlx only through `sqlx::AssertSqlSafe`, each
# with a `// Reason:` beside it saying what the text is built from.
shell-gates: lint-sql-safe
shell-gates: test-sql-safe-gate
shell-gates: lint-ddl-lock
shell-gates: test-ddl-lock-gate

# The same shape one layer up (#1336): a transport that turns a credential into a
# principal and dispatches it unresolved. REST, MCP and gRPC each did for three
# releases, and no test could see it — a transport that skips the resolve answers
# every request reading no enriched field exactly as one that runs it does.
shell-gates: lint-principal-producers
shell-gates: test-principal-producers-gate

# #1337: `deny_unknown_fields` stops at the top level. Every section struct needs
# its own, or a mistyped key is discarded and the setting stays at its default
# without a word — `[auth] require_jti`, `[tls] require_client_cert` and
# `[rate_limiting] enabled` among them.
shell-gates: lint-config-deny-unknown
shell-gates: test-config-deny-unknown-gate
shell-gates: lint-config-refusals
shell-gates: test-config-refusals-gate
shell-gates: lint-audit-lockstep
shell-gates: lint-audit-ledger
shell-gates: lint-doc-claims
shell-gates: lint-release-validation

# A job that runs cargo under a RUSTC_WRAPPER it never installs fails at its
# first rustc (v2.15.0's publish-rust-sdk); the workflow env reaches every job.
shell-gates: lint-rustc-wrapper

# The no-orphan-suites gate: every test target × feature combo maps to a
# leg that executes it (it parses .dagger/main.go, so legs and gate cannot
# drift). Retrospective rule 1 of the 2026-07-27 program.
shell-gates: lint-suite-coverage

# Every `-- --ignored` line names a target with an `#[ignore]` test; on one
# with none it runs 0 tests and exits 0 (#2103). Reads .dagger/main.go, the
# workflows and the Makefile; the pin runs it over a fixture crate.
shell-gates: lint-ignored-lines
shell-gates: test-ignored-lines-gate

# No pipe into `grep -q` where `pipefail` is on: grep exits at its first
# match, the writer takes SIGPIPE, and the pipeline reads the match as a miss
# (#1870, #1961).
shell-gates: lint-grep-q-pipefail
shell-gates: test-grep-q-pipefail-gate
shell-gates: test-clippy-config-paths-gate

# The crates the Doctests leg skips hold no doctest: a skip list refused when a
# skipped crate's source holds anything rustdoc could compile.
shell-gates: lint-doctest-skip
shell-gates: test-doctest-skip-gate

# …and the red-capability pin for its GitHub Actions side (#1120): a
# workflow can look like coverage and provide none.
shell-gates: test-suite-coverage-workflows

# ...and the pin for its INNER feature-gate side (#1179): a test fn behind
# `#[cfg(feature = "x")]` in an ungated module was invisible to the gate,
# counted as covered while compiled out — and a `not(feature)` arm needs a
# leg with the feature OFF, which `--all-features` can never be.
shell-gates: test-suite-coverage-inner-gates

# ...and the pin for its GATING side (#1289): a leg can run every suite it
# names and still be unable to fail a merge, which is how two `*_e2e_pg`
# tests were red on `dev` for two weeks under four green required checks.
shell-gates: test-suite-coverage-gating

# ...and the pin for its FILTER side (#1300): a positional filter matching
# nothing prints `running 0 tests` and exits 0, so a rename retires the
# suite the line was added to run and the leg stays green.
shell-gates: test-suite-coverage-filters

# ...and the pin for the features `--all-features` can turn on (#2619): a suite
# gated on a name its crate does not define is built by no leg.
shell-gates: test-suite-coverage-undefined-feature

# The conformance harness's own four properties (#1118). `project.py` cited a
# `selftest.py` that had never existed, so the growth property — a new construct
# fails every SDK until each implements it or declares the gap — was asserted in
# prose and pinned by nothing. Synthetic dicts only: no toolchain, no CLI, no
# network, so it belongs here rather than in the eleven-runtime SDK job.
shell-gates: test-conformance-selftest

# Comment-only #[test] bodies read as green coverage (#895/#748).
shell-gates: lint-empty-tests

# The rung above: not a comment-only body but a whole test BINARY that
# references no fraiseql crate, so it can only assert against mocks it
# defined itself (#1269/#1270). Its red capability is pinned separately —
# a gate over a tree it just emptied is green for the wrong reason.
shell-gates: lint-test-subject
shell-gates: test-test-subject-gate

# The prelude .dagger/main.go injects into `integrationServer`, pinned against
# the regression that made two of that leg's aborts undiagnosable
# (#1276): a failing suite whose output `set -e` discarded before it was
# printed. Reads `suiteCountPrelude` out of main.go and runs it against a
# fake `cargo`, so it pins the shipped constant rather than a copy.
shell-gates: test-suite-marker-prelude

# Snapshot pairing, both directions (#986): every .snap registered, no
# stale registry rows. Was a pre-commit-only hook that ran nowhere.
shell-gates: lint-snapshot-pairing
shell-gates: lint-deadlines
shell-gates: lint-docs-env-vars
shell-gates: lint-docs-version
shell-gates: test-docs-version-gate

# Every typed TOML config loader has a coverage manifest naming each
# key's consumer (#909). Retrospective rule 2: no unconsumed surface.
shell-gates: lint-config-loaders

# A published crate's public API must not name a third-party type it does
# not re-export (#1198): `JwtValidator::new` took a `jsonwebtoken::Algorithm`
# reachable from nowhere, so the crate's documented first line did not
# compile. Reads manifests with tomllib — there is no cargo in the ShellGates container.
shell-gates: lint-public-api-reexports
shell-gates: test-public-api-reexports-gate

# The SDKs the repo says it publishes are exactly the ones it can publish
# (#1130). Eight sat frozen at 2.1.6 while README called two of them Tier 1
# (Supported), and six had a live publish job that would have pushed 2.1.6
# to a registry that will not take a version number back.
shell-gates: lint-sdk-publication-claims
shell-gates: test-sdk-publication-claims-gate
shell-gates: lint-sdk-dead-surface

# No example provisions or points at a backend #374 removed. The
# PostgreSQL-only de-scope covered crates/; examples/ kept a running
# MySQL topology for three phases afterwards (#940).
shell-gates: lint-examples-postgres-only

# A shipped example must be able to run at all: compose mounts resolve, COPY
# sources exist in the build context, no `|| true` around a build step, no
# health grep that also matches "unhealthy", every documented `cd` lands
# somewhere. Before this, the whole CI coverage of `examples/` was one clippy
# run and three greps, and a nine-issue audit found essentially every
# documented entry point dead (#1050-#1054, #1071-#1073). The two tiers that
# need a toolchain and a database are the `examples` integration suite.
shell-gates: lint-examples-integrity

# A declared-but-unread `feature = []` is a promise the build cannot keep:
# enabling it changes nothing, so the capability it names is either absent or
# reachable another way. This gate existed but ran in NO leg while failing on
# clean dev, so its one real finding (`export-parquet`, deleted in #1012) went
# unreported for as long as it was there. A gate that is red and unrun is worth
# less than no gate — it trains the next reader to assume the surface is checked.
shell-gates: lint-feature-chains

# A per-crate runaway-growth ratchet, and a check that every crate has a
# budget at all. Its only caller was the orphaned tools/lint.sh, so it ran
# nowhere while four crates went over budget and five acquired no budget
# row — and Cargo.toml claimed all along that CI enforced it (#1055/#990).
# It counts production lines only, by rules its self-test pins (#1882).
shell-gates: lint-crate-sizes
shell-gates: test-crate-growth-gate
shell-gates: lint-crate-lists
shell-gates: test-crate-lists-gate
shell-gates: test-crate-sizes-gate

# Every official SDK's own suite runs under ONE unfiltered, required check,
# sdk-suites.yml. Four of eleven per-SDK workflows could not run on a branch
# push (#1119), and the rest were paths-filtered, which a required check
# cannot be, so they gated nothing (#1467).
shell-gates: lint-sdk-workflows
shell-gates: test-sdk-workflows-gate

# The Go SDK's floor is go.mod's `go` line, and the matrix, the setup-go pins,
# the conformance container and the docs all say the same. Four files pinned Go
# independently, and the README promised 1.22 against go.mod's 1.23 (#2278).
shell-gates: lint-go-floor
shell-gates: test-go-floor-gate

# A job `if:` may not name an event or a ref its own workflow cannot
# receive. The 2026-05-31 migration stripped triggers and left the
# conditions: docker-build.yml kept two jobs that read as image coverage
# and had never run, because an unreachable job is absent from the checks
# list rather than reported as skipped (#1206).
shell-gates: lint-workflow-reachability
shell-gates: test-workflow-reachability-gate

# Every self-hosted job a push can start skips a SHA that already passed
# it. The guard was opt-in, so `Dagger — image` reran ~36 min on every
# fast-forward to dev over a tree its chain-branch dispatch had just built.
shell-gates: lint-already-passed-guard
shell-gates: test-already-passed-guard-gate

# Every ghcr.io/fraiseql image above is mirrored from a pinned source. The
# minio row rode `docker.io/minio/minio:latest`, which stopped resolving, and
# the weekly mirror stayed red while the legs pulled a stale copy (#1453).
shell-gates: lint-image-mirror
shell-gates: test-image-mirror-gate

# GitHub's push ref-filter rule — which halves a `push:` leaves defined —
# had four implementations in tools/, and two were wrong in opposite
# directions: one read `branches-ignore`/`tags-ignore` as neither key, the
# other read a missing `branches:` as no restriction, which made sixteen
# tag-only publish contexts look branch-reaching. #1301 collapsed them onto
# one; this refuses a fifth, and the self-test drives the shared table
# through every consumer and mutates each branch of the rule.
shell-gates: lint-trigger-rule-copies
shell-gates: test-workflow-trigger-rule

# Two SQL fixtures declared `public.tb_user` with incompatible shapes,
# both under `CREATE TABLE IF NOT EXISTS`, so whichever loaded second was
# a silent no-op and the loser's dependent objects failed to build. Every
# consumer had grown its own defence; nothing guarded the fixtures (#1281).
shell-gates: lint-fixture-collisions
shell-gates: test-fixture-collisions-gate

# `make test-integration-postgres` and `make test-leg` run the lines of
# .dagger/main.go's integrationPostgres and Test, read at run time by
# tools/leg-lines.py. Before the first existed, the fact that those suites only
# pass under --test-threads=1 lived ONLY in .dagger/main.go, so
# `cargo test -p fraiseql-core` was red by construction (#1169); nothing ran what
# Test runs before a merge to dev (#1257). This holds the reader able to read both
# shards, and in agreement with check-suite-coverage.py on their cargo test lines.
shell-gates: lint-leg-lines
shell-gates: test-leg-lines-gate

# The postgres and observers integration legs build each package with one
# feature list; each extra list rebuilt fraiseql-server and fraiseql-core under
# it, and a cold runner could not finish the postgres leg (#1886, #1906). A line
# that needs a feature OFF names the flag and the suite it keeps running.
shell-gates: lint-leg-feature-sets
shell-gates: test-leg-feature-sets-gate

# cargo-deny's unmatched-skip lints default to WARN, so a stale
# [[bans.skip-tree]] pin covers NOTHING while `cargo deny check` still exits 0.
# The level cannot be set in deny.toml, so `-D` lives on three command lines;
# this keeps them in lockstep (#1020, #933).
# A Rust base image older than [workspace.package] rust-version cannot build
# this workspace at all, and docker-build.yml is tag-only — so without this
# the first witness to a stale pin is the release itself (#1107).
# An uncopied [workspace] member means `cargo build -p fraiseql-server` cannot
# even load the manifest, so the release image cannot be built at all (#1205).
shell-gates: lint-dockerfile-members
shell-gates: test-dockerfile-members-gate
shell-gates: lint-dockerfile-msrv
shell-gates: test-dockerfile-msrv-gate
shell-gates: lint-deny-flags
shell-gates: test-deny-flags-gate

# The bare-DATABASE_URL gate above ran here for its whole life without ever
# being able to reject anything (#1075). Its red capability is now pinned.
shell-gates: test-imports-gate

# A database-gated test binary that loads a compiled document carries
# `the_document_loads_without_a_database`, a test that cannot skip, so a
# load-time refusal of the document fails whichever leg runs the binary
# (2b843cd27). The scan and its self-test need no toolchain. Until #1945 they ran
# only in `make preflight`: a chain carrying `saml_acs_e2e_pg.rs` without its
# guard passed every required context, and only the local preflight refused it.
shell-gates: lint-e2e-documents
shell-gates: test-e2e-documents-gate

# Run the cheap-but-frequent CI gates locally before `git push`, to catch the
# failures the Dagger `preflight` leg would reject — rustfmt drift, clippy
# `-D warnings`, broken rustdoc intra-doc links, and the grep/wc policy gates —
# for free instead of paying for a CI rerun. Runs `shell-gates`, the list the Dagger
# ShellGates leg runs, then the targets the other Preflight functions run. Does NOT run the
# test suite or service-backed integration tests — those are `make test` and the
# separate Dagger test/integration legs.
.PHONY: preflight
preflight: lint-fmt-toolchain fmt-check shell-gates rustdoc rustdoc-default clippy check-default check-fuzz test-fuzz-compiles-gate check-example-crates test-example-crates-gate lint-r-examples test-r-examples-gate test-e2e-documents lint-feature-combo-features
	@echo ""
	@echo "✅ preflight passed — the targets the Dagger preflight leg runs, and"
	@echo "   test-e2e-documents and lint-feature-combo-features, which other legs run."
	@echo "⚠  It ran NO tests but the e2e document guards (test-e2e-documents)."
	@echo "   --all-targets compiles the test binaries; it does not"
	@echo "   execute them, and the workspace suite is 29 commands rather than one"
	@echo "   (81 tests/*.rs binaries named explicitly, which --lib never reaches). Run:"
	@echo "       make test-leg          # what Dagger — test runs, line for line (#1257)"
	@echo "⚠  It does NOT run clippy under a narrow feature set: its clippy pass is"
	@echo "   --all-features (feature-OFF arms are not compiled) and its narrow pass is"
	@echo "   cargo check (no clippy lints). Before pushing anything under a"
	@echo "   #[cfg(feature = ...)], run: make lint-feature-matrix   (#1227)"

# Format code (nightly rustfmt for advanced formatting options).
# The nightly is PINNED in tools/fmt-toolchain.txt — rustfmt's output is not
# stable across nightlies, and the Dagger Fmt gate reads the same file, so a
# bare `+nightly` here would format differently than CI checks.
FMT_TOOLCHAIN := $(shell tail -1 tools/fmt-toolchain.txt)
fmt:
	cargo +$(FMT_TOOLCHAIN) fmt --all

# Check formatting
fmt-check:
	cargo +$(FMT_TOOLCHAIN) fmt --all -- --check

# Gate: the pin above is dated, the Dagger const agrees with it, and nothing
# invokes a bare `+nightly`. First in preflight's prerequisites because
# fmt-check reads the value it guards.
.PHONY: lint-fmt-toolchain
lint-fmt-toolchain:
	bash tools/check-fmt-toolchain.sh

# Run all checks
check: fmt-check clippy test

# Standard lint + test combo (F035). Convenience target that matches what
# CI runs: strict clippy gate over the whole workspace, then nextest.
.PHONY: ci
ci:
	cargo clippy --workspace --all-targets --all-features -- -D warnings
	cargo nextest run --workspace --all-features

# Clean build artifacts
clean:
	cargo clean

# Remove leaked testcontainers Postgres containers (testcontainers-rs 0.26 uses Drop
# for cleanup; containers stored in static OnceCell never drop, so they accumulate
# locally between runs — CI is unaffected because each job has a fresh Docker env).
clean-test-containers:
	@echo "Stopping leaked testcontainers postgres containers..."
	@docker ps -q --filter "ancestor=postgres:18-alpine" | xargs -r docker stop
	@docker container prune -f
	@echo "Done."

# Install CLI tool
install:
	cargo install --path crates/fraiseql-cli

# Run development server
dev:
	cargo run --package fraiseql-server

# Build documentation
doc:
	cargo doc --all-features --no-deps --open

# Run benchmarks
bench:
	cargo bench

## bench-baseline: save current benchmark results as the local 'dev' baseline
bench-baseline:
	cargo bench --workspace -- --save-baseline dev
	@echo "Baseline saved as 'dev'. Run 'make bench-compare' after future changes."

## bench-compare: run benchmarks and compare against the saved 'dev' baseline
## Micro benchmarks (pure computation) use a 5% threshold; slow (DB) benchmarks use 15%.
bench-compare:
	@command -v critcmp >/dev/null 2>&1 || cargo install critcmp --version 0.1.8 --locked
	cargo bench --workspace -- --save-baseline current
	@echo "=== Micro benchmarks (5% threshold) ==="
	critcmp dev current --threshold 5 -f '(projection|federation|design_analysis|saga|typename|payload_size|complete_pipeline)' || true
	@echo "=== Slow benchmarks (15% threshold) ==="
	critcmp dev current --threshold 15 -f '(10k_rows|100k_rows|1m_rows|where_clause|pagination|http_response_pipeline|graphql_transform|god_objects)' || true

## memory-profile: run dhat memory profiling benchmarks
memory-profile:
	cargo test --bench memory_profile -p fraiseql-core --features dhat-heap -- --nocapture --test-threads=1

## bench-critical: run only the latency-sensitive hot-path benchmarks
bench-critical:
	cargo bench -p fraiseql-core -- query_execution cache_lookup rls_injection
	cargo bench -p fraiseql-server -- graphql_handler

# ============================================================================
# K6 Load Testing
# ============================================================================

## Run the mixed-workload k6 load test (requires a running FraiseQL server)
load-test:
	k6 run load-tests/k6/scenarios/mixed-workload.js

## Run all k6 load test scenarios sequentially
load-test-all:
	@for scenario in mixed-workload graphql-queries graphql-mutations auth-flow apq-cache; do \
		echo "=== Running $$scenario ==="; \
		k6 run load-tests/k6/scenarios/$$scenario.js || exit 1; \
		echo ""; \
	done

# ============================================================================
# Helm Chart Validation
# ============================================================================

## Lint and template-test the Helm chart
# The Helm chart resolves to an image that exists, and deploys into a real
# cluster that answers a real query — then a row is inserted behind the deployed
# release and required back out of it.
#
# This replaces `helm-lint`, which ran `helm lint` plus a render into /dev/null.
# A lint never resolves an image, which is how the chart shipped an unpullable
# default for several releases (#1129) — and fixing that alone would still have
# left a chart that could not start a pod for five other reasons.
#
# ⚠ Needs docker, and does NOT run inside Dagger: a kubelet cannot start in a
# Dagger exec on this engine (empty cgroup.controllers — see the script header).
# The IMAGE still comes from Dagger, so the chart is deployed against the
# artifact `buildVariant` builds rather than a second `docker build`.
#
#   make chart-deploy
#   make chart-deploy RUN_ID=abc123
#
# target/ is in the Dagger +ignore list, so parking the tarball there does not
# invalidate the image build cache.
.PHONY: chart-deploy
chart-deploy:
	@mkdir -p target/chart-deploy
	dagger call image-tarball --source=. --variant=fraiseql-server \
		export --path=target/chart-deploy/fraiseql-server.tar
	bash tools/chart-deploy-test.sh \
		--run-id=$(or $(RUN_ID),local-$(shell date +%s%N)) \
		--image-tarball=target/chart-deploy/fraiseql-server.tar

## Bring up the canonical Compose stack and query it
# docker-compose.yml is the ONE Compose stack this repository verifies. Five other
# operator-facing stacks shipped beside it and, measured 2026-08-28, not one of the
# six could serve a query — see the file's own header for the six causes.
#
# The stack is brought up on the image THIS BRANCH builds (loaded under the tag the
# compose file names, so no registry is needed before the tag exists), required to
# become healthy on the IMAGE's own HEALTHCHECK, queried through the published host
# port, and then a row is inserted behind the running engine and required back.
#
# ⚠ Needs docker, and is not a `dagger call`: `docker compose` needs a docker
# daemon, and the same host-docker reasoning as chart-deploy applies. The IMAGE
# still comes from Dagger, so this is not a second copy of the build arguments.
#
#   make compose-stack
#   make compose-stack RUN_ID=abc123
#
# target/ is in the Dagger +ignore list, so parking the tarball there does not
# invalidate the image build cache.
.PHONY: compose-stack
compose-stack:
	@mkdir -p target/compose-stack
	dagger call image-tarball --source=. --variant=fraiseql-server \
		export --path=target/compose-stack/fraiseql-server.tar
	bash tools/compose-stack-test.sh \
		--run-id=$(or $(RUN_ID),local-$(shell date +%s%N)) \
		--image-tarball=target/compose-stack/fraiseql-server.tar

# Watch for changes and run tests
watch:
	cargo watch -x 'test --all-features'

# Watch for changes and run checks
watch-check:
	cargo watch -x 'check --all-features'

# ============================================================================
# Docker-based Test Database Management
# ============================================================================

# Start all test infrastructure (PostgreSQL, Redis, NATS, Vault)
# and wait until each service is healthy.
db-up:
	@echo "Starting test infrastructure..."
	@bash docker/tls/gen-certs.sh
	@docker compose -f docker/docker-compose.test.yml up -d
	@echo "Waiting for all services to be healthy..."
	@for svc in postgres-test postgres-standby-test postgres-failover-test postgres-tls-test postgres-below-floor-test redis-test nats-test vault-test; do \
		printf "  Waiting for %-20s" "$$svc..."; \
		for i in $$(seq 1 60); do \
			status=$$(docker inspect --format='{{.State.Health.Status}}' \
				$$(docker compose -f docker/docker-compose.test.yml ps -q $$svc 2>/dev/null) 2>/dev/null); \
			if [ "$$status" = "healthy" ]; then echo " ready"; break; fi; \
			if [ $$i -eq 60 ]; then echo " TIMEOUT"; exit 1; fi; \
			sleep 2; \
		done; \
	done
	@echo "All services ready."
	@docker compose -f docker/docker-compose.test.yml ps

# Re-clone the failover standby (#957). `pg_promote()` is one-way, so the test
# that exercises a real failover leaves postgres-failover-test a plain writable
# server; restarting it re-runs the entrypoint's pg_basebackup. CI legs start
# from fresh containers and never need this.
db-failover-reset:
	@echo "Re-cloning the failover standby..."
	@docker compose -f docker/docker-compose.test.yml restart postgres-failover-test
	@printf "  Waiting for postgres-failover-test..."
	@for i in $$(seq 1 60); do \
		status=$$(docker inspect --format='{{.State.Health.Status}}' \
			$$(docker compose -f docker/docker-compose.test.yml ps -q postgres-failover-test 2>/dev/null) 2>/dev/null); \
		if [ "$$status" = "healthy" ]; then echo " ready"; break; fi; \
		if [ $$i -eq 60 ]; then echo " TIMEOUT"; exit 1; fi; \
		sleep 2; \
	done

# Stop test databases
db-down:
	@echo "Stopping test databases..."
	@docker compose -f docker/docker-compose.test.yml down

# View database logs
db-logs:
	@docker compose -f docker/docker-compose.test.yml logs -f

# Reset test databases (remove volumes)
db-reset:
	@echo "Resetting test databases (removing volumes)..."
	@docker compose -f docker/docker-compose.test.yml down -v
	@$(MAKE) db-up

# Check database health status
db-status:
	@echo "Database status:"
	@docker compose -f docker/docker-compose.test.yml ps

# Verify test data
db-verify:
	@echo "Verifying PostgreSQL test data..."
	@docker compose -f docker/docker-compose.test.yml exec -T postgres-test \
		psql -U fraiseql_test -d test_fraiseql -c "SELECT 'v_user' AS view, COUNT(*) FROM v_user UNION ALL SELECT 'v_post', COUNT(*) FROM v_post UNION ALL SELECT 'v_product', COUNT(*) FROM v_product;"

# ============================================================================
# Federation stack
# ============================================================================
#
# `federation-up`, `federation-down` and `test-federation` are gone. All three drove
# `docker/federation-ci/docker-compose.yml`, a file that is not in the repository and is
# in no commit reachable from `dev` — so every one of them failed on its first line, while
# `make help` and `docs/testing-matrix.md` advertised them as ways to run the suite
# (#1219). `check-compose-references.sh` now fails on a Makefile line naming a compose
# file that does not exist, so this cannot come back quietly.
#
# What actually covers federation: `make federation-compose-check` below (hermetic, no
# Docker, real Apollo Federation v2 composition) and the Dagger `integration (federation)`
# leg, which stands up router + subgraphs + PostgreSQL in containers.

# Golden two-subgraph compose suite — no Docker. Verifies the committed subgraph SDL
# fixtures still match live FraiseQL rendering (hermetic Rust tests), then composes them
# with real Apollo Federation v2 composition (positive + the #497 negative case + the
# #698 cascade positive case). This is the suite that would have caught the
# #495/#496/#497/#498 federation cluster and the #698 cascade-envelope sharing bug.
#
# Two lock-step tests keep the committed fixtures honest: the core one guards
# catalog/reviews (built from CompiledSchema builders); the cli one guards the cascade
# fixtures, which MUST be rendered through the cli converter where cascade synthesis and
# the #698 @shareable fix live (the core builders bypass it).
federation-compose-check:
	@echo "=== Federation golden compose (SDL invariants + real composition) ==="
	@cargo test -p fraiseql-core --test federation_compose --features federation
	@cargo test -p fraiseql-cli --test cascade_federation_shareable_e2e --features federation
	@cargo test -p fraiseql-cli --test error_union_federation_shareable_e2e --features federation
	@bash tools/federation/run-compose-check.sh

# ============================================================================
# Legacy database commands (local PostgreSQL)
# ============================================================================

# Database setup (local PostgreSQL)
db-setup-local:
	psql -U postgres -c "CREATE DATABASE fraiseql_test;"

# Database teardown (local)
db-teardown-local:
	psql -U postgres -c "DROP DATABASE IF EXISTS fraiseql_test;"

# Coverage report
coverage:
	cargo llvm-cov --all-features --workspace --html
	@echo "Coverage report generated in target/llvm-cov/html/index.html"

# Security audit (cargo-audit only).
# Lockstep check first: deny.toml and .cargo/audit.toml ignore lists must agree,
# otherwise `cargo audit` fails on advisories that deny.toml already accepts.
.PHONY: audit
audit:
	bash tools/check-audit-lockstep.sh
	cargo audit

# Gate: fail if any accepted-advisory deadline in deny.toml has lapsed.
.PHONY: lint-deadlines
.PHONY: lint-audit-ledger
.PHONY: lint-doc-claims
.PHONY: lint-release-validation
lint-deadlines:
	bash tools/check-deadlines.sh

# Gate: the security findings ledger is complete against the tracked audit.
lint-audit-ledger:
	bash tools/check-audit-ledger.sh

# Gate: documents claim only what the tree has (removed backends, architecture file names,
# observer action types, roadmap version lines, the README parity count).
lint-doc-claims:
	bash tools/check-doc-claims.sh

# Gate: the post-publish package validations in release.yml and npm-publish.yml are blocking
# and exercise the published package.
lint-release-validation:
	python3 tools/check-release-validation.py

# Gate: no workflow job runs cargo under a RUSTC_WRAPPER it never installs. v2.15.0's
# publish-rust-sdk inherited `sccache` from the workflow env and died on its first rustc.
.PHONY: lint-rustc-wrapper
lint-rustc-wrapper:
	python3 tools/check-rustc-wrapper.py

# Gate: every [workspace] members entry must reach the release Dockerfile's builder
# stage. cargo loads the whole workspace manifest before building anything, so an
# uncopied member is not a partial build — the image cannot be built at all (#1205).
.PHONY: lint-dockerfile-members
lint-dockerfile-members:
	@bash tools/check-dockerfile-workspace-members.sh

# Unit tests for the gate above. The verdicts worth pinning are the silent ones: a
# `COPY --from=` is a stage copy, a COPY in a later stage does not feed the builder,
# and a member excluded by .dockerignore is copied as nothing.
.PHONY: test-dockerfile-members-gate
test-dockerfile-members-gate:
	@bash tools/tests/dockerfile_workspace_members_test.sh

# Gate: no Dockerfile's Rust base image may be older than [workspace.package]
# rust-version. The release Dockerfile pinned rust:1.92-slim against a 1.94 MSRV and
# could not have built the workspace at all; docker-build.yml is tag-only, so the
# first witness would have been the release (#1107). Floating tags (rust:latest,
# rust:1-slim) are deliberately allowed — they cannot be older than the MSRV.
.PHONY: lint-dockerfile-msrv
lint-dockerfile-msrv:
	@bash tools/check-dockerfile-msrv.sh

# Unit tests for the gate above — its boundaries are what matter: rust:1.94 floats
# the patch and must pass a 1.94.1 MSRV, rust:1.94.0 must not, and 1.100.0 must
# compare as a version rather than a string.
.PHONY: test-dockerfile-msrv-gate
test-dockerfile-msrv-gate:
	@bash tools/tests/dockerfile_msrv_test.sh

# Gate: every `cargo deny check` invocation that covers `bans` must escalate
# cargo-deny's unmatched-skip lints to errors. Both default to WARN, so a
# [[bans.skip-tree]]/[[bans.skip]] entry whose pin has gone stale (its crate
# moved to another semver line) covers nothing and the run still exits 0 (#1020;
# #933 is what that looks like downstream). The level cannot be set in deny.toml, so it lives on three command
# lines — which is the drift shape this gate exists to pin.
.PHONY: lint-deny-flags
lint-deny-flags:
	@python3 tools/check-deny-lint-flags.py

# Unit tests for the gate above. Its red capability is worth pinning: two of its
# assertions cover verdicts an earlier draft got silently wrong (a substring match,
# and a neighbouring command's flags counting toward a flagless invocation).
.PHONY: test-deny-flags-gate
test-deny-flags-gate:
	@bash tools/tests/deny_lint_flags_test.sh

# Gate: the default fraiseql-server build must link exactly one rustls crypto
# provider (ring) and one rustls major (M-dual-crypto). cargo-deny cannot express
# this — ring and aws-lc-rs are distinct crates, so a dual-provider build looks fine
# to its multiple-versions ban. See tools/check-crypto-providers.sh for scope.
.PHONY: lint-crypto-providers
lint-crypto-providers:
	bash tools/check-crypto-providers.sh

# Full security checks: advisory scan + supply-chain policy gate.
# Run before opening a PR to catch new advisories early.
.PHONY: security
security:
	bash tools/check-audit-lockstep.sh
	bash tools/check-deadlines.sh
	bash tools/check-crypto-providers.sh
	python3 tools/check-deny-lint-flags.py
	cargo deny check -D unmatched-skip-root -D unmatched-skip
	cargo audit
	@echo "Security checks passed"

# Report test counts — run this before each release and update overview.md if the order of magnitude changed
test-count:
	@echo "=== Test count report ==="
	@echo "Unit tests (#[test]):         $$(grep -r '#\[test\]' crates/ --include='*.rs' | wc -l)"
	@echo "Async tests (#[tokio::test]): $$(grep -r '#\[tokio::test\]' crates/ --include='*.rs' | wc -l)"
	@echo "Property tests (proptest!):   $$(grep -r 'proptest!' crates/ --include='*.rs' | wc -l)"

# Update dependencies
update:
	cargo update

# Check for outdated dependencies
outdated:
	cargo outdated

# ============================================================================
# E2E Testing - Language Generators
# ============================================================================

## Setup: Start Docker databases and prepare for E2E tests
e2e-setup:
	@echo "🔧 Setting up E2E test infrastructure..."
	@docker compose -f docker/docker-compose.test.yml up -d || echo "ℹ️  Docker compose not available, skipping database setup"
	@echo "✅ E2E infrastructure ready"

## Run E2E tests for Python language generator
e2e-python: e2e-setup
	@echo ""
	@echo "========== PYTHON E2E TEST =========="
	@export PATH="$(PWD)/target/release:$$PATH" && \
		cd sdks/official/fraiseql-python && \
		. .venv/bin/activate && \
		echo "✅ Python environment ready" && \
		echo "" && \
		echo "Running E2E tests..." && \
		python -m pytest ../tests/e2e/python_e2e_test.py -v 2>/dev/null || python ../tests/e2e/python_e2e_test.py && \
		echo "✅ Python E2E tests passed"
	@echo ""

## Run E2E tests for TypeScript language generator
e2e-typescript: e2e-setup
	@echo ""
	@echo "========== TYPESCRIPT E2E TEST =========="
	@echo "✅ TypeScript environment ready"
	@echo "Running E2E tests..."
	@npm test --prefix sdks/official/fraiseql-typescript
	@echo "✅ TypeScript E2E tests passed"
	@echo ""

## Run E2E tests for Java language generator
e2e-java: e2e-setup
	@echo ""
	@echo "========== JAVA E2E TEST =========="
	@echo "Skipping Java E2E (requires Maven setup)"
	@echo ""

## Run E2E tests for Go language generator
e2e-go: e2e-setup
	@echo ""
	@echo "========== GO E2E TEST =========="
	@echo "✅ Go environment ready"
	@echo "Running E2E tests..."
	@cd sdks/official/fraiseql-go && go test ./fraiseql/... -v
	@echo "✅ Go E2E tests passed"
	@echo ""

## Run E2E tests for PHP language generator
e2e-php: e2e-setup
	@echo ""
	@echo "========== PHP E2E TEST =========="
	@echo "Skipping PHP E2E (requires Composer setup)"
	@echo ""

## Run E2E tests for VelocityBench blogging app (integration test)
e2e-velocitybench: e2e-setup
	@echo ""
	@echo "========== VELOCITYBENCH E2E TEST =========="
	@export PATH="$(PWD)/target/release:$$PATH" && \
		. sdks/official/fraiseql-python/.venv/bin/activate && \
		echo "✅ Test environment ready" && \
		echo "" && \
		echo "Running VelocityBench blogging app E2E test..." && \
		python tests/e2e/velocitybench_e2e_test.py && \
		echo "✅ VelocityBench E2E test passed"
	@echo ""

## Run E2E tests for all available languages (sequential)
e2e-all: e2e-python e2e-typescript e2e-go e2e-velocitybench
	@echo ""
	@echo "=============================================="
	@echo "✅ All E2E tests completed!"
	@echo "=============================================="
	@echo ""

## Run FraiseQL performance benchmark via velocitybench (sequential isolation)
## Requires: velocitybench running at VELOCITYBENCH_DIR with postgres seeded
VELOCITYBENCH_DIR   ?= $(HOME)/code/velocitybench
BENCH_VARIANT       ?= fraiseql-tv
BENCH_DURATION      ?= 30
BENCH_WORKERS       ?= 40
# bench_sequential.py --output writes a .md file; JSON lands at the same stem (.json).
# e.g. --output /tmp/bench.md -> JSON at /tmp/bench.json
BENCH_OUTPUT_MD     ?= /tmp/fraiseql-bench-results.md
BENCH_OUTPUT_JSON   ?= /tmp/fraiseql-bench-results.json
BENCH_REPORT_FILE   ?= /tmp/fraiseql-bench-report.md

.PHONY: bench-fraiseql bench-check-regression bench-update-baseline

## Run fraiseql-tv benchmark via velocitybench; inject local binary first.
## Requires: velocitybench postgres running (cd $(VELOCITYBENCH_DIR) && docker compose up -d postgres)
bench-fraiseql:
	@echo "========== VELOCITYBENCH PERFORMANCE BENCHMARK =========="
	@echo "Variant: $(BENCH_VARIANT), Duration: $(BENCH_DURATION)s, Workers: $(BENCH_WORKERS)"
	@echo "Injecting local fraiseql-server binary..."
	cp target/release/fraiseql-server $(VELOCITYBENCH_DIR)/frameworks/fraiseql/fraiseql-server
	cd $(VELOCITYBENCH_DIR) && \
		python tests/benchmark/bench_sequential.py \
		  --frameworks $(BENCH_VARIANT) \
		  --duration $(BENCH_DURATION) \
		  --concurrency $(BENCH_WORKERS) \
		  --output $(BENCH_OUTPUT_MD)
	@echo "Results written to $(BENCH_OUTPUT_JSON)"

# Velocitybench numbers compare only on the host that measured them; the committed
# baseline is forge's (.github/workflows/benchmark-velocity.yml).
BENCH_HOST ?= $(shell hostname -s)

## Compare last bench run against benchmarks/baseline.json; fail on >5% RPS regression
bench-check-regression: bench-fraiseql
	python benchmarks/detect_regression.py \
	  --results $(BENCH_OUTPUT_JSON) \
	  --baseline benchmarks/baseline.json \
	  --framework $(BENCH_VARIANT) \
	  --require-host $(BENCH_HOST) \
	  --output $(BENCH_REPORT_FILE)
	@cat $(BENCH_REPORT_FILE)

## Update benchmarks/baseline.json from the last bench run (commit after running)
bench-update-baseline: bench-fraiseql
	python benchmarks/detect_regression.py \
	  --results $(BENCH_OUTPUT_JSON) \
	  --baseline benchmarks/baseline.json \
	  --framework $(BENCH_VARIANT) \
	  --host $(BENCH_HOST) \
	  --update
	@echo "Run: git add benchmarks/baseline.json && git commit -m 'chore(bench): update performance baseline'"

## Cleanup: Stop Docker containers and remove temp files
e2e-clean:
	@echo "🧹 Cleaning up E2E test infrastructure..."
	@docker compose -f docker/docker-compose.test.yml down -v 2>/dev/null || true
	@rm -rf /tmp/fraiseql-*-test-output
	@echo "✅ Cleanup complete"

## Pipeline E2E: compile schema → run stage-5 query tests
## Requires: Docker (for Postgres), Python 3.12+, FRAISEQL_TEST_URL env var
e2e: e2e-setup
	@echo "[Stage 2] Compiling schema..."
	@cargo run -p fraiseql-cli -- compile tests/e2e/schema.json \
	  --output tests/e2e/schema.compiled.json 2>/dev/null || \
	  echo "Note: compile stage requires a generated schema.json (run: uv run python tests/e2e/schema/types.py > tests/e2e/schema.json)"
	@echo "E2E infrastructure ready."
	@echo "To run query tests: FRAISEQL_TEST_URL=http://localhost:17843 pytest tests/e2e/test_stage5_queries.py -v"

## Status: Check E2E test infrastructure
e2e-status:
	@echo "Docker Compose Status:"
	@docker compose -f docker/docker-compose.test.yml ps 2>/dev/null || echo "Docker not available"
	@echo ""
	@echo "Languages ready:"
	@which python3 > /dev/null && echo "  ✅ Python" || echo "  ❌ Python"
	@which npm > /dev/null && echo "  ✅ TypeScript/Node" || echo "  ❌ TypeScript/Node"
	@which go > /dev/null && echo "  ✅ Go" || echo "  ❌ Go"
	@which mvn > /dev/null 2>&1 || [ -d "$$HOME/.local/opt/apache-maven-"* ] && echo "  ✅ Java" || echo "  ❌ Java"
	@which php > /dev/null && echo "  ✅ PHP" || echo "  ❌ PHP"

# ============================================================================
# Cross-SDK Parity Testing
# ============================================================================

## Run the cross-SDK parity gate locally (absent toolchains reported, not gated)
##
## Same script CI runs — one definition. CI runs it without --allow-missing, so a
## toolchain this box does not have is a hard failure there and a named omission here.
## There used to be a second, divergent copy of this comparison inlined in the Makefile;
## it covered a different set of SDKs, ran in a workflow that had been failing for
## unrelated reasons, and disagreed with the workflow about what "parity" meant (#952).
test-parity:
	@sdks/official/tests/run_parity.sh --allow-missing

## Run the cross-SDK parity gate exactly as CI does — every toolchain required
test-parity-strict:
	@sdks/official/tests/run_parity.sh

# ============================================================================
# examples/ gate — the two tiers that need a toolchain
#
# The static tier (tools/check-examples-integrity.sh) runs in preflight as
# `lint-examples-integrity`. These two execute things, so they need a built CLI
# and, for the smoke, a PostgreSQL. Both mirror the `examples` Dagger integration
# suite; neither skips.
# ============================================================================

## Compile every example's authoring artifact with the CLI in this tree
.PHONY: examples-compile
examples-compile:
	@cargo build -p fraiseql-cli --bin fraiseql-cli
	@FRAISEQL_BIN=$(CURDIR)/target/debug/fraiseql-cli bash tools/check-examples-compile.sh

## Load, compile, resolve every shipped query, and boot the server on one example
##
## Needs a PostgreSQL it may CREATE DATABASE on. The local rig is `make db-up`:
##   DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/postgres make examples-smoke
.PHONY: examples-smoke
examples-smoke:
	@cargo build -p fraiseql-cli --bin fraiseql-cli
	@cargo build -p fraiseql-server --bin fraiseql-server
	@FRAISEQL_BIN=$(CURDIR)/target/debug/fraiseql-cli \
	 SERVER_BIN=$(CURDIR)/target/debug/fraiseql-server \
	 bash tools/examples-smoke.sh
