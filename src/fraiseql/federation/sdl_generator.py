"""GraphQL SDL generation from Federation metadata.

Generates SDL schema with federation directives (@key, @external, @requires, @provides).

Examples:
    Generate SDL for a single entity:

        >>> from fraiseql.federation import entity, sdl_generator
        >>>
        >>> @entity
        ... class User:
        ...     id: str
        ...     name: str
        ...
        >>> sdl = sdl_generator.generate_entity_sdl(User)
        >>> print(sdl)
        type User @key(fields: "id") {
          id: ID!
          name: String!
        }

    Generate complete SDL with all entities:

        >>> sdl = sdl_generator.generate_schema_sdl()
"""

from typing import Any, Optional, Set, Union

from .computed_fields import extract_computed_fields, get_all_field_dependencies
from .decorators import EntityMetadata, get_entity_metadata, get_entity_registry
from .directives import get_method_directives
from .external_fields import extract_external_fields


class SDLGenerator:
    """Generates GraphQL SDL with Apollo Federation directives.

    Handles:
    - @key directives for entity keys
    - @external directives for fields from other subgraphs
    - @requires directives for computed fields
    - @provides directives for eager loading
    """

    # Map Python type hints to GraphQL scalar types
    TYPE_MAP = {
        str: "String",
        int: "Int",
        float: "Float",
        bool: "Boolean",
        "str": "String",
        "int": "Int",
        "float": "Float",
        "bool": "Boolean",
    }

    def __init__(self):
        """Initialize SDL generator."""
        self.indent = "  "

    def generate_entity_sdl(self, entity_class: type) -> str:
        """Generate SDL for a single entity.

        Args:
            entity_class: Entity class decorated with @entity or @extend_entity

        Returns:
            SDL string for the entity type

        Raises:
            ValueError: If entity is not registered
        """
        metadata = get_entity_metadata(entity_class.__name__)
        if metadata is None:
            raise ValueError(f"{entity_class.__name__} is not a registered entity")

        lines = []

        # Type definition with @key directive
        type_def = self._build_type_definition(metadata)
        lines.append(type_def)

        # Fields
        field_lines = self._build_fields(entity_class, metadata)
        lines.extend(field_lines)

        # Closing brace
        lines.append("}")

        return "\n".join(lines)

    def generate_schema_sdl(self) -> str:
        """Generate complete SDL for all registered entities.

        Returns:
            SDL string containing all entity types

        Example:
            >>> sdl = generator.generate_schema_sdl()
            >>> print(sdl)
            type User @key(fields: "id") {
              id: ID!
              name: String!
            }

            extend type Product @key(fields: "id") {
              id: ID! @external
              reviews: [Review!]!
            }
        """
        registry = get_entity_registry()
        if not registry:
            return ""

        entity_sdls = []
        for metadata in registry.values():
            entity_class = metadata.cls
            try:
                sdl = self.generate_entity_sdl(entity_class)
                entity_sdls.append(sdl)
            except ValueError:
                # Skip if entity cannot be generated
                continue

        return "\n\n".join(entity_sdls)

    def _build_type_definition(self, metadata: EntityMetadata) -> str:
        """Build type definition line with @key directive.

        Args:
            metadata: Entity metadata

        Returns:
            Type definition string, e.g., 'type User @key(fields: "id") {'
        """
        # Format key fields for directive
        key_fields = self._format_key_fields(metadata.resolved_key)

        # Use 'extend type' for extensions
        type_keyword = "extend type" if metadata.is_extension else "type"

        return f"{type_keyword} {metadata.type_name} @key(fields: {key_fields}) {{"

    def _format_key_fields(self, resolved_key: Union[str, list[str]]) -> str:
        """Format key fields for @key directive.

        Args:
            resolved_key: Single key field or list of key fields

        Returns:
            Formatted string for @key(fields: "...")

        Example:
            >>> gen._format_key_fields("id")
            '"id"'
            >>> gen._format_key_fields(["org_id", "user_id"])
            '"org_id user_id"'
        """
        if isinstance(resolved_key, str):
            return f'"{resolved_key}"'
        # Composite key: space-separated
        return f'"{" ".join(resolved_key)}"'

    def _build_fields(self, entity_class: type, metadata: EntityMetadata) -> list[str]:
        """Build field definitions with directives.

        Args:
            entity_class: Entity class
            metadata: Entity metadata

        Returns:
            List of field definition lines
        """
        lines = []
        field_annotations = getattr(entity_class, "__annotations__", {})

        # Extract external fields for this class
        external_map, _ = extract_external_fields(entity_class)

        # Extract computed fields and their directives
        computed_fields = extract_computed_fields(entity_class)
        field_deps = get_all_field_dependencies(entity_class)
        method_directives = get_method_directives(entity_class)

        for field_name, field_type in field_annotations.items():
            # Skip special attributes
            if field_name.startswith("_"):
                continue

            # Build field line
            field_line = self._build_field_line(
                field_name,
                field_type,
                is_external=field_name in external_map,
                computed_deps=field_deps.get(field_name),
            )
            lines.append(field_line)

        # Add computed field methods
        for method_name, computed_field in computed_fields.items():
            method_line = self._build_computed_field_line(
                method_name, computed_field, method_directives.get(method_name)
            )
            lines.append(method_line)

        return lines

    def _build_field_line(
        self,
        field_name: str,
        field_type: Any,
        is_external: bool = False,
        computed_deps: Optional[Set[str]] = None,
    ) -> str:
        """Build a single field definition line.

        Args:
            field_name: Name of field
            field_type: Python type annotation
            is_external: Whether field is external
            computed_deps: Dependency set if this field has dependencies

        Returns:
            Field definition line with directives

        Example:
            "  id: ID!"
            "  name: String! @external"
        """
        graphql_type = self._resolve_graphql_type(field_type)
        field_def = f"{self.indent}{field_name}: {graphql_type}"

        # Add @external directive if needed
        if is_external:
            field_def += " @external"

        # Add @requires directive if this field has dependencies
        if computed_deps:
            fields_str = " ".join(sorted(computed_deps))
            field_def += f' @requires(fields: "{fields_str}")'

        return field_def

    def _build_computed_field_line(
        self,
        method_name: str,
        computed_field: Any,
        method_directive_metadata: Optional[Any] = None,
    ) -> str:
        """Build a computed field definition line.

        Args:
            method_name: Name of computed method
            computed_field: ComputedField metadata
            method_directive_metadata: DirectiveMetadata with @requires/@provides

        Returns:
            Computed field definition with directives
        """
        # Determine return type
        return_type = "JSON"  # Default for computed fields
        if computed_field.return_type:
            return_type = self._resolve_graphql_type(computed_field.return_type)

        field_def = f"{self.indent}{method_name}: {return_type}"

        # Add @requires directive
        if computed_field.requires:
            fields_str = " ".join(computed_field.requires)
            field_def += f' @requires(fields: "{fields_str}")'

        # Add @provides directive
        if computed_field.provides:
            fields_str = " ".join(computed_field.provides)
            field_def += f' @provides(fields: "{fields_str}")'

        return field_def

    def _resolve_graphql_type(self, field_type: Any) -> str:
        """Resolve Python type to GraphQL type string.

        Handles:
        - Basic types: str, int, float, bool
        - Optional types: Optional[T], T | None
        - List types: list[T], List[T]
        - ID scalar type

        Args:
            field_type: Python type annotation

        Returns:
            GraphQL type string, e.g., "String!", "ID", "[String!]!"

        Example:
            >>> gen._resolve_graphql_type(str)
            'String!'
            >>> gen._resolve_graphql_type(list[str])
            '[String!]!'
            >>> gen._resolve_graphql_type(Optional[int])
            'Int'
        """
        # Handle None type
        if field_type is type(None):
            return "String"

        # Handle string type names
        if isinstance(field_type, str):
            return self.TYPE_MAP.get(field_type, "JSON")

        # Get string representation for comparison
        type_str = str(field_type)

        # Check for Optional type (Optional[T] or T | None)
        is_optional = "Optional" in type_str or "| None" in type_str or "Union" in type_str

        # Handle list/List types first (before extracting base type)
        if "list" in type_str.lower() or "List" in type_str:
            inner_type = self._extract_list_inner_type(field_type)
            graphql_inner = self._resolve_graphql_type(inner_type)
            # Lists are always non-null in GraphQL federation
            return f"[{graphql_inner}]!"

        # Extract base type from Optional
        base_type = self._extract_base_type(field_type, is_optional)

        # Try direct type lookup first (for type objects like str, int, etc)
        if base_type in self.TYPE_MAP:
            graphql_type = self.TYPE_MAP[base_type]
        elif isinstance(base_type, type):
            # Check if it's a known type
            if base_type in self.TYPE_MAP:
                graphql_type = self.TYPE_MAP[base_type]
            elif base_type.__name__ == "str":
                graphql_type = "String"
            elif base_type.__name__ == "int":
                graphql_type = "Int"
            elif base_type.__name__ == "float":
                graphql_type = "Float"
            elif base_type.__name__ == "bool":
                graphql_type = "Boolean"
            else:
                graphql_type = "JSON"
        # String representation - try to match type names
        elif base_type == "<class 'str'>" or "str'" in str(base_type):
            graphql_type = "String"
        elif base_type == "<class 'int'>" or "int'" in str(base_type):
            graphql_type = "Int"
        elif base_type == "<class 'float'>" or "float'" in str(base_type):
            graphql_type = "Float"
        elif base_type == "<class 'bool'>" or "bool'" in str(base_type):
            graphql_type = "Boolean"
        elif base_type in ("id", "ID"):
            graphql_type = "ID"
        else:
            graphql_type = "JSON"

        # Add non-null marker if not optional
        if not is_optional:
            graphql_type += "!"

        return graphql_type

    def _extract_base_type(self, field_type: Any, is_optional: bool) -> Any:
        """Extract base type from complex type annotation.

        Args:
            field_type: Type annotation
            is_optional: Whether type is optional

        Returns:
            Base type (type object or string representation)
        """
        # If it's already a basic type object, return it
        if field_type in (str, int, float, bool):
            return field_type

        type_str = str(field_type)

        # Handle Optional[T]
        if "Optional" in type_str:
            # Try to use get_args to extract T from Optional[T]
            from typing import get_args

            args = get_args(field_type)
            if args:
                # Optional[T] is Union[T, None], get first arg
                return args[0]
            # Fallback to string extraction
            start = type_str.find("[") + 1
            end = type_str.rfind("]")
            extracted = type_str[start:end].strip()
            if extracted == "str":
                return str
            if extracted == "int":
                return int
            if extracted == "float":
                return float
            if extracted == "bool":
                return bool
            return extracted

        # Handle Union[T, None]
        if "Union" in type_str:
            from typing import get_args

            args = get_args(field_type)
            # Union[T, None], get the first non-None arg
            for arg in args:
                if arg is not type(None):
                    return arg
            # Fallback to string extraction
            parts = type_str.split(",")
            for part in parts:
                cleaned = part.strip()
                if "NoneType" not in cleaned:
                    if cleaned == "str":
                        return str
                    if cleaned == "int":
                        return int
                    if cleaned == "float":
                        return float
                    if cleaned == "bool":
                        return bool
                    return cleaned

        # Handle T | None (Python 3.10+)
        if "|" in type_str:
            parts = type_str.split("|")
            for part in parts:
                cleaned = part.strip()
                if "None" not in cleaned:
                    if cleaned == "str":
                        return str
                    if cleaned == "int":
                        return int
                    if cleaned == "float":
                        return float
                    if cleaned == "bool":
                        return bool
                    return cleaned

        # No optional, return as-is
        return field_type

    def _extract_list_inner_type(self, field_type: Any) -> Any:
        """Extract inner type from List[T] or list[T].

        Args:
            field_type: List type annotation

        Returns:
            Inner type T, or str if cannot extract
        """
        type_str = str(field_type)

        # Try to extract from brackets
        if "[" in type_str and "]" in type_str:
            start = type_str.find("[") + 1
            end = type_str.rfind("]")
            inner = type_str[start:end].strip()

            # Remove trailing comma (from tuple unpacking)
            inner = inner.rstrip(",")

            # Map back to type if possible
            if inner in self.TYPE_MAP:
                return inner
            return inner

        # Default to string
        return str


def generate_entity_sdl(entity_class: type) -> str:
    """Generate SDL for a single entity.

    Convenience function wrapping SDLGenerator.

    Args:
        entity_class: Entity class decorated with @entity

    Returns:
        SDL string for the entity

    Example:
        >>> from fraiseql.federation import entity, sdl_generator
        >>> @entity
        ... class User:
        ...     id: str
        ...     name: str
        >>> print(sdl_generator.generate_entity_sdl(User))
        type User @key(fields: "id") {
          id: String!
          name: String!
        }
    """
    generator = SDLGenerator()
    return generator.generate_entity_sdl(entity_class)


def generate_schema_sdl() -> str:
    """Generate complete SDL for all registered entities.

    Convenience function wrapping SDLGenerator.

    Returns:
        SDL string containing all entities

    Example:
        >>> from fraiseql.federation import sdl_generator
        >>> print(sdl_generator.generate_schema_sdl())
        type User @key(fields: "id") {
          id: String!
          name: String!
        }

        type Post @key(fields: "id") {
          id: String!
          title: String!
          author_id: String!
        }
    """
    generator = SDLGenerator()
    return generator.generate_schema_sdl()
