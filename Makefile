.PHONY: help build test test-unit test-integration test-federation clippy fmt check clean install dev doc bench db-up db-down db-logs db-reset db-status federation-up federation-down demo-start demo-stop demo-logs demo-status demo-clean demo-restart examples-start examples-stop examples-logs examples-status examples-clean e2e-setup e2e-all e2e-python e2e-typescript e2e-java e2e-go e2e-php e2e-velocitybench e2e-clean e2e-status

# Default target
help:
	@echo "FraiseQL v2 Development Commands"
	@echo ""
	@echo "Testing:"
	@echo "  make test               - Run all tests"
	@echo "  make test-unit          - Run unit tests only (fast, no database)"
	@echo "  make test-integration   - Run integration tests (requires Docker)"
	@echo "  make test-federation    - Run federation tests (requires Docker)"
	@echo "  make coverage           - Generate test coverage report"
	@echo ""
	@echo "Database (Docker):"
	@echo "  make db-up              - Start test databases (PostgreSQL, MySQL, SQL Server, Redis)"
	@echo "  make db-down            - Stop test databases"
	@echo "  make db-logs            - View database logs"
	@echo "  make db-reset           - Reset test databases (remove volumes)"
	@echo "  make db-status          - Check database health"
	@echo "  make federation-up      - Start federation stack (Apollo Router + 3 subgraphs)"
	@echo "  make federation-down    - Stop federation stack"
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

# Run all tests (unit + integration)
test: test-unit test-integration

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

# Start test databases (PostgreSQL, MySQL, SQL Server, Redis) and wait until healthy
db-up:
	@echo "Starting test databases..."
	@docker compose -f docker/docker-compose.test.yml up -d
	@echo "Waiting for all services to be healthy..."
	@for svc in postgres-test mysql-test sqlserver-test redis-test; do \
		printf "  Waiting for %-20s" "$$svc..."; \
		for i in $$(seq 1 60); do \
			status=$$(docker inspect --format='{{.State.Health.Status}}' \
				$$(docker compose -f docker/docker-compose.test.yml ps -q $$svc 2>/dev/null) 2>/dev/null); \
			if [ "$$status" = "healthy" ]; then echo " ready"; break; fi; \
			if [ $$i -eq 60 ]; then echo " TIMEOUT"; exit 1; fi; \
			sleep 2; \
		done; \
	done
	@echo "All databases ready."
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
	@docker compose -f docker/docker-compose.test.yml up -d || echo "ℹ️  Docker compose not available, skipping database setup"
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
	@docker compose -f docker/docker-compose.test.yml down -v 2>/dev/null || true
	@rm -rf /tmp/fraiseql-*-test-output
	@echo "✅ Cleanup complete"

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
