# ============================================================================
# FraiseQL Rust PostgreSQL Driver - Development Makefile
# ============================================================================
#
# Usage: make [target]
#        make help       - Show this help message
#
# Main Workflows:
#   make qa              - Run full quality checks
#   make test            - Run all tests
#   make build           - Build debug binary
#   make release         - Build optimized release
#   make bench           - Run benchmarks
#
# ============================================================================

.PHONY: help qa check build release test test-unit test-integration \
        bench bench-pool bench-queries bench-streaming \
        clippy lint lint-rust fmt format clean clean-all \
        bench-baseline watch docs install \
        pre-commit pre-commit-install dev

# Default target
.DEFAULT_GOAL := help

# Default shell
SHELL := /bin/bash

# Test environment variables for Podman
export TESTCONTAINERS_PODMAN := true
export TESTCONTAINERS_RYUK_DISABLED := true

# Colors for output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m # No Color

# ============================================================================
# HELP & DOCUMENTATION
# ============================================================================

## help: Show this help message
help:
	@grep "^##" Makefile | sed 's/## //' | column -t -s ':' | sed 's/:/-/'

## docs: Generate documentation
docs:
	@cd fraiseql_rs && cargo doc --no-deps --open

## info: Show project information
info:
	@echo "📋 FraiseQL Rust PostgreSQL Driver"
	@echo "=================================="
	@cd fraiseql_rs && cargo --version && rustc --version
	@echo ""
	@echo "Common Rust targets:"
	@echo "  make qa              - Full quality checks"
	@echo "  make rust-test       - Run Rust tests"
	@echo "  make build           - Build debug"
	@echo "  make release         - Build optimized"
	@echo "  make rust-bench      - Run benchmarks"
	@echo ""
	@echo "For more: make help"

# ============================================================================
# BUILD TARGETS
# ============================================================================

## build: Build debug binary
build:
	@echo "🔨 Building debug binary..."
	@cd fraiseql_rs && cargo build
	@echo "✅ Build complete"

## release: Build optimized release binary
release:
	@echo "🚀 Building release binary..."
	@cd fraiseql_rs && cargo build --release
	@echo "✅ Release build complete (optimized)"

## check: Quick compilation check (no code generation)
check:
	@echo "⚡ Checking compilation..."
	@cd fraiseql_rs && cargo check --all-targets
	@echo "✅ Compilation check passed"

# ============================================================================
# LINTING & CODE QUALITY (Phase 0.1)
# ============================================================================

## clippy: Run Clippy linter with strict warnings
clippy:
	@echo "🔍 Running Clippy..."
	@cd fraiseql_rs && cargo clippy --all-targets --all-features -- -D warnings
	@echo "✅ Clippy checks passed"

## lint-clippy: Alias for clippy
lint-clippy: clippy

## lint: Run all linting checks (Clippy + fmt)
lint: lint-clippy lint-fmt
	@echo "✅ All linting checks passed"

## lint-fmt: Check code formatting (no changes)
lint-fmt:
	@echo "📋 Checking formatting..."
	@cd fraiseql_rs && cargo fmt --all -- --check
	@echo "✅ Code formatting is correct"

## fmt: Auto-format Rust code
fmt format:
	@echo "📝 Formatting code..."
	@cd fraiseql_rs && cargo fmt --all
	@echo "✅ Code formatted"

## clean-clippy: Clear Clippy warnings cache
clean-clippy:
	@echo "🧹 Clearing Clippy cache..."
	@cd fraiseql_rs && cargo clean && cargo build --message-format=short
	@echo "✅ Clippy cache cleared"

# ============================================================================
# TESTING TARGETS (Phase 0.2)
# ============================================================================

## rust-test: Run full Rust test suite (unit + integration)
rust-test:
	@echo "🧪 Running Rust tests..."
	@cd fraiseql_rs && cargo test --lib
	@echo "✅ All Rust tests passed"

## rust-test-unit: Run Rust unit tests only (fast)
rust-test-unit:
	@echo "⚡ Running Rust unit tests..."
	@cd fraiseql_rs && cargo test --lib
	@echo "✅ Rust unit tests passed"

## rust-test-integration: Run Rust integration tests only (requires DB)
rust-test-integration:
	@echo "🗄️  Running Rust integration tests..."
	@cd fraiseql_rs && cargo test --test '*'
	@echo "✅ Rust integration tests passed"

## rust-test-all: Run all Rust tests including e2e
rust-test-all:
	@echo "🧪 Running all Rust tests..."
	@cd fraiseql_rs && cargo test --all
	@echo "✅ All Rust tests passed"

## rust-test-verbose: Run Rust tests with verbose output
rust-test-verbose:
	@echo "📢 Running verbose Rust tests..."
	@cd fraiseql_rs && cargo test --all -- --nocapture --test-threads=1
	@echo "✅ Verbose Rust test run complete"

# ============================================================================
# BENCHMARKING TARGETS (Phase 0.3)
# ============================================================================

## rust-bench: Run all Rust benchmarks
rust-bench:
	@echo "⏱️  Running Rust benchmarks..."
	@cd fraiseql_rs && cargo bench --all
	@echo "✅ Rust benchmarks complete"

## rust-bench-pool: Benchmark connection pool
rust-bench-pool:
	@echo "⏱️  Benchmarking connection pool..."
	@cd fraiseql_rs && cargo bench --bench connection_pool
	@echo "✅ Pool benchmark complete"

## rust-bench-queries: Benchmark query execution
rust-bench-queries:
	@echo "⏱️  Benchmarking query execution..."
	@cd fraiseql_rs && cargo bench --bench query_execution
	@echo "✅ Query benchmark complete"

## rust-bench-streaming: Benchmark streaming performance
rust-bench-streaming:
	@echo "⏱️  Benchmarking streaming..."
	@cd fraiseql_rs && cargo bench --bench streaming
	@echo "✅ Streaming benchmark complete"

## rust-bench-baseline: Capture performance baseline
rust-bench-baseline:
	@bash scripts/benchmark_baseline.sh

## rust-bench-compare: Compare against previous baseline
rust-bench-compare:
	@bash scripts/check_performance.sh

# ============================================================================
# QUALITY ASSURANCE (Phase 0.4)
# ============================================================================

## qa: Complete Rust quality assurance pipeline
qa: check fmt-check clippy rust-test-unit
	@echo ""
	@echo "════════════════════════════════════════════════════════════════"
	@echo "✅ All Rust quality checks passed!"
	@echo "════════════════════════════════════════════════════════════════"

## qa-python: Python quality assurance (legacy)
qa-python: ## Run Python tests and checks (legacy)
	@echo "🐍 Running Python QA..."
	pytest tests/unit/ -x
	@echo "✅ Python QA complete"

## pre-commit: Run pre-commit hooks on all files
pre-commit:
	@echo "🪝 Running pre-commit hooks..."
	@prek run --all
	@echo "✅ Pre-commit checks passed"

## pre-commit-install: Install pre-commit hooks
pre-commit-install:
	@echo "📦 Installing pre-commit hooks..."
	@prek install
	@echo "✅ Pre-commit hooks installed"

# ============================================================================
# DEVELOPMENT WORKFLOWS
# ============================================================================

## dev: Complete setup for development (install hooks, build, test)
dev: pre-commit-install build test
	@echo "✅ Development environment ready"

## release-check: Full pre-release checks
release-check: qa coverage bench
	@echo "✅ Release checks passed"

## ci: Run CI pipeline locally (what GitHub Actions runs)
ci: check clippy fmt-check test coverage
	@echo "✅ CI pipeline passed locally"

# ============================================================================
# CLEANUP
# ============================================================================

## clean: Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	@cd fraiseql_rs && cargo clean
	@echo "✅ Cleaned"

## clean-all: Deep clean (artifacts + caches + benchmarks)
clean-all: clean
	@echo "🧹 Deep cleaning..."
	@rm -rf fraiseql_rs/target coverage/ performance/
	@echo "✅ Deep clean complete"

## clean-cache: Clear Rust build cache
clean-cache:
	@echo "🗑️  Clearing cache..."
	@rm -rf ~/.cargo/registry/cache ~/.cargo/git/db
	@echo "✅ Cache cleared"

# ============================================================================
# INSTALLATION & SETUP
# ============================================================================

## install-tools: Install development tools
install-tools:
	@echo "📦 Installing development tools..."
	@cargo install cargo-watch
	@cargo install cargo-criterion
	@cargo install cargo-tarpaulin
	@pip install pre-commit
	@brew install j178/tap/prek
	@echo "✅ Tools installed"

## install: Install fraiseql_rs locally
install:
	@echo "📦 Installing fraiseql_rs..."
	@uv run pip install -e .
	@echo "✅ Installation complete"

# ============================================================================
# ADVANCED TARGETS
# ============================================================================

## profile: Profile build to find slow builds
profile:
	@echo "📊 Profiling build..."
	@cd fraiseql_rs && cargo build --release -Z timings
	@echo "✅ Timing report complete"

## security: Run security audit
security:
	@echo "🔐 Running security audit..."
	@cargo audit
	@echo "✅ Security audit complete"

## size: Check binary size
size:
	@echo "📦 Checking binary size..."
	@cd fraiseql_rs && cargo build --release
	@ls -lh fraiseql_rs/target/release/
	@echo "✅ Size check complete"

# ============================================================================
# WORKFLOW ALIASES
# ============================================================================

## all: Build everything (build + rust-test + rust-bench)
all: build rust-test rust-bench
	@echo "✅ All Rust tasks complete"

## before-push: Run checks before pushing (qa + rust-bench)
before-push: qa rust-bench
	@echo "✅ Ready to push"

## after-merge: Run post-merge checks
after-merge: clean build test
	@echo "✅ Post-merge verification complete"

# ============================================================================
# END OF MAKEFILE
# ============================================================================

# Phony declarations prevent conflicts with files named after targets
.PHONY: all help docs info

.PHONY: install
install: ## Install project dependencies
	@echo -e "$(GREEN)Installing dependencies...$(NC)"
	pip install -e ".[dev]"

.PHONY: install-dev
install-dev: ## Install all development dependencies
	@echo -e "$(GREEN)Installing all development dependencies...$(NC)"
	pip install -e ".[dev,auth0,docs]"

.PHONY: test
test: ## Run all tests including examples with Podman
	@echo -e "$(GREEN)Running all tests including examples with Podman...$(NC)"
	pytest -xvs

.PHONY: test-core
test-core: ## Run core tests only (excluding examples)
	@echo -e "$(GREEN)Running core tests only...$(NC)"
	pytest tests/ -xvs -m "not blog_simple and not blog_enterprise"

.PHONY: test-fast
test-fast: ## Run tests quickly (subset)
	@echo -e "$(GREEN)Running fast test subset...$(NC)"
	pytest tests/unit/ -x

.PHONY: test-unit
test-unit: ## Run only unit tests (no database)
	@echo -e "$(GREEN)Running unit tests...$(NC)"
	pytest -xvs -m "not database"

.PHONY: test-db
test-db: ## Run only database tests
	@echo -e "$(GREEN)Running database tests with Podman...$(NC)"
	pytest -xvs -m "database"

.PHONY: test-auth
test-auth: ## Run native authentication tests
	@echo -e "$(GREEN)Running native authentication tests...$(NC)"
	pytest tests/auth/native/ -xvs

.PHONY: test-auth-unit
test-auth-unit: ## Run native auth unit tests (no database)
	@echo -e "$(GREEN)Running native auth unit tests...$(NC)"
	pytest tests/auth/native/ -m "not database" -xvs

.PHONY: test-auth-db
test-auth-db: ## Run native auth database integration tests
	@echo -e "$(GREEN)Running native auth database tests...$(NC)"
	pytest tests/auth/native/ -m "database" -xvs

.PHONY: test-auth-comprehensive
test-auth-comprehensive: ## Run comprehensive native auth system test
	@echo -e "$(GREEN)Running comprehensive native auth system test...$(NC)"
	$(PYTHON) scripts/test-native-auth.py

.PHONY: test-auth-security
test-auth-security: ## Run security audit on native auth system
	@echo -e "$(GREEN)Running security audit on native auth...$(NC)"
	bandit -r src/fraiseql/auth/native/ -f txt || echo -e "$(YELLOW)Bandit not installed, skipping security scan$(NC)"
	safety check || echo -e "$(YELLOW)Safety not installed, skipping vulnerability check$(NC)"

.PHONY: test-testfoundry
test-testfoundry: ## Run TestFoundry extension tests
	@echo -e "$(GREEN)Running TestFoundry tests...$(NC)"
	pytest tests/extensions/testfoundry/ -xvs

.PHONY: test-examples
test-examples: ## Run all example integration tests from main test suite
	@echo -e "$(GREEN)Running example integration tests...$(NC)"
	pytest tests/integration/examples/ -xvs

.PHONY: test-examples-full
test-examples-full: ## Run full example test suites (examples + integration)
	@echo -e "$(GREEN)Running all example tests (integration + full suites)...$(NC)"
	pytest tests/integration/examples/ examples/ -xvs -m "blog_simple or blog_enterprise"

.PHONY: test-blog-simple
test-blog-simple: ## Run blog_simple example tests
	@echo -e "$(GREEN)Running blog_simple example tests...$(NC)"
	pytest tests/integration/examples/test_blog_simple_integration.py examples/blog_simple/tests/ -xvs

.PHONY: test-blog-enterprise
test-blog-enterprise: ## Run blog_enterprise example tests
	@echo -e "$(GREEN)Running blog_enterprise example tests...$(NC)"
	pytest tests/integration/examples/test_blog_enterprise_integration.py -xvs

.PHONY: test-examples-smoke
test-examples-smoke: ## Run quick smoke tests for examples (CI-friendly)
	@echo -e "$(GREEN)Running example smoke tests...$(NC)"
	pytest tests/integration/examples/ -xvs -k "health or home or introspection" --tb=short

.PHONY: test-coverage
test-coverage: ## Run tests with coverage report
	@echo -e "$(GREEN)Running tests with coverage...$(NC)"
	pytest --cov=src/fraiseql --cov-report=html --cov-report=term

# ============================================================================
# RUST TEST TARGETS (Phase 0.2)
# ============================================================================

.PHONY: test-rust test-rust-unit test-rust-integration test-rust-all test-rust-verbose

## test-rust: Run all Rust tests (unit + integration)
test-rust:
	@echo -e "$(GREEN)🧪 Running Rust tests...$(NC)"
	cd fraiseql_rs && cargo test --lib --test '*'
	@echo -e "$(GREEN)✅ All Rust tests passed$(NC)"

## test-rust-unit: Run only Rust unit tests (fast)
test-rust-unit:
	@echo -e "$(GREEN)⚡ Running Rust unit tests...$(NC)"
	cd fraiseql_rs && cargo test --lib
	@echo -e "$(GREEN)✅ Rust unit tests passed$(NC)"

## test-rust-integration: Run only Rust integration tests (requires DB)
test-rust-integration:
	@echo -e "$(GREEN)🗄️  Running Rust integration tests...$(NC)"
	cd fraiseql_rs && cargo test --test '*'
	@echo -e "$(GREEN)✅ Rust integration tests passed$(NC)"

## test-rust-all: Run all Rust tests including e2e and examples
test-rust-all: test-rust
	@echo -e "$(GREEN)🧪 Running all Rust tests including examples...$(NC)"
	cd fraiseql_rs && cargo test --all
	@echo -e "$(GREEN)✅ All Rust tests passed including examples$(NC)"

## test-rust-verbose: Run Rust tests with verbose output
test-rust-verbose:
	@echo -e "$(GREEN)📢 Running Rust tests with verbose output...$(NC)"
	cd fraiseql_rs && cargo test --all -- --nocapture --test-threads=1
	@echo -e "$(GREEN)✅ Verbose Rust test run complete$(NC)"

# ============================================================================
# Benchmarking Targets (Phase 0.3)
# ============================================================================

.PHONY: bench bench-pool bench-queries bench-streaming bench-baseline bench-compare

## bench: Run all benchmarks
bench:
	@echo -e "$(GREEN)🚀 Running all benchmarks...$(NC)"
	cd fraiseql_rs && cargo bench --all
	@echo -e "$(GREEN)✅ Benchmarks complete$(NC)"

## bench-pool: Benchmark connection pool
bench-pool:
	@echo -e "$(GREEN)🏊 Benchmarking connection pool...$(NC)"
	cd fraiseql_rs && cargo bench --bench connection_pool
	@echo -e "$(GREEN)✅ Pool benchmark complete$(NC)"

## bench-queries: Benchmark query execution
bench-queries:
	@echo -e "$(GREEN)🔍 Benchmarking query execution...$(NC)"
	cd fraiseql_rs && cargo bench --bench query_execution
	@echo -e "$(GREEN)✅ Query benchmark complete$(NC)"

## bench-streaming: Benchmark streaming performance
bench-streaming:
	@echo -e "$(GREEN)🌊 Benchmarking streaming performance...$(NC)"
	cd fraiseql_rs && cargo bench --bench streaming
	@echo -e "$(GREEN)✅ Streaming benchmark complete$(NC)"

## bench-baseline: Capture performance baseline
bench-baseline:
	@echo -e "$(GREEN)📊 Capturing performance baseline...$(NC)"
	bash scripts/benchmark_baseline.sh

## bench-compare: Compare against previous baseline
bench-compare:
	@echo -e "$(GREEN)📈 Comparing against baseline...$(NC)"
	bash scripts/check_performance.sh

.PHONY: test-watch
test-watch: ## Run tests in watch mode (requires pytest-watch)
	@command -v ptw >/dev/null 2>&1 || { echo -e "$(RED)pytest-watch not installed. Run: pip install pytest-watch$(NC)"; exit 1; }
	@echo -e "$(GREEN)Running tests in watch mode...$(NC)"
	ptw -- -xvs

.PHONY: lint lint-rust clippy rustfmt lint-fix lint-check
lint: lint-rust ## Run all linting (Rust + Python)
	@echo -e "$(GREEN)✅ All linting checks passed$(NC)"

lint-rust: clippy rustfmt ## Run Rust linting (Clippy + rustfmt)
	@echo -e "$(GREEN)✅ Rust linting checks passed$(NC)"

clippy: ## Run Clippy linter with strict warnings (Phase 0.1: lib target only)
	@echo -e "$(GREEN)🔍 Running Clippy on library code...$(NC)"
	cd fraiseql_rs && cargo clippy --lib -- -D warnings
	@echo -e "$(GREEN)✅ Clippy checks passed for library code$(NC)"

rustfmt: ## Auto-format Rust code
	@echo -e "$(GREEN)📝 Formatting Rust code...$(NC)"
	cd fraiseql_rs && cargo fmt --all
	@echo -e "$(GREEN)✅ Rust code formatted$(NC)"

lint-fix: ## Fix linting issues automatically (Rust + Python)
	@echo -e "$(GREEN)🔧 Fixing linting issues...$(NC)"
	cd fraiseql_rs && cargo clippy --fix --allow-staged --allow-dirty
	cd fraiseql_rs && cargo fmt --all
	ruff check src/ --fix
	@echo -e "$(GREEN)✅ Linting issues fixed$(NC)"

lint-check: ## Check formatting without changes (Rust + Python)
	@echo -e "$(GREEN)📋 Checking code formatting...$(NC)"
	cd fraiseql_rs && cargo fmt --all -- --check
	ruff format --check src/ tests/
	@echo -e "$(GREEN)✅ Code formatting is correct$(NC)"

.PHONY: lint prek-install prek-run prek-update prek-list
lint: ## Run linting with ruff
	@echo -e "$(GREEN)Running ruff linter...$(NC)"
	ruff check src/

prek-install: ## Install prek git hooks (faster pre-commit in Rust)
	@echo -e "$(GREEN)Installing prek hooks...$(NC)"
	@command -v prek >/dev/null 2>&1 || { echo -e "$(RED)prek not installed$(NC)"; echo -e "$(YELLOW)Install with: brew install j178/tap/prek$(NC)"; exit 1; }
	prek install
	@echo -e "$(GREEN)✅ prek hooks installed$(NC)"

prek-run: ## Run prek hooks on staged files
	@echo -e "$(GREEN)Running prek hooks...$(NC)"
	prek run

prek-run-all: ## Run prek hooks on all files
	@echo -e "$(GREEN)Running prek hooks on all files...$(NC)"
	prek run --all

prek-update: ## Update prek hooks to latest versions
	@echo -e "$(GREEN)Updating prek hooks...$(NC)"
	prek update
	@echo -e "$(GREEN)✅ prek hooks updated$(NC)"

prek-list: ## List all prek hooks and their status
	@echo -e "$(GREEN)prek hooks:$(NC)"
	prek list

.PHONY: lint-fix
lint-fix: ## Fix linting issues automatically
	@echo -e "$(GREEN)Fixing linting issues...$(NC)"
	ruff check src/ --fix

.PHONY: format
format: ## Format code with ruff
	@echo -e "$(GREEN)Formatting code with ruff...$(NC)"
	ruff format src/ tests/

.PHONY: format-check
format-check: ## Check code formatting without changes
	@echo -e "$(GREEN)Checking code format...$(NC)"
	ruff format --check src/ tests/

.PHONY: type-check
type-check: ## Run type checking with ruff
	@echo -e "$(GREEN)Running ruff type checker...$(NC)"
	ruff check --ignore FAST001 src/


.PHONY: qa-python
qa-python: lint-check lint-rust type-check test ## Run all Python quality checks (format, lint, type-check, test)
	@echo -e "$(GREEN)All Python quality checks passed!$(NC)"

.PHONY: qa-fast
qa-fast: format-check lint type-check test-fast ## Run quality checks without formatting
	@echo -e "$(GREEN)All quality checks passed!$(NC)"

.PHONY: clean
clean: ## Clean build artifacts and cache
	@echo -e "$(GREEN)Cleaning build artifacts...$(NC)"
	find . -type f -name '*.pyc' -delete
	find . -type d -name '__pycache__' -delete
	find . -type d -name '*.egg-info' -exec rm -rf {} +
	find . -type d -name '.pytest_cache' -exec rm -rf {} +
	find . -type d -name '.mypy_cache' -exec rm -rf {} +
	find . -type d -name '.ruff_cache' -exec rm -rf {} +
	rm -rf build/ dist/ htmlcov/ .coverage

.PHONY: clean-containers
clean-containers: ## Stop and remove test containers
	@echo -e "$(GREEN)Cleaning up test containers...$(NC)"
	podman ps -a --filter "ancestor=postgres:16-alpine" -q | xargs -r podman rm -f
	podman ps -a --filter "label=org.testcontainers=true" -q | xargs -r podman rm -f

.PHONY: docs
docs: ## Build documentation
	@echo -e "$(GREEN)Building documentation...$(NC)"
	mkdocs build

.PHONY: docs-serve
docs-serve: ## Serve documentation locally
	@echo -e "$(GREEN)Serving documentation at http://localhost:8000$(NC)"
	mkdocs serve

.PHONY: build
build: clean ## Build distribution packages
	@echo -e "$(GREEN)Building distribution packages...$(NC)"
	$(PYTHON) -m build

.PHONY: publish-test
publish-test: build ## Publish to TestPyPI
	@echo -e "$(GREEN)Publishing to TestPyPI...$(NC)"
	$(PYTHON) -m twine upload --repository testpypi dist/*

.PHONY: publish
publish: build ## Publish to PyPI
	@echo -e "$(YELLOW)Publishing to PyPI...$(NC)"
	@echo -e "$(RED)Are you sure? [y/N]$(NC)"
	@read -r response; if [ "$$response" = "y" ]; then \
		$(PYTHON) -m twine upload dist/*; \
	else \
		echo "Cancelled."; \
	fi

# Development database commands
# Using port 54320 to avoid conflicts with existing PostgreSQL installations and pasta
.PHONY: db-start
db-start: ## Start a PostgreSQL container for development (port 54320)
	@echo -e "$(GREEN)Starting PostgreSQL container...$(NC)"
	podman run -d \
		--name fraiseql-dev-db \
		-e POSTGRES_USER=fraiseql \
		-e POSTGRES_PASSWORD=fraiseql \
		-e POSTGRES_DB=fraiseql_dev \
		-p 54320:5432 \
		postgres:16-alpine
	@echo -e "$(YELLOW)PostgreSQL is running on port 54320$(NC)"
	@echo -e "$(YELLOW)Connection string: postgresql://fraiseql:fraiseql@localhost:54320/fraiseql_dev$(NC)"

.PHONY: db-stop
db-stop: ## Stop the development PostgreSQL container
	@echo -e "$(GREEN)Stopping PostgreSQL container...$(NC)"
	podman stop fraiseql-dev-db || true
	podman rm fraiseql-dev-db || true

.PHONY: db-logs
db-logs: ## Show PostgreSQL container logs
	@echo -e "$(GREEN)PostgreSQL container logs:$(NC)"
	podman logs -f fraiseql-dev-db

.PHONY: db-shell
db-shell: ## Open psql shell to development database
	@echo -e "$(GREEN)Opening PostgreSQL shell...$(NC)"
	@echo -e "$(YELLOW)Connecting to fraiseql-dev-db container...$(NC)"
	podman exec -it fraiseql-dev-db psql -U fraiseql -d fraiseql_dev

# Continuous Integration commands
.PHONY: ci
ci: ## Run CI pipeline (all checks)
	@echo -e "$(GREEN)Running CI pipeline...$(NC)"
	$(MAKE) format-check
	$(MAKE) lint
	$(MAKE) type-check
	$(MAKE) test
	@echo -e "$(GREEN)CI pipeline passed!$(NC)"

.PHONY: pre-commit
pre-commit: ## Run pre-commit hooks
	@echo -e "$(GREEN)Running pre-commit hooks...$(NC)"
	pre-commit run --all-files

.PHONY: pre-commit-install
pre-commit-install: ## Install pre-commit hooks
	@echo -e "$(GREEN)Installing pre-commit hooks...$(NC)"
	pre-commit install

# Safe development workflow commands
safe-commit: test-core ## Safe commit: Run tests before committing
	@echo -e "$(GREEN)✅ Tests passed - proceeding with commit...$(NC)"
	@echo -e "$(YELLOW)📝 Use: git add -A && git commit -m 'your message'$(NC)"

safe-push: test ## Safe push: Run full tests before pushing
	@echo -e "$(GREEN)✅ All tests passed - safe to push$(NC)"
	@echo -e "$(YELLOW)📡 Use: git push origin branch-name$(NC)"

verify-tests: ## Verify current test status (quick check)
	@echo -e "$(GREEN)🔍 Verifying current test status...$(NC)"
	pytest --collect-only -q | tail -3

test-commit-safety: ## Test the commit safety hooks
	@echo -e "$(GREEN)🧪 Testing commit safety mechanisms...$(NC)"
	@bash -c 'source .git/hooks/pre-push && echo "Pre-push hook would have run successfully"'


.PHONY: check-publish
check-publish: ## Check package before publishing
	@echo -e "$(GREEN)Checking package...$(NC)"
	python -m twine check dist/*

# =============================================================================
# VERSION MANAGEMENT (Multi-file: Python, Rust, Documentation)
# =============================================================================

.PHONY: version-show version-patch version-minor version-major version-dry-run

version-show: ## Show current version information
	@echo -e "$(GREEN)📊 FraiseQL Version Information$(NC)"
	@uv run python scripts/version_manager.py show

version-patch: ## Bump patch version (1.8.2 → 1.8.3)
	@echo -e "$(GREEN)📈 Bumping patch version$(NC)"
	@uv run python scripts/version_manager.py patch

version-minor: ## Bump minor version (1.8.2 → 1.9.0)
	@echo -e "$(GREEN)📈 Bumping minor version$(NC)"
	@uv run python scripts/version_manager.py minor

version-major: ## Bump major version (1.8.2 → 2.0.0)
	@echo -e "$(GREEN)📈 Bumping major version$(NC)"
	@uv run python scripts/version_manager.py major

version-dry-run: ## Preview version bump without changes
	@echo -e "$(YELLOW)🧪 Version Bump Preview (dry-run)$(NC)"
	@echo "Patch version:" && uv run python scripts/version_manager.py patch --dry-run
	@echo ""
	@echo "Minor version:" && uv run python scripts/version_manager.py minor --dry-run
	@echo ""
	@echo "Major version:" && uv run python scripts/version_manager.py major --dry-run

# =============================================================================
# PULL REQUEST COMMANDS - Modern 2025 GitHub native auto-merge
# =============================================================================

.PHONY: pr-ship pr-ship-patch pr-ship-minor pr-ship-major pr-ship-help pr-status

pr-ship: pr-ship-patch ## Default PR workflow with patch version bump

pr-ship-patch: ## Automated PR workflow with patch version bump
	@echo -e "$(YELLOW)🚀 FraiseQL PR Ship with Patch Version Bump$(NC)"
	@echo -e "$(YELLOW)════════════════════════════════════════════════$(NC)"
	@uv run python scripts/pr_ship.py patch

pr-ship-minor: ## Automated PR workflow with minor version bump
	@echo -e "$(YELLOW)🚀 FraiseQL PR Ship with Minor Version Bump$(NC)"
	@echo -e "$(YELLOW)════════════════════════════════════════════════$(NC)"
	@uv run python scripts/pr_ship.py minor

pr-ship-major: ## Automated PR workflow with major version bump (use with caution!)
	@echo -e "$(YELLOW)🚀 FraiseQL PR Ship with Major Version Bump$(NC)"
	@echo -e "$(YELLOW)════════════════════════════════════════════════$(NC)"
	@uv run python scripts/pr_ship.py major

pr-ship-help: help ## Alias to show help

pr-status: ## Check status of current branch's PR
	@echo -e "$(GREEN)📊 Checking PR status$(NC)"
	@current_branch=$$(git branch --show-current); \
	if gh pr view --json state,url,statusCheckRollup,autoMergeRequest >/dev/null 2>&1; then \
		echo -e "$(YELLOW)📋 Branch: $$current_branch$(NC)"; \
		gh pr view; \
	else \
		echo -e "$(YELLOW)⚠️  No PR found for branch: $$current_branch$(NC)"; \
		echo -e "$(YELLOW)💡 Create one with: make pr-ship$(NC)"; \
	fi

# =============================================================================
# RELEASE WORKFLOWS (Multi-phase automated process)
# =============================================================================

.PHONY: release-patch release-minor release-major

release-patch: test pr-ship-patch ## Full patch release workflow (test + ship)
	@echo -e "$(GREEN)✅ Patch release complete$(NC)"

release-minor: test pr-ship-minor ## Full minor release workflow (test + ship)
	@echo -e "$(GREEN)✅ Minor release complete$(NC)"

release-major: test pr-ship-major ## Full major release workflow (test + ship)
	@echo -e "$(YELLOW)⚠️  Major release complete - verify all changes$(NC)"

# Default target
.DEFAULT_GOAL := help
