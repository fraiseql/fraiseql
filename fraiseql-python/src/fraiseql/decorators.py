"""Decorators for FraiseQL schema authoring (compile-time only)."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum as PythonEnum
from types import FunctionType
from typing import TYPE_CHECKING, Any, Generic, TypeVar

from fraiseql.registry import SchemaRegistry
from fraiseql.types import extract_field_info, extract_function_signature

if TYPE_CHECKING:
    from collections.abc import Callable

T = TypeVar("T")
F = TypeVar("F", bound=FunctionType)
E = TypeVar("E", bound=PythonEnum)


@dataclass
class FieldConfig(Generic[T]):
    """Configuration for a GraphQL field with access control.

    This is used as a type annotation wrapper to add metadata to fields,
    particularly for field-level access control.

    Examples:
        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     name: str
        ...     # Protected field - requires scope to access
        ...     salary: Annotated[int, fraiseql.field(requires_scope="read:User.salary")]
        ...     ssn: Annotated[str, fraiseql.field(requires_scope="hr:view_pii")]

    Attributes:
        requires_scope: Scope required to access this field (e.g., "read:User.salary")
        deprecated: Deprecation reason if field is deprecated
        description: Field description for GraphQL schema
    """

    requires_scope: str | None = None
    deprecated: str | None = None
    description: str | None = None


def field(
    *,
    requires_scope: str | None = None,
    deprecated: str | None = None,
    description: str | None = None,
) -> FieldConfig[Any]:
    """Create a field configuration for use with Annotated type hints.

    This function is used to add metadata to GraphQL fields, particularly
    for field-level access control via JWT scopes.

    Args:
        requires_scope: Scope required to access this field.
            If set, users must have this scope in their JWT to query this field.
            Supports patterns like "read:Type.field" or custom scopes like "hr:view_pii".
        deprecated: Deprecation reason if field is deprecated.
        description: Field description for GraphQL schema documentation.

    Returns:
        FieldConfig instance for use with Annotated[T, field(...)]

    Examples:
        >>> from typing import Annotated
        >>> import fraiseql

        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     name: str
        ...     # Requires specific scope to access
        ...     salary: Annotated[int, fraiseql.field(requires_scope="read:User.salary")]
        ...     # Custom scope for PII
        ...     ssn: Annotated[str, fraiseql.field(requires_scope="hr:view_pii")]
        ...     # Deprecated field
        ...     old_email: Annotated[str, fraiseql.field(deprecated="Use email instead")]

        This generates JSON:
        {
            "name": "User",
            "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "name", "type": "String", "nullable": false},
                {"name": "salary", "type": "Int", "nullable": false, "requires_scope": "read:User.salary"},
                {"name": "ssn", "type": "String", "nullable": false, "requires_scope": "hr:view_pii"},
                {"name": "old_email", "type": "String", "nullable": false, "deprecated": {"reason": "Use email instead"}}
            ]
        }

    Notes:
        - Use with typing.Annotated for type safety
        - Multiple FieldConfig annotations on a field are merged
        - Scope format should match your JWT token structure
        - The runtime will reject queries for protected fields without proper scopes
    """
    return FieldConfig(
        requires_scope=requires_scope,
        deprecated=deprecated,
        description=description,
    )


def type(
    cls: type[T] | None = None,
    *,
    implements: list[str] | None = None,
) -> type[T] | Callable[[type[T]], type[T]]:
    """Decorator to mark a Python class as a GraphQL type.

    This decorator registers the class with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        cls: Python class with type annotations
        implements: List of interface names this type implements

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

        >>> @fraiseql.type(implements=["Node"])
        ... class User:
        ...     id: str  # Required by Node interface
        ...     name: str

        This generates JSON with implements:
        {
            "name": "User",
            "fields": [...],
            "implements": ["Node"]
        }

    Notes:
        - Class must have type annotations for all fields
        - Supports nullable types via | None syntax
        - Supports nested types (other @fraiseql.type classes)
        - Supports lists via list[T] syntax
        - Use implements=["InterfaceName"] to implement interfaces
    """

    def decorator(c: type[T]) -> type[T]:
        # Mark class as a FraiseQL type
        c.__fraiseql_type__ = True

        # Check for invalid federation usage
        federation_metadata = getattr(c, "__fraiseql_federation__", None)
        if federation_metadata:
            # Check if @external was used without @extends
            annotations = getattr(c, "__annotations__", {})
            for field_name in annotations:
                from fraiseql.federation import FieldDefault
                field_default = getattr(c, field_name, None)
                if isinstance(field_default, FieldDefault):
                    if field_default.external and not federation_metadata.get("extend", False):
                        from fraiseql.errors import FederationValidationError
                        raise FederationValidationError(
                            "@external requires @extends"
                        )

        # Extract field information from class annotations
        fields = extract_field_info(c)

        # Register type with schema registry
        SchemaRegistry.register_type(
            name=c.__name__,
            fields=fields,
            description=c.__doc__,
            implements=implements or [],
            federation=federation_metadata,
        )

        # Return original class unmodified (no runtime behavior)
        return c

    # Support both @type and @type(...)
    if cls is None:
        # Called with arguments: @type(implements=["Node"])
        return decorator
    # Called without arguments: @type
    return decorator(cls)


def query(func: F | None = None, **config_kwargs: Any) -> F | Callable[[F], F]:
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


def mutation(func: F | None = None, **config_kwargs: Any) -> F | Callable[[F], F]:
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


def enum(cls: type[E]) -> type[E]:
    """Decorator to mark a Python enum as a GraphQL enum.

    This decorator registers the enum with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        cls: Python Enum class

    Returns:
        The original enum class (unmodified)

    Examples:
        >>> @fraiseql.enum
        ... class OrderStatus(Enum):
        ...     '''Status of an order.'''
        ...     PENDING = "pending"
        ...     PROCESSING = "processing"
        ...     SHIPPED = "shipped"
        ...     DELIVERED = "delivered"

        This generates JSON:
        {
            "name": "OrderStatus",
            "description": "Status of an order.",
            "values": [
                {"name": "PENDING"},
                {"name": "PROCESSING"},
                {"name": "SHIPPED"},
                {"name": "DELIVERED"}
            ]
        }

    Notes:
        - Class must inherit from enum.Enum
        - Values are extracted from member names (not values)
        - Use docstrings on the class for descriptions
        - Use `deprecated` marker function for deprecated values
    """
    # Validate that cls is an Enum
    if not issubclass(cls, PythonEnum):
        raise TypeError(f"@enum can only be applied to Enum classes, got {cls.__name__}")

    # Extract enum values
    values = []
    for member in cls:
        value_info: dict[str, Any] = {"name": member.name}

        # Check for deprecation (can be set via class attribute)
        if hasattr(member, "_deprecated"):
            deprecated_info = member._deprecated  # noqa: SLF001
            value_info["deprecated"] = {"reason": deprecated_info}

        values.append(value_info)

    # Register enum with schema registry
    SchemaRegistry.register_enum(
        name=cls.__name__,
        values=values,
        description=cls.__doc__,
    )

    # Return original class unmodified
    return cls


def interface(cls: type[T]) -> type[T]:
    """Decorator to mark a Python class as a GraphQL interface.

    This decorator registers the class with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Interfaces define a common set of fields that multiple object types can implement.
    Per GraphQL spec §3.7, interfaces enable polymorphic queries.

    Args:
        cls: Python class with type annotations

    Returns:
        The original class (unmodified)

    Examples:
        >>> @fraiseql.interface
        ... class Node:
        ...     '''An object with a globally unique ID.'''
        ...     id: str

        >>> @fraiseql.type(implements=["Node"])
        ... class User:
        ...     id: str
        ...     name: str

        This generates JSON:
        {
            "interfaces": [{
                "name": "Node",
                "fields": [{"name": "id", "type": "ID", "nullable": false}],
                "description": "An object with a globally unique ID."
            }],
            "types": [{
                "name": "User",
                "fields": [...],
                "implements": ["Node"]
            }]
        }

    Notes:
        - Class must have type annotations for all fields
        - All implementing types must have the same fields (validated at compile time)
    """
    # Extract field information from class annotations
    fields = extract_field_info(cls)

    # Register interface with schema registry
    SchemaRegistry.register_interface(
        name=cls.__name__,
        fields=fields,
        description=cls.__doc__,
    )

    # Return original class unmodified (no runtime behavior)
    return cls


def input(cls: type[T]) -> type[T]:
    """Decorator to mark a Python class as a GraphQL input object.

    This decorator registers the class with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        cls: Python class with type annotations

    Returns:
        The original class (unmodified)

    Examples:
        >>> @fraiseql.input
        ... class CreateUserInput:
        ...     '''Input for creating a new user.'''
        ...     name: str
        ...     email: str
        ...     role: str = "user"

        This generates JSON:
        {
            "name": "CreateUserInput",
            "description": "Input for creating a new user.",
            "fields": [
                {"name": "name", "type": "String", "nullable": false},
                {"name": "email", "type": "String", "nullable": false},
                {"name": "role", "type": "String", "nullable": false, "default": "user"}
            ]
        }

    Notes:
        - Class must have type annotations for all fields
        - Supports nullable types via | None syntax
        - Supports default values
        - Use docstrings on the class for descriptions
    """
    # Extract field information from class annotations
    field_info = extract_field_info(cls)

    # Convert to list format with default values
    fields = []
    for field_name, info in field_info.items():
        field: dict[str, Any] = {
            "name": field_name,
            "type": info["type"],
            "nullable": info["nullable"],
        }

        # Check for default value
        if hasattr(cls, field_name):
            default_val = getattr(cls, field_name)
            # Only set default if it's not a descriptor or method
            if not callable(default_val) and not isinstance(default_val, property):
                field["default"] = default_val

        fields.append(field)

    # Register input with schema registry
    SchemaRegistry.register_input(
        name=cls.__name__,
        fields=fields,
        description=cls.__doc__,
    )

    # Return original class unmodified
    return cls


def subscription(
    func: F | None = None,
    *,
    entity_type: str | None = None,
    topic: str | None = None,
    operation: str | None = None,
    **config_kwargs: Any,
) -> F | Callable[[F], F]:
    """Decorator to mark a function as a GraphQL subscription.

    This decorator registers the function with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Subscriptions in FraiseQL are compiled projections of database events.
    They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.

    Args:
        func: Python function with type annotations
        entity_type: Entity type being subscribed to (defaults to return type)
        topic: Optional topic/channel name for filtering events
        operation: Optional operation filter ("CREATE", "UPDATE", "DELETE")
        **config_kwargs: Additional configuration options

    Returns:
        The original function (unmodified)

    Examples:
        >>> @fraiseql.subscription(entity_type="Order", topic="order_created")
        ... def order_created(user_id: str | None = None) -> Order:
        ...     '''Subscribe to new orders.'''
        ...     pass

        This generates JSON:
        {
            "name": "order_created",
            "entity_type": "Order",
            "nullable": false,
            "arguments": [
                {"name": "user_id", "type": "String", "nullable": true}
            ],
            "topic": "order_created"
        }

        >>> @fraiseql.subscription(operation="UPDATE")
        ... def user_updated() -> User:
        ...     '''Subscribe to user updates.'''
        ...     pass

    Notes:
        - Function must have type annotations for all parameters and return type
        - Return type determines the entity being subscribed to
        - Use topic to filter events to specific channels
        - Use operation to filter by CREATE/UPDATE/DELETE
        - Arguments become subscription filters (compiled, not resolved)
    """

    def decorator(f: F) -> F:
        # Extract function signature
        signature = extract_function_signature(f)

        # Determine entity type from return type or explicit parameter
        resolved_entity_type = entity_type or signature["return_type"]["type"]

        # Build config
        config: dict[str, Any] = {**config_kwargs}
        if topic:
            config["topic"] = topic
        if operation:
            config["operation"] = operation

        # Register subscription with schema registry
        SchemaRegistry.register_subscription(
            name=f.__name__,
            entity_type=resolved_entity_type,
            nullable=signature["return_type"]["nullable"],
            arguments=signature["arguments"],
            description=f.__doc__,
            **config,
        )

        # Return original function unmodified
        return f

    # Support both @subscription and @subscription(...)
    if func is None:
        # Called with arguments: @subscription(entity_type="Order")
        return decorator
    # Called without arguments: @subscription
    return decorator(func)


def union(
    name: str | None = None,
    members: list[type] | None = None,
) -> Callable[[type[T]], type[T]]:
    """Decorator to mark a Python class as a GraphQL union type.

    Per GraphQL spec §3.10, unions represent a type that could be one of
    several object types. Unlike interfaces, unions don't define common fields.

    This decorator registers the union with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        name: Optional union name (defaults to class name)
        members: List of member type classes

    Returns:
        Decorator function

    Examples:
        >>> @fraiseql.type
        ... class User:
        ...     id: str
        ...     name: str

        >>> @fraiseql.type
        ... class Post:
        ...     id: str
        ...     title: str

        >>> @fraiseql.union(members=[User, Post])
        ... class SearchResult:
        ...     '''Result from a search query.'''
        ...     pass

        This generates JSON:
        {
            "name": "SearchResult",
            "description": "Result from a search query.",
            "member_types": ["User", "Post"]
        }

    Notes:
        - Union members must be @fraiseql.type decorated classes
        - The decorated class itself is just a marker (body is ignored)
        - Use docstrings on the class for descriptions
    """

    def decorator(cls: type[T]) -> type[T]:
        union_name = name if name is not None else cls.__name__

        # Extract member type names
        member_types: list[str] = []
        if members:
            for member in members:
                member_types.append(member.__name__)

        # Register union with schema registry
        SchemaRegistry.register_union(
            name=union_name,
            member_types=member_types,
            description=cls.__doc__,
        )

        # Return original class unmodified (no runtime behavior)
        return cls

    return decorator
