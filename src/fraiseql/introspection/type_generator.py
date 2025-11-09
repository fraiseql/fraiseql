"""Dynamic GraphQL type generation for AutoFraiseQL.

This module provides the TypeGenerator class that creates FraiseQL @type classes
dynamically from PostgreSQL view metadata and @fraiseql annotations.
"""

import logging
from typing import Any, Optional, Type
from uuid import UUID

from .metadata_parser import TypeAnnotation
from .postgres_introspector import ViewMetadata
from .type_mapper import TypeMapper

logger = logging.getLogger(__name__)


class TypeGenerator:
    """Generate FraiseQL @type classes dynamically."""

    def __init__(self, type_mapper: Optional[TypeMapper] = None):
        self.type_mapper = type_mapper or TypeMapper()

    async def generate_type_class(
        self, view_metadata: ViewMetadata, annotation: TypeAnnotation, db_pool: Any
    ) -> Type:
        """Generate a @type class from view metadata.

        Steps:
        1. Introspect JSONB data column to get field structure
        2. Map PostgreSQL types → Python types
        3. Build class dynamically with __annotations__
        4. Apply @type decorator
        5. Register in type registry

        Args:
            view_metadata: Metadata from introspection
            annotation: Parsed @fraiseql:type annotation
            db_pool: Database connection pool

        Returns:
            Decorated Python class
        """
        # 1. Get JSONB structure by querying sample row
        jsonb_fields = await self._introspect_jsonb_column(
            view_metadata.view_name, view_metadata.schema_name, db_pool
        )

        # 2. Build class name (PascalCase from view name)
        class_name = self._view_name_to_class_name(view_metadata.view_name)

        # 3. Build field annotations
        annotations = {}
        for field_name, field_info in jsonb_fields.items():
            python_type = self.type_mapper.pg_type_to_python(
                field_info["type"], field_info["nullable"]
            )
            annotations[field_name] = python_type

        # 4. Create class dynamically
        cls = type(
            class_name,
            (object,),
            {
                "__annotations__": annotations,
                "__doc__": (
                    annotation.description  # Priority 1: Explicit annotation
                    or view_metadata.comment  # Priority 2: PostgreSQL comment (NEW)
                    or f"Auto-generated from {view_metadata.view_name}"  # Priority 3: Fallback
                ),
                "__module__": "fraiseql.introspection.generated",
            },
        )

        # 5. Apply @type decorator
        sql_source = f"{view_metadata.schema_name}.{view_metadata.view_name}"
        decorated_cls = self._apply_type_decorator(cls, sql_source, annotation)

        # 6. Register in type registry
        self._register_type(decorated_cls)

        return decorated_cls

    async def _introspect_jsonb_column(
        self, view_name: str, schema_name: str, db_pool: Any
    ) -> dict[str, dict]:
        """Introspect JSONB data column structure.

        Strategy:
        1. Query one row from view
        2. Extract 'data' column (JSONB)
        3. Infer types from actual values
        4. Return field_name → {type, nullable} mapping
        """
        async with db_pool.connection() as conn:
            # Check if view has a 'data' column (JSONB)
            check_query = """
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = %s AND column_name = 'data'
            """
            result = await conn.execute(check_query, (schema_name, view_name))
            has_data_column = await result.fetchone()

            if not has_data_column:
                # No JSONB column, introspect regular columns
                return await self._introspect_view_definition(view_name, schema_name, conn)

            # View has data column, try to get sample
            result = await conn.execute(f"SELECT data FROM {schema_name}.{view_name} LIMIT 1")
            row = await result.fetchone()

            if not row or not row[0]:
                # View is empty, introspect view definition instead
                return await self._introspect_view_definition(view_name, schema_name, conn)

            # Parse JSONB structure
            data = row[0]  # psycopg returns tuple, not dict
            fields = {}

            for field_name, field_value in data.items():
                fields[field_name] = {
                    "type": self._infer_pg_type_from_value(field_value),
                    "nullable": field_value is None,
                }

            return fields

    async def _introspect_view_definition(
        self, view_name: str, schema_name: str, conn: Any
    ) -> dict[str, dict]:
        """Introspect view definition when no data is available.

        This is a fallback that parses the view definition to extract
        column information from the underlying tables.
        """
        # For now, return a basic structure
        # In a full implementation, this would parse the view definition
        logger.warning(f"View {schema_name}.{view_name} is empty, using fallback introspection")

        # Query view columns from information_schema
        columns_query = """
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = %s AND table_name = %s
            ORDER BY ordinal_position
        """
        result = await conn.execute(columns_query, (schema_name, view_name))
        rows = await result.fetchall()

        fields = {}
        for row in rows:
            column_name, data_type, is_nullable = row[0], row[1], row[2]
            if column_name == "data":  # Skip the JSONB data column
                continue
            fields[column_name] = {"type": data_type, "nullable": is_nullable == "YES"}

        return fields

    def _infer_pg_type_from_value(self, value: Any) -> str:
        """Infer PostgreSQL type from Python value."""
        if isinstance(value, bool):
            return "boolean"
        if isinstance(value, int):
            return "integer"
        if isinstance(value, float):
            return "double precision"
        if isinstance(value, str):
            # Check if it's a UUID string
            try:
                UUID(value)
                return "uuid"
            except ValueError:
                return "text"
        elif isinstance(value, (dict, list)):
            return "jsonb"
        else:
            return "text"  # Default fallback

    def _view_name_to_class_name(self, view_name: str) -> str:
        """Convert v_user_profile → UserProfile."""
        # Remove v_ or tv_ prefix
        name = view_name
        if name.startswith("tv_"):
            name = name[3:]  # Remove 'tv_'
        elif name.startswith("v_"):
            name = name[2:]  # Remove 'v_'
        # Split on underscore, capitalize each part
        parts = name.split("_")
        return "".join(part.capitalize() for part in parts)

    def _apply_type_decorator(self, cls: Type, sql_source: str, annotation: TypeAnnotation) -> Type:
        """Apply the @type decorator to the class."""
        # Import here to avoid circular imports
        from fraiseql import type as fraiseql_type

        # Apply decorator with appropriate parameters
        decorated_cls = fraiseql_type(sql_source=sql_source, jsonb_column="data")(cls)

        return decorated_cls

    def _register_type(self, cls: Type):
        """Register the type in FraiseQL's type registry."""
        # Import here to avoid circular imports
        from fraiseql.db import _type_registry

        _type_registry[cls.__name__] = cls
