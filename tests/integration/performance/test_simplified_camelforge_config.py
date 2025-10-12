import pytest

"""Test the simplified CamelForge configuration approach.

Updated for v0.11.0: CamelForge is now always enabled at the framework level.
The camelforge_enabled flag has been removed from FraiseQLConfig.
"""

import os

from fraiseql.fastapi.camelforge_config import CamelForgeConfig
from fraiseql.fastapi.config import FraiseQLConfig


@pytest.mark.camelforge
class TestSimplifiedCamelForgeConfig:
    """Test the simplified configuration approach."""

    def test_config_defaults(self):
        """Test default configuration values.

        v0.11.0: CamelForge is always enabled, camelforge_enabled flag removed.
        """
        config = FraiseQLConfig(database_url="postgresql://test@localhost/test")

        # CamelForge is always enabled in v0.11.0+
        assert config.camelforge_function == "turbo.fn_camelforge"
        assert config.camelforge_field_threshold == 20

    def test_config_explicit_values(self):
        """Test setting explicit configuration values."""
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_function="custom.fn_camelforge",
            camelforge_field_threshold=30,
        )

        assert config.camelforge_function == "custom.fn_camelforge"
        assert config.camelforge_field_threshold == 30

    def test_camelforge_config_create(self):
        """Test CamelForgeConfig.create() method.

        v0.11.0: CamelForgeConfig still has enabled parameter for per-query control.
        Note: Framework passes enabled=True by default, but the class itself defaults to False.
        """
        # Test defaults (class default is False, but framework passes True)
        cf_config = CamelForgeConfig.create()
        assert cf_config.enabled is False  # Class default
        assert cf_config.function == "turbo.fn_camelforge"
        assert cf_config.field_threshold == 20

        # Test explicit values (how framework uses it)
        cf_config = CamelForgeConfig.create(
            enabled=True,  # Framework always passes True
            function="custom.fn_camelforge",
            field_threshold=30,
        )
        assert cf_config.enabled is True
        assert cf_config.function == "custom.fn_camelforge"
        assert cf_config.field_threshold == 30

        # Test that it can still be disabled for specific queries if needed
        cf_config = CamelForgeConfig.create(enabled=False)
        assert cf_config.enabled is False

    def test_environment_variable_overrides(self):
        """Test that environment variables override config values."""
        # Set environment variables
        os.environ["FRAISEQL_CAMELFORGE_ENABLED"] = "true"
        os.environ["FRAISEQL_CAMELFORGE_FUNCTION"] = "env.fn_camelforge"
        os.environ["FRAISEQL_CAMELFORGE_FIELD_THRESHOLD"] = "50"

        try:
            # Config says disabled, but env var should override
            cf_config = CamelForgeConfig.create(
                enabled=False,  # This should be overridden
                function="config.fn_camelforge",  # This should be overridden
                field_threshold=20,  # This should be overridden
            )

            assert cf_config.enabled is True  # Overridden by env var
            assert cf_config.function == "env.fn_camelforge"  # Overridden by env var
            assert cf_config.field_threshold == 50  # Overridden by env var

        finally:
            # Clean up environment variables
            del os.environ["FRAISEQL_CAMELFORGE_ENABLED"]
            del os.environ["FRAISEQL_CAMELFORGE_FUNCTION"]
            del os.environ["FRAISEQL_CAMELFORGE_FIELD_THRESHOLD"]

    def test_invalid_environment_values(self):
        """Test handling of invalid environment variable values."""
        # Set invalid environment variables
        os.environ["FRAISEQL_CAMELFORGE_ENABLED"] = "invalid"
        os.environ["FRAISEQL_CAMELFORGE_FIELD_THRESHOLD"] = "not_a_number"

        try:
            cf_config = CamelForgeConfig.create(
                enabled=False,  # Should be used as fallback for invalid env var
                field_threshold=25,  # Should be used as fallback
            )

            # Invalid boolean should fall back to provided default (False)
            assert cf_config.enabled is False
            # Invalid integer should use the provided default
            assert cf_config.field_threshold == 25

        finally:
            # Clean up environment variables
            del os.environ["FRAISEQL_CAMELFORGE_ENABLED"]
            del os.environ["FRAISEQL_CAMELFORGE_FIELD_THRESHOLD"]

    def test_simple_usage_examples(self):
        """Test the simplified usage examples from the documentation.

        v0.11.0: CamelForge is always enabled, examples updated accordingly.
        """
        # Example 1: Simple config (CamelForge always enabled)
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
        )
        # CamelForge settings are always available
        assert config.camelforge_function == "turbo.fn_camelforge"

        # Example 2: Custom CamelForge function
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_function="custom.fn_camelforge",
        )
        assert config.camelforge_function == "custom.fn_camelforge"

        # Example 3: Environment variable override
        os.environ["FRAISEQL_CAMELFORGE_FUNCTION"] = "env.fn_camelforge"
        try:
            # This would happen in dependencies.py
            cf_config = CamelForgeConfig.create(
                enabled=True,  # Always enabled in v0.11.0+
                function=config.camelforge_function,
                field_threshold=config.camelforge_field_threshold,
            )

            assert cf_config.enabled is True
            assert cf_config.function == "env.fn_camelforge"  # Environment variable wins

        finally:
            del os.environ["FRAISEQL_CAMELFORGE_FUNCTION"]

    def test_no_conflicting_configuration_sources(self):
        """Test that there are no conflicting configuration sources.

        v0.11.0: Simplified even further - CamelForge always enabled.
        """
        # v0.11.0: Simple hierarchy
        # 1. Environment variables (FRAISEQL_CAMELFORGE_*)
        # 2. Config parameters
        # 3. Defaults

        # CamelForge always enabled - only function and threshold are configurable
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_function="config.fn_camelforge",
        )

        # No environment variables set - should use config values
        cf_config = CamelForgeConfig.create(
            enabled=True,  # Always enabled in v0.11.0+
            function=config.camelforge_function,
        )

        assert cf_config.enabled is True
        assert cf_config.function == "config.fn_camelforge"

        # Set environment variable - should override config
        os.environ["FRAISEQL_CAMELFORGE_FUNCTION"] = "env.fn_camelforge"

        try:
            cf_config = CamelForgeConfig.create(
                enabled=True,
                function=config.camelforge_function,  # Should be overridden
            )

            assert cf_config.enabled is True
            assert cf_config.function == "env.fn_camelforge"  # From env var

        finally:
            del os.environ["FRAISEQL_CAMELFORGE_FUNCTION"]
