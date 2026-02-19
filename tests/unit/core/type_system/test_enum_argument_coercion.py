"""Tests for enum argument coercion in query resolvers.

Verifies that coerce_input_arguments properly converts raw string/int values
from graphql-core into Python Enum instances for enum-typed parameters.

Regression test for: https://github.com/fraiseql/fraiseql-python/issues/286
"""

import sys
import types
from enum import Enum, StrEnum

import pytest

# Mock the Rust extension module before importing fraiseql
if "fraiseql._fraiseql_rs" not in sys.modules:
    _mock = types.ModuleType("fraiseql._fraiseql_rs")
    for attr in (
        "FieldSelection",
        "ParsedQuery",
        "RustSchemaRegistry",
        "parse_graphql_query",
    ):
        setattr(
            _mock,
            attr,
            type(
                attr,
                (),
                {
                    "clear": staticmethod(lambda: None),
                    "get_instance": classmethod(lambda cls: cls()),
                    "register_type": staticmethod(lambda *a, **k: None),
                    "register_input": staticmethod(lambda *a, **k: None),
                    "register_enum": staticmethod(lambda *a, **k: None),
                    "register_query": staticmethod(lambda *a, **k: None),
                    "register_mutation": staticmethod(lambda *a, **k: None),
                },
            ),
        )
    sys.modules["fraiseql._fraiseql_rs"] = _mock

from fraiseql.types.coercion import coerce_input_arguments

pytestmark = pytest.mark.unit


class Color(StrEnum):
    RED = "red"
    GREEN = "green"
    BLUE = "blue"


class Priority(Enum):
    LOW = 1
    MEDIUM = 2
    HIGH = 3


class TimeInterval(StrEnum):
    DAY = "day"
    WEEK = "week"
    MONTH = "month"
    QUARTER = "quarter"


class TestEnumArgumentCoercion:
    """Verify coerce_input_arguments converts raw values to Enum instances."""

    def test_string_enum_coerced_by_value(self) -> None:
        """Raw string matching enum .value should be coerced."""

        async def resolver(info, color: Color) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"color": "red"})
        assert isinstance(result["color"], Color)
        assert result["color"] is Color.RED

    def test_string_enum_coerced_by_name(self) -> None:
        """Raw string matching enum NAME (uppercase) should be coerced."""

        async def resolver(info, color: Color) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"color": "RED"})
        assert isinstance(result["color"], Color)
        assert result["color"] is Color.RED

    def test_integer_enum_coerced_by_value(self) -> None:
        """Raw integer matching enum .value should be coerced."""

        async def resolver(info, priority: Priority) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"priority": 3})
        assert isinstance(result["priority"], Priority)
        assert result["priority"] is Priority.HIGH

    def test_optional_enum_coerced(self) -> None:
        """Optional enum parameter should be coerced when provided."""

        async def resolver(info, time_interval: TimeInterval | None = None) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"time_interval": "month"})
        assert isinstance(result["time_interval"], TimeInterval)
        assert result["time_interval"] is TimeInterval.MONTH

    def test_optional_enum_none_passthrough(self) -> None:
        """Optional enum parameter should pass None through unchanged."""

        async def resolver(info, time_interval: TimeInterval | None = None) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"time_interval": None})
        assert result["time_interval"] is None

    def test_optional_enum_omitted(self) -> None:
        """Optional enum parameter should be omitted when not in raw_args."""

        async def resolver(info, time_interval: TimeInterval | None = None) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {})
        assert "time_interval" not in result

    def test_already_enum_instance_passthrough(self) -> None:
        """Already-enum value should pass through unchanged."""

        async def resolver(info, color: Color) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"color": Color.BLUE})
        assert result["color"] is Color.BLUE

    def test_non_enum_args_unchanged(self) -> None:
        """Non-enum arguments should not be affected."""

        async def resolver(info, name: str, count: int) -> str:
            return "ok"

        result = coerce_input_arguments(resolver, {"name": "test", "count": 42})
        assert result["name"] == "test"
        assert result["count"] == 42
