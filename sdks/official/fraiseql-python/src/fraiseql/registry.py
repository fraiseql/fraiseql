"""Global schema registry for collecting types, queries, and mutations."""

import re
from typing import Any, TypeAlias

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
    _types: dict[str, SchemaElement] = {}
    _enums: dict[str, SchemaElement] = {}
    _input_types: dict[str, SchemaElement] = {}
    _interfaces: dict[str, SchemaElement] = {}
    _unions: dict[str, SchemaElement] = {}
    _queries: dict[str, SchemaElement] = {}
    _mutations: dict[str, SchemaElement] = {}
    _subscriptions: dict[str, SchemaElement] = {}
    _custom_scalars: dict[str, tuple[type, str | None]] = {}  # name -> (class, description)

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
    def register_type(
        cls,
        name: str,
        fields: dict[str, dict[str, Any]],
        description: str | None = None,
        implements: list[str] | None = None,
        relay: bool = False,
        requires_role: str | None = None,
        is_error: bool = False,
        tenant_scoped: bool = False,
        sql_source: str | None = None,
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
            tenant_scoped: Whether this type is tenant-scoped. When True, the compiler
                injects a tenant_id filter into all queries for this type.
            sql_source: Optional explicit SQL source (view name). Defaults to
                ``v_<snake_case(name)>``.
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

        if tenant_scoped:
            type_def["tenant_scoped"] = True

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
    def register_query(
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
    def register_mutation(
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

    # inject_defaults: base applies to both queries and mutations;
    # query-specific and mutation-specific defaults extend the base.
    _inject_defaults_base: dict[str, str] = {}
    _inject_defaults_queries: dict[str, str] = {}
    _inject_defaults_mutations: dict[str, str] = {}

    @classmethod
    def set_inject_defaults(
        cls,
        base: dict[str, str] | None = None,
        queries: dict[str, str] | None = None,
        mutations: dict[str, str] | None = None,
    ) -> None:
        """Configure default inject_params merged into every query/mutation at export.

        Args:
            base: Defaults applied to both queries and mutations.
            queries: Additional defaults for queries only.
            mutations: Additional defaults for mutations only.
        """
        cls._inject_defaults_base = base or {}
        cls._inject_defaults_queries = queries or {}
        cls._inject_defaults_mutations = mutations or {}

    @classmethod
    def register_crud(
        cls,
        type_name: str,
        fields: dict[str, dict[str, Any]],
        crud: bool | list[str],
    ) -> None:
        """Auto-generate CRUD queries and mutations for a type.

        Args:
            type_name: The registered type name (e.g., "Product").
            fields: The type's field definitions from ``extract_field_info``.
            crud: ``True`` for all operations, or a list of specific operations
                  to generate (subset of ``["read", "create", "update", "delete"]``).

        Raises:
            ValueError: If the type has no fields.
        """
        ops: set[str]
        if crud is True:
            ops = {"read", "create", "update", "delete"}
        elif isinstance(crud, list):
            ops = set(crud)
        else:
            return

        if not ops:
            return

        field_list = list(fields.items())
        if not field_list:
            msg = f"Type {type_name!r} has no fields; cannot generate CRUD operations"
            raise ValueError(msg)

        snake = _pascal_to_snake(type_name)
        view = f"v_{snake}"
        pk_name, pk_info = field_list[0]

        if "read" in ops:
            # Get-by-ID query
            cls.register_query(
                name=snake,
                return_type=type_name,
                returns_list=False,
                nullable=True,
                arguments=[{"name": pk_name, "type": pk_info["type"], "nullable": False}],
                description=f"Get {type_name} by ID.",
                sql_source=view,
            )
            # List query with auto_params
            cls.register_query(
                name=f"{snake}s",
                return_type=type_name,
                returns_list=True,
                nullable=False,
                arguments=[],
                description=f"List {type_name} records.",
                sql_source=view,
                auto_params={"where": True, "order_by": True, "limit": True, "offset": True},
            )

        if "create" in ops:
            args = [
                {"name": k, "type": v["type"], "nullable": v["nullable"]} for k, v in field_list
            ]
            cls.register_mutation(
                name=f"create_{snake}",
                return_type=type_name,
                returns_list=False,
                nullable=False,
                arguments=args,
                description=f"Create a new {type_name}.",
                sql_source=f"fn_create_{snake}",
                operation="INSERT",
            )

        if "update" in ops:
            args = [{"name": pk_name, "type": pk_info["type"], "nullable": False}]
            for k, v in field_list[1:]:
                args.append({"name": k, "type": v["type"], "nullable": True})
            cls.register_mutation(
                name=f"update_{snake}",
                return_type=type_name,
                returns_list=False,
                nullable=True,
                arguments=args,
                description=f"Update an existing {type_name}.",
                sql_source=f"fn_update_{snake}",
                operation="UPDATE",
            )

        if "delete" in ops:
            cls.register_mutation(
                name=f"delete_{snake}",
                return_type=type_name,
                returns_list=False,
                nullable=False,
                arguments=[{"name": pk_name, "type": pk_info["type"], "nullable": False}],
                description=f"Delete a {type_name}.",
                sql_source=f"fn_delete_{snake}",
                operation="DELETE",
            )

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
    def _merge_inject_defaults(
        cls,
        operation: dict[str, Any],
        defaults: dict[str, str],
    ) -> dict[str, Any]:
        """Merge inject_defaults into an operation, returning a new copy.

        Default inject_params fill in keys that are NOT already set by the
        operation's own inject_params (last-writer-wins: explicit > default).
        """
        if not defaults:
            return operation

        existing = dict(operation.get("inject_params", {}))
        for key, source_expr in defaults.items():
            if key not in existing:
                parts = source_expr.split(":", 1)
                if len(parts) == 2:
                    existing[key] = {"source": parts[0], "claim": parts[1]}
                else:
                    existing[key] = {"source": source_expr, "claim": ""}

        if existing:
            operation = {**operation, "inject_params": existing}
        return operation

    @classmethod
    def get_schema(cls) -> dict[str, Any]:
        """Get the complete schema as a dictionary.

        Returns:
            Dictionary with "types", "enums", "input_types", "interfaces", "unions",
            "queries", "mutations", "subscriptions", and "customScalars"
        """
        # Build query-level and mutation-level inject defaults
        query_defaults = {**cls._inject_defaults_base, **cls._inject_defaults_queries}
        mutation_defaults = {**cls._inject_defaults_base, **cls._inject_defaults_mutations}

        queries = [cls._merge_inject_defaults(q, query_defaults) for q in cls._queries.values()]
        mutations = [
            cls._merge_inject_defaults(m, mutation_defaults) for m in cls._mutations.values()
        ]

        schema: dict[str, Any] = {
            "types": list(cls._types.values()),
            "enums": list(cls._enums.values()),
            "input_types": list(cls._input_types.values()),
            "interfaces": list(cls._interfaces.values()),
            "unions": list(cls._unions.values()),
            "queries": queries,
            "mutations": mutations,
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
        cls._inject_defaults_base.clear()
        cls._inject_defaults_queries.clear()
        cls._inject_defaults_mutations.clear()


def generate_schema_json(types: list[type] | None = None) -> dict[str, Any]:
    """Generate schema JSON from current registry (convenience function).

    Args:
        types: List of types to include (unused for compatibility, uses full registry)

    Returns:
        Schema dictionary with federation metadata if applicable.
    """
    return SchemaRegistry.get_schema()
