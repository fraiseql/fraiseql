import pytest

"""Tests for ComplexityConfig."""

from fraiseql.analysis.complexity_config import (
    BALANCED_CONFIG,
    RELAXED_CONFIG,
    STRICT_CONFIG,
    ComplexityConfig,
)


@pytest.mark.unit
class TestComplexityConfig:
    """Test ComplexityConfig functionality."""

    def test_default_config_singleton(self) -> None:
        """Test that get_default returns singleton instance."""
        config1 = ComplexityConfig.get_default()
        config2 = ComplexityConfig.get_default()
        assert config1 is config2

    def test_set_default_config(self) -> None:
        """Test setting custom default config."""
        custom_config = ComplexityConfig(depth_multiplier=3.0)
        ComplexityConfig.set_default(custom_config)

        default = ComplexityConfig.get_default()
        assert default is custom_config
        assert default.depth_multiplier == 3.0

        # Reset to default
        ComplexityConfig._default = None

    def test_array_field_detection(self) -> None:
        """Test array field detection logic."""
        config = ComplexityConfig()

        # Plural fields
        assert config.is_array_field("users") is True
        assert config.is_array_field("posts") is True
        assert config.is_array_field("items") is True

        # Non-plural fields
        assert config.is_array_field("user") is False
        assert config.is_array_field("post") is False

        # Special patterns
        assert config.is_array_field("userList") is True
        assert config.is_array_field("allUsers") is True
        assert config.is_array_field("manyThings") is True
        assert config.is_array_field("collection") is True

        # Edge cases
        assert config.is_array_field("s") is False  # Too short
        assert config.is_array_field("as") is False  # Too short

    def test_depth_penalty_calculation(self) -> None:
        """Test depth penalty calculation with bounds."""
        config = ComplexityConfig(depth_multiplier=2.0, max_depth_penalty=100)

        # Zero depth
        assert config.calculate_depth_penalty(0) == 0

        # Normal depths
        assert config.calculate_depth_penalty(1) == 1  # 1^2
        assert config.calculate_depth_penalty(2) == 4  # 2^2
        assert config.calculate_depth_penalty(3) == 9  # 3^2
        assert config.calculate_depth_penalty(4) == 16  # 4^2

        # Max cap
        assert config.calculate_depth_penalty(20) == 100  # Capped at max
        assert config.calculate_depth_penalty(100) == 100  # Still capped

    def test_depth_penalty_overflow_prevention(self) -> None:
        """Test that depth penalty prevents integer overflow."""
        config = ComplexityConfig(depth_multiplier=10.0, max_depth_penalty=1000)

        # Very deep nesting should be capped
        assert config.calculate_depth_penalty(1000) == 1000
        assert config.calculate_depth_penalty(10000) == 1000

        # Should not raise overflow error
        result = config.calculate_depth_penalty(999999)
        assert result == 1000

    def test_array_penalty_calculation(self) -> None:
        """Test array penalty calculation."""
        config = ComplexityConfig(array_field_multiplier=10, array_depth_factor=1.5)

        # No arrays
        assert config.calculate_array_penalty(0, 0) == 0
        assert config.calculate_array_penalty(5, 0) == 0

        # Arrays at depth 0
        assert config.calculate_array_penalty(0, 1) == 10  # 1 * 10 * 1.5^0
        assert config.calculate_array_penalty(0, 3) == 30  # 3 * 10 * 1.5^0

        # Arrays at depth 2
        penalty = config.calculate_array_penalty(2, 1)
        expected = int(1 * 10 * (1.5**2))  # 22
        assert penalty == expected

        # Multiple arrays at depth
        penalty = config.calculate_array_penalty(3, 5)
        expected = int(5 * 10 * (1.5**3))  # 168
        assert penalty == expected

    def test_cache_weight_calculation(self) -> None:
        """Test cache weight calculation for different scores."""
        config = ComplexityConfig()

        # Simple queries
        assert config.get_cache_weight(5) == 0.1
        assert config.get_cache_weight(9) == 0.1

        # Moderate queries
        assert config.get_cache_weight(15) == 0.5
        assert config.get_cache_weight(49) == 0.5

        # Complex queries
        assert config.get_cache_weight(100) == 2.0
        assert config.get_cache_weight(199) == 2.0

        # Very complex queries (exponential growth)
        assert config.get_cache_weight(200) == 2.0  # At threshold
        assert config.get_cache_weight(400) == 4.0  # 2x threshold
        assert config.get_cache_weight(600) == 6.0  # 3x threshold

    def test_preset_configurations(self) -> None:
        """Test preset configuration values."""
        # STRICT config
        assert STRICT_CONFIG.depth_multiplier == 2.0
        assert STRICT_CONFIG.array_field_multiplier == 15
        assert STRICT_CONFIG.complex_query_threshold == 150

        # BALANCED config (defaults)
        assert BALANCED_CONFIG.depth_multiplier == 1.5
        assert BALANCED_CONFIG.array_field_multiplier == 10
        assert BALANCED_CONFIG.complex_query_threshold == 200

        # RELAXED config
        assert RELAXED_CONFIG.depth_multiplier == 1.2
        assert RELAXED_CONFIG.array_field_multiplier == 5
        assert RELAXED_CONFIG.complex_query_threshold == 500

    def test_custom_array_patterns(self) -> None:
        """Test custom array field patterns."""
        # Use lowercase patterns since the method converts to lowercase
        config = ComplexityConfig(array_field_patterns=["foolist", "barcollection", "customarray"])

        # Custom patterns (case insensitive check)
        assert config.is_array_field("foolist") is True
        assert config.is_array_field("fooList") is True  # Case insensitive
        assert config.is_array_field("FooList") is True  # Case insensitive

        assert config.is_array_field("mybarcollection") is True
        assert config.is_array_field("customarray") is True

        # Default patterns no longer work (since we replaced the list)
        assert config.is_array_field("list") is False  # Not in our custom patterns
        assert config.is_array_field("collection") is False  # Would be true with default patterns

        # Plural detection still works
        assert config.is_array_field("users") is True

    def test_config_boundaries(self) -> None:
        """Test configuration with boundary values."""
        # Test with zero/negative values
        config = ComplexityConfig(base_field_cost=0, depth_multiplier=0.0, array_field_multiplier=0)

        # With depth_multiplier=0.0, depth^0 = 1 for any depth > 0
        assert config.calculate_depth_penalty(0) == 0  # Special case for depth=0
        assert config.calculate_depth_penalty(5) == 1  # 5^0 = 1
        assert config.calculate_array_penalty(5, 5) == 0  # 0 multiplier

        # Test with very large values
        config = ComplexityConfig(max_depth_penalty=999999999, complex_query_threshold=999999999)

        # Should handle large values gracefully
        penalty = config.calculate_depth_penalty(100)
        assert isinstance(penalty, int)
        assert penalty > 0
