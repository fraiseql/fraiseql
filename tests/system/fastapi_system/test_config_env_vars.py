"""Test environment variable handling in FraiseQLConfig."""

import pytest

from fraiseql.fastapi.config import FraiseQLConfig

pytestmark = pytest.mark.integration


@pytest.mark.unit
def test_config_ignores_non_prefixed_env_vars(monkeypatch) -> None:
    """Test that config ignores environment variables without FRAISEQL_ prefix."""
    # Set common environment variables that might conflict
    monkeypatch.setenv("ENV", "local")
    monkeypatch.setenv("DEBUG", "true")
    monkeypatch.setenv("DB_USER", "myuser")
    monkeypatch.setenv("DB_PASSWORD", "mypass")
    monkeypatch.setenv("APP_NAME", "MyApp")
    monkeypatch.setenv("ENVIRONMENT", "staging")

    # Set required FraiseQL environment variable
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")

    # Create config - should not raise validation error
    config = FraiseQLConfig()

    # Verify config uses default values, not the non-prefixed env vars
    assert config.environment == "development"  # Not "staging" from ENVIRONMENT
    assert config.app_name == "FraiseQL API"  # Not "MyApp" from APP_NAME
    assert config.database_url == "postgresql://localhost/test"


def test_config_uses_prefixed_env_vars(monkeypatch) -> None:
    """Test that config correctly uses FRAISEQL_ prefixed variables."""
    # Set prefixed environment variables
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://dbuser:dbpass@dbhost/dbname")
    monkeypatch.setenv("FRAISEQL_ENVIRONMENT", "production")
    monkeypatch.setenv("FRAISEQL_APP_NAME", "My FraiseQL App")
    monkeypatch.setenv("FRAISEQL_DATABASE_POOL_SIZE", "50")
    monkeypatch.setenv("FRAISEQL_ENABLE_INTROSPECTION", "false")

    # Create config
    config = FraiseQLConfig()

    # Verify values from environment
    assert config.database_url == "postgresql://dbuser:dbpass@dbhost/dbname"
    assert config.environment == "production"
    assert config.app_name == "My FraiseQL App"
    assert config.database_pool_size == 50
    assert config.enable_introspection is False


def test_config_case_insensitive(monkeypatch) -> None:
    """Test that config handles case variations in env var names."""
    # Mix of cases
    monkeypatch.setenv("fraiseql_database_url", "postgresql://localhost/test1")
    monkeypatch.setenv("FRAISEQL_APP_NAME", "Test App")
    monkeypatch.setenv("FraisEQL_Environment", "testing")

    config = FraiseQLConfig()

    assert config.database_url == "postgresql://localhost/test1"
    assert config.app_name == "Test App"
    assert config.environment == "testing"


def test_config_without_env_file(monkeypatch, tmp_path) -> None:
    """Test that config works without .env file."""
    # Change to a directory without .env file
    monkeypatch.chdir(tmp_path)

    # Set required env var
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")

    # Should work without .env file
    config = FraiseQLConfig()
    assert config.database_url == "postgresql://localhost/test"


def test_config_validation_errors() -> None:
    """Test that config raises appropriate validation errors."""
    # Missing required database_url
    with pytest.raises(ValueError, match="database_url"):
        FraiseQLConfig()

    # Invalid environment value
    with pytest.raises(ValueError, match="environment"):
        FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="invalid",  # Not in allowed values
        )


def test_production_defaults(monkeypatch) -> None:
    """Test that production environment sets appropriate defaults."""
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
    monkeypatch.setenv("FRAISEQL_ENVIRONMENT", "production")

    config = FraiseQLConfig()

    # In production, these should be disabled by validators
    assert config.enable_introspection is False
    assert config.enable_playground is False


def test_auth0_validation(monkeypatch) -> None:
    """Test Auth0 configuration validation."""
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
    monkeypatch.setenv("FRAISEQL_AUTH_PROVIDER", "auth0")

    # Should fail without auth0_domain
    with pytest.raises(ValueError, match="auth0_domain is required"):
        FraiseQLConfig()

    # Should work with auth0_domain
    monkeypatch.setenv("FRAISEQL_AUTH0_DOMAIN", "example.auth0.com")
    config = FraiseQLConfig()
    assert config.auth_provider == "auth0"
    assert config.auth0_domain == "example.auth0.com"
