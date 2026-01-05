"""Unit tests for AxumFraiseQLConfig."""

import os
import pytest
from pydantic import ValidationError

from fraiseql.axum.config import AxumFraiseQLConfig


class TestAxumFraiseQLConfigCreation:
    """Test creating AxumFraiseQLConfig instances."""

    def test_minimal_config(self) -> None:
        """Test creating config with only required parameters."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        assert config.database_url == "postgresql://localhost/test"
        assert config.axum_host == "127.0.0.1"
        assert config.axum_port == 8000
        assert config.environment == "development"
        assert config.production_mode is False

    def test_full_config(self) -> None:
        """Test creating config with all parameters."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://user:pass@host/db",
            database_pool_size=20,
            database_pool_timeout=60,
            database_max_overflow=30,
            environment="production",
            production_mode=True,
            enable_introspection=False,
            enable_playground=False,
            axum_host="0.0.0.0",
            axum_port=3000,
            axum_workers=4,
            cors_origins=["https://example.com"],
        )

        assert config.database_url == "postgresql://user:pass@host/db"
        assert config.database_pool_size == 20
        assert config.database_pool_timeout == 60
        assert config.database_max_overflow == 30
        assert config.environment == "production"
        assert config.production_mode is True
        assert config.enable_introspection is False
        assert config.enable_playground is False
        assert config.axum_host == "0.0.0.0"
        assert config.axum_port == 3000
        assert config.axum_workers == 4
        assert config.cors_origins == ["https://example.com"]

    def test_default_values(self) -> None:
        """Test that all defaults are set correctly."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        # Database
        assert config.database_pool_size == 10
        assert config.database_pool_timeout == 30
        assert config.database_max_overflow == 20

        # Environment
        assert config.environment == "development"
        assert config.production_mode is False

        # GraphQL
        assert config.enable_introspection is True
        assert config.enable_playground is True
        assert config.playground_tool == "graphiql"
        assert config.max_query_depth == 10

        # Security
        assert config.auth_enabled is False
        assert config.jwt_secret is None
        assert config.jwt_algorithm == "HS256"

        # Performance
        assert config.enable_query_caching is False
        assert config.cache_ttl == 300

        # Error handling
        assert config.hide_error_details is False

        # Axum
        assert config.axum_host == "127.0.0.1"
        assert config.axum_port == 8000
        assert config.axum_workers is None
        assert config.axum_metrics_token == ""

        # CORS
        assert config.cors_origins is None
        assert config.cors_allow_credentials is True
        assert config.cors_allow_methods is None
        assert config.cors_allow_headers is None

        # Compression
        assert config.enable_compression is True
        assert config.compression_algorithm == "brotli"
        assert config.compression_min_bytes == 256


class TestAxumFraiseQLConfigValidation:
    """Test configuration validation."""

    def test_invalid_database_url(self) -> None:
        """Test that invalid database URLs are rejected."""
        with pytest.raises(ValidationError) as exc_info:
            AxumFraiseQLConfig(database_url="mysql://localhost/test")

        assert "Database URL must start with" in str(exc_info.value)

    def test_database_url_required(self) -> None:
        """Test that database_url is required."""
        with pytest.raises(ValidationError) as exc_info:
            AxumFraiseQLConfig()  # type: ignore

        errors = exc_info.value.errors()
        assert any(err["loc"] == ("database_url",) for err in errors)

    def test_invalid_environment(self) -> None:
        """Test that invalid environment is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                environment="invalid"
            )

        assert "String should match pattern" in str(exc_info.value)

    def test_invalid_playground_tool(self) -> None:
        """Test that invalid playground tool is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                playground_tool="invalid"
            )

        assert "String should match pattern" in str(exc_info.value)

    def test_invalid_compression_algorithm(self) -> None:
        """Test that invalid compression algorithm is rejected."""
        with pytest.raises(ValidationError) as exc_info:
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                compression_algorithm="invalid"
            )

        assert "String should match pattern" in str(exc_info.value)

    def test_invalid_pool_size(self) -> None:
        """Test that invalid pool size is rejected."""
        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                database_pool_size=0
            )

        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                database_pool_size=101
            )

    def test_invalid_port(self) -> None:
        """Test that invalid port is rejected."""
        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                axum_port=0
            )

        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                axum_port=65536
            )

    def test_invalid_cors_origins(self) -> None:
        """Test that invalid CORS origins are rejected."""
        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                cors_origins=["invalid-url"]
            )

    def test_valid_cors_origins(self) -> None:
        """Test that valid CORS origins are accepted."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            cors_origins=[
                "https://example.com",
                "http://localhost:3000",
                "*"
            ]
        )

        assert config.cors_origins == [
            "https://example.com",
            "http://localhost:3000",
            "*"
        ]

    def test_invalid_workers(self) -> None:
        """Test that invalid workers is rejected."""
        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                axum_workers=-1
            )

        with pytest.raises(ValidationError):
            AxumFraiseQLConfig(
                database_url="postgresql://localhost/test",
                axum_workers=0
            )


class TestAxumFraiseQLConfigProperties:
    """Test configuration properties."""

    def test_effective_workers_with_explicit_value(self) -> None:
        """Test effective_workers when explicitly set."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_workers=4
        )

        assert config.effective_workers == 4

    def test_effective_workers_auto_detect(self) -> None:
        """Test effective_workers auto-detects from CPU count."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_workers=None
        )

        # Should be >= 1 and reasonable (typically 4-16)
        assert config.effective_workers >= 1
        assert config.effective_workers <= 128

    def test_server_url(self) -> None:
        """Test server_url property."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",
            axum_port=3000
        )

        assert config.server_url == "http://0.0.0.0:3000"

    def test_server_url_default(self) -> None:
        """Test server_url with defaults."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        assert config.server_url == "http://127.0.0.1:8000"


class TestAxumFraiseQLConfigEnvironment:
    """Test loading config from environment variables."""

    def test_from_env_required_only(self) -> None:
        """Test creating config from environment with only required vars."""
        os.environ["FRAISEQL_DATABASE_URL"] = "postgresql://localhost/test_env"

        try:
            config = AxumFraiseQLConfig.from_env()
            assert config.database_url == "postgresql://localhost/test_env"
            assert config.environment == "development"
            assert config.axum_host == "127.0.0.1"
        finally:
            del os.environ["FRAISEQL_DATABASE_URL"]

    def test_from_env_all_vars(self) -> None:
        """Test creating config from environment with all vars."""
        env_vars = {
            "FRAISEQL_DATABASE_URL": "postgresql://prod/db",
            "FRAISEQL_ENV": "production",
            "FRAISEQL_HOST": "0.0.0.0",
            "FRAISEQL_PORT": "3000",
            "FRAISEQL_WORKERS": "8",
            "FRAISEQL_AUTH_ENABLED": "true",
            "FRAISEQL_JWT_SECRET": "secret123",
            "FRAISEQL_PRODUCTION": "true",
        }

        for key, value in env_vars.items():
            os.environ[key] = value

        try:
            config = AxumFraiseQLConfig.from_env()
            assert config.database_url == "postgresql://prod/db"
            assert config.environment == "production"
            assert config.axum_host == "0.0.0.0"
            assert config.axum_port == 3000
            assert config.axum_workers == 8
            assert config.auth_enabled is True
            assert config.jwt_secret == "secret123"
            assert config.production_mode is True
        finally:
            for key in env_vars:
                if key in os.environ:
                    del os.environ[key]

    def test_from_env_missing_required(self) -> None:
        """Test that from_env raises error when required var missing."""
        # Ensure FRAISEQL_DATABASE_URL is not set
        if "FRAISEQL_DATABASE_URL" in os.environ:
            del os.environ["FRAISEQL_DATABASE_URL"]

        with pytest.raises(ValueError) as exc_info:
            AxumFraiseQLConfig.from_env()

        assert "FRAISEQL_DATABASE_URL" in str(exc_info.value)


class TestAxumFraiseQLConfigSerialization:
    """Test config serialization."""

    def test_to_dict(self) -> None:
        """Test converting config to dictionary."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",
            axum_port=3000
        )

        config_dict = config.to_dict()

        assert isinstance(config_dict, dict)
        assert config_dict["database_url"] == "postgresql://localhost/test"
        assert config_dict["axum_host"] == "0.0.0.0"
        assert config_dict["axum_port"] == 3000

    def test_to_dict_excludes_none(self) -> None:
        """Test that to_dict excludes None values."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test"
        )

        config_dict = config.to_dict()

        # None values should be excluded
        assert config_dict.get("jwt_secret") is None
        assert config_dict.get("cors_origins") is None

    def test_str_representation(self) -> None:
        """Test string representation."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            axum_host="0.0.0.0",
            axum_port=3000
        )

        str_repr = str(config)

        assert "AxumFraiseQLConfig" in str_repr
        assert "0.0.0.0" in str_repr
        assert "3000" in str_repr


class TestAxumFraiseQLConfigIntegration:
    """Integration tests for config."""

    def test_config_for_development(self) -> None:
        """Test creating dev config."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/dev_db",
            environment="development",
            enable_introspection=True,
            enable_playground=True,
        )

        assert config.environment == "development"
        assert config.production_mode is False
        assert config.enable_introspection is True
        assert config.enable_playground is True

    def test_config_for_production(self) -> None:
        """Test creating prod config."""
        config = AxumFraiseQLConfig(
            database_url="postgresql://prod-host/prod_db",
            environment="production",
            production_mode=True,
            hide_error_details=True,
            enable_introspection=False,
            enable_playground=False,
            enable_query_caching=True,
            enable_compression=True,
        )

        assert config.environment == "production"
        assert config.production_mode is True
        assert config.hide_error_details is True
        assert config.enable_introspection is False
        assert config.enable_playground is False
        assert config.enable_query_caching is True
        assert config.enable_compression is True
