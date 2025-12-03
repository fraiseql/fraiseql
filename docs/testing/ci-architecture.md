# CI/CD Architecture

FraiseQL uses a sophisticated CI/CD pipeline designed for reliability, speed, and scalability. This document explains the architecture and how to work with it.

## Overview

The CI pipeline is split into two workflows to balance speed and comprehensive testing:

```
Main CI Pipeline (quality-gate.yml)
├── Fast & Reliable ✅
├── PostgreSQL-only integration tests
├── Required for all PRs
└── Blocks merges if failing

Enterprise CI Pipeline (enterprise-tests.yml)
├── Comprehensive but optional ⚠️
├── Vault KMS & Auth0 integration tests
├── Runs weekly + manual trigger
└── Doesn't block merges
```

## Main CI Pipeline

### Jobs

| Job | Purpose | Duration | Dependencies |
|-----|---------|----------|--------------|
| `unit-tests` | Fast unit tests, no external deps | ~2 min | None |
| `lint` | Code quality checks (ruff, mypy) | ~1 min | None |
| `security` | Security scanning | ~30 sec | None |
| `integration-postgres` | PostgreSQL integration tests | ~8-10 min | unit-tests, lint |
| `quality-gate` | Final approval gate | ~10 sec | All above |

### Test Categories

The main pipeline runs tests marked with:
- `@pytest.mark.requires_postgres` - Tests needing PostgreSQL
- Excludes `@pytest.mark.requires_vault` - Vault KMS tests
- Excludes `@pytest.mark.requires_auth0` - Auth0 tests

### Quality Gate

The `quality-gate` job ensures:
- All required jobs passed
- No critical security issues
- Code coverage meets minimum thresholds
- Linting passes with zero errors

## Enterprise CI Pipeline

### When It Runs

- **Weekly**: Every Monday at 6 AM UTC
- **On main branch**: When code is pushed/merged to main
- **Manual**: Via GitHub Actions "Run workflow" button

### Jobs

| Job | Purpose | Services | Duration |
|-----|---------|----------|----------|
| `vault-kms-tests` | Vault encryption integration | PostgreSQL + Vault | ~5-15 min |
| `auth0-tests` | Auth0 authentication | PostgreSQL + Auth0 mocks | ~3-5 min |
| `enterprise-summary` | Results summary | None | ~10 sec |

### Reliability Features

**Vault Startup Handling:**
- Exponential backoff (2^attempt seconds wait)
- 10 retry attempts (up to ~17 minutes total)
- 20-second grace period before health checks
- Explicit error messages for debugging

**Failure Handling:**
- `continue-on-error: true` - Individual job failures don't stop the workflow
- Summary job always succeeds (logs results)
- Test artifacts uploaded for analysis

## Test Markers

Tests are categorized using pytest markers for selective execution:

### Core Markers

| Marker | Description | CI Usage |
|--------|-------------|----------|
| `@pytest.mark.requires_postgres` | Tests needing PostgreSQL database | Main CI |
| `@pytest.mark.requires_vault` | Tests needing HashiCorp Vault KMS | Enterprise CI |
| `@pytest.mark.requires_auth0` | Tests needing Auth0 authentication | Enterprise CI |
| `@pytest.mark.requires_all` | Tests needing all services | Enterprise CI |

### Usage Examples

```bash
# Run only PostgreSQL tests (fast, reliable)
pytest -m 'requires_postgres'

# Run everything except enterprise features
pytest -m 'not requires_vault and not requires_auth0'

# Run only enterprise tests
pytest -m 'requires_vault or requires_auth0'
```

## Config Fixtures

Instead of creating `FraiseQLConfig` instances directly, use pre-configured fixtures:

### Available Fixtures

| Fixture | Environment | Purpose | Example Use |
|---------|-------------|---------|-------------|
| `test_config` | testing | Default test configuration | Most integration tests |
| `development_config` | development | Local development setup | Dev environment tests |
| `production_config` | production | Production-like config | Security/behavior tests |
| `apq_required_config` | testing | APQ in required mode | APQ security tests |
| `apq_disabled_config` | testing | APQ completely disabled | APQ disabled tests |
| `vault_kms_config` | testing | Vault KMS enabled | KMS integration tests |
| `custom_config` | flexible | Factory for custom configs | Special test scenarios |

### Before/After Example

```python
# ❌ Before: Direct config creation
def test_something():
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
        environment="testing"
    )

# ✅ After: Use fixture
def test_something(test_config):
    assert test_config.environment == "testing"
```

## Local Development

### Running Tests Locally

```bash
# 1. Start PostgreSQL (if not running)
./scripts/development/start-postgres-daemon.sh

# 2. Install dependencies
uv venv && source .venv/bin/activate
uv pip install ".[dev,all]"

# 3. Run different test categories
pytest tests/unit/                    # Unit tests only
pytest -m 'requires_postgres'         # PostgreSQL integration tests
pytest -m 'requires_vault'            # Vault tests (requires Vault running)
pytest tests/config/ -v               # Config tests with verbose output

# 4. Run with coverage
pytest --cov=src/fraiseql --cov-report=html
```

### Testing Enterprise Features

For enterprise features requiring external services:

```bash
# Vault KMS tests (requires Vault)
docker run -d --name vault -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=fraiseql-ci-token \
  hashicorp/vault:latest

export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-ci-token
pytest -m 'requires_vault'

# Auth0 tests (use mocks, no external service needed)
pytest -m 'requires_auth0'
```

## Troubleshooting

**For detailed troubleshooting procedures, see**: [`docs/runbooks/ci-troubleshooting.md`](../runbooks/ci-troubleshooting.md)

### Quick Reference

**Main CI Issues:**
- PostgreSQL tests failing → Check database connection and `FRAISEQL_ENVIRONMENT=testing`
- Quality gate blocked → Check logs for failed jobs (unit-tests, lint, security, integration)

**Enterprise CI Issues:**
- Vault not starting → Wait for exponential backoff (up to 17 min), check Docker resources
- Auth0 tests failing → Verify mocks configured, check JWT validation

**Local Development:**
- Tests can't connect → Run `pg_isready -h localhost -p 5432`, restart PostgreSQL if needed
- Markers not working → Run `pytest --markers` to list available markers

**For step-by-step diagnostics and solutions**, see the [CI Troubleshooting Runbook](../runbooks/ci-troubleshooting.md)

## Contributing

When adding new tests:

1. **Choose appropriate markers** based on dependencies
2. **Use config fixtures** instead of direct config creation
3. **Test locally** before pushing
4. **Update this documentation** if adding new patterns

### Adding New Markers

```python
# In pyproject.toml, add to [tool.pytest.ini_options].markers
"new_service: Tests requiring new external service"
```

### Adding New Config Fixtures

```python
# In tests/fixtures/config/conftest.py
@pytest.fixture
def new_service_config(postgres_url: str):
    """Config for new service integration tests."""
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        # new service config here
    )
```

## Performance Optimization

The CI pipeline is optimized for speed:

- **Parallel jobs**: Unit tests, lint, security run in parallel
- **Selective testing**: Only PostgreSQL tests in main CI
- **Schema isolation**: Each test class gets its own PostgreSQL schema
- **Connection pooling**: Reused connections within test classes

Typical CI times:
- **Main pipeline**: 10-12 minutes
- **Enterprise pipeline**: 8-20 minutes (variable due to external services)

## Security Considerations

- **Secrets management**: Vault integration for KMS operations
- **Environment isolation**: Testing, development, production configs
- **Dependency scanning**: Automated security vulnerability checks
- **Access controls**: Enterprise features require proper authentication

## Future Improvements

- **Test parallelization**: Split large test suites across multiple runners
- **Performance regression detection**: Automated benchmarking
- **Environment parity**: Closer alignment between CI and production
- **Test result analysis**: Better failure pattern recognition
