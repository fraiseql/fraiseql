"""Schema export functionality."""

from __future__ import annotations

__all__ = [
    "CompiledSchema",
    "Federation",
    "Schema",
    "export_schema",
    "export_types",
    "get_schema_dict",
]

import json
from typing import Any

from fraiseql.registry import SchemaRegistry


def _fields_with_directive(type_def: dict[str, Any], directive: str) -> list[str]:
    """Names of the type's fields carrying the given field-level federation directive.

    Field-level directives are recorded on each field's ``federation`` sub-dict by
    ``field(external=...)`` / ``field(shareable=...)`` etc.
    """
    return [
        field["name"]
        for field in type_def.get("fields", [])
        if field.get("federation", {}).get(directive)
    ]


def _build_federation_block(federation: Federation, schema: dict[str, Any]) -> dict[str, Any]:
    """Build the ``federation`` block for schema output.

    Derives the per-entity directive lists (#496) the Rust compiler/SDL renderer
    consumes from the per-type/field decorators, so a project never has to
    hand-author them:

    - ``entities[].extends`` from ``@type(extends=True)``;
    - ``entities[].external_fields`` from ``field(external=True)``;
    - ``entities[].shareable_fields`` from ``field(shareable=True)``;
    - top-level ``shareable_types`` from ``@type(shareable=True)`` /
      ``@error(shareable=True)`` — keyless value types that are excluded from the
      ``_Entity`` union and carry a type-level ``@shareable``.

    A plain entity with no directives stays exactly ``{name, key_fields}`` so the
    common case is byte-for-byte unchanged.

    Args:
        federation: Federation metadata from the caller.
        schema: The full schema dict (used to iterate types and their fields).

    Returns:
        Dictionary suitable for inclusion as ``schema["federation"]``.
    """
    entities: list[dict[str, Any]] = []
    shareable_types: list[str] = []
    for type_def in schema.get("types", []):
        name = type_def["name"]

        # Type-level @shareable → a keyless shared value type, not an entity. (The
        # Rust renderer gives it a type-level @shareable and keeps it out of the
        # _Entity union.) Covers shareable error/value types too.
        if type_def.get("shareable"):
            shareable_types.append(name)
            continue

        # Non-shareable error types stay out of the federation graph entirely.
        if type_def.get("is_error"):
            continue

        entity: dict[str, Any] = {
            "name": name,
            "key_fields": type_def.get("key_fields", federation.default_key_fields),
        }
        # Additive keys only when set, so the no-directive entity is unchanged.
        if type_def.get("extends"):
            entity["extends"] = True
        external_fields = _fields_with_directive(type_def, "external")
        if external_fields:
            entity["external_fields"] = external_fields
        shareable_fields = _fields_with_directive(type_def, "shareable")
        if shareable_fields:
            entity["shareable_fields"] = shareable_fields
        entities.append(entity)

    apollo_version = 2 if federation.version == "v2" else 1
    block: dict[str, Any] = {
        "enabled": True,
        "service_name": federation.service_name,
        # The Rust core FederationConfig reads `version` (the @link spec URL); the
        # legacy int `apollo_version` is kept for back-compat but ignored there.
        "version": federation.version,
        "apollo_version": apollo_version,
        "entities": entities,
    }
    if shareable_types:
        block["shareable_types"] = shareable_types
    return block


def _validate_schema_before_export(schema: dict[str, Any]) -> None:
    """Validate schema structure before writing to disk.

    Raises:
        ValueError: If the schema contains structural errors such as
            undefined return types or missing queries.
    """
    errors: list[str] = []

    registered_type_names: set[str] = {t["name"] for t in schema.get("types", [])}
    # Also include enum and scalar names as valid return types
    registered_type_names.update(e["name"] for e in schema.get("enums", []))
    registered_type_names.update(s["name"] for s in schema.get("custom_scalars", []))
    # Built-in GraphQL scalar names are always valid
    builtin_scalars = {"String", "Int", "Float", "Boolean", "ID"}
    registered_type_names.update(builtin_scalars)

    for query in schema.get("queries", []):
        ret = query.get("return_type", "")
        if ret and ret not in registered_type_names:
            errors.append(
                f"Query {query['name']!r} has return type {ret!r} which is not a registered type."
            )

    for mutation in schema.get("mutations", []):
        ret = mutation.get("return_type", "")
        if ret and ret not in registered_type_names:
            errors.append(
                f"Mutation {mutation['name']!r} has return type {ret!r} "
                "which is not a registered type."
            )

    if errors:
        error_list = "\n  - ".join(errors)
        raise ValueError(
            f"Schema validation failed before export. Fix the following errors:\n  - {error_list}"
        )


class Federation:
    """Federation metadata for subgraph service registration.

    Args:
        service_name: Name of the federated subgraph service.
        version: Apollo Federation version (default: ``"v2"``).
        default_key_fields: Key fields applied to types that don't declare
            their own ``key_fields`` (default: ``["id"]``).
    """

    def __init__(
        self,
        service_name: str,
        version: str = "v2",
        default_key_fields: list[str] | None = None,
    ) -> None:
        self.service_name = service_name
        self.version = version
        self.default_key_fields = default_key_fields or ["id"]


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

    def compile(self) -> CompiledSchema:
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
        self.federation: bool = self._extract_federation_info()

    def _extract_federation_info(self) -> bool:
        """Return True if any type in the schema carries federation metadata."""
        return any(t.get("federation") for t in self.schema.get("types", []))

    def get_type(self, name: str) -> TypeInfo | None:
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


def export_schema(
    output_path: str,
    pretty: bool = True,
    include_custom_scalars: bool = True,
    federation: Federation | None = None,
) -> None:
    """Export the schema registry to a JSON file.

    This function should be called after all decorators have been processed
    (typically at the end of the schema definition file).

    Args:
        output_path: Path to output schema.json file
        pretty: If True, format JSON with indentation (default: True)
        include_custom_scalars: If True, include custom scalars in output (default: True)
        federation: Optional federation metadata. When provided, the output
            includes a ``"federation"`` block with entity information.

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
    if federation is not None:
        schema["federation"] = _build_federation_block(federation, schema)
    if not include_custom_scalars:
        # The registry emits custom scalars under the camelCase "customScalars"
        # key; filtering "custom_scalars" (snake_case) was a no-op (M-export-schema).
        schema = {k: v for k, v in schema.items() if k not in ("customScalars", "custom_scalars")}

    _validate_schema_before_export(schema)

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


def export_types(output_path: str, pretty: bool = True) -> None:
    """Export ONLY types to a minimal types.json file (TOML-based workflow).

    This is the new minimal export function for the TOML-based configuration approach.
    It exports only the type definitions (types, enums, input_types, interfaces) without
    queries, mutations, federation, security, observers, or analytics metadata.

    All configuration moves to fraiseql.toml, which is merged with this types.json
    by the fraiseql-cli compile command.

    Args:
        output_path: Path to output types.json file
        pretty: If True, format JSON with indentation (default: True)

    Examples:
        >>> # At end of schema.py
        >>> if __name__ == "__main__":
        ...     fraiseql.export_types("user_types.json")

    Notes:
        - Call this after all @fraiseql decorators have been applied
        - The output types.json contains only type definitions
        - Queries, mutations, and all configuration moves to fraiseql.toml
        - Use with: fraiseql compile fraiseql.toml --types user_types.json
    """
    full_schema = SchemaRegistry.get_schema()

    # Extract only types, enums, input_types, interfaces
    # (no queries/mutations/federation/security/observers/analytics)
    minimal_schema = {
        "types": full_schema.get("types", []),
        "enums": full_schema.get("enums", []),
        "input_types": full_schema.get("input_types", []),
        "interfaces": full_schema.get("interfaces", []),
    }

    # Write to file
    with open(output_path, "w", encoding="utf-8") as f:
        if pretty:
            json.dump(minimal_schema, f, indent=2, ensure_ascii=False)
            f.write("\n")  # Add trailing newline
        else:
            json.dump(minimal_schema, f, ensure_ascii=False)

    print(f"✅ Types exported to {output_path}")
    print(f"   Types: {len(minimal_schema['types'])}")
    if minimal_schema["enums"]:
        print(f"   Enums: {len(minimal_schema['enums'])}")
    if minimal_schema["input_types"]:
        print(f"   Input types: {len(minimal_schema['input_types'])}")
    if minimal_schema["interfaces"]:
        print(f"   Interfaces: {len(minimal_schema['interfaces'])}")
    print(f"   → Use with: fraiseql compile fraiseql.toml --types {output_path}")


def get_schema_dict(federation: Federation | None = None) -> dict[str, Any]:
    """Get the current schema as a dictionary (without exporting to file).

    Args:
        federation: Optional federation metadata. When provided, the returned
            dict includes a ``"federation"`` block with entity information.

    Returns:
        Schema dictionary with "types", "queries", and "mutations"

    Examples:
        >>> schema = fraiseql.get_schema_dict()
        >>> print(schema["types"])
        [{"name": "User", "fields": [...]}]
    """
    schema = SchemaRegistry.get_schema()
    if federation is not None:
        schema["federation"] = _build_federation_block(federation, schema)
    return schema
