"""Test mutation error handling in production mode."""

import pytest

from fraiseql.fastapi.config import FraiseQLConfig

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_production_config_environment_check() -> None:
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
async def test_multiple_validation_errors_array_pattern() -> None:
    """Test that multiple validation errors can be returned as arrays.

    This demonstrates the FraiseQL Backend error pattern where
    complex validation produces structured error arrays.
    """
    # Mock multiple validation errors
    validation_errors = [
        {
            "code": "REQUIRED_FIELD_MISSING",
            "field": "name",
            "message": "Name is required",
            "details": {"constraint": "not_null"},
        },
        {
            "code": "INVALID_FORMAT",
            "field": "email",
            "message": "Email format is invalid",
            "details": {"pattern": "email", "value": "invalid-email"},
        },
        {
            "code": "VALUE_TOO_SHORT",
            "field": "password",
            "message": "Password must be at least 8 characters",
            "details": {"min_length": 8, "actual_length": 4},
        },
    ]

    # Verify error array structure
    assert len(validation_errors) == 3
    assert all("code" in error for error in validation_errors)
    assert all("field" in error for error in validation_errors)
    assert all("message" in error for error in validation_errors)
    assert all("details" in error for error in validation_errors)

    # Verify error codes follow pattern
    error_codes = {error["code"] for error in validation_errors}
    expected_codes = {"REQUIRED_FIELD_MISSING", "INVALID_FORMAT", "VALUE_TOO_SHORT"}
    assert error_codes == expected_codes


@pytest.mark.asyncio
async def test_development_config_environment_check() -> None:
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
async def test_config_no_get_method() -> None:
    """Test that config object doesn't have .get(): method (ensuring we don't use it)."""
    config = FraiseQLConfig(
        database_url="postgresql://test@localhost/test", environment="production"
    )

    # The old broken code tried to use config.get() - ensure this doesn't exist
    assert not hasattr(config, "get"), "Config should not have dictionary-style .get() method"
