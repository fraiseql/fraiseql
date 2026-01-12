"""Type mapping and introspection for GraphQL schema generation."""

from typing import Any, get_args, get_origin


def python_type_to_graphql(py_type: Any) -> tuple[str, bool]:
    """Convert Python type hint to GraphQL type string.

    Args:
        py_type: Python type annotation (int, str, list[User], User | None, etc.)

    Returns:
        Tuple of (graphql_type, is_nullable)

    Examples:
        >>> python_type_to_graphql(int)
        ('Int', False)
        >>> python_type_to_graphql(str | None)
        ('String', True)
        >>> python_type_to_graphql(list[int])
        ('[Int]', False)
    """
    origin = get_origin(py_type)
    args = get_args(py_type)

    # Handle Union types (including | None for nullable)
    if origin is type(...) or (hasattr(origin, "__name__") and origin.__name__ == "UnionType"):
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

    # Handle basic types
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


def extract_field_info(cls: type) -> dict[str, dict[str, Any]]:
    """Extract field information from a class with type annotations.

    Args:
        cls: Python class with type annotations

    Returns:
        Dictionary of field_name -> {"type": graphql_type, "nullable": bool}

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
    """
    if not hasattr(cls, "__annotations__"):
        return {}

    fields = {}
    for field_name, field_type in cls.__annotations__.items():
        graphql_type, nullable = python_type_to_graphql(field_type)
        fields[field_name] = {
            "type": graphql_type,
            "nullable": nullable,
        }

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
