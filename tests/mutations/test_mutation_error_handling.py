"""Test mutation error handling in production mode."""

import pytest

from fraiseql.fastapi.config import FraiseQLConfig


@pytest.mark.asyncio
async def test_production_config_environment_check():
    """Test that production config properly sets environment attribute."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )

    # Verify the config has the environment attribute and it's accessible
    assert hasattr(config, "environment")
    assert config.environment == "production"

    # This should NOT raise AttributeError (the bug we're testing)
    try:
        # This is the pattern used in the error handling code
        hide_errors = config.environment == "production"
        assert hide_errors is True
    except AttributeError as e:
        pytest.fail(f"Config environment access raised AttributeError: {e}")


@pytest.mark.asyncio
async def test_development_config_environment_check():
    """Test that development config properly sets environment attribute."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="development"
    )

    # Verify the config has the environment attribute and it's accessible
    assert hasattr(config, "environment")
    assert config.environment == "development"

    # This should NOT raise AttributeError
    try:
        # This is the pattern used in the error handling code
        hide_errors = config.environment == "production"
        assert hide_errors is False
    except AttributeError as e:
        pytest.fail(f"Config environment access raised AttributeError: {e}")


@pytest.mark.asyncio
async def test_config_no_get_method():
    """Test that config object doesn't have .get(): method (ensuring we don't use it)."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )

    # The old broken code tried to use config.get() - ensure this doesn't exist
    assert not hasattr(config, "get"), "Config should not have dictionary-style .get() method"
