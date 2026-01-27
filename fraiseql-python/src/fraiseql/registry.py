"""Global schema registry for collecting types, queries, and mutations."""

from typing import Any


class SchemaRegistry:
    """Global registry for schema definitions.

    This class maintains a singleton registry of all types, queries, and mutations
    defined via decorators. The registry is used to generate the final schema.json.
    """

    # Class-level storage (singleton pattern)
    _types: dict[str, dict[str, Any]] = {}
    _enums: dict[str, dict[str, Any]] = {}
    _input_types: dict[str, dict[str, Any]] = {}
    _interfaces: dict[str, dict[str, Any]] = {}
    _unions: dict[str, dict[str, Any]] = {}
    _queries: dict[str, dict[str, Any]] = {}
    _mutations: dict[str, dict[str, Any]] = {}
    _subscriptions: dict[str, dict[str, Any]] = {}
    _fact_tables: dict[str, dict[str, Any]] = {}
    _aggregate_queries: dict[str, dict[str, Any]] = {}
    _observers: dict[str, dict[str, Any]] = {}

    @classmethod
    def register_type(
        cls,
        name: str,
        fields: dict[str, dict[str, Any]],
        description: str | None = None,
        implements: list[str] | None = None,
        federation: dict[str, Any] | None = None,
    ) -> None:
        """Register a GraphQL type.

        Args:
            name: Type name (e.g., "User")
            fields: Dictionary of field_name -> {"type": str, "nullable": bool}
            description: Optional type description from docstring
            implements: List of interface names this type implements
            federation: Federation metadata (keys, extends, external_fields, etc.)
        """
        # Build field list with federation metadata
        field_list = []
        for field_name, field_info in fields.items():
            field_def: dict[str, Any] = {
                "name": field_name,
                "type": field_info["type"],
                "nullable": field_info["nullable"],
            }

            # Add field-level federation metadata
            if federation:
                field_fed: dict[str, Any] = {
                    "external": field_name in federation.get("external_fields", []),
                }
                if field_name in federation.get("requires", {}):
                    field_fed["requires"] = federation["requires"][field_name]
                if federation.get("provides_data"):
                    # Check if this field provides any data
                    field_provides = [
                        p
                        for p in federation["provides_data"]
                        if f"{name}.{field_name}" in str(p) or p.startswith(f"{field_name}:")
                    ]
                    if field_provides:
                        field_fed["provides"] = field_provides

                field_def["federation"] = field_fed

            field_list.append(field_def)

        type_def: dict[str, Any] = {
            "name": name,
            "fields": field_list,
            "description": description,
        }

        # Add implements if specified
        if implements:
            type_def["implements"] = implements

        # Add federation metadata if specified, or default empty federation
        if federation:
            type_def["federation"] = federation
        else:
            # Add default empty federation metadata for all types
            type_def["federation"] = {
                "keys": [],
                "extend": False,
                "external_fields": [],
                "provides_data": [],
            }

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
            fields: Dictionary of field_name -> {"type": str, "nullable": bool}
            description: Optional interface description from docstring
        """
        cls._interfaces[name] = {
            "name": name,
            "fields": [
                {
                    "name": field_name,
                    "type": field_info["type"],
                    "nullable": field_info["nullable"],
                }
                for field_name, field_info in fields.items()
            ],
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
        cls._subscriptions[name] = {
            "name": name,
            "entity_type": entity_type,
            "nullable": nullable,
            "arguments": arguments,
            "description": description,
            **config,
        }

    @classmethod
    def register_fact_table(
        cls,
        table_name: str,
        measures: list[dict[str, Any]],
        dimensions: dict[str, Any],
        denormalized_filters: list[dict[str, Any]],
    ) -> None:
        """Register a fact table definition.

        Args:
            table_name: Fact table name (e.g., "tf_sales")
            measures: List of measure column definitions
            dimensions: Dimension column metadata
            denormalized_filters: List of filter column definitions
        """
        cls._fact_tables[table_name] = {
            "table_name": table_name,
            "measures": measures,
            "dimensions": dimensions,
            "denormalized_filters": denormalized_filters,
        }

    @classmethod
    def register_aggregate_query(
        cls,
        name: str,
        fact_table: str,
        auto_group_by: bool,
        auto_aggregates: bool,
        description: str | None = None,
    ) -> None:
        """Register an aggregate query definition.

        Args:
            name: Query name (e.g., "sales_aggregate")
            fact_table: Fact table name (e.g., "tf_sales")
            auto_group_by: Auto-generate groupBy fields
            auto_aggregates: Auto-generate aggregate fields
            description: Optional query description
        """
        cls._aggregate_queries[name] = {
            "name": name,
            "fact_table": fact_table,
            "auto_group_by": auto_group_by,
            "auto_aggregates": auto_aggregates,
            "description": description,
        }

    @classmethod
    def register_observer(
        cls,
        name: str,
        entity: str,
        event: str,
        actions: list[dict[str, Any]],
        condition: str | None = None,
        retry: dict[str, Any] | None = None,
    ) -> None:
        """Register an observer.

        Args:
            name: Observer function name (e.g., "on_order_created")
            entity: Entity type to observe (e.g., "Order")
            event: Event type (INSERT, UPDATE, or DELETE)
            actions: List of action configurations
            condition: Optional condition expression
            retry: Optional retry configuration
        """
        cls._observers[name] = {
            "name": name,
            "entity": entity,
            "event": event.upper(),
            "actions": actions,
            "condition": condition,
            "retry": retry,
        }

    @classmethod
    def get_schema(cls) -> dict[str, Any]:
        """Get the complete schema as a dictionary.

        Returns:
            Dictionary with "types", "enums", "input_types", "interfaces", "unions",
            "queries", "mutations", "fact_tables", and "aggregate_queries"
        """
        types_list = list(cls._types.values())

        # Check if any type has actual federation keys (not just empty metadata)
        has_federation = any(t.get("federation", {}).get("keys") for t in types_list)

        schema: dict[str, Any] = {
            "types": types_list,
            "enums": list(cls._enums.values()),
            "input_types": list(cls._input_types.values()),
            "interfaces": list(cls._interfaces.values()),
            "unions": list(cls._unions.values()),
            "queries": list(cls._queries.values()),
            "mutations": list(cls._mutations.values()),
            "subscriptions": list(cls._subscriptions.values()),
        }

        # Add federation root metadata if any type uses federation
        if has_federation:
            schema["federation"] = {
                "enabled": True,
                "version": "v2",
            }

        # Add analytics sections if present
        if cls._fact_tables:
            schema["fact_tables"] = list(cls._fact_tables.values())
        if cls._aggregate_queries:
            schema["aggregate_queries"] = list(cls._aggregate_queries.values())

        # Add observers if present
        if cls._observers:
            schema["observers"] = list(cls._observers.values())

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
        cls._fact_tables.clear()
        cls._aggregate_queries.clear()
        cls._observers.clear()


def generate_schema_json(types: list[type] | None = None) -> dict[str, Any]:
    """Generate schema JSON from current registry (convenience function).

    Args:
        types: List of types to include (unused for compatibility, uses full registry)

    Returns:
        Schema dictionary with federation metadata if applicable.
    """
    return SchemaRegistry.get_schema()
