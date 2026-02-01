.PHONY: help build test test-unit test-integration clippy fmt check clean install dev doc bench db-up db-down db-logs db-reset db-status demo-start demo-stop demo-logs demo-status demo-clean demo-restart examples-start examples-stop examples-logs examples-status examples-clean e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-velocitybench e2e-clean e2e-status

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
	@echo "Docker Demo (Newcomers):"
	@echo "  make demo-start         - Start single-example stack (blog only)"
	@echo "  make demo-stop          - Stop demo stack"
	@echo "  make demo-logs          - View demo logs"
	@echo "  make demo-status        - Check demo health"
	@echo "  make demo-restart       - Restart demo stack"
	@echo "  make demo-clean         - Remove demo volumes and stop"
	@echo ""
	@echo "Docker Examples (Advanced):"
	@echo "  make examples-start     - Start multi-example stack (blog, ecommerce, streaming)"
	@echo "  make examples-stop      - Stop examples stack"
	@echo "  make examples-logs      - View examples logs"
	@echo "  make examples-status    - Check examples health"
	@echo "  make examples-clean     - Remove examples volumes and stop"
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

# ============================================================================
# E2E Testing - Language Generators
# ============================================================================

## Setup: Start Docker databases and prepare for E2E tests
e2e-setup:
	@echo "🔧 Setting up E2E test infrastructure..."
	@docker compose -f docker-compose.test.yml up -d || echo "ℹ️  Docker compose not available, skipping database setup"
	@echo "✅ E2E infrastructure ready"

## Run E2E tests for Python language generator
e2e-python: e2e-setup
	@echo ""
	@echo "========== PYTHON E2E TEST =========="
	@export PATH="$(PWD)/target/release:$$PATH" && \
		cd fraiseql-python && \
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
	@npm test --prefix fraiseql-typescript
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
	@cd fraiseql-go && go test ./fraiseql/... -v
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
		. fraiseql-python/.venv/bin/activate && \
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
	@docker compose -f docker-compose.test.yml down -v 2>/dev/null || true
	@rm -rf /tmp/fraiseql-*-test-output
	@echo "✅ Cleanup complete"

## Status: Check E2E test infrastructure
e2e-status:
	@echo "Docker Compose Status:"
	@docker compose -f docker-compose.test.yml ps 2>/dev/null || echo "Docker not available"
	@echo ""
	@echo "Languages ready:"
	@which python3 > /dev/null && echo "  ✅ Python" || echo "  ❌ Python"
	@which npm > /dev/null && echo "  ✅ TypeScript/Node" || echo "  ❌ TypeScript/Node"
	@which go > /dev/null && echo "  ✅ Go" || echo "  ❌ Go"
	@which mvn > /dev/null 2>&1 || [ -d "$$HOME/.local/opt/apache-maven-"* ] && echo "  ✅ Java" || echo "  ❌ Java"
	@which php > /dev/null && echo "  ✅ PHP" || echo "  ❌ PHP"

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
