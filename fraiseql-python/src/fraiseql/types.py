"""Type mapping and introspection for GraphQL schema generation."""

from __future__ import annotations

from typing import TYPE_CHECKING, Annotated, Any, Union, get_args, get_origin

# Import FraiseQL scalars for type mapping
from fraiseql.scalars import (
    ID,
    UUID,
    Date,
    DateTime,
    Decimal,
    Json,
    Time,
    Vector,
)

if TYPE_CHECKING:
    from fraiseql.decorators import FieldConfig


# Mapping from FraiseQL scalar NewTypes to GraphQL type strings
# NewType creates a callable, so we need to map by __name__
_SCALAR_NAMES: dict[str, str] = {
    "ID": "ID",
    "UUID": "UUID",
    "DateTime": "DateTime",
    "Date": "Date",
    "Time": "Time",
    "Json": "Json",
    "Decimal": "Decimal",
    "Vector": "Vector",
}

# Keep references to prevent unused import warnings
_SCALARS = (ID, UUID, DateTime, Date, Time, Json, Decimal, Vector)


def python_type_to_graphql(py_type: Any) -> tuple[str, bool]:
    """Convert Python type hint to GraphQL type string.

    Args:
        py_type: Python type annotation (int, str, list[User], ID, DateTime, etc.)

    Returns:
        Tuple of (graphql_type, is_nullable)

    Examples:
        >>> python_type_to_graphql(int)
        ('Int', False)
        >>> python_type_to_graphql(str | None)
        ('String', True)
        >>> python_type_to_graphql(list[int])
        ('[Int]', False)
        >>> from fraiseql.scalars import ID, DateTime
        >>> python_type_to_graphql(ID)
        ('ID', False)
        >>> python_type_to_graphql(DateTime)
        ('DateTime', False)
    """
    origin = get_origin(py_type)
    args = get_args(py_type)

    # Handle Annotated types - extract the base type
    if origin is Annotated:
        # First arg is the actual type, rest are metadata
        base_type = args[0]
        return python_type_to_graphql(base_type)

    # Handle Union types (including | None for nullable)
    # This handles both typing.Union and the | operator (UnionType in Python 3.10+)
    is_union = (
        origin is Union
        or (hasattr(origin, "__name__") and origin.__name__ == "UnionType")
    )
    if is_union:
        # Check if it's a nullable type (T | None)
        if len(args) == 2 and type(None) in args:
            # Get the non-None type
            non_none_type = args[0] if args[1] is type(None) else args[1]
            base_type, _ = python_type_to_graphql(non_none_type)
            return (base_type, True)
        # Other unions not supported in GraphQL
        raise ValueError(f"Union types other than | None are not supported: {py_type}")

    # Handle list types
    if origin is list:
        if not args:
            raise ValueError("List type must have element type: list[T]")
        element_type, element_nullable = python_type_to_graphql(args[0])
        if element_nullable:
            return (f"[{element_type}]", False)
        return (f"[{element_type}!]", False)

    # Handle NewType scalars (ID, DateTime, etc.)
    # NewType creates a callable with __name__ and __supertype__ attributes
    if callable(py_type) and hasattr(py_type, "__supertype__"):
        type_name = getattr(py_type, "__name__", None)
        if type_name and type_name in _SCALAR_NAMES:
            return (_SCALAR_NAMES[type_name], False)

    # Handle basic Python types
    type_map = {
        int: "Int",
        float: "Float",
        str: "String",
        bool: "Boolean",
    }

    if py_type in type_map:
        return (type_map[py_type], False)

    # Custom types (classes decorated with @fraiseql.type)
    if isinstance(py_type, type):
        return (py_type.__name__, False)

    raise ValueError(f"Unsupported type: {py_type}")


def extract_field_config(field_type: Any) -> FieldConfig | None:
    """Extract FieldConfig from an Annotated type hint.

    Args:
        field_type: Python type annotation, possibly Annotated[T, FieldConfig(...)]

    Returns:
        FieldConfig if found in annotations, None otherwise
    """
    # Import here to avoid circular import
    from fraiseql.decorators import FieldConfig

    origin = get_origin(field_type)
    if origin is not Annotated:
        return None

    args = get_args(field_type)
    # args[0] is the type, args[1:] are metadata
    for arg in args[1:]:
        if isinstance(arg, FieldConfig):
            return arg

    return None


def extract_field_info(cls: type) -> dict[str, dict[str, Any]]:
    """Extract field information from a class with type annotations.

    Args:
        cls: Python class with type annotations

    Returns:
        Dictionary of field_name -> {"type": graphql_type, "nullable": bool, ...}

    Examples:
        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     name: str
        ...     email: str | None
        >>> extract_field_info(User)
        {
            "id": {"type": "Int", "nullable": False},
            "name": {"type": "String", "nullable": False},
            "email": {"type": "String", "nullable": True}
        }

        >>> from typing import Annotated
        >>> @fraiseql.type
        ... class Employee:
        ...     id: int
        ...     salary: Annotated[int, fraiseql.field(requires_scope="hr:compensation")]
        >>> extract_field_info(Employee)
        {
            "id": {"type": "Int", "nullable": False},
            "salary": {"type": "Int", "nullable": False, "requires_scope": "hr:compensation"}
        }
    """
    if not hasattr(cls, "__annotations__"):
        return {}

    fields = {}
    for field_name, field_type in cls.__annotations__.items():
        graphql_type, nullable = python_type_to_graphql(field_type)
        field_info: dict[str, Any] = {
            "type": graphql_type,
            "nullable": nullable,
        }

        # Check for FieldConfig metadata in Annotated types
        config = extract_field_config(field_type)
        if config is not None:
            if config.requires_scope:
                field_info["requires_scope"] = config.requires_scope
            if config.deprecated:
                field_info["deprecated"] = {"reason": config.deprecated}
            if config.description:
                field_info["description"] = config.description

        fields[field_name] = field_info

    return fields


def extract_function_signature(func: Any) -> dict[str, Any]:
    """Extract GraphQL-relevant information from function signature.

    Args:
        func: Python function with type annotations

    Returns:
        Dictionary with "arguments" and "return_type" information

    Examples:
        >>> @fraiseql.query
        ... def users(limit: int = 10, offset: int = 0) -> list[User]:
        ...     ...
        >>> extract_function_signature(users)
        {
            "arguments": [
                {"name": "limit", "type": "Int", "nullable": False, "default": 10},
                {"name": "offset", "type": "Int", "nullable": False, "default": 0}
            ],
            "return_type": {"type": "[User!]", "nullable": False, "is_list": True}
        }
    """
    import inspect

    sig = inspect.signature(func)
    annotations = func.__annotations__

    # Extract arguments
    arguments = []
    for param_name, param in sig.parameters.items():
        if param_name == "self" or param_name == "info":
            continue

        if param_name not in annotations:
            raise ValueError(f"Parameter {param_name} missing type annotation in {func.__name__}")

        param_type = annotations[param_name]
        graphql_type, nullable = python_type_to_graphql(param_type)

        arg_info: dict[str, Any] = {
            "name": param_name,
            "type": graphql_type,
            "nullable": nullable,
        }

        # Add default value if present
        if param.default is not inspect.Parameter.empty:
            arg_info["default"] = param.default

        arguments.append(arg_info)

    # Extract return type
    if "return" not in annotations:
        raise ValueError(f"Function {func.__name__} missing return type annotation")

    return_type_annotation = annotations["return"]
    return_type, return_nullable = python_type_to_graphql(return_type_annotation)

    # Check if return type is a list
    is_list = return_type.startswith("[") and return_type.endswith("]")

    return {
        "arguments": arguments,
        "return_type": {
            "type": return_type,
            "nullable": return_nullable,
            "is_list": is_list,
        },
    }
