# CI/CD Architecture

FraiseQL uses a sophisticated CI/CD pipeline designed for reliability, speed, and scalability. This document explains the architecture and how to work with it.

## Overview

The CI pipeline uses a **three-tier architecture** designed for speed, reliability, and resilience testing:

```
Quality Gate CI/CD (quality-gate.yml)       Enterprise CI/CD (enterprise-tests.yml)     Chaos Engineering (chaos-engineering-tests.yml)
├── Speed: 15-20 minutes                   ├── Speed: 8-20 minutes                    ├── Speed: 45-60 minutes
├── Purpose: Correctness                   ├── Purpose: Enterprise Features          ├── Purpose: Resilience
├── PostgreSQL-only tests                  ├── Vault KMS & Auth0 tests               ├── Docker + PostgreSQL + Chaos
├── Required for all PRs                   ├── Weekly + manual trigger               ├── Weekly + manual trigger + on-demand
├── Blocks merges: YES ✅                  ├── Blocks merges: NO ⚠️                  ├── Blocks merges: NO ⚠️ (informational)
└── Environment: Lightweight               └── Environment: Full services             └── Environment: Chaos simulation
```

**[📖 Chaos Engineering Strategy](../testing/chaos-engineering-strategy.md)** - Complete guide to resilience testing

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

## Chaos Engineering CI Pipeline

### Purpose & Philosophy

The chaos engineering pipeline validates **system resilience** under adverse conditions, separate from correctness testing:

- **🎯 Goal**: Ensure FraiseQL remains stable when things go wrong
- **🔄 Approach**: Intentionally break systems to verify graceful degradation
- **📊 Method**: 71+ chaos test categories across network, database, and application layers
- **⏰ Timing**: Weekly schedule + manual triggers (never on every PR)

### When It Runs

- **Weekly Schedule**: Every Monday at 6 AM UTC
- **Manual Trigger**: Via GitHub Actions "Run workflow" button
- **On-Demand**: Via workflow dispatch with custom parameters
- **Post-Major Release**: After significant architectural changes

### Chaos Test Categories

| Category | Examples | Purpose |
|----------|----------|---------|
| **Network Chaos** | Connection drops, latency injection, DNS failures | Test resilience to network instability |
| **Database Chaos** | Connection pool exhaustion, query timeouts, replica failures | Validate database fault tolerance |
| **Resource Chaos** | Memory pressure, CPU spikes, disk space exhaustion | Test resource constraint handling |
| **Dependency Chaos** | External service failures, API rate limits | Verify graceful degradation |
| **Application Chaos** | Thread pool exhaustion, deadlock injection | Test internal fault handling |

### Workflow Architecture

```
chaos-engineering-tests.yml
├── Setup Phase (10-15 min)
│   ├── Spin up Docker PostgreSQL
│   ├── Initialize test schema
│   ├── Configure chaos injection tools
│   └── Health check all services
│
├── Execution Phase (30-40 min)
│   ├── Run chaos test suites in parallel
│   ├── Inject failures during test execution
│   ├── Monitor system behavior under stress
│   └── Collect metrics and logs
│
└── Analysis Phase (5-10 min)
    ├── Generate chaos test reports
    ├── Upload artifacts for analysis
    └── Send notifications (success/failure)
```

### Key Differences from Standard CI

| Aspect | Standard CI | Chaos Engineering CI |
|--------|-------------|---------------------|
| **Environment** | Lightweight (schema isolation) | Full Docker containers |
| **Dependencies** | Python + PostgreSQL | Docker + PostgreSQL + Chaos tools |
| **Determinism** | 100% predictable | Intentionally random failures |
| **Success Criteria** | All tests pass | System remains stable despite failures |
| **Performance** | Fast feedback (<20 min) | Comprehensive testing (45-60 min) |
| **Resource Usage** | Low (shared runners) | High (dedicated runners, Docker) |

### Manual Trigger Options

#### Via GitHub Actions UI

1. Go to **Actions** tab in GitHub repository
2. Select **"Chaos Engineering Tests"** workflow
3. Click **"Run workflow"** button
4. Configure parameters (optional):
   - **Test categories**: Which chaos tests to run
   - **Intensity level**: Low/Medium/High chaos injection
   - **Duration**: How long to run chaos tests

#### Via GitHub CLI

```bash
# Run all chaos tests
gh workflow run chaos-engineering-tests.yml

# Run specific test categories
gh workflow run chaos-engineering-tests.yml \
  -f test_categories="network,database" \
  -f intensity="medium" \
  -f duration="30"

# Run with custom parameters
gh workflow run chaos-engineering-tests.yml \
  -f chaos_enabled="true" \
  -f parallel_execution="4"
```

### Chaos Test Markers

Chaos tests use specialized markers for selective execution:

| Marker | Description | Example Usage |
|--------|-------------|---------------|
| `@pytest.mark.chaos` | All chaos tests | Full resilience validation |
| `@pytest.mark.chaos_real_db` | Chaos tests using real PostgreSQL | Database integration chaos |
| `@pytest.mark.chaos_network` | Network failure scenarios | Connection drops, latency |
| `@pytest.mark.chaos_database` | Database failure scenarios | Connection pool issues |
| `@pytest.mark.chaos_cache` | Cache failure scenarios | Redis/Memcached failures |
| `@pytest.mark.chaos_auth` | Authentication failure scenarios | JWT/Auth0 timeouts |
| `@pytest.mark.chaos_resources` | Resource exhaustion scenarios | Memory/CPU pressure |
| `@pytest.mark.chaos_concurrency` | Concurrent execution scenarios | Race conditions |
| `@pytest.mark.chaos_validation` | Success criteria validation | Recovery verification |
| `@pytest.mark.chaos_verification` | Infrastructure verification | Environment checks |

### Execution Time Expectations

| Test Category | Duration | Parallel Jobs | Total Chaos Tests |
|---------------|----------|---------------|------------------|
| **Network Chaos** | 8-12 min | 2-3 jobs | 15+ test scenarios |
| **Database Chaos** | 10-15 min | 1-2 jobs | 20+ test scenarios |
| **Resource Chaos** | 6-10 min | 2 jobs | 12+ test scenarios |
| **Dependency Chaos** | 5-8 min | 1-2 jobs | 10+ test scenarios |
| **Application Chaos** | 4-7 min | 1 job | 8+ test scenarios |
| **Setup/Teardown** | 3-5 min | N/A | Infrastructure setup |
| **Analysis/Reporting** | 2-3 min | N/A | Report generation |

**Total Expected Runtime**: 45-60 minutes (varies by test intensity and system load)

## Test Markers

Tests are categorized using pytest markers for selective execution:

### Core Markers

| Marker | Description | CI Usage |
|--------|-------------|----------|
| `@pytest.mark.requires_postgres` | Tests needing PostgreSQL database | Main CI |
| `@pytest.mark.requires_vault` | Tests needing HashiCorp Vault KMS | Enterprise CI |
| `@pytest.mark.requires_auth0` | Tests needing Auth0 authentication | Enterprise CI |
| `@pytest.mark.requires_all` | Tests needing all services | Enterprise CI |

### Chaos Engineering Markers

| Marker | Description | CI Usage |
|--------|-------------|----------|
| `@pytest.mark.chaos` | Base marker for all chaos tests | Chaos Engineering CI |
| `@pytest.mark.chaos_real_db` | Chaos tests using real PostgreSQL | Chaos Engineering CI |
| `@pytest.mark.chaos_network` | Network failure scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_database` | Database failure scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_cache` | Cache failure scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_auth` | Authentication failure scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_resources` | Resource exhaustion scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_concurrency` | Concurrent execution scenarios | Chaos Engineering CI |
| `@pytest.mark.chaos_validation` | Success criteria validation | Chaos Engineering CI |
| `@pytest.mark.chaos_verification` | Infrastructure verification | Chaos Engineering CI |

### Usage Examples

```bash
# Run only PostgreSQL tests (fast, reliable)
pytest -m 'requires_postgres'

# Run everything except enterprise features
pytest -m 'not requires_vault and not requires_auth0'

# Run only enterprise tests
pytest -m 'requires_vault or requires_auth0'

# Run all chaos engineering tests (requires Docker)
pytest -m 'chaos'

# Run network chaos tests only
pytest -m 'chaos_network'

# Run chaos tests with real database
pytest -m 'chaos_real_db'

# Run all tests except chaos tests (for fast local development)
pytest -m 'not chaos'

# Run standard CI tests (quality gate equivalent)
pytest -m 'requires_postgres and not chaos and not requires_vault and not requires_auth0'
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

### Running Chaos Engineering Tests Locally

Chaos tests require Docker and can be resource-intensive:

```bash
# 1. Ensure Docker is running
docker --version

# 2. Run all chaos tests (requires Docker infrastructure)
pytest -m 'chaos' --chaos-intensity=low

# 3. Run specific chaos categories
pytest -m 'chaos_network' --chaos-duration=30
pytest -m 'chaos_database' --chaos-parallel=2

# 4. Run chaos tests with real database
pytest -m 'chaos_real_db' --chaos-intensity=medium

# 5. Debug chaos tests (reduced intensity, more logging)
pytest -m 'chaos_network' -v -s \
  --chaos-intensity=minimal \
  --chaos-log-level=DEBUG
```

**⚠️ Chaos Test Prerequisites:**

- Docker daemon running
- Sufficient system resources (4GB+ RAM recommended)
- Network access for container downloads
- May require `sudo` for network manipulation tests

## Troubleshooting

**For detailed troubleshooting procedures, see**: [`docs/runbooks/ci-troubleshooting.md`](../runbooks/ci-troubleshooting.md)

### Quick Reference

**Main CI Issues:**

- PostgreSQL tests failing → Check database connection and `FRAISEQL_ENVIRONMENT=testing`
- Quality gate blocked → Check logs for failed jobs (unit-tests, lint, security, integration)

**Enterprise CI Issues:**

- Vault not starting → Wait for exponential backoff (up to 17 min), check Docker resources
- Auth0 tests failing → Verify mocks configured, check JWT validation

**Chaos Engineering CI Issues:**

- Docker containers failing to start → Check Docker daemon, system resources, and network connectivity
- Chaos injection not working → Verify testcontainers version, check system permissions for network manipulation
- Tests timing out → Increase `--chaos-timeout` parameter or reduce `--chaos-intensity`
- Resource exhaustion during tests → Run with lower intensity (`--chaos-intensity=low`) or add more system resources
- Network chaos tests failing → Check if running in restricted environment (corporate firewalls, VPN issues)

**Local Development:**

- Tests can't connect → Run `pg_isready -h localhost -p 5432`, restart PostgreSQL if needed
- Markers not working → Run `pytest --markers` to list available markers
- Chaos tests can't run → Ensure Docker is running, check `docker ps` and `docker logs`

**For chaos engineering specific issues**, see the [Chaos Engineering Strategy Guide](../testing/chaos-engineering-strategy.md#troubleshooting)

## Contributing

When adding new tests:

1. **Choose appropriate markers** based on dependencies
2. **Use config fixtures** instead of direct config creation
3. **Test locally** before pushing
4. **Update this documentation** if adding new patterns

### Adding Chaos Engineering Tests

When developing chaos tests:

1. **Use appropriate chaos markers** (`@pytest.mark.chaos`, `@pytest.mark.chaos_real_db`, `@pytest.mark.chaos_<category>`)

**Example Chaos Test Structure:**

```python
@pytest.mark.chaos
@pytest.mark.chaos_real_db
@pytest.mark.chaos_database
def test_connection_pool_exhaustion_recovery():
    """Test graceful handling when database connection pool is exhausted.

    Chaos Scenario: All database connections become unavailable
    Expected Behavior: System queues requests and recovers when connections return
    Recovery Time: Should recover within 30 seconds of connection restoration
    """
    # Implementation here
```

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

The CI pipeline is optimized for speed and resource efficiency:

### Standard CI Optimizations

- **Parallel jobs**: Unit tests, lint, security run in parallel
- **Selective testing**: Only PostgreSQL tests in main CI (chaos tests run in separate workflow)
- **Schema isolation**: Each test class gets its own PostgreSQL schema
- **Connection pooling**: Reused connections within test classes

### Chaos Engineering Optimizations

- **Container reuse**: Docker containers cached between test runs where possible
- **Parallel execution**: Chaos test categories run in parallel when safe to do so
- **Intensity scaling**: Adjustable chaos injection levels (minimal/low/medium/high)
- **Selective targeting**: Run specific chaos categories instead of all tests

### Execution Time Comparisons

| Pipeline | Purpose | Duration | Frequency | Blocks Merges |
|----------|---------|----------|-----------|---------------|
| **Quality Gate CI** | Correctness validation | 15-20 min | Every PR | ✅ YES |
| **Enterprise CI** | Feature validation | 8-20 min | Weekly + Manual | ⚠️ NO |
| **Chaos Engineering CI** | Resilience validation | 45-60 min | Weekly + Manual | ⚠️ NO |
| **Combined Runtime** | All pipelines | 68-100 min | On-demand only | ⚠️ NO |

**Strategy Benefits:**

- **Fast Feedback**: Developers get correctness results in <20 minutes
- **Comprehensive Coverage**: Enterprise and chaos testing run separately
- **Resource Efficiency**: Heavy chaos tests don't slow down development
- **Reliable Merges**: Only correctness validation blocks PR merges

## Security Considerations

- **Secrets management**: Vault integration for KMS operations
- **Environment isolation**: Testing, development, production configs
- **Dependency scanning**: Automated security vulnerability checks
- **Access controls**: Enterprise features require proper authentication

## Future Improvements

### Standard CI Improvements

- **Test parallelization**: Split large test suites across multiple runners
- **Performance regression detection**: Automated benchmarking
- **Environment parity**: Closer alignment between CI and production
- **Test result analysis**: Better failure pattern recognition

### Chaos Engineering Improvements

- **Chaos automation**: AI-driven chaos scenario generation
- **Performance under chaos**: Benchmarking system performance during failures
- **Chaos in production**: Safe chaos testing in production environments (feature flags)
- **Chaos intelligence**: Machine learning to identify high-risk failure patterns
- **Multi-region chaos**: Testing distributed system resilience across regions
