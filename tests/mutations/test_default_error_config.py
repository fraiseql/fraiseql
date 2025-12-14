"""Tests for default_error_config in FraiseQLConfig."""

from fraiseql import (
    ALWAYS_DATA_CONFIG,
    DEFAULT_ERROR_CONFIG,
    STRICT_STATUS_CONFIG,
    FraiseQLConfig,
    MutationErrorConfig,
    mutation,
)
from fraiseql.gql.builders.registry import SchemaRegistry


class TestDefaultErrorConfig:
    """Test default_error_config resolution in mutations."""

    def setup_method(self):
        """Reset registry before each test."""
        registry = SchemaRegistry.get_instance()
        registry.config = None

    def teardown_method(self):
        """Clean up registry after each test."""
        registry = SchemaRegistry.get_instance()
        registry.config = None

    def test_mutation_uses_global_default_error_config(self):
        """Test that mutations use default_error_config from FraiseQLConfig when not specified."""
        # Setup: Create config with default_error_config
        config = FraiseQLConfig(
            database_url="postgresql://test",
            default_error_config=DEFAULT_ERROR_CONFIG,
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation without explicit error_config
        @mutation(function="test_mutation")
        class TestMutation:
            input: dict
            success: dict
            error: dict

        # Verify: Mutation should use global default
        mutation_def = TestMutation.__fraiseql_mutation__

        assert mutation_def is not None
        # This will fail initially - we need to implement the feature
        assert mutation_def.error_config == DEFAULT_ERROR_CONFIG

    def test_explicit_error_config_overrides_default(self):
        """Test that explicit error_config on decorator overrides global default."""
        # Setup: Config with DEFAULT_ERROR_CONFIG as default
        config = FraiseQLConfig(
            database_url="postgresql://test",
            default_error_config=DEFAULT_ERROR_CONFIG,
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation WITH explicit error_config (different from default)
        custom_config = MutationErrorConfig(
            error_prefixes={"custom:"},  # Different from default
        )

        @mutation(function="test_override", error_config=custom_config)
        class TestMutation:
            input: dict
            success: dict
            error: dict

        # Verify: Should use explicit config, not global default
        mutation_def = TestMutation.__fraiseql_mutation__
        assert mutation_def is not None
        assert mutation_def.error_config == custom_config
        assert mutation_def.error_config != DEFAULT_ERROR_CONFIG

    def test_no_default_error_config_returns_none(self):
        """Test that mutations get None when no global default is set."""
        # Setup: Config WITHOUT default_error_config
        config = FraiseQLConfig(
            database_url="postgresql://test",
            # default_error_config not set (None)
        )

        registry = SchemaRegistry.get_instance()
        registry.config = config

        # Define mutation without explicit error_config
        @mutation(function="test_no_default")
        class TestMutation:
            input: dict
            success: dict
            error: dict

        # Verify: Should be None
        mutation_def = TestMutation.__fraiseql_mutation__
        assert mutation_def is not None
        assert mutation_def.error_config is None

    def test_no_config_returns_none(self):
        """Test that mutations get None when registry has no config."""
        # Setup: No config in registry
        registry = SchemaRegistry.get_instance()
        registry.config = None

        # Define mutation without explicit error_config
        @mutation(function="test_no_config")
        class TestMutation:
            input: dict
            success: dict
            error: dict

        # Verify: Should be None
        mutation_def = TestMutation.__fraiseql_mutation__
        assert mutation_def is not None
        assert mutation_def.error_config is None

    def test_different_default_error_configs(self):
        """Test that different global defaults work correctly."""
        test_cases = [
            (DEFAULT_ERROR_CONFIG, "default"),
            (STRICT_STATUS_CONFIG, "strict"),
            (ALWAYS_DATA_CONFIG, "always_data"),
        ]

        for expected_config, suffix in test_cases:
            # Setup
            config = FraiseQLConfig(
                database_url="postgresql://test",
                default_error_config=expected_config()
                if expected_config == ALWAYS_DATA_CONFIG
                else expected_config,
            )
            registry = SchemaRegistry.get_instance()
            registry.config = config

            # Define mutation
            function_name = f"test_{suffix}"

            @mutation(function=function_name)
            class TestMutation:
                input: dict
                success: dict
                error: dict

            # Verify
            mutation_def = TestMutation.__fraiseql_mutation__
            assert mutation_def is not None
            assert mutation_def.error_config == (
                expected_config() if expected_config == ALWAYS_DATA_CONFIG else expected_config
            )
