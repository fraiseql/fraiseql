.PHONY: help build test clippy fmt check clean install dev doc bench

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "  make build       - Build all crates"
	@echo "  make test        - Run all tests"
	@echo "  make clippy      - Run Clippy linter"
	@echo "  make fmt         - Format code with rustfmt"
	@echo "  make check       - Run all checks (fmt + clippy + test)"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make install     - Install CLI tool"
	@echo "  make dev         - Run development server"
	@echo "  make doc         - Build documentation"
	@echo "  make bench       - Run benchmarks"
	@echo ""

# Build all crates
build:
	cargo build --all-features

# Build release
build-release:
	cargo build --release --all-features

# Run all tests
test:
	cargo test --all-features

# Run integration tests
test-integration:
	cargo test --test '*' --all-features

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

# Database setup (PostgreSQL)
db-setup:
	psql -U postgres -c "CREATE DATABASE fraiseql_test;"

# Database teardown
db-teardown:
	psql -U postgres -c "DROP DATABASE IF EXISTS fraiseql_test;"

# Database reset
db-reset: db-teardown db-setup

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
