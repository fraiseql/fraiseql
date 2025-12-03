# Developer Test Guide

This guide helps developers run and understand FraiseQL's test suite locally.

## Quick Start

```bash
# 1. Set up environment
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
pip install -e ".[dev,all]"

# 2. Start PostgreSQL
./scripts/development/start-postgres-daemon.sh

# 3. Run tests
pytest -m 'requires_postgres'  # Fast, reliable tests
```

## Test Categories

### Main CI Tests (Always Run)

These tests run in the main CI pipeline and are required for all PRs:

```bash
# Unit tests (no external dependencies)
pytest tests/unit/

# PostgreSQL integration tests
pytest -m 'requires_postgres'

# Config tests
pytest tests/config/

# Combined main CI test suite
pytest -m 'requires_postgres and not requires_vault and not requires_auth0'
```

### Enterprise Tests (Optional)

These tests run in the separate enterprise CI pipeline:

```bash
# Vault KMS tests (requires Vault running)
pytest -m 'requires_vault'

# Auth0 tests (uses mocks)
pytest -m 'requires_auth0'

# All enterprise tests
pytest -m 'requires_vault or requires_auth0'
```

## Local Setup

### PostgreSQL Setup

```bash
# Option 1: Use the provided script
./scripts/development/start-postgres-daemon.sh

# Option 2: Manual setup
createdb fraiseql_test
export DATABASE_URL="postgresql://localhost/fraiseql_test"
```

### Vault Setup (for Enterprise Tests)

```bash
# Start Vault in development mode
docker run -d --name vault -p 8200:8200 \
  -e VAULT_DEV_ROOT_TOKEN_ID=fraiseql-ci-token \
  hashicorp/vault:latest

# Set environment variables
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=fraiseql-ci-token

# Initialize Vault for testing
curl -X POST -H "X-Vault-Token: $VAULT_TOKEN" \
  http://localhost:8200/v1/sys/mounts/transit \
  -d '{"type":"transit"}'
```

## Running Specific Test Types

### By Test File

```bash
# Run a specific test file
pytest tests/config/test_apq_backend_config.py

# Run with verbose output
pytest tests/config/test_apq_backend_config.py -v

# Run with debugging
pytest tests/config/test_apq_backend_config.py -s
```

### By Marker

```bash
# PostgreSQL tests only
pytest -m 'requires_postgres'

# Exclude enterprise tests
pytest -m 'not requires_vault and not requires_auth0'

# Only integration tests
pytest -m 'integration'

# Only slow tests
pytest -m 'slow'
```

### By Directory

```bash
# Unit tests
pytest tests/unit/

# Integration tests
pytest tests/integration/

# Config tests
pytest tests/config/

# Enterprise tests
pytest tests/integration/enterprise/
```

## Debugging Tests

### Common Issues

**Tests can't connect to database:**
```bash
# Check PostgreSQL is running
pg_isready -h localhost -p 5432

# Check database exists
psql -l | grep fraiseql_test

# Reset database
dropdb fraiseql_test
createdb fraiseql_test
```

**Vault tests failing:**
```bash
# Check Vault is running
curl http://localhost:8200/v1/sys/health

# Check environment variables
echo $VAULT_ADDR
echo $VAULT_TOKEN

# Restart Vault container
docker restart vault
```

**Import errors:**
```bash
# Reinstall in development mode
pip install -e ".[dev,all]"

# Check Python path
python -c "import fraiseql; print(fraiseql.__file__)"
```

### Debugging Commands

```bash
# List all available tests
pytest --collect-only

# List tests with markers
pytest --collect-only -q | head -20

# Run tests with detailed output
pytest -v -s

# Run tests with coverage
pytest --cov=src/fraiseql --cov-report=html

# Run failed tests only
pytest --lf

# Run tests and stop on first failure
pytest -x

# Run tests in parallel (if pytest-xdist installed)
pytest -n auto
```

## Test Configuration

### Environment Variables

```bash
# Database
export DATABASE_URL="postgresql://localhost/fraiseql_test"

# Vault (for enterprise tests)
export VAULT_ADDR="http://localhost:8200"
export VAULT_TOKEN="fraiseql-ci-token"

# Test settings
export PYTEST_DISABLE_PLUGIN_AUTOLOAD=1  # Disable auto-loading
```

### Config Fixtures

Use pre-configured fixtures instead of creating configs manually:

```python
# ✅ Good: Use fixtures
def test_feature(test_config, db_connection):
    # test_config has safe defaults
    # db_connection provides database access
    pass

# ❌ Avoid: Manual config creation
def test_feature():
    config = FraiseQLConfig(database_url="...", environment="testing")
    # Error-prone, inconsistent
```

Available fixtures:
- `test_config`: Default testing configuration
- `development_config`: Development environment
- `production_config`: Production-like settings
- `custom_config`: Factory for custom configurations

## Writing Tests

### Test Structure

```python
import pytest
from fraiseql import FraiseQLConfig

@pytest.mark.requires_postgres  # Required marker
def test_user_creation(test_config, db_connection):
    """Test creating a new user."""

    # Arrange
    user_data = {"name": "Alice", "email": "alice@example.com"}

    # Act
    # ... test code ...

    # Assert
    # ... assertions ...
```

### Adding Markers

```python
# Single marker
@pytest.mark.requires_postgres
def test_database_feature():
    pass

# Multiple markers
@pytest.mark.requires_postgres
@pytest.mark.integration
def test_complex_feature():
    pass

# Conditional markers
@pytest.mark.requires_vault
@pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not available")
def test_vault_feature():
    pass
```

### Test Naming

```python
# ✅ Good: Descriptive names
def test_user_creation_with_valid_data()
def test_user_creation_fails_with_invalid_email()
def test_database_connection_pool_handles_concurrency()

# ❌ Avoid: Vague names
def test_user()
def test_database()
def test_integration()
```

## Performance Testing

### Running Benchmarks

```bash
# Run performance benchmarks
pytest tests/performance/ -v

# Run with profiling
pytest tests/performance/ --profile

# Generate performance reports
pytest tests/performance/ --benchmark-json=results.json
```

### Load Testing

```bash
# Run load tests (if available)
pytest tests/load/ -v

# Run with different concurrency levels
pytest tests/load/ --concurrency=10
```

## CI Simulation

### Simulate Main CI

```bash
# Run the same tests as main CI
pytest -m 'requires_postgres and not requires_vault and not requires_auth0' \
       --cov=src/fraiseql \
       --cov-report=term-missing \
       -v
```

### Simulate Enterprise CI

```bash
# Run enterprise tests (requires services)
pytest -m 'requires_vault or requires_auth0' \
       -v \
       --tb=short
```

## Troubleshooting

### Test Failures

**Database connection issues:**
- Ensure PostgreSQL is running
- Check DATABASE_URL environment variable
- Verify database exists and is accessible

**Import errors:**
- Reinstall package in development mode
- Check Python path includes src/
- Verify all dependencies are installed

**Fixture errors:**
- Check fixture is defined in conftest.py
- Ensure proper marker usage
- Verify fixture dependencies are available

### Performance Issues

**Slow tests:**
- Use markers to skip slow tests during development
- Run tests in parallel with pytest-xdist
- Profile with pytest --profile

**Memory issues:**
- Use database fixtures that clean up after tests
- Avoid loading large datasets in memory
- Use streaming for large result sets

### Common Error Messages

**"fixture 'db_connection' not found":**
```bash
# Add the postgres marker
@pytest.mark.requires_postgres
def test_something(db_connection):
    pass
```

**"No module named 'fraiseql'":**
```bash
# Install in development mode
pip install -e .
```

**"Connection refused":**
```bash
# Check PostgreSQL is running
pg_isready -h localhost -p 5432
```

## Advanced Usage

### Custom Test Configuration

```python
# Custom pytest configuration in conftest.py
def pytest_configure(config):
    config.addinivalue_line(
        "markers", "custom_marker: Custom test marker"
    )

# Custom fixtures
@pytest.fixture
def custom_app(test_config):
    return create_fraiseql_app(config=test_config)
```

### Test Parallelization

```bash
# Install pytest-xdist
pip install pytest-xdist

# Run tests in parallel
pytest -n auto

# Run on multiple CPUs
pytest -n 4
```

### Test Selection

```bash
# Run tests matching pattern
pytest -k "user and create"

# Run tests from specific class
pytest -k "TestUser"

# Run tests slower than 1 second
pytest --durations=10
```

## Contributing

When adding new tests:

1. Use appropriate markers (`@pytest.mark.requires_postgres`, etc.)
2. Use config fixtures instead of manual FraiseQLConfig creation
3. Write descriptive test names and docstrings
4. Test both success and failure cases
5. Ensure tests clean up after themselves
6. Run tests locally before submitting PR

## Resources

- [CI Architecture Documentation](./ci-architecture.md)
- [Pytest Markers Guide](./pytest-markers.md)
- [Config Fixtures Guide](./config-fixtures.md)
- [Pytest Documentation](https://docs.pytest.org/)
