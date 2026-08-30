.PHONY: help build test test-unit test-integration federation-compose-check test-full test-all-ignored clippy fmt check clean clean-test-containers install dev doc bench memory-profile db-up db-down db-logs db-reset db-failover-reset db-status e2e e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-velocitybench e2e-clean e2e-status test-parity test-parity-strict security audit test-count lint-gate lint-gate-db lint-gate-wire lint-gate-core lint-unwrap lint-expect lint-tests-layout lint-guard-parity release release-validate release-validate-semver load-test load-test-all chart-deploy compose-stack changelog changelog-full

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run unit + integration tests (PostgreSQL) — ~50 min, serialized"
	@echo "  make test-unit          - Run unit tests only (fast, no database)"
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
# Tokenless and read-only — never publishes. Run before cutting a release tag to
# catch the "tag shipped but publish failed" class (v2.3.0/v2.3.1) up front.
#   make release-validate                 # self-test the publish order + dry-run every crate
#   make release-validate VERSION=2.4.0   # also assert Cargo.toml is on 2.4.0
release-validate:
	dagger call publish-order-selftest --source=.
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
	@echo "=== Running full test suite (8 steps) ==="
	@echo ""
	@echo "[1/8] Unit tests..."
	@cargo test --lib --all-features
	@echo ""
	@echo "[2/8] SQL snapshot tests..."
	@cargo nextest run --test sql_snapshots 2>/dev/null || cargo test --test sql_snapshots
	@echo ""
	@echo "[3/8] Database integration tests (PostgreSQL)..."
# Was the same `-p fraiseql-core … -- --ignored` line as test-integration's, with
# the same result: 1 test of 2828, under a banner claiming the database
# integration tests had run (#1169). A second corrected copy would drift from the
# first, so both call the one mirror target.
	@$(MAKE) --no-print-directory test-integration-postgres
	@echo ""
	@echo "[4/8] (retired — cross-database parity removed with the non-PostgreSQL backends, G2/#374)"
	@echo ""
	@echo "[5/8] Redis tests (APQ + observer queue/lease)..."
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-core --features "redis-apq" --lib redis -- --ignored --test-threads=1
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-observers --features "caching,queue,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[6/8] NATS + observer bridge tests..."
	cargo test -p fraiseql-observers --features "nats" --test nats_integration -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,nats" --test bridge_integration -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[7/8] Vault secrets manager tests..."
	VAULT_ADDR="http://localhost:8200" \
	VAULT_TOKEN="fraiseql-test-token" \
		cargo test -p fraiseql-server --test secrets_manager_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "[8/8] Server integration tests (database queries + observers)..."
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --test database_query_test -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --features "observers-nats" --test observer_runtime_integration_test -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "postgres,dedup,caching,testing" -p fraiseql-observers --test integration_test -- --ignored
	@echo ""
	@echo "=== Full test suite complete (all 8 steps passed) ==="

# Run unit tests only (no database required)
test-unit:
	@echo "Running unit tests..."
	@cargo test --lib --all-features

# Run integration tests (requires Docker databases)
# Runs each suite with the correct feature flags and env vars.
#
# The PostgreSQL section is `test-integration-postgres` — the mirror of the
# Dagger shard. It used to be one `-p fraiseql-core … -- --ignored` line, which
# ran 1 test out of 2828 across 77 binaries and then printed "All integration
# tests passed": none of those suites are `#[ignore]`d (they self-skip on an
# absent DATABASE_URL), and `--ignored` runs ONLY ignored tests (#1169).
test-integration: test-integration-postgres
	@echo ""
	@echo "=== fraiseql-observers integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "postgres,dedup,caching,testing" -p fraiseql-observers --test integration_test -- --ignored
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
# The line list is held to the shard's by `make lint-integration-parity`, in
# both directions. Do not edit one side alone. The rationale for each individual
# line lives with it in `.dagger/main.go` and is deliberately not duplicated
# here: two copies of a rationale drift, and only one copy can be right.
#
# Serialized on purpose: expect this to take a while. What is NOT mirrored is
# the shard's build environment (sccache, mold, CARGO_INCREMENTAL=0, a pinned
# toolchain image) — that is the local box's `.cargo/config.toml` to decide, and
# none of it changes what the tests assert.
test-integration-postgres: export DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql
test-integration-postgres: export STANDBY_DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5436/test_fraiseql
test-integration-postgres: export FAILOVER_STANDBY_DATABASE_URL := postgresql://fraiseql_test:fraiseql_test_password@localhost:5437/test_fraiseql
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
	@echo "=== integration (postgres) — mirroring .dagger/main.go integrationPostgres ==="
	@echo "### toolchain: $$(rustc --version)"
	@echo ""
	@echo "### core/db --test '*' sweep (serialized: shared public schema)"
	@cargo test -p fraiseql-core --features 'arrow,audit-syslog,audit-webhook,federation,kafka,postgres,redis-apq,schema-lint,test-utils,wire-backend,test-postgres' --test '*' -- --test-threads=1
	@cargo test -p fraiseql-db --features 'postgres,wire-backend,test-postgres' --test '*' -- --test-threads=1
	@echo ""
	@echo "### live-PostgreSQL lib tests"
	@cargo test -p fraiseql-db --lib --features 'postgres,wire-backend,test-postgres' -- --test-threads=1
	@cargo test -p fraiseql-functions --lib migrations::tests -- --test-threads=1
	@echo ""
	@echo "### auth: durable identity store, password, reset, verification, linking, single-use, sweeps"
	@cargo test -p fraiseql-auth --test postgres_account_store -- --test-threads=1
	@cargo test -p fraiseql-auth --test local_password -- --test-threads=1
	@cargo test -p fraiseql-auth --test password_reset -- --test-threads=1
	@cargo test -p fraiseql-auth --test email_verification -- --test-threads=1
	@cargo test -p fraiseql-auth --test social_linking -- --test-threads=1
	@cargo test -p fraiseql-auth --test postgres_single_use_consume -- --test-threads=1
	@cargo test -p fraiseql-auth --test postgres_expiry_sweep -- --test-threads=1
	@cargo test -p fraiseql-server --features auth --test integration -- --test-threads=1
	@cargo test -p fraiseql-auth --test session_state_integration -- --test-threads=1
	@echo ""
	@echo "### inbound spine, storage policy admin, sources, functions runtime"
	@cargo test -p fraiseql-webhooks --test inbound_pipeline_pg -- --test-threads=1
	@cargo test -p fraiseql-server --features inbound,inbound-email --lib inbound:: -- --test-threads=1
	@cargo test -p fraiseql-server --features 'arrow,auth,aws-s3,federation,grpc,mcp,metrics,observers,redis-apq,redis-pkce,redis-rate-limiting,rest,secrets,storage-transforms,testing,tracing-opentelemetry,webhooks,wire-backend' --lib server::routing::storage_policy_admin_tests -- --test-threads=1
	@cargo test -p fraiseql-server --features inbound-email --test inbound_email_dedup_scope_pg -- --test-threads=1
	@cargo test -p fraiseql-server --features sources --lib sources:: -- --test-threads=1
	@cargo test -p fraiseql-server --features functions-runtime,observers --lib -- cron:: routes::after_mutation:: query_bridge:: subsystems::loader:: function_metrics:: observers::pg_function_dlq:: --test-threads=1
	@cargo test -p fraiseql-server --features functions-runtime --test functions_schema_seam_test
	@cargo test -p fraiseql-server --features functions-runtime --test functions_query_bridge_pin_test
	@echo ""
	@echo "### saga: forward execution, compensation, recovery, remote dispatch"
	@cargo test -p fraiseql-federation --features saga,test-utils --test saga_integration -- --include-ignored --test-threads=1
	@echo ""
	@echo "### fraiseql-cli against-db suites (each self-skips without DATABASE_URL)"
	@cargo test -p fraiseql-cli --features test-postgres --test init_first_run_pg -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test generate_views_validate_pg -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test compile_drift_fail_pg -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test mutation_contract_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test doctor_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test source_probe_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test cascade_rls_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test setup_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test sources_against_db -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test validate_sql_sources_gate -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test runtime_smoke -- --test-threads=1
	@cargo test -p fraiseql-cli --features test-postgres --test perf_against_db -- --test-threads=1
	@echo ""
	@echo "### seed-fixture integrity (runs last: names any clobber a suite above left)"
	@cargo test -p fraiseql-db --features postgres,wire-backend,test-postgres --test seed_fixture_integrity -- --test-threads=1
	@echo ""
	@echo "test-integration OK: postgres suite passed"

# Run ALL #[ignore] tests — requires full test infrastructure (make db-up first).
# Covers: Redis APQ, NATS transport, observer bridge, Vault secrets, server DB queries.
# Stress tests (60s+ each) are excluded; run them separately with:
#   cargo test -p fraiseql-observers --test stress_tests -- --ignored
test-all-ignored: db-up
	@echo ""
	@echo "=== Redis tests (APQ + observer queue/lease) ==="
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-core --features "redis-apq" --lib redis -- --ignored --test-threads=1
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-observers --features "caching,queue,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "=== NATS transport tests ==="
	cargo test -p fraiseql-observers --features "nats" --test nats_integration -- --ignored --test-threads=1
	@echo ""
	@echo "=== Observer bridge tests (PostgreSQL + NATS) ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,nats" --test bridge_integration -- --ignored --test-threads=1
	@echo ""
	@echo "=== Observer PostgreSQL transport + lease tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "=== Vault secrets manager tests ==="
	VAULT_ADDR="http://localhost:8200" \
	VAULT_TOKEN="fraiseql-test-token" \
		cargo test -p fraiseql-server --test secrets_manager_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "=== Server database query tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --test database_query_test -- --ignored --test-threads=1
	@echo ""
	@echo "=== Observer server runtime tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --features "observers-nats" --test observer_runtime_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "All ignored tests passed."

# Run end-to-end tests
test-e2e:
	cargo test --test 'test_*' --all-features -- --ignored

# ============================================================================
# Changelog (git-cliff)
# ============================================================================

## Preview unreleased changelog entries
changelog:
	git cliff --unreleased --strip header

## Generate full changelog (overwrites CHANGELOG.md)
changelog-full:
	git cliff --output CHANGELOG.md

# Run Clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

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
# Baseline: 0 (plan-09 replaced the NaiveDate::from_ymd_opt().unwrap() with unreachable!).
# `#![allow]` inside `#[cfg(test)]` modules are excluded via `grep -v '#!\[allow'`.
# Raise UNWRAP_ALLOW_LIMIT only with a PR comment justifying each new addition.
UNWRAP_ALLOW_LIMIT ?= 0
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
ASYNC_TRAIT_LIMIT := 199
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
	@ALLOW_COUNT=$$(grep -c '#!\[allow(clippy::' crates/fraiseql-core/src/lib.rs); \
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
	@count=$$(grep -c '#!\[allow(clippy' crates/fraiseql-db/src/lib.rs); \
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
FRAISEQL_WIRE_LIB_ALLOWS_MAX ?= 15
.PHONY: lint-gate-wire
lint-gate-wire:
	@count=$$(grep -c '#!\[allow(clippy::' crates/fraiseql-wire/src/lib.rs); \
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
# extraction, internal floor bump, dry-run first-publish-sibling tolerance) cannot
# silently bit-rot.
.PHONY: test-release-tooling
test-release-tooling:
	@bash tools/tests/release_helpers_test.sh
	@bash tools/tests/dry_run_tolerance_test.sh
	@bash tools/tests/publish_parity_test.sh

# Unit tests for the advisory-deadline gate. Its boundary behaviour is worth
# pinning: an off-by-one is a day on which every open branch is blocked by a
# required check, and a file it forgets to scan is a risk acceptance that
# expires invisibly — both of which had already happened (#1103).
.PHONY: test-deadline-gate
test-deadline-gate:
	@bash tools/tests/check_deadlines_test.sh

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

.PHONY: lint-guard-parity
lint-guard-parity:
	@bash tools/check-guard-parity.sh

# Gate: every test suite (tests/*.rs binary and feature-gated lib test module)
# maps to a CI leg that actually executes it — execution coverage is a checked
# artifact, not an inference. Exemptions: tools/suite-coverage-exemptions.toml.
.PHONY: lint-suite-coverage
lint-suite-coverage:
	@python3 tools/check-suite-coverage.py

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

# Gate: fuzz.yml's matrix may only name targets that exist in the crate it names.
# `{crate: fraiseql-core, target: toml_config}` named a target that had MOVED to
# fraiseql-server, so `cargo fuzz build` failed and every scheduled run was red from at
# least 2026-05-17 (#1128). One-directional by design — 14 of the 26 targets in the tree
# are deliberately outside the curated "#441 minimum" matrix.
.PHONY: lint-fuzz-targets
lint-fuzz-targets:
	@bash tools/check-fuzz-targets.sh

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

# Unit tests for the gate above. Both halves have to be able to go red: a fuzz crate
# that does not compile, and a target file with no [[bin]] — the second is the gate's
# own blind spot, since `cargo check` only builds the bins the manifest declares.
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

# Gate: every publishable workspace crate is published by release.yml, in the same
# order .dagger/release.go dry-runs and topologically self-tests. fraiseql-cdc-sinks
# (#382) reached the workspace and legacyPublishOrder but never reached release.yml;
# because it is an OPTIONAL dependency of fraiseql-server, the pre-tag dry-run
# tolerated the gap (its tolerance forgives a sibling that is in the list it was
# handed) while the real publish could not resolve it at all. Nothing compared the
# two lists until this gate.
.PHONY: lint-publish-parity
lint-publish-parity:
	@python3 tools/check-publish-parity.py

# Gate: pin the set of production files that READ TypeDefinition.internal (#665), so a
# property-named flag cannot silently grow new consumers. See tools/check-internal-flag-sites.sh.
.PHONY: lint-internal-flag
lint-internal-flag:
	@bash tools/check-internal-flag-sites.sh

# Gate: the third-party GraphQL parser is called from exactly one seam (#976). It
# panics on a client-controlled block string, so every parse must go through
# `parse_graphql_document`, which rejects the input the parser cannot handle.
.PHONY: lint-graphql-parse
lint-graphql-parse:
	@bash tools/check-graphql-parse-sites.sh

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

# Gate: a runaway-growth ratchet on each crate's src/ line count, and a check that
# every crate HAS a budget. Ran in no leg at all until #1055/#990 — its only caller
# was the orphaned tools/lint.sh — by which time four crates were over budget and
# five had no budget row.
.PHONY: lint-crate-sizes
lint-crate-sizes:
	@bash tools/check-crate-sizes.sh

# Gate: every official SDK is gated by a workflow that runs on a BRANCH push. Four of
# the eleven were not — two declared `tags` with no `branches` (which suppresses every
# branch push), one was post-merge-only, and the official Ruby SDK's tests ran nowhere
# because the workflow named for it watches the community copy (#1119).
.PHONY: lint-sdk-workflows
lint-sdk-workflows:
	@python3 tools/check-sdk-workflow-coverage.py

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

.PHONY: lint-preflight-parity
lint-preflight-parity:
	@python3 tools/check-preflight-parity.py

# Red-capability pin for the parity gate. A gate asserting two lists agree is
# itself a third place the assurance can be false, so each way they can diverge
# has a fixture that must be reported.
.PHONY: test-preflight-parity
test-preflight-parity:
	@bash tools/tests/preflight_parity_test.sh

# Gate: `make test-integration-postgres` must run exactly what the Dagger
# `integration (postgres)` shard runs. The knowledge that these suites only pass
# serialized lived only in `.dagger/main.go`, so the two commands a developer
# reaches for were a false-red and a false-green respectively (#1169). A mirror
# maintained by hand in a second file is only worth having if it cannot drift.
.PHONY: lint-integration-parity
lint-integration-parity:
	@python3 tools/check-integration-parity.py

# Red-capability pin for the integration parity gate: a dropped line, an added
# line, a changed flag, and a shard shape the parser cannot read must each be
# reported rather than passed over.
.PHONY: test-integration-parity
test-integration-parity:
	@bash tools/tests/integration_parity_test.sh

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

# Run the cheap-but-frequent CI gates locally before `git push`, to catch the
# failures the Dagger `preflight` leg would reject — rustfmt drift, clippy
# `-D warnings`, broken rustdoc intra-doc links, and the grep/wc policy gates —
# for free instead of paying for a CI rerun. Mirrors the `.dagger` Preflight +
# ShellGates leg (UNWRAP_ALLOW_LIMIT pinned to 3 to match CI). Does NOT run the
# test suite or service-backed integration tests — those are `make test` and the
# separate Dagger test/integration legs.
.PHONY: preflight
preflight: fmt-check lint-sdk-dead-surface lint-tests-layout lint-expect lint-async-trait lint-gate-db lint-gate-core lint-deadlines lint-deploy-security lint-deploy-versions lint-fuzz-targets lint-compose-references lint-doc-image-refs lint-phases-citations lint-image-context lint-publish-parity lint-routes lint-guard-parity lint-internal-flag lint-value-json lint-graphql-parse lint-docs-env-vars lint-docs-version lint-config-loaders lint-public-api-reexports lint-sdk-publication-claims lint-examples-postgres-only lint-examples-integrity lint-r-examples lint-suite-coverage lint-snapshot-pairing lint-empty-tests lint-feature-chains lint-crate-sizes lint-sdk-workflows lint-workflow-reachability lint-preflight-parity lint-integration-parity lint-deny-flags lint-dockerfile-msrv lint-dockerfile-members lint-image-parity lint-delivery-coverage lint-sdk-lockfile-freshness test-release-tooling test-changelog-gate test-deadline-gate test-preflight-parity test-integration-parity test-imports-gate test-suite-coverage-workflows test-workflow-reachability-gate test-deny-flags-gate test-dockerfile-msrv-gate test-dockerfile-members-gate test-image-parity-gate test-delivery-coverage-gate test-sdk-lockfile-freshness-gate test-feature-matrix-gate test-suite-coverage-inner-gates test-conformance-selftest test-public-api-reexports-gate test-sdk-publication-claims-gate test-fuzz-compiles-gate test-compose-references-gate test-doc-image-refs-gate test-example-crates-gate test-r-examples-gate test-phases-citations-gate test-image-context-gate
	@echo "=== preflight: lint-unwrap (UNWRAP_ALLOW_LIMIT=3) ==="
	@$(MAKE) --no-print-directory lint-unwrap UNWRAP_ALLOW_LIMIT=3
	@echo "=== preflight: check-test-imports ==="
	@bash tools/check-test-imports.sh
	@echo "=== preflight: check-audit-lockstep ==="
	@bash tools/check-audit-lockstep.sh
	@echo "=== preflight: rustdoc (-D warnings, --all-features) ==="
	RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
	@echo "=== preflight: rustdoc (default features — the links --all-features resolves) ==="
	RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --keep-going
	@echo "=== preflight: clippy (--all-targets --all-features -D warnings) ==="
	@$(MAKE) --no-print-directory clippy
	@echo "=== preflight: check-default (default features — the feature-OFF arms) ==="
	@$(MAKE) --no-print-directory check-default
	@echo "=== preflight: check-fuzz (the crates/*/fuzz crates nothing else compiles) ==="
	@$(MAKE) --no-print-directory check-fuzz
	@echo "=== preflight: check-example-crates (the standalone examples/ crates nothing else compiles) ==="
	@$(MAKE) --no-print-directory check-example-crates
	@echo ""
	@echo "✅ preflight passed — mirrors the Dagger preflight leg."
	@echo "⚠  It does NOT run clippy under a narrow feature set: its clippy pass is"
	@echo "   --all-features (feature-OFF arms are not compiled) and its narrow pass is"
	@echo "   cargo check (no clippy lints). Before pushing anything under a"
	@echo "   #[cfg(feature = ...)], run: make lint-feature-matrix   (#1227)"

# Format code (nightly rustfmt for advanced formatting options)
fmt:
	cargo +nightly fmt --all

# Check formatting
fmt-check:
	cargo +nightly fmt --all -- --check

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
	@docker ps -q --filter "ancestor=postgres:11-alpine" | xargs -r docker stop
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
	@command -v critcmp >/dev/null 2>&1 || cargo install critcmp --locked
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
	@for svc in postgres-test postgres-standby-test postgres-failover-test postgres-tls-test redis-test nats-test vault-test; do \
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
lint-deadlines:
	bash tools/check-deadlines.sh

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
# [[bans.skip-tree]]/[[bans.skip]] entry whose exact-version pin has gone stale
# covers nothing and the run still exits 0 (#1020; #933 is what that looks like
# downstream). The level cannot be set in deny.toml, so it lives on three command
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

## Compare last bench run against benchmarks/baseline.json; fail on >5% RPS regression
bench-check-regression: bench-fraiseql
	python benchmarks/detect_regression.py \
	  --results $(BENCH_OUTPUT_JSON) \
	  --baseline benchmarks/baseline.json \
	  --framework $(BENCH_VARIANT) \
	  --output $(BENCH_REPORT_FILE)
	@cat $(BENCH_REPORT_FILE)

## Update benchmarks/baseline.json from the last bench run (commit after running)
bench-update-baseline: bench-fraiseql
	python benchmarks/detect_regression.py \
	  --results $(BENCH_OUTPUT_JSON) \
	  --baseline benchmarks/baseline.json \
	  --framework $(BENCH_VARIANT) \
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
