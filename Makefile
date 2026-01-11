.PHONY: help build test test-unit test-integration clippy fmt check clean install dev doc bench db-up db-down db-logs db-reset db-status

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run all tests"
	@echo "  make test-unit          - Run unit tests only (fast, no database)"
	@echo "  make test-integration   - Run integration tests (requires Docker)"
	@echo "  make coverage           - Generate test coverage report"
	@echo ""
	@echo "Database (Docker):"
	@echo "  make db-up              - Start test databases (PostgreSQL, MySQL)"
	@echo "  make db-down            - Stop test databases"
	@echo "  make db-logs            - View database logs"
	@echo "  make db-reset           - Reset test databases (remove volumes)"
	@echo "  make db-status          - Check database health"
	@echo ""
	@echo "Code Quality:"
	@echo "  make build              - Build all crates"
	@echo "  make clippy             - Run Clippy linter"
	@echo "  make fmt                - Format code with rustfmt"
	@echo "  make check              - Run all checks (fmt + clippy + test)"
	@echo "  make clean              - Clean build artifacts"
	@echo ""
	@echo "Development:"
	@echo "  make dev                - Run development server"
	@echo "  make doc                - Build documentation"
	@echo "  make bench              - Run benchmarks"
	@echo "  make install            - Install CLI tool"
	@echo ""

# Build all crates
build:
	cargo build --all-features

# Build release
build-release:
	cargo build --release --all-features

# Run all tests (unit + integration)
test: test-unit test-integration

# Run unit tests only (no database required)
test-unit:
	@echo "Running unit tests..."
	@cargo test --lib --all-features

# Run integration tests (requires Docker databases)
test-integration: db-up
	@echo "Running integration tests..."
	@sleep 2  # Wait for databases to be fully ready
	@cargo test --all-features -- --ignored

# Run end-to-end tests
test-e2e:
	cargo test --test 'test_*' --all-features -- --ignored

# Run Clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
	cargo fmt --all

# Check formatting
fmt-check:
	cargo fmt --all -- --check

# Run all checks
check: fmt-check clippy test

# Clean build artifacts
clean:
	cargo clean

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

# Watch for changes and run tests
watch:
	cargo watch -x 'test --all-features'

# Watch for changes and run checks
watch-check:
	cargo watch -x 'check --all-features'

# ============================================================================
# Docker-based Test Database Management
# ============================================================================

# Start test databases (PostgreSQL + MySQL)
db-up:
	@echo "Starting test databases..."
	@docker compose -f docker-compose.test.yml up -d
	@echo "Waiting for databases to be healthy..."
	@sleep 3
	@docker compose -f docker-compose.test.yml ps

# Stop test databases
db-down:
	@echo "Stopping test databases..."
	@docker compose -f docker-compose.test.yml down

# View database logs
db-logs:
	@docker compose -f docker-compose.test.yml logs -f

# Reset test databases (remove volumes)
db-reset:
	@echo "Resetting test databases (removing volumes)..."
	@docker compose -f docker-compose.test.yml down -v
	@docker compose -f docker-compose.test.yml up -d
	@sleep 3
	@echo "Databases reset and started"

# Check database health status
db-status:
	@echo "Database status:"
	@docker compose -f docker-compose.test.yml ps

# Verify test data
db-verify:
	@echo "Verifying PostgreSQL test data..."
	@docker compose -f docker-compose.test.yml exec -T postgres-test \
		psql -U fraiseql_test -d test_fraiseql -c "SELECT 'v_user' AS view, COUNT(*) FROM v_user UNION ALL SELECT 'v_post', COUNT(*) FROM v_post UNION ALL SELECT 'v_product', COUNT(*) FROM v_product;"

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

# Security audit
audit:
	cargo audit

# Update dependencies
update:
	cargo update

# Check for outdated dependencies
outdated:
	cargo outdated
