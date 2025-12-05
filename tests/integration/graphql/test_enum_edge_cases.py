"""Test edge cases for enum parameter conversion."""

from enum import Enum
from typing import Optional

import pytest
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql.gql.resolver_wrappers import _coerce_to_enum, wrap_resolver

pytestmark = pytest.mark.integration


@fraiseql.enum
class Color(Enum):
    """Test enum with string values."""

    RED = "red"
    GREEN = "green"
    BLUE = "blue"


@fraiseql.enum
class Level(Enum):
    """Test enum with integer values."""

    BASIC = 1
    INTERMEDIATE = 2
    ADVANCED = 3


@fraiseql.enum
class MixedEnum(Enum):
    """Test enum with mixed value types."""

    STRING_VAL = "test"
    INT_VAL = 42
    FLOAT_VAL = 3.14


class TestEnumCoercion:
    """Test the _coerce_to_enum helper function."""

    def test_coerce_by_value_string(self) -> None:
        """Test coercing string value to enum."""
        result = _coerce_to_enum("red", Color)
        assert result == Color.RED

    def test_coerce_by_value_int(self) -> None:
        """Test coercing integer value to enum."""
        result = _coerce_to_enum(2, Level)
        assert result == Level.INTERMEDIATE

    def test_coerce_by_name(self) -> None:
        """Test coercing by enum member name."""
        result = _coerce_to_enum("GREEN", Color)
        assert result == Color.GREEN

    def test_coerce_already_enum(self) -> None:
        """Test that enum instances pass through unchanged."""
        result = _coerce_to_enum(Color.BLUE, Color)
        assert result == Color.BLUE

    def test_coerce_invalid_value_raises(self) -> None:
        """Test that invalid values raise ValueError."""
        with pytest.raises(ValueError) as exc_info:
            _coerce_to_enum("purple", Color)

        assert "Cannot convert 'purple' to Color" in str(exc_info.value)
        assert "RED=red" in str(exc_info.value)

    def test_coerce_mixed_types(self) -> None:
        """Test coercing mixed enum types."""
        assert _coerce_to_enum("test", MixedEnum) == MixedEnum.STRING_VAL
        assert _coerce_to_enum(42, MixedEnum) == MixedEnum.INT_VAL
        assert _coerce_to_enum(3.14, MixedEnum) == MixedEnum.FLOAT_VAL


@pytest.mark.asyncio
async def test_resolver_with_optional_enum() -> None:
    """Test that optional enum parameters work correctly."""

    async def resolver(info: GraphQLResolveInfo, color: Optional[Color] = None) -> str:
        if color is None:
            return "No color"
        return f"Color: {color.value}"

    field = wrap_resolver(resolver)

    # Test with None (omitted parameter)
    class MockInfo:
        pass

    result = await field.resolve(None, MockInfo())
    assert result == "No color"

    # Test with valid enum value
    result = await field.resolve(None, MockInfo(), color="blue")
    assert result == "Color: blue"


@pytest.mark.asyncio
async def test_resolver_with_multiple_enums() -> None:
    """Test resolver with multiple enum parameters."""

    async def resolver(
        info: GraphQLResolveInfo,
        color: Color,
        level: Level,
    ) -> str:
        return f"{color.name}-{level.value}"

    field = wrap_resolver(resolver)

    class MockInfo:
        pass

    result = await field.resolve(None, MockInfo(), color="green", level=3)
    assert result == "GREEN-3"


@pytest.mark.asyncio
async def test_resolver_preserves_non_enum_types() -> None:
    """Test that non-enum parameters are not affected."""

    async def resolver(
        info: GraphQLResolveInfo,
        name: str,
        age: int,
        color: Color,
    ) -> str:
        return f"{name}-{age}-{color.value}"

    field = wrap_resolver(resolver)

    class MockInfo:
        pass

    result = await field.resolve(None, MockInfo(), name="Alice", age=30, color="red")
    assert result == "Alice-30-red"


@pytest.mark.asyncio
async def test_invalid_enum_value_handling() -> None:
    """Test handling of invalid enum values."""

    async def resolver(info: GraphQLResolveInfo, color: Color) -> str:
        # This should not be reached if validation works
        if isinstance(color, str):
            return f"Got string: {color}"
        return f"Got enum: {color.value}"

    field = wrap_resolver(resolver)

    class MockInfo:
        pass

    # Invalid value should be passed through as-is (GraphQL layer should handle validation)
    result = await field.resolve(None, MockInfo(), color="invalid")
    assert result == "Got string: invalid"


@pytest.mark.asyncio
async def test_enum_by_name_resolution() -> None:
    """Test that enum names (not just values) can be resolved."""

    async def resolver(info: GraphQLResolveInfo, level: Level) -> str:
        return f"Level: {level.name}={level.value}"

    field = wrap_resolver(resolver)

    class MockInfo:
        pass

    # Test by value
    result = await field.resolve(None, MockInfo(), level=2)
    assert result == "Level: INTERMEDIATE=2"

    # Test by name
    result = await field.resolve(None, MockInfo(), level="ADVANCED")
    assert result == "Level: ADVANCED=3"
