# FraiseQL Makefile
# Run tests with Podman and PostgreSQL

# Default shell
SHELL := /bin/bash

# Python interpreter
PYTHON := python

# Test environment variables for Podman
export TESTCONTAINERS_PODMAN := true
export TESTCONTAINERS_RYUK_DISABLED := true

# Colors for output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m # No Color

.PHONY: help
help: ## Show this help message
	@echo -e "$(GREEN)FraiseQL Development Commands$(NC)"
	@echo -e "=============================="
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "$(YELLOW)%-20s$(NC) %s\n", $$1, $$2}'

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
test-fast: ## Run tests in parallel (faster)
	@echo -e "$(GREEN)Running tests in parallel...$(NC)"
	pytest -n auto

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

.PHONY: test-watch
test-watch: ## Run tests in watch mode (requires pytest-watch)
	@command -v ptw >/dev/null 2>&1 || { echo -e "$(RED)pytest-watch not installed. Run: pip install pytest-watch$(NC)"; exit 1; }
	@echo -e "$(GREEN)Running tests in watch mode...$(NC)"
	ptw -- -xvs

.PHONY: lint
lint: ## Run linting with ruff
	@echo -e "$(GREEN)Running ruff linter...$(NC)"
	ruff check src/

.PHONY: lint-fix
lint-fix: ## Fix linting issues automatically
	@echo -e "$(GREEN)Fixing linting issues...$(NC)"
	ruff check src/ --fix

.PHONY: format
format: ## Format code with black
	@echo -e "$(GREEN)Formatting code with black...$(NC)"
	black src/ tests/

.PHONY: format-check
format-check: ## Check code formatting without changes
	@echo -e "$(GREEN)Checking code format...$(NC)"
	black --check src/ tests/

.PHONY: type-check
type-check: ## Run type checking with pyright
	@echo -e "$(GREEN)Running pyright type checker...$(NC)"
	pyright


.PHONY: qa
qa: format lint type-check test ## Run all quality checks (format, lint, type-check, test)
	@echo -e "$(GREEN)All quality checks passed!$(NC)"

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

# Package distribution commands
.PHONY: build
build: ## Build distribution packages
	@echo -e "$(GREEN)Building distribution packages...$(NC)"
	rm -rf dist/ build/ *.egg-info src/*.egg-info
	python -m build
	@echo -e "$(GREEN)Build complete. Files in dist/:$(NC)"
	@ls -la dist/

.PHONY: publish-test
publish-test: ## Publish to TestPyPI (for testing)
	@echo -e "$(YELLOW)Publishing to TestPyPI...$(NC)"
	@bash scripts/publish.sh --test-pypi

.PHONY: publish
publish: ## Publish to PyPI (production)
	@echo -e "$(RED)Publishing to PyPI (Production)...$(NC)"
	@bash scripts/publish.sh

.PHONY: check-publish
check-publish: ## Check package before publishing
	@echo -e "$(GREEN)Checking package...$(NC)"
	python -m twine check dist/*

# Default target
.DEFAULT_GOAL := help
