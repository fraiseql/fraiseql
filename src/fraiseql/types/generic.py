"""Generic type support for FraiseQL GraphQL schemas.

This module provides support for generic types like Connection[T], Edge[T], and
PaginatedResponse[T] that are essential for pagination and reusable patterns.
"""

from typing import Any, Generic, TypeVar, get_args, get_origin

from fraiseql.fields import fraise_field
from fraiseql.types.definitions import FraiseQLTypeDefinition
from fraiseql.types.fraise_type import fraise_type

T = TypeVar("T")


def is_generic_type(typ: type) -> bool:
    """Check if a type is a generic type like Connection[T]."""
    return hasattr(typ, "__origin__") and hasattr(typ, "__args__")


def get_generic_origin(typ: type) -> type | None:
    """Get the origin type of a generic type (e.g., Connection from Connection[Post])."""
    return get_origin(typ)


def get_generic_args(typ: type) -> tuple[type, ...]:
    """Get the type arguments of a generic type (e.g., (Post,) from Connection[Post])."""
    return get_args(typ)


def is_fraise_generic(typ: type) -> bool:
    """Check if a type is a FraiseQL generic type (decorated with @fraise_type)."""
    origin = get_generic_origin(typ)
    if origin is None:
        return False
    return hasattr(origin, "__fraiseql_definition__")


def create_concrete_type(generic_type: type, concrete_arg: type) -> type:
    """Create a concrete type from a generic type and a type argument.

    Args:
        generic_type: The generic type class (e.g., Connection)
        concrete_arg: The concrete type argument (e.g., Post)

    Returns:
        A new concrete type class (e.g., ConnectionPost)

    Example:
        ConnectionPost = create_concrete_type(Connection, Post)
    """
    if not hasattr(generic_type, "__fraiseql_definition__"):
        msg = f"Type {generic_type} must be decorated with @fraise_type to be used as a generic"
        raise TypeError(
            msg,
        )

    # Create a unique name for the concrete type
    generic_name = generic_type.__name__
    concrete_name = concrete_arg.__name__
    new_name = f"{generic_name}{concrete_name}"

    # Get the original fields and type hints
    original_fields = getattr(generic_type, "__gql_fields__", {})
    original_type_hints = getattr(generic_type, "__gql_type_hints__", {})

    # Create new fields with concrete types
    new_fields = {}
    new_type_hints = {}

    for field_name, field_def in original_fields.items():
        field_type = field_def.field_type or original_type_hints.get(field_name)
        if field_type is not None:
            # Replace TypeVar with concrete type
            concrete_field_type = _substitute_typevar(field_type, concrete_arg)

            # Create a new field with the concrete type
            from fraiseql.fields import FraiseQLField

            new_field = FraiseQLField(
                field_type=concrete_field_type,
                default=field_def.default,
                default_factory=field_def.default_factory,
                init=field_def.init,
                repr=field_def.repr,
                compare=field_def.compare,
                purpose=field_def.purpose,
                description=field_def.description,
            )
            new_field.name = field_def.name
            new_field.index = field_def.index

            new_fields[field_name] = new_field
            new_type_hints[field_name] = concrete_field_type

    # Create the new class dynamically
    new_class = type(
        new_name,
        (generic_type,),
        {
            "__module__": generic_type.__module__,
            "__qualname__": new_name,
            "__gql_fields__": new_fields,
            "__gql_type_hints__": new_type_hints,
        },
    )

    # Copy the FraiseQL definition
    original_def = generic_type.__fraiseql_definition__
    new_definition = FraiseQLTypeDefinition(
        python_type=new_class,
        is_input=original_def.is_input,
        kind=original_def.kind,
        sql_source=original_def.sql_source,
        fields=new_fields,
        type_hints=new_type_hints,
        is_frozen=original_def.is_frozen,
        kw_only=original_def.kw_only,
    )
    new_class.__fraiseql_definition__ = new_definition

    return new_class


def _substitute_typevar(type_annotation: Any, concrete_type: type) -> Any:
    """Substitute TypeVar instances with concrete types in a type annotation."""
    from types import UnionType
    from typing import Union

    # Handle TypeVar directly
    if isinstance(type_annotation, TypeVar):
        return concrete_type

    # Handle generic types like List[T]
    origin = get_origin(type_annotation)
    args = get_args(type_annotation)

    if origin is not None and args:
        # Recursively substitute in args
        new_args = tuple(_substitute_typevar(arg, concrete_type) for arg in args)

        # Handle Union and UnionType specially
        if origin in (Union, UnionType):
            # Use Union for consistency
            if len(new_args) == 2 and type(None) in new_args:
                # Handle Optional[T] (T | None)
                non_none_arg = new_args[0] if new_args[1] is type(None) else new_args[1]
                return Union[non_none_arg, type(None)]
            return Union[new_args]
        # Reconstruct the type with new args
        return origin[new_args]

    # Return as-is if no substitution needed
    return type_annotation


# Generic type cache to avoid recreating the same concrete types
_concrete_type_cache: dict[tuple[type, type], type] = {}


def get_or_create_concrete_type(generic_type: type, concrete_arg: type) -> type:
    """Get or create a concrete type, using cache for efficiency."""
    cache_key = (generic_type, concrete_arg)
    if cache_key not in _concrete_type_cache:
        _concrete_type_cache[cache_key] = create_concrete_type(generic_type, concrete_arg)
    return _concrete_type_cache[cache_key]


# Pagination generic types that will be used extensively
@fraise_type
class PageInfo:
    """Pagination information following Relay cursor specification."""

    has_next_page: bool = fraise_field(description="Whether there is a next page")
    has_previous_page: bool = fraise_field(description="Whether there is a previous page")
    start_cursor: str | None = fraise_field(default=None, description="Cursor for the first item")
    end_cursor: str | None = fraise_field(default=None, description="Cursor for the last item")
    total_count: int | None = fraise_field(default=None, description="Total number of items")

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "PageInfo":
        """Create PageInfo from dictionary."""
        return cls(
            has_next_page=data.get("has_next_page", False),
            has_previous_page=data.get("has_previous_page", False),
            start_cursor=data.get("start_cursor"),
            end_cursor=data.get("end_cursor"),
            total_count=data.get("total_count"),
        )


@fraise_type
class Edge(Generic[T]):
    """An edge in a connection containing a node and its cursor."""

    node: T = fraise_field(description="The item at this edge")
    cursor: str = fraise_field(description="Cursor for this item")


@fraise_type
class Connection(Generic[T]):
    """A paginated connection of nodes following Relay specification."""

    edges: list[Edge[T]] = fraise_field(description="List of edges in this connection")
    page_info: PageInfo = fraise_field(description="Information about pagination")
    total_count: int | None = fraise_field(default=None, description="Total number of items")


# Convenience alias for common usage (matches printoptim pattern)
PaginatedResponse = Connection


def create_connection(data: dict[str, Any], node_type: type[T]) -> Any:
    """Create a typed Connection from pagination result dictionary.

    This is a helper function to work around the limitation that the @fraise_type
    decorator overwrites custom from_dict methods.

    Args:
        data: Dictionary with edges, page_info, and optional total_count
        node_type: The type to use for nodes (must have from_dict method)

    Returns:
        Typed Connection instance

    Example:
        result = await repo.paginate("v_posts", first=20)
        connection = create_connection(result, Post)  # Returns Connection[Post]
    """
    # Create concrete Edge and Connection types
    edge_concrete = get_or_create_concrete_type(Edge, node_type)
    connection_concrete = get_or_create_concrete_type(Connection, node_type)

    # Convert edges
    edges = []
    for edge_data in data.get("edges", []):
        # Convert node data to typed object
        if hasattr(node_type, "from_dict"):
            node = node_type.from_dict(edge_data["node"])
        else:
            # Fallback for types without from_dict - just use constructor
            node = node_type(**edge_data["node"])

        # Create typed edge
        edge = edge_concrete(node=node, cursor=edge_data["cursor"])
        edges.append(edge)

    # Create page info
    page_info = PageInfo.from_dict(data.get("page_info", {}))

    # Create and return typed connection
    return connection_concrete(
        edges=edges,
        page_info=page_info,
        total_count=data.get("total_count"),
    )
