# Performance Benchmark Makefile
SHELL := /bin/bash
.PHONY: all setup build start stop clean test-fraiseql test-strawberry test-all analyze help

# Default target
all: setup build start test-all analyze

# Colors for output
GREEN := \033[0;32m
YELLOW := \033[0;33m
RED := \033[0;31m
NC := \033[0m # No Color

# Help command
help:
	@echo "FraiseQL Performance Benchmark Commands:"
	@echo ""
	@echo "  make setup          - Initialize environment and check dependencies"
	@echo "  make build          - Build all container images"
	@echo "  make start          - Start all services"
	@echo "  make stop           - Stop all services"
	@echo "  make clean          - Clean up containers and volumes"
	@echo "  make test-fraiseql  - Run benchmarks on FraiseQL only"
	@echo "  make test-strawberry - Run benchmarks on Strawberry only"
	@echo "  make test-all       - Run benchmarks on all frameworks"
	@echo "  make analyze        - Generate comparison reports"
	@echo "  make logs-fraiseql  - Show FraiseQL container logs"
	@echo "  make logs-strawberry - Show Strawberry container logs"
	@echo "  make shell-postgres - Open PostgreSQL shell"
	@echo "  make stats          - Show container resource usage"

# Check if podman-compose is available
check-podman:
	@command -v podman-compose >/dev/null 2>&1 || { \
		echo "$(RED)Error: podman-compose is not installed$(NC)"; \
		echo "Install with: pip install podman-compose"; \
		exit 1; \
	}

# Setup environment
setup: check-podman
	@echo "$(GREEN)Setting up benchmark environment...$(NC)"
	@mkdir -p results-aggregator/reports
	@mkdir -p fraiseql/results
	@mkdir -p strawberry-sqlalchemy/results
	@mkdir -p graphene-sqlalchemy/results
	@mkdir -p ariadne/results
	@chmod -R 777 */results results-aggregator/reports
	@echo "$(GREEN)Environment setup complete!$(NC)"

# Build container images
build: check-podman
	@echo "$(GREEN)Building container images...$(NC)"
	podman-compose build --no-cache
	@echo "$(GREEN)Build complete!$(NC)"

# Start all services
start: check-podman
	@echo "$(GREEN)Starting services...$(NC)"
	podman-compose up -d
	@echo "$(YELLOW)Waiting for services to be healthy...$(NC)"
	@sleep 10
	@podman-compose ps
	@echo "$(GREEN)Services started!$(NC)"

# Stop all services
stop: check-podman
	@echo "$(YELLOW)Stopping services...$(NC)"
	podman-compose down
	@echo "$(GREEN)Services stopped!$(NC)"

# Clean up everything
clean: stop
	@echo "$(RED)Cleaning up containers and volumes...$(NC)"
	podman-compose down -v
	@rm -rf */results/* results-aggregator/reports/*
	@echo "$(GREEN)Cleanup complete!$(NC)"

# Run benchmarks on FraiseQL
test-fraiseql: check-podman
	@echo "$(GREEN)Running benchmarks on FraiseQL...$(NC)"
	@podman-compose exec k6 k6 run --out json=/results/fraiseql-results.json /scripts/scenarios/simple-queries.js --env TARGET=http://fraiseql:8000
	@podman-compose exec k6 k6 run --out json=/results/fraiseql-nested.json /scripts/scenarios/nested-queries.js --env TARGET=http://fraiseql:8000
	@podman-compose exec k6 k6 run --out json=/results/fraiseql-load.json /scripts/scenarios/load-test.js --env TARGET=http://fraiseql:8000
	@echo "$(GREEN)FraiseQL benchmarks complete!$(NC)"

# Run benchmarks on Strawberry
test-strawberry: check-podman
	@echo "$(GREEN)Running benchmarks on Strawberry + SQLAlchemy...$(NC)"
	@podman-compose exec k6 k6 run --out json=/results/strawberry-results.json /scripts/scenarios/simple-queries.js --env TARGET=http://strawberry-sqlalchemy:8000
	@podman-compose exec k6 k6 run --out json=/results/strawberry-nested.json /scripts/scenarios/nested-queries.js --env TARGET=http://strawberry-sqlalchemy:8000
	@podman-compose exec k6 k6 run --out json=/results/strawberry-load.json /scripts/scenarios/load-test.js --env TARGET=http://strawberry-sqlalchemy:8000
	@echo "$(GREEN)Strawberry benchmarks complete!$(NC)"

# Run all benchmarks
test-all: test-fraiseql test-strawberry
	@echo "$(GREEN)All benchmarks complete!$(NC)"

# Generate analysis reports
analyze:
	@echo "$(GREEN)Generating performance analysis reports...$(NC)"
	@podman-compose exec k6 python3 /results/analyze.py
	@echo "$(GREEN)Reports generated in results-aggregator/reports/$(NC)"

# View logs
logs-fraiseql:
	podman-compose logs -f fraiseql

logs-strawberry:
	podman-compose logs -f strawberry-sqlalchemy

logs-postgres:
	podman-compose logs -f postgres

# Database shell
shell-postgres:
	podman-compose exec postgres psql -U benchmark -d benchmark_db

# Show container stats
stats:
	@echo "$(GREEN)Container Resource Usage:$(NC)"
	@podman stats --no-stream $(shell podman-compose ps -q)

# Database operations
db-reset:
	@echo "$(YELLOW)Resetting database...$(NC)"
	@podman-compose exec postgres psql -U benchmark -d benchmark_db -c "DROP SCHEMA IF EXISTS benchmark CASCADE;"
	@podman-compose exec postgres psql -U benchmark -d benchmark_db -f /docker-entrypoint-initdb.d/01-schema.sql
	@podman-compose exec postgres psql -U benchmark -d benchmark_db -f /docker-entrypoint-initdb.d/02-seed-data.sql
	@echo "$(GREEN)Database reset complete!$(NC)"

# Quick benchmark for development
quick-test:
	@echo "$(GREEN)Running quick benchmark test...$(NC)"
	@curl -X POST http://localhost:8001/graphql \
		-H "Content-Type: application/json" \
		-d '{"query":"{ users(limit: 5) { id email username } }"}' | jq .
	@curl -X POST http://localhost:8002/graphql \
		-H "Content-Type: application/json" \
		-d '{"query":"{ users(limit: 5) { id email username } }"}' | jq .

# Monitor metrics
metrics:
	@echo "$(GREEN)Prometheus metrics:$(NC)"
	@echo "FraiseQL metrics: http://localhost:8001/metrics"
	@echo "Strawberry metrics: http://localhost:8002/metrics"
	@curl -s http://localhost:8001/metrics | grep graphql_
	@echo ""
	@curl -s http://localhost:8002/metrics | grep graphql_
