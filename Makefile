.PHONY: help build test test-unit test-integration test-federation test-full test-all-ignored clippy fmt check clean clean-test-containers install dev doc bench memory-profile db-up db-down db-logs db-reset db-status federation-up federation-down demo-start demo-stop demo-logs demo-status demo-clean demo-restart examples-start examples-stop examples-logs examples-status examples-clean e2e e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-velocitybench e2e-clean e2e-status parity-generate parity-compare test-parity security audit test-count lint-gate lint-gate-db lint-gate-core lint-unwrap lint-expect release load-test load-test-all helm-lint changelog changelog-full

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run unit + integration tests (PostgreSQL)"
	@echo "  make test-unit          - Run unit tests only (fast, no database)"
	@echo "  make test-integration   - Run integration tests (requires Docker)"
	@echo "  make test-full          - Run ALL categories: unit + snapshots + DBs + Redis/NATS/Vault + server + federation"
	@echo "  make test-federation    - Run federation tests (requires Docker)"
	@echo "  make test-all-ignored   - Run ALL #[ignore] tests (requires full infra: db-up)"
	@echo "  make test-parity        - Run cross-SDK parity checks (requires uv, bun, go, mvn, php)"
	@echo "  make coverage           - Generate test coverage report"
	@echo "  make load-test          - Run k6 mixed-workload load test (requires running server)"
	@echo "  make load-test-all      - Run all k6 load test scenarios"
	@echo ""
	@echo "Database (Docker):"
	@echo "  make db-up              - Start all test infrastructure (PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault)"
	@echo "  make db-down            - Stop test infrastructure"
	@echo "  make db-logs            - View infrastructure logs"
	@echo "  make db-reset           - Reset test infrastructure (remove volumes)"
	@echo "  make db-status          - Check infrastructure health"
	@echo "  make federation-up      - Start federation stack (Apollo Router + 3 subgraphs)"
	@echo "  make federation-down    - Stop federation stack"
	@echo ""
	@echo "Code Quality:"
	@echo "  make build              - Build all crates"
	@echo "  make clippy             - Run Clippy linter"
	@echo "  make fmt                - Format code with rustfmt"
	@echo "  make check              - Run all checks (fmt + clippy + test)"
	@echo "  make helm-lint          - Lint and template-test the Helm chart"
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
	@echo "Docker Demo (Newcomers):"
	@echo "  make demo-start         - Start single-example stack (blog only)"
	@echo "  make demo-stop          - Stop demo stack"
	@echo "  make demo-logs          - View demo logs"
	@echo "  make demo-status        - Check demo health"
	@echo "  make demo-restart       - Restart demo stack"
	@echo "  make demo-clean         - Remove demo volumes and stop"
	@echo ""
	@echo "Docker Examples (Advanced - with local build):"
	@echo "  make examples-start     - Start multi-example stack (blog, ecommerce, streaming)"
	@echo "  make examples-stop      - Stop examples stack"
	@echo "  make examples-logs      - View examples logs"
	@echo "  make examples-status    - Check examples health"
	@echo "  make examples-clean     - Remove examples volumes and stop"
	@echo ""
	@echo "Docker Production (Pre-built Images - No Local Build):"
	@echo "  make prod-start         - Start production demo (single example, pre-built)"
	@echo "  make prod-stop          - Stop production demo"
	@echo "  make prod-status        - Check production health"
	@echo "  make prod-logs          - View production logs"
	@echo "  make prod-clean         - Remove production volumes"
	@echo "  make prod-examples-start - Start production multi-example (all 3, pre-built)"
	@echo "  make prod-examples-stop  - Stop production multi-example"
	@echo "  make prod-examples-status - Check multi-example health"
	@echo "  make prod-examples-clean  - Remove multi-example volumes"
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

# Run all tests (unit + integration)
test: test-unit test-integration

# Run the full test suite: unit + snapshots + all DBs + Redis/NATS/Vault + server + federation
# Requires full infrastructure: Docker with PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault + Apollo Router
# Reports a single pass/fail at the end.
test-full: db-up federation-up
	@echo "=== Running full test suite (9 steps) ==="
	@echo ""
	@echo "[1/9] Unit tests..."
	@cargo test --lib --all-features
	@echo ""
	@echo "[2/9] SQL snapshot tests..."
	@cargo nextest run --test sql_snapshots 2>/dev/null || cargo test --test sql_snapshots
	@echo ""
	@echo "[3/9] Database integration tests (PostgreSQL, MySQL, SQL Server)..."
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-postgres -p fraiseql-core -- --ignored --test-threads=4
	DATABASE_URL="mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-mysql -p fraiseql-core -- --ignored --test-threads=1
	DATABASE_URL="server=localhost,1434;database=test_fraiseql;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-sqlserver -p fraiseql-core -- --ignored --test-threads=1
	@echo ""
	@echo "[4/9] Cross-database parity tests..."
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	MYSQL_URL="mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql" \
		cargo test --features test-postgres,test-mysql -p fraiseql-core \
		    --test cross_database_test -- --ignored --test-threads=1
	@echo ""
	@echo "[5/9] Redis tests (APQ + observer queue/lease)..."
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-core --features "redis-apq" --lib redis -- --ignored --test-threads=1
	REDIS_URL="redis://localhost:6379" \
		cargo test -p fraiseql-observers --features "caching,queue,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[6/9] NATS + observer bridge tests..."
	cargo test -p fraiseql-observers --features "nats" --test nats_integration -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,nats" --test bridge_integration -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	TEST_DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-observers --features "postgres,redis-lease" --lib -- --ignored --test-threads=1
	@echo ""
	@echo "[7/9] Vault secrets manager tests..."
	VAULT_ADDR="http://localhost:8200" \
	VAULT_TOKEN="fraiseql-test-token" \
		cargo test -p fraiseql-server --test secrets_manager_integration_test -- --ignored --test-threads=1
	@echo ""
	@echo "[8/9] Server integration tests (database queries + observers)..."
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --test database_query_test -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server --features "observers-nats" --test observer_runtime_integration_test -- --ignored --test-threads=1
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "postgres,dedup,caching,testing" -p fraiseql-observers --test integration_test -- --ignored
	@echo ""
	@echo "[9/9] Federation integration tests..."
	@cd docker/federation-ci && pytest -q --tb=short
	@echo ""
	@echo "=== Full test suite complete (all 9 steps passed) ==="

# Run unit tests only (no database required)
test-unit:
	@echo "Running unit tests..."
	@cargo test --lib --all-features

# Run integration tests (requires Docker databases)
# Runs each suite with the correct feature flags and env vars.
test-integration: db-up
	@echo ""
	@echo "=== PostgreSQL integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-postgres -p fraiseql-core -- --ignored --test-threads=4
	@echo ""
	@echo "=== MySQL integration tests ==="
	DATABASE_URL="mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-mysql -p fraiseql-core -- --ignored --test-threads=1
	@echo ""
	@echo "=== SQL Server integration tests ==="
	DATABASE_URL="server=localhost,1434;database=test_fraiseql;user=sa;password=FraiseQL_Test1234;TrustServerCertificate=true" \
	SAGA_STORE_TEST_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test --features test-sqlserver -p fraiseql-core -- --ignored --test-threads=1
	@echo ""
	@echo "=== fraiseql-observers integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
	REDIS_URL="redis://localhost:6379" \
		cargo test --features "postgres,dedup,caching,testing" -p fraiseql-observers --test integration_test -- --ignored
	@echo ""
	@echo "=== fraiseql-server integration tests ==="
	DATABASE_URL="postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql" \
		cargo test -p fraiseql-server -- --ignored
	@echo ""
	@echo "All integration tests passed."

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
ASYNC_TRAIT_LIMIT := 144
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

# Format code (nightly rustfmt for advanced formatting options)
fmt:
	cargo +nightly fmt --all

# Check formatting
fmt-check:
	cargo +nightly fmt --all -- --check

# Run all checks
check: fmt-check clippy test

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
helm-lint:
	helm lint deploy/kubernetes/helm/fraiseql/
	helm template test deploy/kubernetes/helm/fraiseql/ > /dev/null

# Watch for changes and run tests
watch:
	cargo watch -x 'test --all-features'

# Watch for changes and run checks
watch-check:
	cargo watch -x 'check --all-features'

# ============================================================================
# Docker-based Test Database Management
# ============================================================================

# Start all test infrastructure (PostgreSQL, MySQL, SQL Server, Redis, NATS, Vault)
# and wait until each service is healthy.
db-up:
	@echo "Starting test infrastructure..."
	@docker compose -f docker/docker-compose.test.yml up -d
	@echo "Waiting for all services to be healthy..."
	@for svc in postgres-test mysql-test sqlserver-test redis-test nats-test vault-test; do \
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
# Federation stack (Apollo Router + 3 subgraphs)
# ============================================================================

# Start the federation Docker stack and wait for all services to be healthy
federation-up:
	@echo "Starting federation stack..."
	@docker compose -f docker/federation-ci/docker-compose.yml up -d --build
	@echo "Waiting for federation services to be healthy..."
	@for url in \
		"http://localhost:8088/health" \
		"http://localhost:4001/health" \
		"http://localhost:4002/health" \
		"http://localhost:4003/health"; do \
		printf "  Waiting for %-35s" "$$url..."; \
		for i in $$(seq 1 30); do \
			if curl -sf "$$url" >/dev/null 2>&1; then echo " ready"; break; fi; \
			if [ $$i -eq 30 ]; then echo " TIMEOUT"; exit 1; fi; \
			sleep 4; \
		done; \
	done
	@echo "Federation stack ready."
	@echo "  Apollo Router:   http://localhost:4000/graphql"
	@echo "  Users service:   http://localhost:4001/graphql"
	@echo "  Orders service:  http://localhost:4002/graphql"
	@echo "  Products service:http://localhost:4003/graphql"

# Stop the federation stack and remove volumes
federation-down:
	@echo "Stopping federation stack..."
	@docker compose -f docker/federation-ci/docker-compose.yml down -v

# Run federation integration tests (starts stack, runs tests, tears down)
test-federation: federation-up
	@echo ""
	@echo "=== Federation integration tests ==="
	@cargo test -p fraiseql-core federation -- --ignored --test-threads=4; \
		EXIT=$$?; \
		$(MAKE) federation-down; \
		exit $$EXIT

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

# Security audit (cargo-audit only)
audit:
	cargo audit

# Full security checks: advisory scan + supply-chain policy gate.
# Run before opening a PR to catch new advisories early.
.PHONY: security
security:
	cargo deny check
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

PARITY_GOLDEN := tests/fixtures/golden/parity-schema.json

## Generate parity schemas from all 5 authoring SDKs into /tmp/parity-*.json
parity-generate:
	@echo "=== Generating parity schemas ==="
	@cd sdks/official/fraiseql-python && uv run python tests/generate_parity_schema.py \
	    > /tmp/parity-python.json
	@echo "  [1/5] Python done"
	@cd sdks/official/fraiseql-typescript && PATH="$$PATH:$$HOME/.bun/bin:$$HOME/.local/bin" \
	    bun run tests/generate-parity-schema.ts > /tmp/parity-typescript.json
	@echo "  [2/5] TypeScript done"
	@cd sdks/official/fraiseql-go && go test -run TestGenerateParitySchema -v ./fraiseql/ 2>&1 | \
	    python3 -c "import sys; d=sys.stdin.read(); s=d.find('{'); print(d[s:d.rfind('}')+1])" \
	    > /tmp/parity-go.json
	@echo "  [3/5] Go done"
	@cd sdks/official/fraiseql-java && \
	    JAVA_HOME="$${JAVA_HOME:-$$(ls -d /usr/lib/jvm/java-*-openjdk 2>/dev/null | grep -v runtime | head -1)}" \
	    mvn -q test -Dtest=GenerateParitySchema "-DschemaOutputFile=/tmp/parity-java.json"
	@echo "  [4/5] Java done"
	@cd sdks/official/fraiseql-php && php tests/GenerateParitySchema.php \
	    > /tmp/parity-php.json
	@echo "  [5/5] PHP done"

## Compare parity schemas against each other and the golden fixture
parity-compare: parity-generate
	@echo "=== Comparing parity schemas ==="
	@python3 tools/compare_parity_schemas.py \
	    /tmp/parity-python.json \
	    /tmp/parity-typescript.json \
	    /tmp/parity-go.json \
	    /tmp/parity-java.json \
	    /tmp/parity-php.json \
	    $(PARITY_GOLDEN)

## Run all parity checks (generate + compare)
test-parity: parity-compare
	@echo "=== All SDK parity tests passed ==="

# ============================================================================
# Docker Demo Platform (Newcomer Onboarding)
# ============================================================================

## Start demo stack (GraphQL IDE, tutorial, server, database)
demo-start:
	@echo "🚀 Starting FraiseQL demo stack..."
	@docker compose -f docker/docker-compose.demo.yml up -d
	@echo ""
	@echo "⏳ Waiting for services to be healthy..."
	@sleep 5
	@docker compose -f docker/docker-compose.demo.yml ps
	@echo ""
	@echo "✅ Demo stack is running!"
	@echo ""
	@echo "Open your browser:"
	@echo "  🖥️  GraphQL IDE:      http://localhost:3000"
	@echo "  📚 Tutorial:          http://localhost:3001"
	@echo "  📊 Admin Dashboard:   http://localhost:3002"
	@echo "  🔌 API Server:        http://localhost:8000"
	@echo ""
	@echo "📖 Quick start: See docs/docker-quickstart.md"
	@echo ""

## Stop demo stack
demo-stop:
	@echo "🛑 Stopping FraiseQL demo stack..."
	@docker compose -f docker/docker-compose.demo.yml down
	@echo "✅ Demo stack stopped"

## View demo logs
demo-logs:
	@docker compose -f docker/docker-compose.demo.yml logs -f

## Check demo health status
demo-status:
	@echo "📊 Demo Stack Status:"
	@docker compose -f docker/docker-compose.demo.yml ps
	@echo ""
	@echo "Service Health:"
	@echo -n "  FraiseQL Server: "
	@curl -s http://localhost:8000/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  GraphQL IDE: "
	@curl -s http://localhost:3000/ > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Tutorial: "
	@curl -s http://localhost:3001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  PostgreSQL: "
	@docker compose -f docker/docker-compose.demo.yml exec -T postgres-blog pg_isready -U fraiseql > /dev/null 2>&1 && echo "✅ Healthy" || echo "❌ Unhealthy"

## Restart demo stack
demo-restart: demo-stop demo-start
	@echo "✅ Demo stack restarted"

## Remove demo volumes and stop (fresh start)
demo-clean:
	@echo "🧹 Cleaning up demo stack (removing volumes)..."
	@docker compose -f docker/docker-compose.demo.yml down -v
	@echo "✅ Demo stack cleaned"
	@echo ""
	@echo "💡 Run 'make demo-start' to start fresh"

# ============================================================================
# Docker Multi-Example Stack (Blog + E-Commerce + Streaming)
# ============================================================================

## Start multi-example stack (all 3 domains simultaneously)
examples-start:
	@echo "🚀 Starting FraiseQL multi-example stack..."
	@echo "   Running: Blog, E-Commerce, and Streaming examples"
	@docker compose -f docker/docker-compose.examples.yml up -d
	@echo ""
	@echo "⏳ Waiting for services to be healthy..."
	@sleep 8
	@docker compose -f docker/docker-compose.examples.yml ps
	@echo ""
	@echo "✅ Multi-example stack is running!"
	@echo ""
	@echo "Open your browser:"
	@echo "  📝 Blog IDE:           http://localhost:3000"
	@echo "  🛒 E-Commerce IDE:     http://localhost:3100"
	@echo "  ⚡ Streaming IDE:       http://localhost:3200"
	@echo "  📚 Tutorial:           http://localhost:3001"
	@echo "  📊 Admin Dashboard:    http://localhost:3002"
	@echo ""
	@echo "📖 Quick reference:"
	@echo "  - Blog: Simple product management (5 users, 10 posts)"
	@echo "  - E-Commerce: Orders & inventory (5 categories, 12 products, 7 orders)"
	@echo "  - Streaming: Real-time events (subscriptions, metrics, activity)"
	@echo ""

## Stop multi-example stack
examples-stop:
	@echo "🛑 Stopping FraiseQL multi-example stack..."
	@docker compose -f docker/docker-compose.examples.yml down
	@echo "✅ Multi-example stack stopped"

## View multi-example logs
examples-logs:
	@docker compose -f docker/docker-compose.examples.yml logs -f

## Check multi-example health status
examples-status:
	@echo "📊 Multi-Example Stack Status:"
	@docker compose -f docker/docker-compose.examples.yml ps
	@echo ""
	@echo "Service Health:"
	@echo -n "  Blog Server: "
	@curl -s http://localhost:8000/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  E-Commerce Server: "
	@curl -s http://localhost:8001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Streaming Server: "
	@curl -s http://localhost:8002/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Tutorial: "
	@curl -s http://localhost:3001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Admin Dashboard: "
	@curl -s http://localhost:3002/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"

## Remove multi-example volumes and stop (fresh start)
examples-clean:
	@echo "🧹 Cleaning up multi-example stack (removing volumes)..."
	@docker compose -f docker/docker-compose.examples.yml down -v
	@echo "✅ Multi-example stack cleaned"
	@echo ""
	@echo "💡 Run 'make examples-start' to start fresh"

# ============================================================================
# Docker Production Stack (Pre-built Images from Docker Hub)
# ============================================================================

## Start production demo stack (pre-built images, no local build)
prod-start:
	@echo "🚀 Starting FraiseQL production demo stack (pre-built images)..."
	@docker compose -f docker/docker-compose.prod.yml up -d
	@echo ""
	@echo "⏳ Waiting for services to be healthy..."
	@sleep 5
	@docker compose -f docker/docker-compose.prod.yml ps
	@echo ""
	@echo "✅ Production demo stack is running!"
	@echo ""
	@echo "Open your browser:"
	@echo "  🖥️  GraphQL IDE:      http://localhost:3000"
	@echo "  📚 Tutorial:          http://localhost:3001"
	@echo "  📊 Admin Dashboard:   http://localhost:3002"
	@echo "  🔌 API Server:        http://localhost:8000"
	@echo ""

## Stop production demo stack
prod-stop:
	@echo "🛑 Stopping FraiseQL production demo stack..."
	@docker compose -f docker/docker-compose.prod.yml down
	@echo "✅ Production demo stack stopped"

## View production demo logs
prod-logs:
	@docker compose -f docker/docker-compose.prod.yml logs -f

## Check production demo health status
prod-status:
	@echo "📊 Production Demo Stack Status:"
	@docker compose -f docker/docker-compose.prod.yml ps
	@echo ""
	@echo "Service Health:"
	@echo -n "  FraiseQL Server: "
	@curl -s http://localhost:8000/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  GraphQL IDE: "
	@curl -s http://localhost:3000/ > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Tutorial: "
	@curl -s http://localhost:3001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  PostgreSQL: "
	@docker compose -f docker/docker-compose.prod.yml exec -T postgres-blog pg_isready -U fraiseql > /dev/null 2>&1 && echo "✅ Healthy" || echo "❌ Unhealthy"

## Clean production demo stack
prod-clean:
	@echo "🧹 Cleaning up production demo stack (removing volumes)..."
	@docker compose -f docker/docker-compose.prod.yml down -v
	@echo "✅ Production demo stack cleaned"
	@echo ""
	@echo "💡 Run 'make prod-start' to start fresh"

## Start production multi-example stack (all 3 examples with pre-built images)
prod-examples-start:
	@echo "🚀 Starting FraiseQL production multi-example stack..."
	@echo "   Running: Blog, E-Commerce, and Streaming examples (pre-built images)"
	@docker compose -f docker/docker-compose.prod-examples.yml up -d
	@echo ""
	@echo "⏳ Waiting for services to be healthy..."
	@sleep 8
	@docker compose -f docker/docker-compose.prod-examples.yml ps
	@echo ""
	@echo "✅ Production multi-example stack is running!"
	@echo ""
	@echo "Open your browser:"
	@echo "  📝 Blog IDE:           http://localhost:3000"
	@echo "  🛒 E-Commerce IDE:     http://localhost:3100"
	@echo "  ⚡ Streaming IDE:       http://localhost:3200"
	@echo "  📚 Tutorial:           http://localhost:3001"
	@echo "  📊 Admin Dashboard:    http://localhost:3002"
	@echo ""

## Stop production multi-example stack
prod-examples-stop:
	@echo "🛑 Stopping FraiseQL production multi-example stack..."
	@docker compose -f docker/docker-compose.prod-examples.yml down
	@echo "✅ Production multi-example stack stopped"

## View production multi-example logs
prod-examples-logs:
	@docker compose -f docker/docker-compose.prod-examples.yml logs -f

## Check production multi-example health status
prod-examples-status:
	@echo "📊 Production Multi-Example Stack Status:"
	@docker compose -f docker/docker-compose.prod-examples.yml ps
	@echo ""
	@echo "Service Health:"
	@echo -n "  Blog Server: "
	@curl -s http://localhost:8000/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  E-Commerce Server: "
	@curl -s http://localhost:8001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Streaming Server: "
	@curl -s http://localhost:8002/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Tutorial: "
	@curl -s http://localhost:3001/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"
	@echo -n "  Admin Dashboard: "
	@curl -s http://localhost:3002/health > /dev/null && echo "✅ Healthy" || echo "❌ Unhealthy"

## Clean production multi-example stack
prod-examples-clean:
	@echo "🧹 Cleaning up production multi-example stack (removing volumes)..."
	@docker compose -f docker/docker-compose.prod-examples.yml down -v
	@echo "✅ Production multi-example stack cleaned"
	@echo ""
	@echo "💡 Run 'make prod-examples-start' to start fresh"
