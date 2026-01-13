.PHONY: help build test clippy fmt check clean docker-build docker-up docker-down docker-logs integration-test

help:
	@echo "fraiseql-wire development commands:"
	@echo ""
	@echo "  make build               - Build the project"
	@echo "  make test                - Run unit tests"
	@echo "  make integration-test    - Run integration tests (requires Postgres)"
	@echo "  make clippy              - Run clippy linter"
	@echo "  make fmt                 - Format code"
	@echo "  make fmt-check           - Check code formatting"
	@echo "  make check               - Run all checks (fmt, clippy, test)"
	@echo "  make clean               - Clean build artifacts"
	@echo "  make doc                 - Build documentation"
	@echo ""
	@echo "  make docker-build        - Build Docker image"
	@echo "  make docker-up           - Start Docker containers"
	@echo "  make docker-down         - Stop Docker containers"
	@echo "  make docker-logs         - View Docker logs"
	@echo "  make docker-clean        - Remove Docker containers and volumes"
	@echo ""

build:
	cargo build

test:
	cargo test --lib

integration-test:
	cargo test --test integration -- --ignored --nocapture
	cargo test --test streaming_integration -- --ignored --nocapture

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

check: fmt-check clippy test
	@echo "All checks passed!"

clean:
	cargo clean

doc:
	cargo doc --no-deps --open

docker-build:
	docker-compose build

docker-up:
	docker-compose up -d
	@echo "Waiting for PostgreSQL to be ready..."
	@sleep 5
	@echo "PostgreSQL is running on localhost:5432"
	@echo "User: postgres, Password: postgres, Database: fraiseql_test"

docker-down:
	docker-compose down

docker-logs:
	docker-compose logs -f postgres

docker-clean:
	docker-compose down -v
	@echo "Docker containers and volumes cleaned"

.DEFAULT_GOAL := help
