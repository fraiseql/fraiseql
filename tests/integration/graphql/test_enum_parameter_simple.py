"""Simple test for enum parameter conversion bug."""

from enum import Enum

import pytest
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql.gql.resolver_wrappers import wrap_resolver

pytestmark = pytest.mark.integration


@fraiseql.enum
class Status(Enum):
    """Test enum."""

    ACTIVE = "ACTIVE"
    INACTIVE = "INACTIVE"


async def sample_resolver(info: GraphQLResolveInfo, status: Status) -> str:
    """Sample resolver for testing enum conversion."""
    return f"{type(status).__name__}:{status}"


@pytest.mark.asyncio
async def test_enum_parameter_conversion_direct() -> None:
    """Test enum conversion directly in wrap_resolver."""
    # Wrap the resolver
    field = wrap_resolver(sample_resolver)

    # Create mock info object
    class MockInfo:
        pass

    info = MockInfo()

    # Call the resolver directly with enum value (as GraphQL would pass it)
    # GraphQL passes the raw enum value (string) not the enum instance
    result = await field.resolve(None, info, status="ACTIVE")

    # Check what was returned
    # If the bug exists, it will return "str:ACTIVE" instead of "Status:Status.ACTIVE"
    assert result == "Status:Status.ACTIVE", f"Enum not converted! Got: {result}"


@pytest.mark.asyncio
async def test_enum_instance_check() -> None:
    """Test that we can detect enum types."""
    from inspect import signature

    sig = signature(sample_resolver)
    status_param = sig.parameters["status"]

    # Check if we can detect it's an enum
    assert status_param.annotation == Status
    assert issubclass(status_param.annotation, Enum)
