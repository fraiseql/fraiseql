# Phase 0.5: Build System & Makefile Consolidation

**Phase**: 0.5 of 0.5 (Final part of Phase 0 - Setup)
**Effort**: 1 hour
**Status**: Ready to implement
**Prerequisite**: Phase 0.1-0.4

---

## Objective

Consolidate all build and development commands into unified Makefile:

1. Combine all Makefile targets from Phases 0.1-0.4
2. Add development convenience targets
3. Create complete build/test/release pipeline
4. Document all targets

**Success Criteria**:

- ✅ `make help` shows all targets
- ✅ `make qa` runs full quality pipeline
- ✅ `make release` builds optimized binary
- ✅ All development workflows covered

---

## Implementation: Complete Makefile

**File**: `Makefile` (Consolidated version)

```makefile
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
        clippy lint fmt format clean clean-all \
        bench-baseline watch docs install \
        pre-commit pre-commit-install dev

# Default target
.DEFAULT_GOAL := help

# ============================================================================
# HELP & DOCUMENTATION
# ============================================================================

## help: Show this help message
help:
 @grep "^##" Makefile | sed 's/## //' | column -t -s ':' | sed 's/:/-/'

## docs: Generate documentation
docs:
 @cd fraiseql_rs && cargo doc --no-deps --open

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

## lint: Alias for clippy
lint: clippy

## fmt: Auto-format Rust code
fmt format:
 @echo "📝 Formatting code..."
 @cd fraiseql_rs && cargo fmt --all
 @echo "✅ Code formatted"

## fmt-check: Check formatting without changes
fmt-check:
 @echo "📋 Checking formatting..."
 @cd fraiseql_rs && cargo fmt --all -- --check
 @echo "✅ Formatting is correct"

# ============================================================================
# TESTING TARGETS (Phase 0.2)
# ============================================================================

## test: Run full test suite (unit + integration)
test:
 @echo "🧪 Running tests..."
 @cd fraiseql_rs && cargo test --lib --test '*'
 @echo "✅ All tests passed"

## test-unit: Run unit tests only (fast)
test-unit:
 @echo "⚡ Running unit tests..."
 @cd fraiseql_rs && cargo test --lib
 @echo "✅ Unit tests passed"

## test-integration: Run integration tests only (requires DB)
test-integration:
 @echo "🗄️  Running integration tests..."
 @cd fraiseql_rs && cargo test --test '*'
 @echo "✅ Integration tests passed"

## test-all: Run all tests including e2e
test-all:
 @echo "🧪 Running all tests..."
 @cd fraiseql_rs && cargo test --all
 @echo "✅ All tests passed"

## test-verbose: Run tests with verbose output
test-verbose:
 @echo "📢 Running verbose tests..."
 @cd fraiseql_rs && cargo test --all -- --nocapture --test-threads=1
 @echo "✅ Verbose test run complete"

## coverage: Generate code coverage report
coverage:
 @echo "📊 Generating coverage report..."
 @cd fraiseql_rs && cargo tarpaulin --out Html --output-dir coverage/
 @echo "✅ Coverage report generated in coverage/index.html"

## watch: Watch files and run tests on changes (requires cargo-watch)
watch:
 @echo "👀 Watching for changes..."
 @cargo watch -x "test --lib" -x clippy
 @echo "✅ Watch mode stopped"

# ============================================================================
# BENCHMARKING TARGETS (Phase 0.3)
# ============================================================================

## bench: Run all benchmarks
bench:
 @echo "⏱️  Running benchmarks..."
 @cd fraiseql_rs && cargo bench --all
 @echo "✅ Benchmarks complete"

## bench-pool: Benchmark connection pool
bench-pool:
 @echo "⏱️  Benchmarking connection pool..."
 @cd fraiseql_rs && cargo bench --bench connection_pool
 @echo "✅ Pool benchmark complete"

## bench-queries: Benchmark query execution
bench-queries:
 @echo "⏱️  Benchmarking query execution..."
 @cd fraiseql_rs && cargo bench --bench query_execution
 @echo "✅ Query benchmark complete"

## bench-streaming: Benchmark streaming performance
bench-streaming:
 @echo "⏱️  Benchmarking streaming..."
 @cd fraiseql_rs && cargo bench --bench streaming
 @echo "✅ Streaming benchmark complete"

## bench-baseline: Capture performance baseline
bench-baseline:
 @bash scripts/benchmark_baseline.sh

## bench-compare: Compare against previous baseline
bench-compare:
 @bash scripts/check_performance.sh

# ============================================================================
# QUALITY ASSURANCE (Phase 0.4)
# ============================================================================

## qa: Complete quality assurance pipeline
qa: check fmt-check clippy test
 @echo ""
 @echo "════════════════════════════════════════════════════════════════"
 @echo "✅ All quality checks passed!"
 @echo "════════════════════════════════════════════════════════════════"

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

## info: Show project information
info:
 @echo "📋 FraiseQL Rust PostgreSQL Driver"
 @echo "=================================="
 @cd fraiseql_rs && cargo --version && rustc --version
 @echo ""
 @echo "Common targets:"
 @echo "  make qa           - Full quality checks"
 @echo "  make test         - Run tests"
 @echo "  make build        - Build debug"
 @echo "  make release      - Build optimized"
 @echo "  make bench        - Run benchmarks"
 @echo ""
 @echo "For more: make help"

# ============================================================================
# WORKFLOW ALIASES
# ============================================================================

## all: Build everything (build + test + bench)
all: build test bench
 @echo "✅ All tasks complete"

## before-push: Run checks before pushing (qa + bench)
before-push: qa bench
 @echo "✅ Ready to push"

## after-merge: Run post-merge checks
after-merge: clean build test
 @echo "✅ Post-merge verification complete"

# ============================================================================
# END OF MAKEFILE
# ============================================================================

# Phony declarations prevent conflicts with files named after targets
.PHONY: all help docs info
```

---

## Usage Guide

### For Daily Development

```bash
# After making changes
make qa                 # Check everything

# Before committing
make pre-commit        # Run pre-commit hooks

# Before pushing
make before-push       # QA + benchmarks
```

### For Testing

```bash
make test              # Quick test
make test-verbose      # Debug failures
make coverage          # See coverage
make watch             # Auto-run tests
```

### For Performance

```bash
make bench             # All benchmarks
make bench-baseline    # Capture baseline
make bench-compare     # Check for regressions
```

### For Release

```bash
make qa                # All checks pass
make release-check     # Full release validation
make release           # Build optimized binary
```

---

## Verification

```bash
# Show all targets
make help

# Show project info
make info

# Test a target
make check             # Should succeed
```

---

## Success Criteria

- ✅ `make help` displays all targets
- ✅ `make qa` runs and passes
- ✅ `make test` runs full test suite
- ✅ `make bench` runs benchmarks
- ✅ All Phase 0 sub-documents referenced

---

## Next: Phase 1 Foundation

Phase 0 setup complete! Ready to start:

```bash
# Complete Phase 0.1-0.5
make qa
make pre-commit-install
make benchmark-baseline

# Now ready for Phase 1
cd fraiseql_rs
cargo build
# See phase-1-foundation.md for next steps
```

---

**Last Updated**: 2025-12-18
