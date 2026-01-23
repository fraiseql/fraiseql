"""Schema export functionality."""

import json
from typing import Any

from fraiseql.registry import SchemaRegistry


class _ConfigHolder:
    """Temporary holder for config during function definition."""

    _pending_config: dict[str, Any] | None = None


def config(**kwargs: Any) -> None:
    """Configuration helper for queries and mutations.

    This function is called inside decorated functions to specify SQL mapping
    and other configuration. The config is stored temporarily and applied by
    the decorator.

    Args:
        **kwargs: Configuration options:
            - sql_source: SQL view name (queries) or function name (mutations)
            - operation: Mutation operation type (CREATE, UPDATE, DELETE, CUSTOM)
            - auto_params: Auto-parameter configuration (limit, offset, where, order_by)
            - jsonb_column: JSONB column name for flexible schemas

    Examples:
        >>> @fraiseql.query
        ... def users(limit: int = 10) -> list[User]:
        ...     return fraiseql.config(
        ...         sql_source="v_user",
        ...         auto_params={"limit": True, "offset": True, "where": True}
        ...     )

        >>> @fraiseql.mutation
        ... def create_user(name: str, email: str) -> User:
        ...     return fraiseql.config(
        ...         sql_source="fn_create_user",
        ...         operation="CREATE"
        ...     )
    """
    # Store config temporarily - decorator will pick it up
    _ConfigHolder._pending_config = kwargs


def export_schema(output_path: str, pretty: bool = True) -> None:
    """Export the schema registry to a JSON file.

    This function should be called after all decorators have been processed
    (typically at the end of the schema definition file).

    Args:
        output_path: Path to output schema.json file
        pretty: If True, format JSON with indentation (default: True)

    Examples:
        >>> # At end of schema.py
        >>> if __name__ == "__main__":
        ...     fraiseql.export_schema("schema.json")

    Notes:
        - Call this after all @fraiseql decorators have been applied
        - The output schema.json is consumed by fraiseql-cli
        - Pretty formatting is recommended for version control
    """
    schema = SchemaRegistry.get_schema()

    # Write to file
    with open(output_path, "w", encoding="utf-8") as f:
        if pretty:
            json.dump(schema, f, indent=2, ensure_ascii=False)
            f.write("\n")  # Add trailing newline
        else:
            json.dump(schema, f, ensure_ascii=False)

    print(f"✅ Schema exported to {output_path}")
    print(f"   Types: {len(schema['types'])}")
    print(f"   Queries: {len(schema['queries'])}")
    print(f"   Mutations: {len(schema['mutations'])}")
    if "observers" in schema:
        print(f"   Observers: {len(schema['observers'])}")


def get_schema_dict() -> dict[str, Any]:
    """Get the current schema as a dictionary (without exporting to file).

    Returns:
        Schema dictionary with "types", "queries", and "mutations"

    Examples:
        >>> schema = fraiseql.get_schema_dict()
        >>> print(schema["types"])
        [{"name": "User", "fields": [...]}]
    """
    return SchemaRegistry.get_schema()
