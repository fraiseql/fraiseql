"""GraphQL type conversions and query translation utilities for FraiseQL.

Converts Python FraiseQL input and output dataclasses to GraphQL types.
Supports:
- FraiseQL output types with SQL backing (via @fraise_type)
- FraiseQL input types (via @fraise_input)
- Scalar, optional, list types
- Enum types (via @fraise_enum)
- Caching for repeated conversions
"""

from enum import Enum
from types import UnionType
from typing import (
    Annotated,
    Any,
    TypeVar,
    Union,
    cast,
    get_args,
    get_origin,
)

from graphql import (
    GraphQLEnumType,
    GraphQLError,
    GraphQLField,
    GraphQLInputField,
    GraphQLInputObjectType,
    GraphQLInterfaceType,
    GraphQLList,
    GraphQLObjectType,
    GraphQLScalarType,
    GraphQLType,
    GraphQLUnionType,
)
from psycopg.sql import SQL, Composed

from fraiseql.core.translate_query import translate_query
from fraiseql.mutations.decorators import FraiseUnion
from fraiseql.sql.where_generator import DynamicType
from fraiseql.types.scalars.graphql_utils import convert_scalar_to_graphql
from fraiseql.types.scalars.json import JSONScalar, parse_json_value
from fraiseql.utils.annotations import (
    get_non_optional_type,
    is_optional_type,
    unwrap_annotated,
)

_graphql_type_cache: dict[tuple[str, str], GraphQLType] = {}

DICT_ARG_LENGTH = 2

T = TypeVar("T", bound=type)


def _convert_fraise_union(
    typ: type[Any],
    annotation: FraiseUnion,
) -> GraphQLUnionType:
    origin = get_origin(typ)
    if origin not in (Union, UnionType):
        msg = f"FraiseUnion must wrap a union type, got: {typ!r} (origin: {origin})"
        raise TypeError(msg)

    args = get_args(typ)
    if not args:
        msg = f"FraiseUnion {annotation.name} has no union members"
        raise TypeError(msg)

    gql_object_types: list[GraphQLObjectType] = []

    for arg in args:
        gql = convert_type_to_graphql_output(arg)
        if not isinstance(gql, GraphQLObjectType):
            msg = f"GraphQLUnionType can only include GraphQLObjectType members, got: {type(gql)} from {arg!r}"
            raise TypeError(msg)
        gql_object_types.append(gql)

    def resolve_union_type(obj: Any, info: Any, type_: Any) -> str | None:
        """Resolve the GraphQL type name from a Python object."""
        return obj.__class__.__name__ if hasattr(obj, "__class__") else None

    union_type = GraphQLUnionType(
        name=annotation.name,
        types=gql_object_types,
        resolve_type=resolve_union_type,
    )

    key = (annotation.name, typ.__module__)
    _graphql_type_cache[key] = union_type
    return union_type


def _convert_list_type(
    origin: type | None,
    args: tuple[Any, ...],
    *,
    is_input: bool,
) -> GraphQLList[Any]:
    if origin is list and args:
        inner = args[0]
        inner_gql_type = (
            convert_type_to_graphql_input(inner)
            if is_input
            else convert_type_to_graphql_output(inner)
        )
        return GraphQLList(inner_gql_type)

    msg = f"Unsupported list type: {origin}[{args}]"
    raise TypeError(msg)


def convert_type_to_graphql_input(
    typ: Any,
) -> GraphQLInputObjectType | GraphQLScalarType | GraphQLList[Any] | GraphQLEnumType:
    """Convert a FraiseQL input class or scalar into a GraphQLInputObjectType or scalar."""
    typ, _ = unwrap_annotated(typ)

    # Handle Optional[...] or | None types (e.g., JSONField | None)
    if is_optional_type(typ):
        typ = get_non_optional_type(typ)

    # Handle generic types like PaginationInput[T]
    origin = get_origin(typ)
    args = get_args(typ)
    if origin is not None and args:
        # Import here to avoid circular imports
        from fraiseql.types.generic import (
            get_or_create_concrete_type,
            is_fraise_generic,
        )

        if is_fraise_generic(typ):
            # Create concrete type from generic
            concrete_type = get_or_create_concrete_type(origin, args[0])
            return convert_type_to_graphql_input(concrete_type)

    # Handle FraiseQL input objects
    if (
        isinstance(typ, type)
        and hasattr(typ, "__fraiseql_definition__")
        and getattr(typ.__fraiseql_definition__, "kind", None) == "input"
    ):
        # Use the already collected fields from the decorator
        fields = getattr(typ, "__gql_fields__", {})
        type_hints = getattr(typ, "__gql_type_hints__", {})

        gql_fields = {}
        for name, field in fields.items():
            field_type = field.field_type or type_hints.get(name)
            if field_type is None:
                continue

            # Check for JSONScalar and validate data
            if field_type == JSONScalar:
                try:
                    # Assuming the field has some default value to validate
                    parse_json_value(
                        getattr(typ, name, None)
                    )  # Validate the field's default value
                except GraphQLError as e:
                    msg = f"Invalid JSON value in field {name}: {e!s}"
                    raise GraphQLError(msg) from None

            gql_fields[name] = GraphQLInputField(
                convert_type_to_graphql_input(field_type)
            )

        return GraphQLInputObjectType(name=typ.__name__, fields=gql_fields)

    # Handle list types like List[str]
    origin = get_origin(typ)
    if origin is list:
        inner = get_args(typ)[0]
        return GraphQLList(convert_type_to_graphql_input(inner))

    # Handle enum types
    if isinstance(typ, type) and issubclass(typ, Enum):
        # Check if it has been decorated with @fraise_enum
        graphql_type = getattr(typ, "__graphql_type__", None)
        if isinstance(graphql_type, GraphQLEnumType):
            return graphql_type
        # If not decorated, raise error
        msg = f"Enum {typ.__name__} must be decorated with @fraise_enum to be used in GraphQL schema"
        raise TypeError(msg)

    # Handle scalar types using the existing scalar mapping utility
    if isinstance(typ, type):
        try:
            return convert_scalar_to_graphql(typ)
        except TypeError:
            msg = f"Invalid type passed to convert_type_to_graphql_input: {typ}"
            raise TypeError(msg) from None

    msg = f"Invalid type passed to convert_type_to_graphql_input: {typ}"
    raise TypeError(msg)


def convert_type_to_graphql_output(
    typ: Any,
) -> (
    GraphQLObjectType
    | GraphQLList[Any]
    | GraphQLScalarType
    | GraphQLUnionType
    | GraphQLInterfaceType
    | GraphQLEnumType
):
    """Convert a FraiseQL output type to a corresponding GraphQL output type."""
    # Handle Annotated[T, ...]
    if get_origin(typ) is Annotated:
        base_type, *annotations = get_args(typ)
        for annotation in annotations:
            if isinstance(annotation, FraiseUnion):
                return _convert_fraise_union(base_type, annotation)
        typ = base_type

    # Handle Optional[T] (e.g., T | None)
    if is_optional_type(typ):
        return convert_type_to_graphql_output(get_non_optional_type(typ))

    # Handle generic types like Connection[Post], Edge[User], etc.
    origin = get_origin(typ)
    args = get_args(typ)
    if origin is not None and args:
        # Import here to avoid circular imports
        from fraiseql.types.generic import (
            get_or_create_concrete_type,
            is_fraise_generic,
        )

        if is_fraise_generic(typ):
            # Create concrete type from generic (e.g., Connection[Post] -> ConnectionPost)
            concrete_type = get_or_create_concrete_type(origin, args[0])
            return convert_type_to_graphql_output(concrete_type)

    # Disallow plain Union/UnionType
    if get_origin(typ) in (Union, UnionType):
        msg = "Use a FraiseUnion wrapper for result unions, not plain Union"
        raise TypeError(msg)

    # Handle list types
    if get_origin(typ) is list:
        (inner_type,) = get_args(typ)
        inner_gql_type = convert_type_to_graphql_output(inner_type)
        return GraphQLList(inner_gql_type)

    # Handle Any as JSON scalar
    if typ == Any or str(typ) == "typing.Any":
        return convert_scalar_to_graphql(dict)

    # Handle enum types
    if isinstance(typ, type) and issubclass(typ, Enum):
        # Check if it has been decorated with @fraise_enum
        graphql_type = getattr(typ, "__graphql_type__", None)
        if isinstance(graphql_type, GraphQLEnumType):
            return graphql_type
        # If not decorated, raise error
        msg = f"Enum {typ.__name__} must be decorated with @fraise_enum to be used in GraphQL schema"
        raise TypeError(msg)

    # Handle built-in scalar types
    try:
        return convert_scalar_to_graphql(typ)
    except TypeError:
        pass  # Not a scalar — continue

    # Cache based on name/module for user-defined types
    if isinstance(typ, type):
        key = (typ.__name__, typ.__module__)
        if key in _graphql_type_cache:
            return cast(
                GraphQLObjectType
                | GraphQLList[Any]
                | GraphQLScalarType
                | GraphQLUnionType
                | GraphQLInterfaceType,
                _graphql_type_cache[key],
            )

        # Handle FraiseQL object-like types
        if hasattr(typ, "__fraiseql_definition__"):
            definition = typ.__fraiseql_definition__
            if definition.kind in {"type", "success", "failure", "output"}:
                # Use the already collected fields from the decorator
                fields = getattr(typ, "__gql_fields__", {})
                type_hints = getattr(typ, "__gql_type_hints__", {})

                gql_fields = {}
                for name, field in fields.items():
                    field_type = field.field_type or type_hints.get(name)
                    if field_type is not None:
                        # Create a field resolver that handles enum serialization
                        def make_field_resolver(field_name: str):
                            def resolve_field(obj: Any, info: Any) -> Any:
                                value = getattr(obj, field_name, None)
                                # Handle enum serialization at field level
                                if isinstance(value, Enum):
                                    return value.name
                                elif isinstance(value, list):
                                    # Handle lists of enums
                                    return [
                                        item.name if isinstance(item, Enum) else item
                                        for item in value
                                    ]
                                return value

                            return resolve_field

                        gql_fields[name] = GraphQLField(
                            type_=convert_type_to_graphql_output(field_type),
                            description=field.description,
                            resolve=make_field_resolver(name),
                        )

                # Get interfaces this type implements
                interfaces = []
                if hasattr(typ, "__fraiseql_interfaces__"):
                    for interface_cls in typ.__fraiseql_interfaces__:
                        interface_gql = convert_type_to_graphql_output(interface_cls)
                        if isinstance(interface_gql, GraphQLInterfaceType):
                            interfaces.append(interface_gql)

                # Add is_type_of function to help with interface resolution
                def is_type_of(obj, info):
                    """Check if an object is of this type."""
                    return (
                        obj.__class__.__name__ == typ.__name__
                        if hasattr(obj, "__class__")
                        else False
                    )

                gql_type = GraphQLObjectType(
                    name=typ.__name__,
                    fields=gql_fields,
                    interfaces=interfaces if interfaces else None,
                    is_type_of=is_type_of,
                )
                _graphql_type_cache[key] = gql_type
                return gql_type
            elif definition.kind == "interface":
                # Handle interface types
                fields = getattr(typ, "__gql_fields__", {})
                type_hints = getattr(typ, "__gql_type_hints__", {})

                gql_fields = {}
                for name, field in fields.items():
                    field_type = field.field_type or type_hints.get(name)
                    if field_type is not None:
                        gql_fields[name] = GraphQLField(
                            type_=convert_type_to_graphql_output(field_type),
                            description=field.description,
                        )

                # Create interface type with type resolver
                def resolve_type(obj, info, type_):
                    """Resolve the concrete type for an interface."""
                    if hasattr(obj, "__class__") and hasattr(obj.__class__, "__name__"):
                        return obj.__class__.__name__
                    return None

                gql_type = GraphQLInterfaceType(
                    name=typ.__name__,
                    fields=gql_fields,
                    resolve_type=resolve_type,
                    description=typ.__doc__,
                )
                _graphql_type_cache[key] = gql_type
                return gql_type

    msg = f"Unsupported output type: {typ}"
    raise TypeError(msg)


def translate_query_from_type(
    query: str,
    root_type: type[Any],
    *,
    where: DynamicType | None = None,
    auto_camel_case: bool = False,
) -> SQL | Composed:
    """Missing docstring."""
    if (
        not hasattr(root_type, "__gql_typename__")
        or not hasattr(root_type, "__gql_table__")
        or root_type.__gql_table__ is None
    ):
        msg = f"{root_type.__name__} must be a FraiseQL output type decorated with @fraise_type and linked to a SQL table"
        raise ValueError(
            msg,
        )
    where_clause: SQL | None = None
    if where:
        where_clause = where.to_sql()
    table: str = cast(str, root_type.__gql_table__)
    typename: str = cast(str, root_type.__gql_typename__)
    return translate_query(
        query=query,
        table=table,
        typename=typename,
        where_clause=where_clause,
        auto_camel_case=auto_camel_case,
    )
