"""GraphQL-compatible order by input type generator.

This module provides utilities to dynamically generate GraphQL input types
for ordering. These types can be used directly in GraphQL resolvers and are
automatically converted to SQL ORDER BY clauses.
"""

from dataclasses import make_dataclass
from enum import Enum
from typing import Any, List, Optional, TypeVar, Union, get_args, get_origin, get_type_hints

from fraiseql import fraise_enum, fraise_input
from fraiseql.sql.order_by_generator import OrderBy, OrderBySet

# Type variable for generic types
T = TypeVar("T")

# Cache for generated order by input types to handle circular references
_order_by_input_cache: dict[type, type] = {}
# Stack to track types being generated to detect circular references
_generation_stack: set[type] = set()


# Order direction enum
@fraise_enum
class OrderDirection(Enum):
    """Order direction for sorting."""

    ASC = "asc"
    DESC = "desc"


@fraise_input
class OrderByItem:
    """Single order by instruction."""

    field: str
    direction: OrderDirection = OrderDirection.ASC


def _is_fraiseql_type(field_type: type) -> bool:
    """Check if a type is a FraiseQL type (has __fraiseql_definition__)."""
    # Handle Optional types first
    origin = get_origin(field_type)

    # For Python 3.10+, we need to check for UnionType as well
    import types

    if origin is Union or (hasattr(types, "UnionType") and isinstance(field_type, types.UnionType)):
        args = get_args(field_type)
        # Filter out None type
        non_none_types = [arg for arg in args if arg is not type(None)]
        if non_none_types:
            field_type = non_none_types[0]
            # Re-check origin after unwrapping
            origin = get_origin(field_type)

    # Don't consider list types as FraiseQL types
    if origin in (list, List):
        return False

    return hasattr(field_type, "__fraiseql_definition__")


def _convert_order_by_input_to_sql(order_by_input: Any) -> OrderBySet | None:
    """Convert GraphQL order by input to SQL OrderBySet."""
    if order_by_input is None:
        return None

    instructions = []

    # Handle single OrderByItem
    if hasattr(order_by_input, "field") and hasattr(order_by_input, "direction"):
        direction = (
            order_by_input.direction.value
            if hasattr(order_by_input.direction, "value")
            else order_by_input.direction
        )
        instructions.append(OrderBy(field=order_by_input.field, direction=direction))
        return OrderBySet(instructions=instructions)

    # Handle list of OrderByItem or list of dicts
    if isinstance(order_by_input, list):
        for item in order_by_input:
            # Handle OrderByItem objects
            if hasattr(item, "field") and hasattr(item, "direction"):
                direction = (
                    item.direction.value if hasattr(item.direction, "value") else item.direction
                )
                instructions.append(OrderBy(field=item.field, direction=direction))
            # Handle dictionary items like {'ipAddress': 'asc'}
            elif isinstance(item, dict):
                for field_name, value in item.items():
                    if value is not None:
                        # Convert camelCase field names to snake_case for database fields
                        from fraiseql.utils.casing import to_snake_case

                        snake_field_name = to_snake_case(field_name)

                        # Handle OrderDirection enum
                        if isinstance(value, OrderDirection):
                            instructions.append(
                                OrderBy(field=snake_field_name, direction=value.value)
                            )
                        # Handle string direction (GraphQL might pass "ASC" or "DESC" as strings)
                        elif isinstance(value, str) and value.upper() in ["ASC", "DESC"]:
                            instructions.append(
                                OrderBy(field=snake_field_name, direction=value.lower())
                            )
        return OrderBySet(instructions=instructions) if instructions else None

    # Handle object with field-specific order directions
    if hasattr(order_by_input, "__gql_fields__"):

        def process_order_by(obj, prefix=""):
            """Recursively process order by object."""
            for field_name in obj.__gql_fields__:
                value = getattr(obj, field_name)
                if value is not None:
                    field_path = f"{prefix}.{field_name}" if prefix else field_name
                    # If it's an OrderDirection enum, use it
                    if isinstance(value, OrderDirection):
                        instructions.append(OrderBy(field=field_path, direction=value.value))
                    # If it's a nested order by input, process recursively
                    elif hasattr(value, "__gql_fields__"):
                        process_order_by(value, field_path)

        process_order_by(order_by_input)

    # Handle plain dict (common from GraphQL frameworks)
    elif isinstance(order_by_input, dict):

        def process_dict_order_by(obj_dict, prefix=""):
            """Process dictionary-style order by input."""
            for field_name, value in obj_dict.items():
                if value is not None:
                    # Convert camelCase field names to snake_case for database fields
                    from fraiseql.utils.casing import to_snake_case

                    snake_field_name = to_snake_case(field_name)
                    field_path = f"{prefix}.{snake_field_name}" if prefix else snake_field_name

                    # Handle OrderDirection enum
                    if isinstance(value, OrderDirection):
                        instructions.append(OrderBy(field=field_path, direction=value.value))
                    # Handle string direction (GraphQL might pass "ASC" or "DESC" as strings)
                    elif isinstance(value, str) and value.upper() in ["ASC", "DESC"]:
                        instructions.append(OrderBy(field=field_path, direction=value.lower()))
                    # Handle nested dict
                    elif isinstance(value, dict):
                        process_dict_order_by(value, field_path)

        process_dict_order_by(order_by_input)

    return OrderBySet(instructions=instructions) if instructions else None


def create_graphql_order_by_input(cls: type, name: str | None = None) -> type:
    """Create a GraphQL-compatible order by input type.

    This generates an input type where each field can be set to an OrderDirection
    to specify sorting. For nested objects, it creates nested order by inputs.

    Args:
        cls: The dataclass or fraise_type to generate order by fields for
        name: Optional name for the generated input type (defaults to {ClassName}OrderByInput)

    Returns:
        A new dataclass decorated with @fraise_input that supports field-based ordering

    Example:
        ```python
        @fraise_type
        class User:
            id: UUID
            name: str
            age: int
            created_at: datetime

        UserOrderByInput = create_graphql_order_by_input(User)

        # Usage in resolver
        @fraiseql.query
        async def users(info, order_by: UserOrderByInput | None = None) -> list[User]:
            return await info.context["db"].find("user_view", order_by=order_by)

        # GraphQL query
        query {
            users(orderBy: { name: ASC, createdAt: DESC }) {
                id
                name
            }
        }
        ```
    """
    # Handle case where cls might be a Union type
    origin = get_origin(cls)
    import types

    if origin is Union or (hasattr(types, "UnionType") and isinstance(cls, types.UnionType)):
        # Should not happen in normal usage
        raise TypeError(f"Cannot create order by input for Union type: {cls}")

    # Check cache first (only for unnamed types to allow custom names)
    if name is None and cls in _order_by_input_cache:
        return _order_by_input_cache[cls]

    # Add to generation stack to detect circular references
    _generation_stack.add(cls)

    try:
        # Get type hints from the class
        try:
            type_hints = get_type_hints(cls)
        except Exception:
            # Fallback for classes that might not have proper annotations
            type_hints = {}
            if hasattr(cls, "__annotations__"):
                for key, value in cls.__annotations__.items():
                    type_hints[key] = value

        # Generate field definitions for the input type
        field_definitions = []
        field_defaults = {}
        deferred_fields = {}  # For circular references

        for field_name, field_type in type_hints.items():
            # Skip private fields
            if field_name.startswith("_"):
                continue

            # Check if this is a nested FraiseQL type
            if _is_fraiseql_type(field_type):
                # Check cache first
                origin_type = field_type
                # Unwrap Optional
                origin = get_origin(field_type)
                import types as _types

                if origin is Union or (
                    hasattr(_types, "UnionType") and isinstance(field_type, _types.UnionType)
                ):
                    args = get_args(field_type)
                    non_none_types = [arg for arg in args if arg is not type(None)]
                    if non_none_types:
                        origin_type = non_none_types[0]

                if origin_type in _order_by_input_cache:
                    nested_order_by = _order_by_input_cache[origin_type]
                elif origin_type in _generation_stack:
                    # Circular reference - defer for later
                    deferred_fields[field_name] = origin_type
                    # Use OrderDirection as temporary placeholder
                    nested_order_by = OrderDirection
                else:
                    # Generate nested order by input recursively
                    # Make sure to pass the unwrapped type, not the Union
                    # Extra check to ensure we're not passing a Union type
                    import types as _types

                    if get_origin(origin_type) is Union or (
                        hasattr(_types, "UnionType") and isinstance(origin_type, _types.UnionType)
                    ):
                        # This shouldn't happen but let's be defensive
                        args = get_args(origin_type)
                        non_none_types = [arg for arg in args if arg is not type(None)]
                        if non_none_types:
                            origin_type = non_none_types[0]
                    nested_order_by = create_graphql_order_by_input(origin_type)

                field_definitions.append((field_name, Optional[nested_order_by], None))
            else:
                # For scalar fields, use OrderDirection
                field_definitions.append((field_name, Optional[OrderDirection], None))

            field_defaults[field_name] = None

        # Generate class name
        class_name = name or f"{cls.__name__}OrderByInput"

        # Create the dataclass
        OrderByInputClass = make_dataclass(
            class_name,
            field_definitions,
            bases=(),
            frozen=False,
        )

        # Add the fraise_input decorator
        OrderByInputClass = fraise_input(OrderByInputClass)

        # Cache before processing deferred fields (only for unnamed types)
        if name is None:
            _order_by_input_cache[cls] = OrderByInputClass

        # Process deferred fields (circular references)
        for field_name, field_type in deferred_fields.items():
            # Now that we're cached, try to get the actual order by input type
            if field_type in _order_by_input_cache:
                # Update the field annotation
                OrderByInputClass.__annotations__[field_name] = Optional[
                    _order_by_input_cache[field_type]
                ]
                # Update the dataclass field
                if hasattr(OrderByInputClass, "__dataclass_fields__"):
                    from dataclasses import MISSING, Field

                    field = Field(
                        default=None,
                        default_factory=MISSING,
                        init=True,
                        repr=True,
                        hash=None,
                        compare=True,
                        metadata={},
                    )
                    field.name = field_name
                    field.type = Optional[_order_by_input_cache[field_type]]
                    OrderByInputClass.__dataclass_fields__[field_name] = field

        # Add conversion method
        OrderByInputClass._target_class = cls
        OrderByInputClass._to_sql_order_by = lambda self: _convert_order_by_input_to_sql(self)

        # Add helpful docstring
        OrderByInputClass.__doc__ = (
            f"GraphQL order by input type for {cls.__name__} with field-based sorting."
        )

        return OrderByInputClass

    finally:
        # Remove from generation stack
        _generation_stack.discard(cls)


# Alternative approach: List-based ordering
def create_graphql_order_by_list_input(cls: type, name: str | None = None) -> type:
    """Create a GraphQL order by input that accepts a list of OrderByItem.

    This generates an input type that accepts a list of field/direction pairs,
    allowing for multiple sort criteria with explicit ordering.

    Args:
        cls: The dataclass or fraise_type to validate fields against
        name: Optional name for the generated input type

    Returns:
        A new list type that accepts OrderByItem instances

    Example:
        ```python
        @fraiseql.query
        async def users(info, order_by: list[OrderByItem] | None = None) -> list[User]:
            # Validates that field names exist in User type
            return await info.context["db"].find("user_view", order_by=order_by)

        # GraphQL query
        query {
            users(orderBy: [
                { field: "age", direction: DESC },
                { field: "name", direction: ASC }
            ]) {
                id
                name
            }
        }
        ```
    """
    # For list-based approach, we just return list[OrderByItem]
    # The validation would happen at runtime
    return list[OrderByItem]
