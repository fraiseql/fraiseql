"""Configuration fixtures for testing.

Provides pre-configured FraiseQLConfig instances for different scenarios.
Use these fixtures instead of creating configs directly to ensure consistency.
"""

import pytest
from fraiseql.fastapi import FraiseQLConfig
from fraiseql.fastapi.config import IntrospectionPolicy, APQMode


@pytest.fixture
def test_config(postgres_url: str):
    """Base test configuration with safe defaults.

    This is the default config for most tests. It has:
    - Environment: testing
    - Auth: disabled
    - Playground: enabled
    - Introspection: public
    - APQ: optional mode with memory backend

    Use this for tests that don't need specific config values.

    Example:
        def test_something(test_config):
            assert test_config.environment == "testing"
            assert test_config.auth_enabled is False
    """
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        auth_enabled=False,
        enable_playground=True,
        introspection_policy=IntrospectionPolicy.PUBLIC,
        apq_storage_backend="memory",
        apq_mode=APQMode.OPTIONAL,
    )


@pytest.fixture
def development_config(postgres_url: str):
    """Development environment configuration.

    Mimics local development setup. Use this to test development-specific
    behaviors (e.g., debug logging, permissive CORS).

    Example:
        def test_dev_behavior(development_config):
            assert development_config.environment == "development"
            assert development_config.enable_playground is True
    """
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="development",
        auth_enabled=False,
        enable_playground=True,
        introspection_policy=IntrospectionPolicy.PUBLIC,
        cors_enabled=True,
        cors_origins=["http://localhost:3000"],
    )


@pytest.fixture
def production_config(postgres_url: str):
    """Production-like configuration.

    Mimics production environment with stricter security defaults.
    Use this to test production-specific behaviors (e.g., introspection
    disabled, playground disabled, auth required).

    Example:
        def test_prod_security(production_config):
            assert production_config.environment == "production"
            assert production_config.enable_playground is False
            assert production_config.introspection_policy == "disabled"
    """
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="production",
        auth_enabled=True,
        auth_provider="auth0",
        auth0_domain="test.auth0.com",
        auth0_api_identifier="https://api.test.com",
        enable_playground=False,  # Auto-disabled in production
        introspection_policy=IntrospectionPolicy.DISABLED,  # Auto-disabled in production
        cors_enabled=False,
    )


@pytest.fixture
def apq_required_config(postgres_url: str):
    """Config with APQ in required mode.

    Use this to test APQ security features where only persisted queries
    are allowed (no arbitrary queries).

    Example:
        def test_apq_security(apq_required_config):
            assert apq_required_config.apq_mode == "required"
    """
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        apq_storage_backend="postgresql",
        apq_mode=APQMode.REQUIRED,
        auth_enabled=False,
    )


@pytest.fixture
def apq_disabled_config(postgres_url: str):
    """Config with APQ disabled.

    Use this to test behavior when APQ is completely turned off.

    Example:
        def test_without_apq(apq_disabled_config):
            assert apq_disabled_config.apq_mode == "disabled"
    """
    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        apq_mode=APQMode.DISABLED,
        auth_enabled=False,
    )


@pytest.fixture
def vault_kms_config(postgres_url: str):
    """Config with Vault KMS encryption enabled.

    Use this for tests that need KMS encryption features.
    Only works when VAULT_ADDR is set.

    Example:
        @pytest.mark.requires_vault
        def test_encryption(vault_kms_config):
            # Config ready for KMS operations
            pass
    """
    import os

    if not os.environ.get("VAULT_ADDR"):
        pytest.skip("Vault not available (VAULT_ADDR not set)")

    return FraiseQLConfig(
        database_url=postgres_url,
        environment="testing",
        auth_enabled=False,
        # Vault KMS settings would go here
        # (depends on your implementation)
    )


@pytest.fixture
def custom_config(postgres_url: str):
    """Factory fixture for custom config creation.

    Use this when you need specific config values that aren't covered
    by other fixtures.

    Example:
        def test_custom(custom_config):
            config = custom_config(
                environment="testing",
                max_query_depth=5,
                cache_ttl=600
            )
            assert config.max_query_depth == 5
    """

    def _create_config(**kwargs):
        # Merge with defaults
        defaults = {
            "database_url": postgres_url,
            "environment": "testing",
            "auth_enabled": False,
        }
        defaults.update(kwargs)
        return FraiseQLConfig(**defaults)

    return _create_config
