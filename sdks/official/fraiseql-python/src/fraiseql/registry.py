"""Global schema registry for collecting types, queries, and mutations."""

import re
from typing import Any, ClassVar, TypeAlias

SchemaElement: TypeAlias = dict[str, Any]

_CAMEL_RE = re.compile(r"(?<!^)(?=[A-Z])")


def _pascal_to_snake(name: str) -> str:
    """Convert PascalCase to snake_case (e.g. OrderItem → order_item)."""
    return _CAMEL_RE.sub("_", name).lower()


class SchemaRegistry:
    """Global registry for schema definitions.

    This class maintains a singleton registry of all types, queries, and mutations
    defined via decorators. The registry is used to generate the final schema.json.
    """

    # Class-level storage (singleton pattern)
    _types: ClassVar[dict[str, SchemaElement]] = {}
    _enums: ClassVar[dict[str, SchemaElement]] = {}
    _input_types: ClassVar[dict[str, SchemaElement]] = {}
    _interfaces: ClassVar[dict[str, SchemaElement]] = {}
    _unions: ClassVar[dict[str, SchemaElement]] = {}
    _queries: ClassVar[dict[str, SchemaElement]] = {}
    _mutations: ClassVar[dict[str, SchemaElement]] = {}
    _subscriptions: ClassVar[dict[str, SchemaElement]] = {}
    # Maps scalar name -> (CustomScalar class, optional description)
    _custom_scalars: ClassVar[dict[str, tuple[type, str | None]]] = {}

    @staticmethod
    def _build_field_def(field_name: str, field_info: SchemaElement) -> SchemaElement:
        """Build a single field definition dict from a field name and info mapping."""
        field_def: dict[str, Any] = {
            "name": field_name,
            "type": field_info["type"],
            "nullable": field_info["nullable"],
        }
        for key in ("requires_scope", "on_deny", "deprecated", "description"):
            if key in field_info:
                field_def[key] = field_info[key]
        return field_def

    @classmethod
    def register_type(  # noqa: PLR0913 — public API; all parameters are meaningful
        cls,
        name: str,
        fields: dict[str, dict[str, Any]],
        description: str | None = None,
        implements: list[str] | None = None,
        relay: bool = False,
        requires_role: str | None = None,
        is_error: bool = False,
        sql_source: str | None = None,
        key_fields: list[str] | None = None,
        extends: bool = False,
    ) -> None:
        """Register a GraphQL type.

        Args:
            name: Type name (e.g., "User")
            fields: Dictionary of field_name -> {"type": str, "nullable": bool}
            description: Optional type description from docstring
            implements: List of interface names this type implements
            relay: Whether this type implements the Relay Node interface. When True,
                the compiler generates global node IDs and validates pk_{entity} exists
                in the view's data JSONB.
            requires_role: Role required to access this type. If set, only users with
                this role can see or query this type.
            sql_source: Override the default SQL view name. When None, defaults to
                ``v_{snake_case(name)}``.
            key_fields: Federation entity key fields (e.g., ``["id", "region"]``).
            extends: Whether this type extends a type from another subgraph.
        """
        field_list = [cls._build_field_def(k, v) for k, v in fields.items()]

        type_def: dict[str, Any] = {
            "name": name,
            "sql_source": sql_source or f"v_{_pascal_to_snake(name)}",
            "fields": field_list,
            "description": description,
        }

        if name in cls._types:
            raise ValueError(
                f"Type {name!r} is already registered. Each name must be unique within a schema."
            )

        # Add implements if specified
        if implements:
            type_def["implements"] = implements

        if relay:
            type_def["relay"] = True

        if requires_role:
            type_def["requires_role"] = requires_role

        if is_error:
            type_def["is_error"] = True

        if key_fields:
            type_def["key_fields"] = key_fields

        if extends:
            type_def["extends"] = True

        cls._types[name] = type_def

    @classmethod
    def register_interface(
        cls,
        name: str,
        fields: dict[str, dict[str, Any]],
        description: str | None = None,
    ) -> None:
        """Register a GraphQL interface type.

        Args:
            name: Interface name (e.g., "Node")
            fields: Dictionary of field_name -> {"type": str, "nullable": bool, ...metadata}
            description: Optional interface description from docstring
        """
        field_list = [cls._build_field_def(k, v) for k, v in fields.items()]

        if name in cls._interfaces:
            raise ValueError(
                f"Interface {name!r} is already registered. "
                "Each name must be unique within a schema."
            )

        cls._interfaces[name] = {
            "name": name,
            "fields": field_list,
            "description": description,
        }

    @classmethod
    def register_enum(
        cls,
        name: str,
        values: list[dict[str, Any]],
        description: str | None = None,
    ) -> None:
        """Register a GraphQL enum type.

        Args:
            name: Enum name (e.g., "OrderStatus")
            values: List of enum value definitions
            description: Optional enum description from docstring
        """
        if name in cls._enums:
            raise ValueError(
                f"Enum {name!r} is already registered. Each name must be unique within a schema."
            )
        cls._enums[name] = {
            "name": name,
            "values": values,
            "description": description,
        }

    @classmethod
    def register_input(
        cls,
        name: str,
        fields: list[dict[str, Any]],
        description: str | None = None,
    ) -> None:
        """Register a GraphQL input object type.

        Args:
            name: Input type name (e.g., "UserFilter")
            fields: List of field definitions
            description: Optional input type description from docstring
        """
        if name in cls._input_types:
            raise ValueError(
                f"Input type {name!r} is already registered. "
                "Each name must be unique within a schema."
            )
        cls._input_types[name] = {
            "name": name,
            "fields": fields,
            "description": description,
        }

    @classmethod
    def register_union(
        cls,
        name: str,
        member_types: list[str],
        description: str | None = None,
    ) -> None:
        """Register a GraphQL union type.

        Per GraphQL spec §3.10, unions represent a type that could be one of
        several object types.

        Args:
            name: Union name (e.g., "SearchResult")
            member_types: List of object type names that belong to this union
            description: Optional union description from docstring
        """
        if name in cls._unions:
            raise ValueError(
                f"Union {name!r} is already registered. Each name must be unique within a schema."
            )
        cls._unions[name] = {
            "name": name,
            "member_types": member_types,
            "description": description,
        }

    @classmethod
    def register_query(  # noqa: PLR0913 — public API; all parameters are meaningful
        cls,
        name: str,
        return_type: str,
        returns_list: bool,
        nullable: bool,
        arguments: list[dict[str, Any]],
        description: str | None = None,
        **config: Any,
    ) -> None:
        """Register a GraphQL query.

        Args:
            name: Query name (e.g., "users")
            return_type: Return type name (e.g., "User" or "[User]")
            returns_list: True if query returns a list
            nullable: True if result can be null
            arguments: List of argument definitions
            description: Optional query description from docstring
            **config: Additional configuration (sql_source, etc.)
        """
        if name in cls._queries:
            raise ValueError(
                f"Query {name!r} is already registered. Each name must be unique within a schema."
            )

        # Clean return type (remove list brackets for returns_list queries)
        clean_type = return_type.strip("[]!") if returns_list else return_type

        cls._queries[name] = {
            "name": name,
            "return_type": clean_type,
            "returns_list": returns_list,
            "nullable": nullable,
            "arguments": arguments,
            "description": description,
            **config,
        }

    @classmethod
    def register_mutation(  # noqa: PLR0913 — public API; all parameters are meaningful
        cls,
        name: str,
        return_type: str,
        returns_list: bool,
        nullable: bool,
        arguments: list[dict[str, Any]],
        description: str | None = None,
        **config: Any,
    ) -> None:
        """Register a GraphQL mutation.

        Args:
            name: Mutation name (e.g., "createUser")
            return_type: Return type name (e.g., "User")
            returns_list: True if mutation returns a list
            nullable: True if result can be null
            arguments: List of argument definitions
            description: Optional mutation description from docstring
            **config: Additional configuration (sql_source, operation, etc.)
        """
        if name in cls._mutations:
            raise ValueError(
                f"Mutation {name!r} is already registered. "
                "Each name must be unique within a schema."
            )

        # Clean return type (remove list brackets for returns_list mutations)
        clean_type = return_type.strip("[]!") if returns_list else return_type

        cls._mutations[name] = {
            "name": name,
            "return_type": clean_type,
            "returns_list": returns_list,
            "nullable": nullable,
            "arguments": arguments,
            "description": description,
            **config,
        }

    @classmethod
    def register_subscription(
        cls,
        name: str,
        entity_type: str,
        nullable: bool,
        arguments: list[dict[str, Any]],
        description: str | None = None,
        **config: Any,
    ) -> None:
        """Register a GraphQL subscription.

        Subscriptions in FraiseQL are compiled projections of database events.
        They are sourced from LISTEN/NOTIFY or CDC, not resolver-based.

        Args:
            name: Subscription name (e.g., "orderCreated")
            entity_type: Entity type name being subscribed to (e.g., "Order")
            nullable: True if result can be null
            arguments: List of argument definitions (filters)
            description: Optional subscription description from docstring
            **config: Additional configuration (topic, operation, etc.)
        """
        if name in cls._subscriptions:
            raise ValueError(
                f"Subscription {name!r} is already registered. "
                "Each name must be unique within a schema."
            )
        cls._subscriptions[name] = {
            "name": name,
            "entity_type": entity_type,
            "nullable": nullable,
            "arguments": arguments,
            "description": description,
            **config,
        }

    @classmethod
    def register_scalar(
        cls,
        name: str,
        scalar_class: type,
        description: str | None = None,
    ) -> None:
        """Register a custom scalar.

        Args:
            name: Scalar name (e.g., "Email")
            scalar_class: The CustomScalar subclass
            description: Optional scalar description from docstring

        Raises:
            ValueError: If scalar name is not unique
        """
        if name in cls._custom_scalars:
            raise ValueError(f"Scalar {name!r} is already registered")

        cls._custom_scalars[name] = (scalar_class, description)

    @classmethod
    def get_custom_scalars(cls) -> dict[str, type]:
        """Get all registered custom scalars.

        Returns:
            Dictionary mapping scalar names to CustomScalar classes
        """
        return {name: scalar_class for name, (scalar_class, _) in cls._custom_scalars.items()}

    @classmethod
    def get_schema(cls) -> dict[str, Any]:
        """Get the complete schema as a dictionary.

        Returns:
            Dictionary with "types", "enums", "input_types", "interfaces", "unions",
            "queries", "mutations", "subscriptions", and "customScalars"
        """
        schema: dict[str, Any] = {
            "types": list(cls._types.values()),
            "enums": list(cls._enums.values()),
            "input_types": list(cls._input_types.values()),
            "interfaces": list(cls._interfaces.values()),
            "unions": list(cls._unions.values()),
            "queries": list(cls._queries.values()),
            "mutations": list(cls._mutations.values()),
            "subscriptions": list(cls._subscriptions.values()),
        }

        # Include custom scalars if any are registered
        if cls._custom_scalars:
            custom_scalars = {}
            for name, (scalar_class, description) in cls._custom_scalars.items():
                custom_scalars[name] = {
                    "name": name,
                    "description": description or scalar_class.__doc__,
                    "validate": True,
                }
            schema["customScalars"] = custom_scalars

        return schema

    @classmethod
    def clear(cls) -> None:
        """Clear the registry (useful for testing)."""
        cls._types.clear()
        cls._enums.clear()
        cls._input_types.clear()
        cls._interfaces.clear()
        cls._unions.clear()
        cls._queries.clear()
        cls._mutations.clear()
        cls._subscriptions.clear()
        cls._custom_scalars.clear()


def generate_schema_json(_types: list[type] | None = None) -> dict[str, Any]:
    """Generate schema JSON from current registry (convenience function).

    Args:
        _types: Unused; accepted for compatibility, full registry is always used.

    Returns:
        Schema dictionary with federation metadata if applicable.
    """
    return SchemaRegistry.get_schema()
