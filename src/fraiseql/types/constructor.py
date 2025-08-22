"""Shared logic to define FraiseQL types across @fraise_input, @fraise_type, etc."""

import types
import typing
from typing import Any, Literal, Protocol, TypeVar, cast, get_type_hints
from uuid import UUID

from fraiseql.fields import FraiseQLField
from fraiseql.types.definitions import FraiseQLTypeDefinition
from fraiseql.utils.casing import to_snake_case
from fraiseql.utils.fraiseql_builder import collect_fraise_fields, make_init

T = TypeVar("T")


def _extract_type(field_type: Any) -> Any:
    """Extract the actual type from Optional, Union, etc."""
    origin = typing.get_origin(field_type)

    # Handle Optional[T] which is Union[T, None] or T | None (UnionType in Python 3.10+)
    if origin is typing.Union or origin is types.UnionType:
        args = typing.get_args(field_type)
        # Filter out None to get the actual type
        non_none_args = [arg for arg in args if arg is not type(None)]
        if len(non_none_args) == 1:
            return non_none_args[0]
        return field_type

    return field_type


def _process_field_value(value: Any, field_type: Any) -> Any:
    """Process a field value based on its type hint.

    Handles:
    - Nested FraiseQL objects
    - Lists of FraiseQL objects
    - UUID conversion
    - None values
    """
    if value is None:
        return None

    # Extract actual type from Optional
    actual_type = _extract_type(field_type)
    origin = typing.get_origin(actual_type)

    # Handle lists
    if origin is list:
        args = typing.get_args(actual_type)
        if args:
            item_type = args[0]
            if isinstance(value, list):
                return [_process_field_value(item, item_type) for item in value]

    # Handle FraiseQL types
    if hasattr(actual_type, "__fraiseql_definition__") and isinstance(value, dict):
        # Recursively instantiate nested object
        return actual_type.from_dict(value)

    # Handle UUID conversion
    if actual_type is UUID and isinstance(value, str):
        try:
            return UUID(value)
        except ValueError:
            return value

    # Handle IPv4Address and IPv6Address objects
    import ipaddress

    if isinstance(value, (ipaddress.IPv4Address, ipaddress.IPv6Address)):
        return str(value)

    # Handle FraiseQL scalar types
    from fraiseql.types.definitions import ScalarMarker

    if isinstance(actual_type, type) and issubclass(actual_type, ScalarMarker):
        if isinstance(value, str):
            return value
        return str(value)

    # Return value as-is for other types
    return value


class HasFraiseQLAttrs(Protocol):
    """Missing docstring."""

    __gql_typename__: str
    __gql_Fields__: dict[str, FraiseQLField]
    __gql_type_hints__: dict[str, type]
    __fraiseql_definition__: FraiseQLTypeDefinition


def define_fraiseql_type(
    cls: type[T],
    kind: Literal["input", "output", "type", "interface"],
) -> type[T]:
    """Core logic to define a FraiseQL input or output type.

    Applies FraiseQL metadata, constructs __init__, and attaches FraiseQL runtime markers.
    """
    typed_cls = cast("type[Any]", cls)

    type_hints = get_type_hints(cls, include_extras=True)
    field_map, patched_annotations = collect_fraise_fields(typed_cls, type_hints, kind=kind)

    # For type and interface decorators, set all fields to "output" purpose if they are "both"
    if kind in ("type", "interface"):
        for field in field_map.values():
            if field.purpose == "both":
                field.purpose = "output"

    typed_cls.__annotations__ = patched_annotations
    typed_cls.__init__ = make_init(field_map, kw_only=True)

    # Set FraiseQL runtime metadata
    typed_cls.__gql_typename__ = typed_cls.__name__
    typed_cls.__gql_fields__ = field_map
    typed_cls.__gql_type_hints__ = type_hints

    definition = FraiseQLTypeDefinition(
        python_type=typed_cls,
        is_input=(kind == "input"),
        kind=kind,  # ✅ required by tests
        sql_source=None,
        jsonb_column=None,
        resolve_nested=False,  # Default to not resolving nested
        fields=field_map,
        type_hints=patched_annotations,
    )
    definition.field_map = dict(field_map)  # ✅ required by tests
    definition.type = typed_cls  # ✅ required by tests

    typed_cls.__fraiseql_definition__ = definition

    # Add from_dict classmethod for output types
    if kind in ("output", "type", "interface"):

        @classmethod
        def from_dict(cls: type[T], data: dict[str, Any]) -> T:
            """Create an instance from a dictionary with camelCase keys.

            Converts camelCase keys to snake_case to match Python naming conventions.
            Recursively instantiates nested objects based on type hints.
            """
            # Get type hints for the class
            type_hints = getattr(cls, "__gql_type_hints__", {})

            # Convert camelCase keys to snake_case and handle nested objects
            snake_case_data = {}
            for key, value in data.items():
                if key == "__typename":  # Skip GraphQL metadata
                    continue
                snake_key = to_snake_case(key)

                # Process the value based on type hints
                if snake_key in type_hints:
                    field_type = type_hints[snake_key]
                    processed_value = _process_field_value(value, field_type)
                    snake_case_data[snake_key] = processed_value
                else:
                    snake_case_data[snake_key] = value

            # Create instance with converted data
            return cls(**snake_case_data)

        typed_cls.from_dict = from_dict

    return cast("type[T]", typed_cls)
