"""Shared logic to define FraiseQL types across @fraise_input, @fraise_type, etc."""

from typing import Any, Literal, Protocol, TypeVar, cast, get_type_hints

from fraiseql.fields import FraiseQLField
from fraiseql.types.definitions import FraiseQLTypeDefinition
from fraiseql.utils.casing import to_snake_case
from fraiseql.utils.fraiseql_builder import collect_fraise_fields, make_init

T = TypeVar("T")


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
            """
            # Convert camelCase keys to snake_case
            snake_case_data = {}
            for key, value in data.items():
                if key == "__typename":  # Skip GraphQL metadata
                    continue
                snake_key = to_snake_case(key)
                snake_case_data[snake_key] = value

            # Create instance with converted data
            return cls(**snake_case_data)

        typed_cls.from_dict = from_dict

    return cast("type[T]", typed_cls)
