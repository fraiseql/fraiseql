"""GraphQL type annotation helpers for FraiseQL.

Provides utility functions for working with Python typing annotations used
in FraiseQL input/output type conversion. Supports detection and unwrapping of:
- `Annotated[...]` types and their metadata
- `Optional[...]` and `| None` unions
- underlying base types

These helpers are used during GraphQL schema generation.
"""

import types
from typing import Annotated, Any, Union, get_args, get_origin


def unwrap_annotated(typ: type) -> tuple[type, list[Any]]:
    """Unwrap Annotated[...] types and return the base type and metadata.

    Args:
        typ: A type, possibly wrapped with typing.Annotated.

    Returns:
        A tuple of (base type, list of metadata annotations).
    """
    origin = get_origin(typ)
    if origin is Annotated:
        base_type = get_args(typ)[0]  # The main type, e.g., str in Annotated[str]
        annotations = list(get_args(typ)[1:])  # Any annotations after the base type
        return (
            base_type,
            annotations,
        )  # Return the base type and the list of annotations
    return typ, []


def is_optional_type(typ: type[Any]) -> bool:
    """Check if a type is Optional[...] or includes None in a union.

    Args:
        typ: The type to check.

    Returns:
        True if the type is optional (i.e., includes None).
    """
    origin = get_origin(typ)
    args = get_args(typ)
    return origin in (Union, types.UnionType) and any(a is type(None) or a is None for a in args)


def get_non_optional_type(typ: type[Any]) -> type[Any]:
    """Extract the non-None part of an Optional[...] or union type.

    Args:
        typ: A possibly optional type.

    Returns:
        The type excluding None.

    Raises:
        TypeError: If a valid non-optional type could not be determined.
    """
    if not is_optional_type(typ):
        return typ
    args = get_args(typ)
    non_none_args = [arg for arg in args if arg is not type(None) and arg is not None]
    if len(non_none_args) == 1:
        return non_none_args[0]
    if len(non_none_args) > 1:
        return non_none_args[0] | non_none_args[1]
    msg = f"Could not extract a valid non-optional type from {typ}. Arguments were: {args}"
    raise TypeError(msg)
