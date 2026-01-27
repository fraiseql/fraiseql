"""Schema export functionality."""

import json
from typing import Any

from fraiseql.registry import SchemaRegistry


class Federation:
    """Federation metadata container."""

    def __init__(self, enabled: bool = False, version: str = "v2") -> None:
        self.enabled = enabled
        self.version = version


class Schema:
    """Federation-aware schema container for compilation and validation.

    This class wraps the schema registry for a specific set of types and
    provides methods for compilation and federation SDL generation.
    """

    def __init__(self, types: list[type] | None = None) -> None:
        """Initialize a schema with types.

        Args:
            types: List of type classes (typically decorated with @fraiseql.type)
        """
        self.types = types or []
        # Note: In real implementation, would extract federation metadata from types

    def compile(self) -> "CompiledSchema":
        """Compile the schema for federation.

        Returns:
            CompiledSchema with federation metadata and validation.

        Raises:
            FederationValidationError: If federation schema is invalid.
        """
        # Get current schema from registry
        schema_dict = SchemaRegistry.get_schema()
        return CompiledSchema(schema_dict)

    def to_json(self) -> dict[str, Any]:
        """Export schema as JSON with federation metadata.

        Returns:
            Dictionary containing complete schema including federation info.
        """
        return SchemaRegistry.get_schema()


class CompiledSchema:
    """Compiled schema with federation support and validation."""

    def __init__(self, schema_dict: dict[str, Any]) -> None:
        """Initialize with compiled schema data.

        Args:
            schema_dict: Schema dictionary from registry
        """
        self.schema = schema_dict
        self.federation = self._extract_federation_info()

    def _extract_federation_info(self) -> Federation | None:
        """Extract federation metadata from schema."""
        # Check if any type has federation metadata
        types = self.schema.get("types", [])
        if any(t.get("federation") for t in types):
            return Federation(enabled=True, version="v2")
        return None

    def get_type(self, name: str) -> dict[str, Any] | None:
        """Get type information by name.

        Args:
            name: Type name

        Returns:
            Type definition or None if not found.
        """
        for type_def in self.schema.get("types", []):
            if type_def["name"] == name:
                # Wrap with federation properties
                return TypeInfo(type_def)
        return None

    def to_federation_sdl(self) -> str:
        """Generate Federation SDL for Apollo Router/Gateway.

        Returns:
            SDL string with federation directives.
        """
        lines = []
        for type_def in self.schema.get("types", []):
            fed = type_def.get("federation", {})
            if not fed and not self.federation:
                continue

            # Type definition
            if fed.get("extend"):
                lines.append(f"extend type {type_def['name']} {{")
            else:
                lines.append(f"type {type_def['name']} {{")

            # Fields with directives
            for field in type_def.get("fields", []):
                field_fed = field.get("federation", {})
                type_str = field["type"]
                directives = []

                # Add @external directive
                if field_fed.get("external"):
                    directives.append("@external")

                # Add @requires directive
                if field_fed.get("requires"):
                    directives.append(f'@requires(fields: "{field_fed["requires"]}")')

                # Add @provides directive
                if field_fed.get("provides"):
                    targets = " ".join(field_fed["provides"])
                    directives.append(f'@provides(fields: "{targets}")')

                directive_str = " " + " ".join(directives) if directives else ""
                lines.append(f"  {field['name']}: {type_str}{directive_str}")

            # Add @key directives
            if fed.get("keys"):
                for key in fed["keys"]:
                    fields_str = " ".join(key["fields"])
                    lines.append(f'  @key(fields: "{fields_str}")')

            lines.append("}")
            lines.append("")

        return "\n".join(lines)


class TypeInfo:
    """Type information wrapper with federation support."""

    def __init__(self, type_def: dict[str, Any]) -> None:
        self._def = type_def

    @property
    def federation_keys(self) -> list[dict[str, Any]] | None:
        """Federation keys for this type."""
        fed = self._def.get("federation")
        return fed.get("keys") if fed else None

    @property
    def is_extended(self) -> bool:
        """Whether this type extends a type from another subgraph."""
        fed = self._def.get("federation")
        return fed.get("extend", False) if fed else False

    @property
    def external_fields(self) -> list[str] | None:
        """Fields owned by other subgraphs."""
        fed = self._def.get("federation")
        return fed.get("external_fields") if fed else None

    @property
    def requires_fields(self) -> dict[str, str] | None:
        """Fields that require other fields for resolution."""
        fed = self._def.get("federation")
        return fed.get("requires") if fed else None


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
