# Pytest Markers

FraiseQL uses pytest markers to categorize tests by their dependencies and execution requirements. This enables selective test execution and CI job splitting.

## Overview

Markers are labels applied to test functions that describe their requirements:

```python
import pytest

@pytest.mark.requires_postgres
def test_database_operation():
    # This test needs PostgreSQL
    pass

@pytest.mark.requires_vault
def test_encryption():
    # This test needs Vault KMS
    pass
```

## Available Markers

### Service Dependency Markers

| Marker | Description | CI Usage | Example |
|--------|-------------|----------|---------|
| `@pytest.mark.requires_postgres` | Test requires PostgreSQL database | Main CI | Database queries, schema operations |
| `@pytest.mark.requires_vault` | Test requires HashiCorp Vault KMS | Enterprise CI | Encryption, key management |
| `@pytest.mark.requires_auth0` | Test requires Auth0 authentication | Enterprise CI | JWT validation, user auth |
| `@pytest.mark.requires_all` | Test requires all services | Enterprise CI | Full integration scenarios |

### Legacy Markers (Still Supported)

| Marker | Description | Usage |
|--------|-------------|-------|
| `@pytest.mark.integration` | Integration test (broader category) | General categorization |
| `@pytest.mark.database` | Database-related test | General categorization |
| `@pytest.mark.e2e` | End-to-end test | General categorization |
| `@pytest.mark.enterprise` | Enterprise feature test | General categorization |

## Usage Examples

### Basic Usage

```python
import pytest

# PostgreSQL-only test
@pytest.mark.requires_postgres
def test_user_creation(db_connection):
    """Test creating a user in the database."""
    # Test code here
    pass

# Vault KMS test
@pytest.mark.requires_vault
def test_data_encryption(vault_client):
    """Test encrypting data with Vault."""
    # Test code here
    pass

# Auth0 authentication test
@pytest.mark.requires_auth0
def test_jwt_validation(auth0_config):
    """Test JWT token validation."""
    # Test code here
    pass
```

### Combining Markers

```python
# Test needs both PostgreSQL and Vault
@pytest.mark.requires_postgres
@pytest.mark.requires_vault
def test_encrypted_database_storage(db_connection, vault_client):
    """Test storing encrypted data in database."""
    pass
```

### Conditional Testing

```python
import os
import pytest

# Skip if Vault not available
@pytest.mark.requires_vault
@pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not available")
def test_vault_integration():
    pass
```

## Running Tests by Marker

### Command Line Usage

```bash
# Run only PostgreSQL tests (fast, reliable)
pytest -m 'requires_postgres'

# Run everything except enterprise features
pytest -m 'not requires_vault and not requires_auth0'

# Run only enterprise tests
pytest -m 'requires_vault or requires_auth0'

# Run tests requiring all services
pytest -m 'requires_all'

# Combine with other filters
pytest -m 'requires_postgres and not slow'
```

### CI Usage

The CI pipeline uses markers to split test execution:

**Main CI (quality-gate.yml):**
```yaml
# Only PostgreSQL tests - fast and reliable
pytest -m 'requires_postgres and not requires_vault and not requires_auth0'
```

**Enterprise CI (enterprise-tests.yml):**
```yaml
# Vault tests
pytest -m 'requires_vault'

# Auth0 tests
pytest -m 'requires_auth0'
```

## Adding Markers to Tests

### Manual Addition

Add markers above test functions:

```python
import pytest

@pytest.mark.requires_postgres
@pytest.mark.integration
def test_something(db_connection):
    pass
```

### Batch Application

For applying markers to many tests, use scripts or IDE find/replace:

```bash
# Add markers to all tests in a directory
find tests/integration -name "*.py" -exec sed -i '1a import pytest' {} \;
find tests/integration -name "*.py" -exec sed -i '/^def test_/i @pytest.mark.requires_postgres\n@pytest.mark.integration' {} \;
```

### Automated Tools

Use the helper script from the CI implementation guide:

```python
# scripts/testing/add_test_markers.py
import re

def add_postgres_marker(filepath):
    # Add @pytest.mark.requires_postgres to all test functions
    pass
```

## Marker Definitions

Markers are defined in `pyproject.toml`:

```toml
[tool.pytest.ini_options]
markers = [
    # Service dependencies
    "requires_postgres: Test requires PostgreSQL database only",
    "requires_vault: Test requires HashiCorp Vault for KMS encryption",
    "requires_auth0: Test requires Auth0 authentication service",
    "requires_all: Test requires all services (PostgreSQL, Vault, Auth0)",

    # Legacy markers
    "asyncio: Async tests",
    "database: Requires database",
    "integration: Integration tests",
    "e2e: End-to-end tests",
    "enterprise: Enterprise features",
    "slow: Slow-running tests",
    "forked: Requires process isolation",
]
```

## Best Practices

### When to Use Markers

1. **Always use service markers** for integration tests
2. **Use legacy markers** for additional categorization
3. **Combine markers** when tests have multiple requirements
4. **Use skipif** for optional external dependencies

### Marker Selection Guidelines

| Test Type | Markers |
|-----------|---------|
| Unit test | None (usually) |
| PostgreSQL integration | `@pytest.mark.requires_postgres` |
| Vault encryption | `@pytest.mark.requires_postgres`<br>`@pytest.mark.requires_vault` |
| Auth0 authentication | `@pytest.mark.requires_postgres`<br>`@pytest.mark.requires_auth0` |
| Full enterprise integration | `@pytest.mark.requires_all` |

### Naming Conventions

- Use `requires_<service>` for service dependencies
- Use descriptive names that explain the requirement
- Keep marker names consistent across the codebase

## Troubleshooting

### Common Issues

**"Marker not recognized"**
```bash
# Check marker definitions
pytest --markers | grep requires_

# Ensure pyproject.toml has the marker
grep "requires_postgres" pyproject.toml
```

**Tests not running with marker**
```bash
# Check marker application
pytest --collect-only -m 'requires_postgres' -q

# Verify test has the marker
grep "@pytest.mark.requires_postgres" tests/path/to/test.py
```

**CI marker filtering not working**
```bash
# Test the exact command locally
pytest -m 'requires_postgres and not requires_vault and not requires_auth0' --collect-only
```

### Debugging Marker Issues

```bash
# List all markers in use
pytest --collect-only | grep -E "test_.*\[" | sed 's/.*\[//' | sed 's/\].*//' | sort | uniq

# Find tests without markers
pytest --collect-only | grep "test_" | grep -v "::" | grep -v "\["
```

## Migration Guide

When adding markers to existing tests:

1. **Identify test dependencies** by examining fixtures and imports
2. **Add appropriate markers** based on service usage
3. **Test marker application** with `pytest --collect-only`
4. **Update CI configuration** if new markers are added
5. **Document marker usage** in test docstrings

### Example Migration

**Before:**
```python
def test_database_query(db_connection):
    # Uses db_connection fixture → needs PostgreSQL
    pass
```

**After:**
```python
@pytest.mark.requires_postgres
def test_database_query(db_connection):
    """Test database query functionality."""
    pass
```

## Future Extensions

### Potential New Markers

- `@pytest.mark.requires_redis` - For Redis caching tests
- `@pytest.mark.requires_kafka` - For event streaming tests
- `@pytest.mark.performance` - For performance regression tests
- `@pytest.mark.flaky` - For tests that occasionally fail

### Marker Evolution

As the codebase grows, markers may evolve:

- Split markers for different PostgreSQL features
- Add markers for specific authentication methods
- Create markers for different test execution environments

## Reference

- [Pytest Markers Documentation](https://docs.pytest.org/en/stable/how-to/mark.html)
- [FraiseQL CI Architecture](./ci-architecture.md)
- [Config Fixtures](./config-fixtures.md)
