import pytest

"""Test the simplified CamelForge configuration approach."""

import os

from fraiseql.fastapi.camelforge_config import CamelForgeConfig
from fraiseql.fastapi.config import FraiseQLConfig



@pytest.mark.camelforge
class TestSimplifiedCamelForgeConfig:
    """Test the simplified configuration approach."""

    def test_config_defaults(self):
        """Test default configuration values."""
        config = FraiseQLConfig(database_url="postgresql://test@localhost/test")

        # CamelForge should be disabled by default
        assert config.camelforge_enabled is False
        assert config.camelforge_function == "turbo.fn_camelforge"
        assert config.camelforge_field_threshold == 20

    def test_config_explicit_values(self):
        """Test setting explicit configuration values."""
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=True,
            camelforge_function="custom.fn_camelforge",
            camelforge_field_threshold=30,
        )

        assert config.camelforge_enabled is True
        assert config.camelforge_function == "custom.fn_camelforge"
        assert config.camelforge_field_threshold == 30

    def test_camelforge_config_create(self):
        """Test CamelForgeConfig.create() method."""
        # Test defaults
        cf_config = CamelForgeConfig.create()
        assert cf_config.enabled is False
        assert cf_config.function == "turbo.fn_camelforge"
        assert cf_config.field_threshold == 20

        # Test explicit values
        cf_config = CamelForgeConfig.create(
            enabled=True,
            function="custom.fn_camelforge",
            field_threshold=30,
        )
        assert cf_config.enabled is True
        assert cf_config.function == "custom.fn_camelforge"
        assert cf_config.field_threshold == 30

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
                enabled=True,  # Should be used as fallback
                field_threshold=25,  # Should be used as fallback
            )

            # Invalid boolean should default to False
            assert cf_config.enabled is False
            # Invalid integer should use the provided default
            assert cf_config.field_threshold == 25

        finally:
            # Clean up environment variables
            del os.environ["FRAISEQL_CAMELFORGE_ENABLED"]
            del os.environ["FRAISEQL_CAMELFORGE_FIELD_THRESHOLD"]

    def test_simple_usage_examples(self):
        """Test the simplified usage examples from the documentation."""
        # Example 1: Simple enable via config
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=True,
        )
        assert config.camelforge_enabled is True

        # Example 2: Environment variable override
        os.environ["FRAISEQL_CAMELFORGE_ENABLED"] = "true"
        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                camelforge_enabled=False,  # Should be overridden
            )

            # This would happen in dependencies.py
            cf_config = CamelForgeConfig.create(
                enabled=config.camelforge_enabled,
                function=config.camelforge_function,
                field_threshold=config.camelforge_field_threshold,
            )

            assert cf_config.enabled is True  # Environment variable wins

        finally:
            del os.environ["FRAISEQL_CAMELFORGE_ENABLED"]

    def test_no_conflicting_configuration_sources(self):
        """Test that there are no conflicting configuration sources."""
        # Before: multiple sources could conflict
        # camelforge_enabled (config) vs FRAISEQL_CAMELFORGE_BETA (env) vs feature_flags.camelforge_beta_enabled

        # After: simple hierarchy
        # 1. Environment variables (FRAISEQL_CAMELFORGE_*)
        # 2. Config parameters
        # 3. Defaults

        # This is much clearer and easier to understand
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            camelforge_enabled=True,
            camelforge_function="config.fn_camelforge",
        )

        # No environment variables set - should use config values
        cf_config = CamelForgeConfig.create(
            enabled=config.camelforge_enabled,
            function=config.camelforge_function,
        )

        assert cf_config.enabled is True
        assert cf_config.function == "config.fn_camelforge"

        # Set environment variable - should override config
        os.environ["FRAISEQL_CAMELFORGE_FUNCTION"] = "env.fn_camelforge"

        try:
            cf_config = CamelForgeConfig.create(
                enabled=config.camelforge_enabled,
                function=config.camelforge_function,  # Should be overridden
            )

            assert cf_config.enabled is True  # From config
            assert cf_config.function == "env.fn_camelforge"  # From env var

        finally:
            del os.environ["FRAISEQL_CAMELFORGE_FUNCTION"]
