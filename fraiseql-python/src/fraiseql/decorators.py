"""Decorators for FraiseQL schema authoring (compile-time only)."""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import Enum as PythonEnum
from types import FunctionType
from typing import TYPE_CHECKING, Any, Generic, TypeVar

from fraiseql.registry import SchemaRegistry
from fraiseql.scope import validate_scope
from fraiseql.types import extract_field_info, extract_function_signature

_INJECT_SOURCE_RE = re.compile(r"^jwt:[A-Za-z_][A-Za-z0-9_]*$")
_IDENTIFIER_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_SQL_IDENTIFIER_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)?$")


def _validate_sql_identifier(value: str, param: str, context: str) -> None:
    """Raise ValueError if value is not a safe SQL identifier.

    Valid: 'v_user', 'public.v_user', 'fn_create_post'
    Invalid: anything containing ; " -- spaces or SQL syntax
    """
    if not isinstance(value, str) or not _SQL_IDENTIFIER_RE.match(value):
        raise ValueError(
            f"{context}: {param}={value!r} is not a valid SQL identifier. "
            "Use only ASCII letters, digits, and underscores, with an optional "
            "schema prefix (e.g. 'v_user' or 'public.v_user')."
        )


def _validate_inject(
    inject: dict[str, str],
    arg_names: set[str],
    context: str,
) -> None:
    """Validate the ``inject`` mapping supplied to @query/@mutation.

    Args:
        inject: Mapping of SQL parameter name → source expression.
        arg_names: Names of the declared GraphQL arguments (must not overlap).
        context: Human-readable description of the decorator (for error messages).

    Raises:
        ValueError: If any key or value fails validation, or a name collision is found.
    """
    for param_name, source in inject.items():
        if not _IDENTIFIER_RE.match(param_name):
            msg = (
                f"{context}: inject key {param_name!r} is not a valid identifier. "
                "Keys must start with a letter or underscore and contain only "
                "letters, digits, and underscores."
            )
            raise ValueError(msg)
        if param_name in arg_names:
            msg = (
                f"{context}: inject key {param_name!r} conflicts with a declared "
                "GraphQL argument of the same name. Use a different parameter name."
            )
            raise ValueError(msg)
        if not _INJECT_SOURCE_RE.match(source):
            msg = (
                f"{context}: inject source {source!r} for param {param_name!r} is "
                "invalid. Supported format: 'jwt:<claim_name>' "
                "(e.g. 'jwt:org_id', 'jwt:sub')."
            )
            raise ValueError(msg)


if TYPE_CHECKING:
    from collections.abc import Callable

    from fraiseql.scalars import CustomScalar

T = TypeVar("T")
F = TypeVar("F", bound=FunctionType)
E = TypeVar("E", bound=PythonEnum)
S = TypeVar("S", bound="CustomScalar")


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
        ...     # Protected field with masking (returns null instead of rejecting)
        ...     email: Annotated[str, fraiseql.field(requires_scope="read:User.email", on_deny="mask")]

    Attributes:
        requires_scope: Scope required to access this field (e.g., "read:User.salary")
        on_deny: Policy when user lacks required scope: "reject" (default) or "mask"
        deprecated: Deprecation reason if field is deprecated
        description: Field description for GraphQL schema
    """

    requires_scope: str | None = None
    on_deny: str | None = None
    deprecated: str | None = None
    description: str | None = None


def field(
    *,
    requires_scope: str | None = None,
    on_deny: str | None = None,
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
            See fraiseql.scope module for format documentation.
        on_deny: Policy when user lacks the required scope.
            "reject" (default): entire query fails with FORBIDDEN error.
            "mask": query succeeds, field returns null.
        deprecated: Deprecation reason if field is deprecated.
        description: Field description for GraphQL schema documentation.

    Returns:
        FieldConfig instance for use with Annotated[T, field(...)]

    Raises:
        ScopeValidationError: If requires_scope format is invalid
        ValueError: If on_deny is not "reject" or "mask"

    Examples:
        >>> from typing import Annotated
        >>> import fraiseql

        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     name: str
        ...     # Requires specific scope to access (default: reject)
        ...     salary: Annotated[int, fraiseql.field(requires_scope="read:User.salary")]
        ...     # Mask mode: returns null for unauthorized users
        ...     email: Annotated[str, fraiseql.field(requires_scope="read:User.email", on_deny="mask")]
        ...     # Deprecated field
        ...     old_email: Annotated[str, fraiseql.field(deprecated="Use email instead")]
    """
    # Validate scope format
    validate_scope(requires_scope)

    # Validate on_deny
    if on_deny is not None:
        if on_deny not in ("reject", "mask"):
            msg = (
                f"on_deny must be 'reject' or 'mask' (got {on_deny!r}). "
                "'reject' fails the query, 'mask' returns null."
            )
            raise ValueError(msg)
        if requires_scope is None:
            msg = "on_deny has no effect without requires_scope"
            raise ValueError(msg)

    return FieldConfig(
        requires_scope=requires_scope,
        on_deny=on_deny,
        deprecated=deprecated,
        description=description,
    )


def type(
    cls: type[T] | None = None,
    *,
    implements: list[str] | None = None,
    relay: bool = False,
    requires_role: str | None = None,
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

        # Extract field information from class annotations
        fields = extract_field_info(c)

        # Register type with schema registry
        SchemaRegistry.register_type(
            name=c.__name__,
            fields=fields,
            description=c.__doc__,
            implements=implements or [],
            relay=relay,
            requires_role=requires_role,
        )

        # Return original class unmodified (no runtime behavior)
        return c

    # Support both @type and @type(...)
    if cls is None:
        # Called with arguments: @type(implements=["Node"]) or @type(relay=True)
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

        # Work with a local copy so we can safely modify it
        cfg = dict(config_kwargs)

        # sql_source validation — block injection at authoring time
        if sql_source := cfg.get("sql_source"):
            _validate_sql_identifier(sql_source, "sql_source", f"@fraiseql.query on {f.__name__!r}")

        # Inject validation — fail fast at authoring time
        if inject := cfg.get("inject"):
            if not isinstance(inject, dict):
                msg = (
                    f"@fraiseql.query inject= on {f.__name__!r} must be a dict "
                    f"(got {inject.__class__.__name__!r})."
                )
                raise TypeError(msg)
            arg_names = {arg["name"] for arg in signature["arguments"]}
            _validate_inject(inject, arg_names, f"@fraiseql.query {f.__name__!r}")
            # Emit structured inject_params alongside raw inject
            cfg["inject_params"] = {
                k: {"source": v.split(":", 1)[0], "claim": v.split(":", 1)[1]}
                for k, v in inject.items()
            }

        # cache_ttl_seconds validation — fail fast at authoring time
        if (ttl := cfg.get("cache_ttl_seconds")) is not None:
            if not isinstance(ttl, int) or ttl < 0:
                msg = (
                    f"@fraiseql.query cache_ttl_seconds= on {f.__name__!r} must be a "
                    f"non-negative integer (got {ttl!r})."
                )
                raise TypeError(msg)

        # additional_views validation — fail fast at authoring time
        if (av := cfg.get("additional_views")) is not None:
            if not isinstance(av, list):
                msg = (
                    f"@fraiseql.query additional_views= on {f.__name__!r} must be a list "
                    f"(got {av.__class__.__name__!r})."
                )
                raise TypeError(msg)
            for entry in av:
                if not isinstance(entry, str) or not _IDENTIFIER_RE.match(entry):
                    msg = (
                        f"@fraiseql.query additional_views= on {f.__name__!r}: "
                        f"entry {entry!r} is not a valid SQL identifier. "
                        "Use only letters, digits, and underscores (must start with a letter "
                        "or underscore)."
                    )
                    raise ValueError(msg)

        # deprecated= → deprecation={reason: ...}
        if "deprecated" in cfg:
            cfg["deprecation"] = {"reason": cfg.pop("deprecated")}

        # auto_params=True → expand to all-true dict
        if cfg.get("auto_params") is True:
            cfg["auto_params"] = {"where": True, "order_by": True, "limit": True, "offset": True}

        # Relay validation — fail fast at authoring time
        if cfg.get("relay"):
            if not signature["return_type"]["is_list"]:
                msg = (
                    f"@fraiseql.query relay=True on {f.__name__!r} requires a list return type "
                    f"(got {signature['return_type']['type']!r}). "
                    "Relay connections only apply to list queries."
                )
                raise ValueError(msg)
            if not cfg.get("sql_source"):
                msg = (
                    f"@fraiseql.query relay=True on {f.__name__!r} requires sql_source to be set. "
                    "The compiler needs the view name to derive the cursor column."
                )
                raise ValueError(msg)
            # Strip limit/offset from auto_params — relay uses first/after/last/before instead
            if "auto_params" in cfg:
                ap = dict(cfg["auto_params"])
                ap.pop("limit", None)
                ap.pop("offset", None)
                cfg["auto_params"] = ap

        # Register query with schema registry
        # description= in cfg overrides the docstring
        description = cfg.pop("description", f.__doc__)
        SchemaRegistry.register_query(
            name=f.__name__,
            return_type=signature["return_type"]["type"],
            returns_list=signature["return_type"]["is_list"],
            nullable=signature["return_type"]["nullable"],
            arguments=signature["arguments"],
            description=description,
            **cfg,
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

        # Work with a local copy so we can safely modify it
        cfg = dict(config_kwargs)

        # sql_source validation — block injection at authoring time
        if sql_source := cfg.get("sql_source"):
            _validate_sql_identifier(
                sql_source, "sql_source", f"@fraiseql.mutation on {f.__name__!r}"
            )

        # Inject validation — fail fast at authoring time
        if inject := cfg.get("inject"):
            if not isinstance(inject, dict):
                msg = (
                    f"@fraiseql.mutation inject= on {f.__name__!r} must be a dict "
                    f"(got {inject.__class__.__name__!r})."
                )
                raise TypeError(msg)
            arg_names = {arg["name"] for arg in signature["arguments"]}
            _validate_inject(inject, arg_names, f"@fraiseql.mutation {f.__name__!r}")
            # Emit structured inject_params alongside raw inject
            cfg["inject_params"] = {
                k: {"source": v.split(":", 1)[0], "claim": v.split(":", 1)[1]}
                for k, v in inject.items()
            }

        # deprecated= → deprecation={reason: ...}
        if "deprecated" in cfg:
            cfg["deprecation"] = {"reason": cfg.pop("deprecated")}

        # invalidates_fact_tables validation — fail fast at authoring time
        if (ift := cfg.get("invalidates_fact_tables")) is not None:
            if not isinstance(ift, list):
                msg = (
                    f"@fraiseql.mutation invalidates_fact_tables= on {f.__name__!r} "
                    f"must be a list (got {ift.__class__.__name__!r})."
                )
                raise TypeError(msg)
            for entry in ift:
                if not isinstance(entry, str) or not _IDENTIFIER_RE.match(entry):
                    msg = (
                        f"@fraiseql.mutation invalidates_fact_tables= on {f.__name__!r}: "
                        f"entry {entry!r} is not a valid SQL identifier. "
                        "Use only letters, digits, and underscores (must start with a "
                        "letter or underscore)."
                    )
                    raise ValueError(msg)

        # invalidates_views validation — fail fast at authoring time
        if (iv := cfg.get("invalidates_views")) is not None:
            if not isinstance(iv, list):
                msg = (
                    f"@fraiseql.mutation invalidates_views= on {f.__name__!r} "
                    f"must be a list (got {iv.__class__.__name__!r})."
                )
                raise TypeError(msg)
            for entry in iv:
                if not isinstance(entry, str) or not _IDENTIFIER_RE.match(entry):
                    msg = (
                        f"@fraiseql.mutation invalidates_views= on {f.__name__!r}: "
                        f"entry {entry!r} is not a valid SQL identifier. "
                        "Use only letters, digits, and underscores (must start with a "
                        "letter or underscore)."
                    )
                    raise ValueError(msg)

        # Register mutation with schema registry
        # description= in cfg overrides the docstring
        description = cfg.pop("description", f.__doc__)
        SchemaRegistry.register_mutation(
            name=f.__name__,
            return_type=signature["return_type"]["type"],
            returns_list=signature["return_type"]["is_list"],
            nullable=signature["return_type"]["nullable"],
            arguments=signature["arguments"],
            description=description,
            **cfg,
        )

        # Return original function unmodified
        return f

    # Support both @mutation and @mutation(...)
    if func is None:
        # Called with arguments: @mutation(sql_source="...")
        return decorator
    # Called without arguments: @mutation
    return decorator(func)


def error(cls: type[T]) -> type[T]:
    """Decorator to mark a Python class as a GraphQL error type.

    Like @type, but sets is_error=True in the schema output. Error types are
    returned by mutations when an operation fails, and their fields are populated
    from the mutation_response.metadata JSONB column.

    Args:
        cls: Python class with type annotations

    Returns:
        The original class (unmodified)

    Examples:
        >>> @fraiseql.error
        ... class UserNotFound:
        ...     '''Error when user lookup fails.'''
        ...     message: str
        ...     code: str
    """
    cls.__fraiseql_type__ = True  # type: ignore[attr-defined]
    fields = extract_field_info(cls)
    SchemaRegistry.register_type(
        name=cls.__name__,
        fields=fields,
        description=cls.__doc__,
        is_error=True,
    )
    return cls


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


def scalar(cls: type[S]) -> type[S]:
    """Decorator to register a custom scalar with the schema.

    This decorator registers the scalar globally so it can be:
    1. Used in type annotations
    2. Exported to schema.json
    3. Validated at runtime

    Args:
        cls: CustomScalar subclass

    Returns:
        The original class (unmodified)

    Raises:
        TypeError: If class doesn't inherit from CustomScalar
        ValueError: If scalar name is not unique or invalid

    Examples:
        >>> from fraiseql import CustomScalar, scalar

        >>> @scalar
        ... class Email(CustomScalar):
        ...     name = "Email"
        ...
        ...     def serialize(self, value):
        ...         return str(value)
        ...
        ...     def parse_value(self, value):
        ...         if "@" not in str(value):
        ...             raise ValueError("Invalid email")
        ...         return str(value)
        ...
        ...     def parse_literal(self, ast):
        ...         if hasattr(ast, 'value'):
        ...             return self.parse_value(ast.value)
        ...         raise ValueError("Invalid email literal")

    Usage in types:
        >>> @fraiseql.type
        ... class User:
        ...     id: int
        ...     email: Email  # Uses registered Email scalar
        ...     name: str

        >>> schema = fraiseql.export_schema("schema.json")
        >>> # schema contains: "customScalars": {"Email": {...}}

    Notes:
        - Decorator returns class unmodified (no runtime FFI)
        - Registration is global (per-process)
        - Name must be unique within schema
        - Scalar must be defined before @type that uses it
        - Classes can only be imported from fraiseql.scalars at runtime
    """
    # Import here to avoid circular import
    from fraiseql.scalars import CustomScalar

    # Validate that cls is a CustomScalar subclass
    if not issubclass(cls, CustomScalar):
        raise TypeError(
            f"@scalar can only be applied to CustomScalar subclasses, got {cls.__name__}"
        )

    # Validate that cls has a name
    if not hasattr(cls, "name"):
        raise ValueError(f"CustomScalar {cls.__name__} must have a 'name' class attribute")

    scalar_name = cls.name
    if not isinstance(scalar_name, str) or not scalar_name:
        raise ValueError(f"CustomScalar name must be a non-empty string, got {scalar_name!r}")

    # Register with schema registry
    SchemaRegistry.register_scalar(
        name=scalar_name,
        scalar_class=cls,
        description=cls.__doc__,
    )

    return cls
