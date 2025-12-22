# Config Fixtures

FraiseQL provides pre-configured `FraiseQLConfig` fixtures to ensure consistent test configurations and eliminate environment variable dependencies.

## Overview

Instead of manually creating `FraiseQLConfig` instances in tests, use the provided fixtures that encapsulate common configuration patterns.

```python
# ❌ Avoid: Direct config creation
def test_something():
    config = FraiseQLConfig(database_url="...", environment="testing")

# ✅ Prefer: Use fixtures
def test_something(test_config):
    assert test_config.environment == "testing"
```

## Available Fixtures

### Core Fixtures

| Fixture | Environment | Purpose | Key Settings |
|---------|-------------|---------|--------------|
| `test_config` | testing | Default test configuration | Safe defaults, playground enabled |
| `development_config` | development | Development environment | Debug features, permissive CORS |
| `production_config` | production | Production-like setup | Security hardened, features disabled |
| `custom_config` | flexible | Factory for custom configs | Build your own configuration |

### Specialized Fixtures

| Fixture | Purpose | Key Features |
|---------|---------|--------------|
| `apq_required_config` | APQ in required mode | Forces persisted queries only |
| `apq_disabled_config` | APQ completely disabled | No automatic persisted queries |
| `vault_kms_config` | Vault KMS integration | Requires Vault to be available |

## Fixture Details

### test_config

**Default testing configuration with safe, predictable settings.**

```python
@pytest.fixture
def test_config(postgres_url: str):
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        auth_enabled=False,           # No authentication
        enable_playground=True,       # GraphQL playground available
        introspection_policy="public", # Full introspection allowed
        apq_storage_backend="memory", # In-memory APQ storage
        apq_mode="optional",          # APQ optional (can use or ignore)
    )
```

**Use for:** Most integration tests, general functionality testing.

**Example:**
```python
def test_basic_query(test_config):
    """Test basic GraphQL query functionality."""
    app = create_fraiseql_app(config=test_config)
    # Test code...
```

### development_config

**Configuration mimicking local development environment.**

```python
@pytest.fixture
def development_config(postgres_url: str):
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="development",
        auth_enabled=False,           # No auth for easier development
        enable_playground=True,       # Playground for exploration
        introspection_policy="public", # Full introspection
        cors_enabled=True,            # CORS for frontend development
        cors_origins=["http://localhost:3000"],  # Common dev origins
    )
```

**Use for:** Testing development-specific behaviors, CORS handling, debug features.

**Example:**
```python
def test_cors_in_development(development_config):
    """Test CORS behavior in development."""
    assert development_config.cors_enabled is True
    assert "http://localhost:3000" in development_config.cors_origins
```

### production_config

**Configuration mimicking production environment with security hardening.**

```python
@pytest.fixture
def production_config(postgres_url: str):
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="production",
        auth_enabled=True,            # Authentication required
        auth_provider="auth0",        # Auth0 authentication
        auth0_domain="test.auth0.com",
        auth0_api_identifier="https://api.test.com",
        enable_playground=False,      # Auto-disabled in production
        introspection_policy="disabled", # Auto-disabled in production
        cors_enabled=False,           # Minimal CORS in production
    )
```

**Use for:** Testing production security features, authentication requirements, feature disabling.

**Example:**
```python
def test_production_security(production_config):
    """Test that playground is disabled in production."""
    assert production_config.enable_playground is False
    assert production_config.introspection_policy == "disabled"
```

### apq_required_config

**Configuration with Automatic Persisted Queries (APQ) in required mode.**

```python
@pytest.fixture
def apq_required_config(postgres_url: str):
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        apq_storage_backend="postgresql",  # Database storage
        apq_mode="required",               # APQ mandatory
        auth_enabled=False,
    )
```

**Use for:** Testing APQ security, ensuring only persisted queries are allowed.

**Example:**
```python
def test_apq_required_mode(apq_required_config):
    """Test that APQ is required."""
    assert apq_required_config.apq_mode == "required"
    assert apq_required_config.apq_storage_backend == "postgresql"
```

### apq_disabled_config

**Configuration with Automatic Persisted Queries completely disabled.**

```python
@pytest.fixture
def apq_disabled_config(postgres_url: str):
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        apq_mode="disabled",  # No APQ at all
        auth_enabled=False,
    )
```

**Use for:** Testing behavior without APQ, ensuring queries work without persistence.

**Example:**
```python
def test_without_apq(apq_disabled_config):
    """Test behavior when APQ is disabled."""
    assert apq_disabled_config.apq_mode == "disabled"
```

### vault_kms_config

**Configuration with Vault KMS encryption enabled.**

```python
@pytest.fixture
def vault_kms_config(postgres_url: str):
    import os
    if not os.environ.get("VAULT_ADDR"):
        pytest.skip("Vault not available")

    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        auth_enabled=False,
        # Vault KMS settings would be configured here
    )
```

**Use for:** Testing Vault KMS integration, encryption features.

**Example:**
```python
@pytest.mark.requires_vault
def test_vault_encryption(vault_kms_config):
    """Test data encryption with Vault KMS."""
    # Requires Vault to be running
    pass
```

### custom_config

**Factory fixture for creating custom configurations.**

```python
@pytest.fixture
def custom_config(postgres_url: str):
    def _create_config(**kwargs):
        defaults = {
            "database_url": postgres_url,
            "environment": "testing",
            "auth_enabled": False,
        }
        defaults.update(kwargs)
        return FraiseQLConfig(**defaults)

    return _create_config
```

**Use for:** Tests requiring specific configuration combinations not covered by other fixtures.

**Example:**
```python
def test_custom_apq_settings(custom_config):
    """Test custom APQ configuration."""
    config = custom_config(
        apq_mode="required",
        apq_storage_backend="redis",
        max_query_depth=10
    )

    assert config.apq_mode == "required"
    assert config.apq_storage_backend == "redis"
    assert config.max_query_depth == 10
```

## Usage Patterns

### Basic Integration Test

```python
@pytest.mark.requires_postgres
def test_user_operations(test_config, db_connection):
    """Test basic user CRUD operations."""
    # test_config provides consistent settings
    # db_connection provides database access
    pass
```

### Environment-Specific Behavior

```python
def test_development_features(development_config):
    """Test features only available in development."""
    assert development_config.enable_playground is True

def test_production_security(production_config):
    """Test security features in production."""
    assert production_config.auth_enabled is True
```

### APQ Testing

```python
def test_apq_required_security(apq_required_config):
    """Test that APQ required mode enforces security."""
    # Only persisted queries allowed
    pass

def test_apq_disabled_fallback(apq_disabled_config):
    """Test fallback when APQ is disabled."""
    # All queries must be sent fresh
    pass
```

### Custom Configuration

```python
def test_rate_limiting(custom_config):
    """Test rate limiting with custom config."""
    config = custom_config(
        rate_limit_enabled=True,
        rate_limit_requests_per_minute=60
    )

    assert config.rate_limit_enabled is True
    assert config.rate_limit_requests_per_minute == 60
```

## Best Practices

### Choosing the Right Fixture

1. **Use `test_config`** for most tests - it provides safe, predictable defaults
2. **Use `development_config`** when testing development-specific features
3. **Use `production_config`** when testing security or production behaviors
4. **Use specialized fixtures** (`apq_*`, `vault_*`) for specific feature testing
5. **Use `custom_config`** only when other fixtures don't fit your needs

### Fixture Guidelines

- **Don't modify fixtures** - they're shared across tests
- **Use appropriate markers** - combine with `@pytest.mark.requires_postgres` etc.
- **Document special requirements** - explain why a custom config is needed
- **Keep tests focused** - one fixture per test when possible

### Common Patterns

```python
# ✅ Good: Clear, focused test
@pytest.mark.requires_postgres
def test_query_execution(test_config, db_connection):
    pass

# ❌ Avoid: Over-customization
def test_something(custom_config):
    config = custom_config(
        # 10+ custom settings - too complex
    )
```

## Troubleshooting

### Fixture Not Found

**Error:** `fixture 'test_config' not found`

**Solution:** Ensure you're importing the fixture or it's in `conftest.py`:

```python
# In conftest.py or test file
from tests.fixtures.config.conftest import test_config
```

### Config Validation Errors

**Error:** Pydantic validation errors when using fixtures

**Solution:** Check that the fixture provides all required fields:

```python
# Debug fixture contents
def test_debug_config(test_config):
    print(f"Environment: {test_config.environment}")
    print(f"Database URL: {test_config.database_url}")
    # Add debug prints to understand fixture contents
```

### Environment Conflicts

**Error:** Tests fail due to environment variable conflicts

**Solution:** Fixtures override environment variables - don't rely on `FRAISEQL_ENVIRONMENT`:

```python
# ❌ Don't do this
def test_env_var():
    config = FraiseQLConfig(database_url="...")  # Uses env vars

# ✅ Do this
def test_explicit_config(test_config):
    # test_config has explicit environment="testing"
    pass
```

## Migration Guide

### From Environment Variables

**Before:**
```python
def test_something():
    # Relies on FRAISEQL_ENVIRONMENT=testing
    config = FraiseQLConfig(database_url="postgresql://...")
```

**After:**
```python
def test_something(test_config):
    # Explicit, no environment dependencies
    assert test_config.environment == "testing"
```

### From Manual Config Creation

**Before:**
```python
def test_production_behavior():
    config = FraiseQLConfig(
        database_url="postgresql://...",
        environment="production",
        auth_enabled=True,
        enable_playground=False,
        # Many more settings...
    )
```

**After:**
```python
def test_production_behavior(production_config):
    # All production settings pre-configured
    assert production_config.auth_enabled is True
```

## Extending Fixtures

### Adding New Fixtures

To add a new fixture for a specific use case:

```python
# In tests/fixtures/config/conftest.py
@pytest.fixture
def my_special_config(postgres_url: str):
    """Configuration for my special test case."""
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        # Special settings here
        my_special_feature=True,
    )
```

### Modifying Existing Fixtures

**Don't modify existing fixtures** - they might break other tests. Instead:

```python
# ✅ Create a new fixture
@pytest.fixture
def modified_test_config(test_config):
    # Start with test_config and modify
    test_config.my_setting = "modified"
    return test_config

# ❌ Don't do this
@pytest.fixture
def test_config(postgres_url: str):
    # Modifying the main fixture breaks other tests
    return FraiseQLConfig(...)  # Different from original
```

## Reference

- [CI Architecture](./ci-architecture.md)
- [Pytest Markers](./pytest-markers.md)
