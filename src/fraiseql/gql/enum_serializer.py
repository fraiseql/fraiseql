"""Enum serialization helpers for GraphQL resolvers."""

from enum import Enum
from typing import Any


def serialize_enum_value(value: Any) -> Any:
    """Convert Python enum instances to their names for GraphQL serialization."""
    if isinstance(value, Enum):
        return value.name
    if isinstance(value, list):
        return [serialize_enum_value(item) for item in value]
    if isinstance(value, dict):
        return {k: serialize_enum_value(v) for k, v in value.items()}
    if hasattr(value, "__fraiseql_definition__"):
        # For FraiseQL types, we need to preserve the object for interface resolution
        # but GraphQL needs to serialize the fields. Return the object as-is.
        # GraphQL will handle field resolution through its own mechanism.
        return value
    if hasattr(value, "__dict__"):
        # Handle other dataclass/object instances
        result = {}
        for attr_name in dir(value):
            if not attr_name.startswith("_"):
                attr_value = getattr(value, attr_name, None)
                if not callable(attr_value):
                    result[attr_name] = serialize_enum_value(attr_value)
        return result
    return value


def wrap_resolver_with_enum_serialization(resolver):
    """Wrap a resolver to automatically serialize enum values."""
    import asyncio
    import inspect

    if asyncio.iscoroutinefunction(resolver) or inspect.iscoroutinefunction(resolver):

        async def wrapped_resolver(*args, **kwargs):
            result = await resolver(*args, **kwargs)
            return serialize_enum_value(result)

        return wrapped_resolver

    def sync_wrapped_resolver(*args, **kwargs):
        result = resolver(*args, **kwargs)
        return serialize_enum_value(result)

    return sync_wrapped_resolver
