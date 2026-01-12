"""Decorators for FraiseQL schema authoring (compile-time only)."""

from collections.abc import Callable
from types import FunctionType
from typing import Any, TypeVar

from fraiseql.registry import SchemaRegistry
from fraiseql.types import extract_field_info, extract_function_signature

T = TypeVar("T")
F = TypeVar("F", bound=FunctionType)


def type(cls: type[T]) -> type[T]:
    """Decorator to mark a Python class as a GraphQL type.

    This decorator registers the class with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        cls: Python class with type annotations

    Returns:
        The original class (unmodified)

    Examples:
        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     name: str
        ...     email: str | None
        ...     created_at: str

        This generates JSON:
        {
            "name": "User",
            "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "name", "type": "String", "nullable": false},
                {"name": "email", "type": "String", "nullable": true},
                {"name": "created_at", "type": "String", "nullable": false}
            ]
        }

    Notes:
        - Class must have type annotations for all fields
        - Supports nullable types via | None syntax
        - Supports nested types (other @fraiseql.type classes)
        - Supports lists via list[T] syntax
    """
    # Extract field information from class annotations
    fields = extract_field_info(cls)

    # Register type with schema registry
    SchemaRegistry.register_type(
        name=cls.__name__,
        fields=fields,
        description=cls.__doc__,
    )

    # Return original class unmodified (no runtime behavior)
    return cls


def query(
    func: F | None = None, **config_kwargs: Any
) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL query.

    This decorator registers the function with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        func: Python function with type annotations
        **config_kwargs: Configuration options (sql_source, auto_params, etc.)

    Returns:
        The original function (unmodified)

    Examples:
        >>> @fraiseql.query(sql_source="v_user")
        ... def users(limit: int = 10, offset: int = 0) -> list[User]:
        ...     '''Get all users.'''
        ...     pass

        This generates JSON:
        {
            "name": "users",
            "return_type": "User",
            "returns_list": true,
            "nullable": false,
            "arguments": [
                {"name": "limit", "type": "Int", "nullable": false, "default": 10},
                {"name": "offset", "type": "Int", "nullable": false, "default": 0}
            ],
            "sql_source": "v_user"
        }

    Notes:
        - Function must have type annotations for all parameters and return type
        - Pass configuration as decorator arguments: @query(sql_source="v_user")
        - Returns list[T] for list queries
        - Returns T | None for nullable results
    """

    def decorator(f: F) -> F:
        # Extract function signature
        signature = extract_function_signature(f)

        # Register query with schema registry
        SchemaRegistry.register_query(
            name=f.__name__,
            return_type=signature["return_type"]["type"],
            returns_list=signature["return_type"]["is_list"],
            nullable=signature["return_type"]["nullable"],
            arguments=signature["arguments"],
            description=f.__doc__,
            **config_kwargs,
        )

        # Return original function unmodified
        return f

    # Support both @query and @query(...)
    if func is None:
        # Called with arguments: @query(sql_source="...")
        return decorator
    # Called without arguments: @query
    return decorator(func)


def mutation(
    func: F | None = None, **config_kwargs: Any
) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL mutation.

    This decorator registers the function with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        func: Python function with type annotations
        **config_kwargs: Configuration options (sql_source, operation, etc.)

    Returns:
        The original function (unmodified)

    Examples:
        >>> @fraiseql.mutation(sql_source="fn_create_user", operation="CREATE")
        ... def create_user(name: str, email: str) -> User:
        ...     '''Create a new user.'''
        ...     pass

        This generates JSON:
        {
            "name": "create_user",
            "return_type": "User",
            "returns_list": false,
            "nullable": false,
            "arguments": [
                {"name": "name", "type": "String", "nullable": false},
                {"name": "email", "type": "String", "nullable": false}
            ],
            "sql_source": "fn_create_user",
            "operation": "CREATE"
        }

    Notes:
        - Function must have type annotations for all parameters and return type
        - Pass configuration as decorator arguments: @mutation(sql_source="...", operation="CREATE")
        - operation: CREATE, UPDATE, DELETE, or CUSTOM
    """

    def decorator(f: F) -> F:
        # Extract function signature
        signature = extract_function_signature(f)

        # Register mutation with schema registry
        SchemaRegistry.register_mutation(
            name=f.__name__,
            return_type=signature["return_type"]["type"],
            returns_list=signature["return_type"]["is_list"],
            nullable=signature["return_type"]["nullable"],
            arguments=signature["arguments"],
            description=f.__doc__,
            **config_kwargs,
        )

        # Return original function unmodified
        return f

    # Support both @mutation and @mutation(...)
    if func is None:
        # Called with arguments: @mutation(sql_source="...")
        return decorator
    # Called without arguments: @mutation
    return decorator(func)
