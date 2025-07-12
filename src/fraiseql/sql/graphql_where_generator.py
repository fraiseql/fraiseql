"""GraphQL-compatible where input type generator.

This module provides utilities to dynamically generate GraphQL input types
that support operator-based filtering. These types can be used directly in
GraphQL resolvers and are automatically converted to SQL where types.
"""

from dataclasses import make_dataclass
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Optional, TypeVar, Union, get_args, get_origin, get_type_hints
from uuid import UUID

from fraiseql import fraise_input
from fraiseql.fields import fraise_field
from fraiseql.sql.where_generator import safe_create_where_type

# Type variable for generic filter types
T = TypeVar("T")


# Base operator filter types for GraphQL inputs
@fraise_input
class StringFilter:
    """String field filter operations."""

    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    startswith: str | None = None
    endswith: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None


@fraise_input
class IntFilter:
    """Integer field filter operations."""

    eq: int | None = None
    neq: int | None = None
    gt: int | None = None
    gte: int | None = None
    lt: int | None = None
    lte: int | None = None
    in_: list[int] | None = fraise_field(default=None, graphql_name="in")
    nin: list[int] | None = None
    isnull: bool | None = None


@fraise_input
class FloatFilter:
    """Float field filter operations."""

    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    in_: list[float] | None = fraise_field(default=None, graphql_name="in")
    nin: list[float] | None = None
    isnull: bool | None = None


@fraise_input
class DecimalFilter:
    """Decimal field filter operations."""

    eq: Decimal | None = None
    neq: Decimal | None = None
    gt: Decimal | None = None
    gte: Decimal | None = None
    lt: Decimal | None = None
    lte: Decimal | None = None
    in_: list[Decimal] | None = fraise_field(default=None, graphql_name="in")
    nin: list[Decimal] | None = None
    isnull: bool | None = None


@fraise_input
class BooleanFilter:
    """Boolean field filter operations."""

    eq: bool | None = None
    neq: bool | None = None
    isnull: bool | None = None


@fraise_input
class UUIDFilter:
    """UUID field filter operations."""

    eq: UUID | None = None
    neq: UUID | None = None
    in_: list[UUID] | None = fraise_field(default=None, graphql_name="in")
    nin: list[UUID] | None = None
    isnull: bool | None = None


@fraise_input
class DateFilter:
    """Date field filter operations."""

    eq: date | None = None
    neq: date | None = None
    gt: date | None = None
    gte: date | None = None
    lt: date | None = None
    lte: date | None = None
    in_: list[date] | None = fraise_field(default=None, graphql_name="in")
    nin: list[date] | None = None
    isnull: bool | None = None


@fraise_input
class DateTimeFilter:
    """DateTime field filter operations."""

    eq: datetime | None = None
    neq: datetime | None = None
    gt: datetime | None = None
    gte: datetime | None = None
    lt: datetime | None = None
    lte: datetime | None = None
    in_: list[datetime] | None = fraise_field(default=None, graphql_name="in")
    nin: list[datetime] | None = None
    isnull: bool | None = None


def _get_filter_type_for_field(field_type: type) -> type:
    """Get the appropriate filter type for a field type."""
    # Handle Optional types
    origin = get_origin(field_type)
    if origin is Union:
        args = get_args(field_type)
        # Filter out None type
        non_none_types = [arg for arg in args if arg is not type(None)]
        if non_none_types:
            field_type = non_none_types[0]

    # Map Python types to filter types
    type_mapping = {
        str: StringFilter,
        int: IntFilter,
        float: FloatFilter,
        Decimal: DecimalFilter,
        bool: BooleanFilter,
        UUID: UUIDFilter,
        date: DateFilter,
        datetime: DateTimeFilter,
    }

    return type_mapping.get(field_type, StringFilter)  # Default to StringFilter


def _convert_filter_to_dict(filter_obj: Any) -> dict[str, Any]:
    """Convert a filter object to a dictionary for SQL where type."""
    if filter_obj is None:
        return {}

    result = {}
    # Check if it's a FraiseQL type with __gql_fields__
    if hasattr(filter_obj, "__gql_fields__"):
        for field_name in filter_obj.__gql_fields__:
            value = getattr(filter_obj, field_name)
            if value is not None:
                # Handle 'in_' field mapping to 'in'
                if field_name == "in_":
                    result["in"] = value
                else:
                    result[field_name] = value
    # Fallback for regular objects - use __dict__
    elif hasattr(filter_obj, "__dict__"):
        for field_name, value in filter_obj.__dict__.items():
            if value is not None:
                # Handle 'in_' field mapping to 'in'
                if field_name == "in_":
                    result["in"] = value
                else:
                    result[field_name] = value

    return result


def _convert_graphql_input_to_where_type(graphql_input: Any, target_class: type) -> Any:
    """Convert a GraphQL where input to SQL where type."""
    if graphql_input is None:
        return None

    # Create SQL where type
    SqlWhereType = safe_create_where_type(target_class)
    where_obj = SqlWhereType()

    # Convert each field
    # Check if it's a FraiseQL type with __gql_fields__
    if hasattr(graphql_input, "__gql_fields__"):
        for field_name in graphql_input.__gql_fields__:
            filter_value = getattr(graphql_input, field_name)
            if filter_value is not None:
                # Convert filter object to operator dict
                operator_dict = _convert_filter_to_dict(filter_value)
                if operator_dict:
                    setattr(where_obj, field_name, operator_dict)
                else:
                    # If the filter is empty, set to None instead of empty dict
                    setattr(where_obj, field_name, None)
    # Fallback for regular objects
    elif hasattr(graphql_input, "__dict__"):
        for field_name, filter_value in graphql_input.__dict__.items():
            if filter_value is not None:
                # Convert filter object to operator dict
                operator_dict = _convert_filter_to_dict(filter_value)
                if operator_dict:
                    setattr(where_obj, field_name, operator_dict)
                else:
                    # If the filter is empty, set to None instead of empty dict
                    setattr(where_obj, field_name, None)

    return where_obj


def create_graphql_where_input(cls: type, name: str | None = None) -> type:
    """Create a GraphQL-compatible where input type with operator filters.

    Args:
        cls: The dataclass or fraise_type to generate filters for
        name: Optional name for the generated input type (defaults to {ClassName}WhereInput)

    Returns:
        A new dataclass decorated with @fraise_input that supports operator-based filtering

    Example:
        ```python
        @fraise_type
        class User:
            id: UUID
            name: str
            age: int
            is_active: bool

        UserWhereInput = create_graphql_where_input(User)

        # Usage in resolver
        @fraiseql.query
        async def users(info, where: UserWhereInput | None = None) -> list[User]:
            return await info.context["db"].find("user_view", where=where)
        ```
    """
    # Get type hints from the class
    try:
        type_hints = get_type_hints(cls)
    except Exception:
        # Fallback for classes that might not have proper annotations
        type_hints = {}
        for key, value in cls.__annotations__.items():
            type_hints[key] = value

    # Generate field definitions for the input type
    field_definitions = []
    field_defaults = {}

    for field_name, field_type in type_hints.items():
        # Skip private fields
        if field_name.startswith("_"):
            continue

        # Get the appropriate filter type
        filter_type = _get_filter_type_for_field(field_type)

        # Add as optional field
        field_definitions.append((field_name, Optional[filter_type], None))
        field_defaults[field_name] = None

    # Generate class name
    class_name = name or f"{cls.__name__}WhereInput"

    # Create the dataclass
    WhereInputClass = make_dataclass(
        class_name,
        field_definitions,
        bases=(),
        frozen=False,
    )

    # Add the fraise_input decorator
    WhereInputClass = fraise_input(WhereInputClass)

    # Add conversion method
    WhereInputClass._target_class = cls
    WhereInputClass._to_sql_where = lambda self: _convert_graphql_input_to_where_type(self, cls)

    # Add helpful docstring
    WhereInputClass.__doc__ = (
        f"GraphQL where input type for {cls.__name__} with operator-based filtering."
    )

    return WhereInputClass
