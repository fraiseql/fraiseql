"""Analyzes FraiseQL types and generates TestFoundry metadata."""

import re
from dataclasses import dataclass
from typing import Optional, Union, get_args, get_origin

from fraiseql.fields import FraiseQLField


@dataclass
class FieldMapping:
    """Represents a field mapping for TestFoundry."""

    input_type: str
    field_name: str
    generator_type: str = "random"
    fk_mapping_key: Optional[str] = None
    fk_dependency_fields: Optional[list[str]] = None
    random_function: Optional[str] = None
    required: bool = True
    generator_group: Optional[str] = None
    group_leader: bool = False
    group_dependency_fields: Optional[list[str]] = None


@dataclass
class FKMapping:
    """Represents a foreign key mapping for TestFoundry."""

    input_type: str
    from_expression: str
    select_field: str
    random_pk_field: str
    random_value_field: str
    random_select_where: str = "deleted_at IS NULL"
    dependency_fields: Optional[list[str]] = None
    dependency_field_mapping: Optional[dict[str, str]] = None


class FoundryAnalyzer:
    """Analyzes FraiseQL types to generate TestFoundry metadata."""

    def __init__(self, schema_builder=None):
        self.schema_builder = schema_builder  # Not currently used
        self.field_mappings: list[FieldMapping] = []
        self.fk_mappings: list[FKMapping] = []

    def analyze_input_type(self, input_type: type) -> list[FieldMapping]:
        """Analyze a FraiseQL input type and generate field mappings."""
        if not hasattr(input_type, "__fraiseql_definition__"):
            raise ValueError(f"{input_type} is not a FraiseQL input type")

        type_name = self._to_snake_case(input_type.__name__)

        mappings = []

        # Analyze each field
        for field_name, field_type in input_type.__annotations__.items():
            if field_name.startswith("_"):
                continue

            field_value = getattr(input_type, field_name, None)
            is_fraise_field = isinstance(field_value, FraiseQLField)

            # Determine if field is required
            origin = get_origin(field_type)
            # Check for Union with None (e.g., str | None or Optional[str])
            is_optional = False
            if origin is Union:
                args = get_args(field_type)
                is_optional = type(None) in args
            required = not is_optional

            # Create field mapping
            mapping = self._create_field_mapping(
                type_name,
                field_name,
                field_type,
                required,
                field_value if is_fraise_field else None,
            )
            mappings.append(mapping)

        return mappings

    def _create_field_mapping(
        self,
        input_type: str,
        field_name: str,
        field_type: type,
        required: bool,
        fraise_field: Optional[FraiseQLField],
    ) -> FieldMapping:
        """Create a field mapping based on field type and metadata."""
        # Strip Optional wrapper if present
        actual_type = field_type
        origin = get_origin(field_type)

        # Handle Union types (e.g., str | None)
        if origin is Union:
            args = get_args(field_type)
            # Find the non-None type
            actual_type = next(
                (arg for arg in args if arg is not type(None)), field_type
            )
        elif origin is type(None):
            actual_type = field_type

        # Determine generator type and random function
        generator_type = "random"
        random_function = None
        fk_mapping_key = None

        # Check if it's a FK reference (ends with _id or has FK metadata)
        # Common FK patterns to detect:
        # - user_id, post_id, author_id (entity names followed by _id)
        # - parent_comment_id (composite entity names)
        # - fk_* (explicit FK prefix)
        # Patterns to exclude:
        # - id (primary key)
        # - external_id, request_id (descriptive IDs that aren't FKs)
        is_fk = False
        if field_name.endswith("_id") and field_name != "id":
            # Heuristic: if removing _id gives us something that looks like an entity name, it's an FK
            # Entity names are typically single words or snake_case entity names
            base_name = field_name[:-3]  # Remove _id
            # Common FK patterns we want to match
            fk_patterns = [
                "user",
                "post",
                "author",
                "comment",
                "parent_comment",
                "category",
                "group",
                "organization",
            ]
            # Check if base_name matches common entity patterns
            if any(
                base_name == pattern or base_name.endswith("_" + pattern)
                for pattern in fk_patterns
            ):
                is_fk = True
            # Or if it's a simple entity name (no complex underscores like external_)
            elif "_" not in base_name or base_name.count("_") == 1:
                # Simple entity names or compound entity names like parent_comment
                is_fk = True
            # Special case: exclude known non-FK patterns
            if base_name in ["external", "request", "session", "transaction"]:
                is_fk = False
        elif field_name.startswith("fk_"):
            is_fk = True

        if is_fk:
            generator_type = "resolve_fk"
            # Extract entity name from field
            if field_name.endswith("_id"):
                entity = field_name[:-3]  # Remove _id suffix
            else:
                entity = field_name[3:]  # Remove fk_ prefix
            fk_mapping_key = f"{entity}_id"

        # Check field name first for special cases
        elif field_name in ("latitude", "lat") and actual_type is float:
            random_function = "testfoundry_random_latitude"
        elif field_name in ("longitude", "lon", "lng") and actual_type is float:
            random_function = "testfoundry_random_longitude"
        # Then check type-based functions
        elif actual_type is str:
            # Special cases based on field name
            if "email" in field_name:
                random_function = "testfoundry_random_email"
            elif "url" in field_name or "link" in field_name:
                random_function = "testfoundry_random_url"
            elif "phone" in field_name:
                random_function = "testfoundry_random_phone"
        elif actual_type is bool:
            random_function = "testfoundry_random_boolean"
        elif actual_type is int:
            random_function = "testfoundry_random_integer"
        elif actual_type is float:
            random_function = "testfoundry_random_float"
        elif hasattr(actual_type, "__name__"):
            if actual_type.__name__ == "UUID":
                random_function = "gen_random_uuid"
            elif actual_type.__name__ == "UUIDField":
                random_function = "gen_random_uuid"
            elif actual_type.__name__ == "datetime":
                random_function = "testfoundry_random_timestamp"
            elif actual_type.__name__ == "date":
                random_function = "testfoundry_random_date"

        # Check if it's a list type
        if get_origin(actual_type) is list:
            # Handle list types (like tags)
            random_function = "testfoundry_random_array"

        return FieldMapping(
            input_type=input_type,
            field_name=field_name,
            generator_type=generator_type,
            fk_mapping_key=fk_mapping_key,
            random_function=random_function,
            required=required,
        )

    def analyze_entity_relationships(
        self, entity_name: str, table_name: str
    ) -> list[FKMapping]:
        """Generate FK mappings for an entity based on its relationships."""
        # This is a simplified version - in practice, you'd analyze the actual
        # database schema or FraiseQL type relationships

        mappings = []

        # Common pattern: entity_id mapping
        fk_mapping = FKMapping(
            input_type=f"{entity_name}_id",
            from_expression=table_name,
            select_field="id",
            random_pk_field="id",
            random_value_field=self._guess_display_field(entity_name),
            random_select_where="deleted_at IS NULL",
        )
        mappings.append(fk_mapping)

        return mappings

    def _guess_display_field(self, entity_name: str) -> str:
        """Guess the display field for an entity."""
        # Common patterns
        if entity_name == "user":
            return "email"
        elif entity_name in ("post", "article", "page"):
            return "title"
        elif entity_name == "comment":
            return "content"
        else:
            return "name"  # Default fallback

    def _to_snake_case(self, name: str) -> str:
        """Convert PascalCase or camelCase to snake_case."""
        # Insert underscore before uppercase letters
        s1 = re.sub("(.)([A-Z][a-z]+)", r"\1_\2", name)
        s2 = re.sub("([a-z0-9])([A-Z])", r"\1_\2", s1)

        # Remove 'Input' suffix if present
        s2 = re.sub(r"_?[Ii]nput$", "", s2)

        return s2.lower()

    def generate_sql_statements(self) -> str:
        """Generate SQL INSERT statements for TestFoundry metadata."""
        sql_parts = []

        # Generate field mapping inserts
        if self.field_mappings:
            sql_parts.append("-- Field mappings")
            sql_parts.append(
                "INSERT INTO testfoundry.testfoundry_tb_input_field_mapping"
            )
            sql_parts.append(
                "(input_type, field_name, generator_type, fk_mapping_key, "
                "random_function, required, generator_group, group_leader, "
                "group_dependency_fields)"
            )
            sql_parts.append("VALUES")

            values = []
            for mapping in self.field_mappings:
                fk_key = (
                    f"'{mapping.fk_mapping_key}'" if mapping.fk_mapping_key else "NULL"
                )
                random_fn = (
                    f"'{mapping.random_function}'"
                    if mapping.random_function
                    else "NULL"
                )
                gen_group = (
                    f"'{mapping.generator_group}'"
                    if mapping.generator_group
                    else "NULL"
                )
                group_deps = (
                    f"ARRAY{mapping.group_dependency_fields}"
                    if mapping.group_dependency_fields
                    else "NULL"
                )

                value = (
                    f"('{mapping.input_type}', '{mapping.field_name}', "
                    f"'{mapping.generator_type}', {fk_key}, {random_fn}, "
                    f"{mapping.required}, {gen_group}, {mapping.group_leader}, "
                    f"{group_deps})"
                )
                values.append(value)

            sql_parts.append(",\n".join(values) + ";")

        # Generate FK mapping inserts
        if self.fk_mappings:
            sql_parts.append("\n-- FK mappings")
            sql_parts.append("INSERT INTO testfoundry.testfoundry_tb_fk_mapping")
            sql_parts.append(
                "(input_type, from_expression, select_field, "
                "random_pk_field, random_value_field, random_select_where)"
            )
            sql_parts.append("VALUES")

            values = []
            for fk in self.fk_mappings:
                value = (
                    f"('{fk.input_type}', '{fk.from_expression}', "
                    f"'{fk.select_field}', '{fk.random_pk_field}', "
                    f"'{fk.random_value_field}', '{fk.random_select_where}')"
                )
                values.append(value)

            sql_parts.append(",\n".join(values) + ";")

        return "\n".join(sql_parts)
