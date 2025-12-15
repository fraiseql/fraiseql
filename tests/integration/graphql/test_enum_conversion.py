"""Test that demonstrates the enum parameter conversion bug fix.

This test verifies that enum parameters in GraphQL resolvers are properly
converted from their raw values (strings/ints) to Python Enum instances.

Related issue: /tmp/fraiseql_enum_issue.md
"""

from enum import Enum
from typing import Optional

import pytest
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql.gql.resolver_wrappers import wrap_resolver

pytestmark = pytest.mark.integration


@fraiseql.enum
class TaskStatus(Enum):
    """Example enum for task status."""

    TODO = "TODO"
    IN_PROGRESS = "IN_PROGRESS"
    DONE = "DONE"


@fraiseql.enum
class Priority(Enum):
    """Example enum with integer values."""

    LOW = 1
    MEDIUM = 2
    HIGH = 3


# Test resolvers that expect enum parameters
async def task_resolver(
    info: GraphQLResolveInfo,
    status: TaskStatus,
    priority: Optional[Priority] = None,
) -> str:
    """Resolver that expects enum parameters."""
    # Status should be properly converted to TaskStatus enum instance
    assert isinstance(status, TaskStatus), f"Expected TaskStatus, got {type(status)}"

    if priority is not None:
        assert isinstance(priority, Priority), f"Expected Priority, got {type(priority)}"
        return f"{status.name}:{priority.value}"

    return status.name


class TestEnumParameterConversion:
    """Test suite for enum parameter conversion bug fix."""

    @pytest.mark.asyncio
    async def test_enum_string_value_converted_to_instance(self) -> None:
        """Test that string enum values are converted to enum instances."""
        # Wrap the resolver to apply type coercion
        field = wrap_resolver(task_resolver)

        class MockInfo:
            pass

        # GraphQL passes "TODO" as a string, not TaskStatus.TODO
        result = await field.resolve(None, MockInfo(), status="TODO")

        # The fix ensures the resolver receives TaskStatus.TODO
        assert result == "TODO"

    @pytest.mark.asyncio
    async def test_enum_integer_value_converted_to_instance(self) -> None:
        """Test that integer enum values are converted to enum instances."""
        field = wrap_resolver(task_resolver)

        class MockInfo:
            pass

        # GraphQL passes integer values for integer enums
        result = await field.resolve(None, MockInfo(), status="IN_PROGRESS", priority=3)

        # The fix ensures both enums are properly converted
        assert result == "IN_PROGRESS:3"

    @pytest.mark.asyncio
    async def test_optional_enum_with_none_value(self) -> None:
        """Test that optional enum parameters handle None correctly."""
        field = wrap_resolver(task_resolver)

        class MockInfo:
            pass

        # Optional parameter can be omitted
        result = await field.resolve(None, MockInfo(), status="DONE")
        assert result == "DONE"

    @pytest.mark.asyncio
    async def test_enum_comparison_works_after_conversion(self) -> None:
        """Test that enum comparisons work correctly after conversion."""

        async def comparison_resolver(
            info: GraphQLResolveInfo,
            status: TaskStatus,
        ) -> bool:
            # This comparison would fail if status was a string
            return status == TaskStatus.IN_PROGRESS

        field = wrap_resolver(comparison_resolver)

        class MockInfo:
            pass

        # Test exact match
        result = await field.resolve(None, MockInfo(), status="IN_PROGRESS")
        assert result is True

        # Test non-match
        result = await field.resolve(None, MockInfo(), status="TODO")
        assert result is False

    @pytest.mark.asyncio
    async def test_enum_methods_available_after_conversion(self) -> None:
        """Test that enum methods are available after conversion."""

        async def method_resolver(
            info: GraphQLResolveInfo,
            priority: Priority,
        ) -> str:
            # These would fail if priority was an int/string
            return f"name:{priority.name},value:{priority.value}"

        field = wrap_resolver(method_resolver)

        class MockInfo:
            pass

        result = await field.resolve(None, MockInfo(), priority=2)
        assert result == "name:MEDIUM,value:2"


@pytest.mark.asyncio
async def test_real_world_example() -> None:
    """Test a real-world example similar to the original bug report."""

    @fraiseql.enum
    class Period(Enum):
        CURRENT = "CURRENT"
        STOCK = "STOCK"
        PAST = "PAST"
        FUTURE = "FUTURE"

    async def allocation_resolver(
        info: GraphQLResolveInfo,
        period: Optional[Period] = None,
    ) -> str:
        """Example resolver from the bug report."""
        if period is None:
            return "all periods"

        # Period should be properly converted to Period enum instance
        if period == Period.CURRENT:
            return "current period data"
        if period == Period.PAST:
            return "past period data"
        return f"other period: {period.name}"

    field = wrap_resolver(allocation_resolver)

    class MockInfo:
        pass

    # Test that enum comparison works (the main issue from the bug report)
    result = await field.resolve(None, MockInfo(), period="CURRENT")
    assert result == "current period data"

    # Test other enum values
    result = await field.resolve(None, MockInfo(), period="PAST")
    assert result == "past period data"

    # Test None handling
    result = await field.resolve(None, MockInfo())
    assert result == "all periods"
