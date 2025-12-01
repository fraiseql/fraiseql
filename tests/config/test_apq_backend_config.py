"""Tests for APQ backend configuration extensions."""

import pytest
from pydantic import ValidationError

from fraiseql.fastapi.config import FraiseQLConfig

pytestmark = pytest.mark.integration


def test_apq_backend_config_defaults() -> None:
    """Test that APQ backend config has sensible defaults."""
    config = FraiseQLConfig(database_url="postgresql://test@localhost/test")

    # Test default values
    assert config.apq_storage_backend == "memory"
    assert config.apq_cache_responses is False
    assert config.apq_response_cache_ttl == 600
    assert config.apq_backend_config == {}


def test_apq_backend_config_memory() -> None:
    """Test memory backend configuration."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
        apq_storage_backend="memory",
        apq_cache_responses=True,
        apq_response_cache_ttl=300,
    )

    assert config.apq_storage_backend == "memory"
    assert config.apq_cache_responses is True
    assert config.apq_response_cache_ttl == 300


def test_apq_backend_config_postgresql() -> None:
    """Test PostgreSQL backend configuration."""
    backend_config = {"table_prefix": "apq_", "connection_pool_size": 10}

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
        apq_storage_backend="postgresql",
        apq_cache_responses=True,
        apq_backend_config=backend_config,
    )

    assert config.apq_storage_backend == "postgresql"
    assert config.apq_cache_responses is True
    assert config.apq_backend_config == backend_config


def test_apq_backend_config_custom() -> None:
    """Test custom backend configuration."""
    backend_config = {
        "backend_class": "myapp.storage.CustomAPQBackend",
        "redis_url": "redis://localhost:6379",
        "key_prefix": "apq:",
    }

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
        apq_storage_backend="custom",
        apq_backend_config=backend_config,
    )

    assert config.apq_storage_backend == "custom"
    assert config.apq_backend_config == backend_config


def test_apq_backend_config_validation() -> None:
    """Test validation of APQ backend config fields."""
    # Valid backend names should work
    # Note: redis was removed in v1.6.0 - use custom backend with redis config instead
    valid_backends = ["memory", "postgresql", "custom"]

    for backend in valid_backends:
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", apq_storage_backend=backend
        )
        assert config.apq_storage_backend == backend

    # Invalid backend names should raise validation error
    with pytest.raises(ValidationError):
        FraiseQLConfig(
            database_url="postgresql://test@localhost/test", apq_storage_backend="invalid_backend"
        )

    # Redis is no longer a built-in backend (use custom instead)
    with pytest.raises(ValidationError):
        FraiseQLConfig(database_url="postgresql://test@localhost/test", apq_storage_backend="redis")


def test_apq_cache_ttl_validation() -> None:
    """Test validation of cache TTL values."""
    # Valid TTL values
    valid_ttls = [0, 1, 300, 3600, 86400]

    for ttl in valid_ttls:
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", apq_response_cache_ttl=ttl
        )
        assert config.apq_response_cache_ttl == ttl

    # Negative TTL should raise validation error
    with pytest.raises(ValidationError):
        FraiseQLConfig(database_url="postgresql://test@localhost/test", apq_response_cache_ttl=-1)


def test_apq_config_environment_specific_defaults() -> None:
    """Test that APQ config has appropriate defaults for different environments."""
    # Development environment
    dev_config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="development"
    )
    assert dev_config.apq_cache_responses is False  # Conservative default

    # Production environment
    prod_config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )
    assert prod_config.apq_cache_responses is False  # Should remain conservative


def test_apq_config_from_environment_variables(monkeypatch: pytest.MonkeyPatch) -> None:
    """Test reading APQ config from environment variables."""
    # Set environment variables
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://test@localhost/test")
    monkeypatch.setenv("FRAISEQL_APQ_STORAGE_BACKEND", "postgresql")
    monkeypatch.setenv("FRAISEQL_APQ_CACHE_RESPONSES", "true")
    monkeypatch.setenv("FRAISEQL_APQ_RESPONSE_CACHE_TTL", "1800")

    config = FraiseQLConfig()

    assert config.apq_storage_backend == "postgresql"
    assert config.apq_cache_responses is True
    assert config.apq_response_cache_ttl == 1800


def test_apq_backend_config_as_dict() -> None:
    """Test that backend config accepts complex dictionary structures."""
    complex_config = {
        "database": {"host": "localhost", "port": 5432, "database": "apq_cache"},
        "tables": {"queries": "persisted_queries", "responses": "cached_responses"},
        "features": {"compression": True, "encryption": False, "ttl_enabled": True},
    }

    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test",
        apq_storage_backend="custom",
        apq_backend_config=complex_config,
    )

    assert config.apq_backend_config == complex_config
    assert config.apq_backend_config["database"]["host"] == "localhost"
    assert config.apq_backend_config["features"]["compression"] is True
