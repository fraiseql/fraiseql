"""Type mapping utilities for AutoFraiseQL.

This module provides utilities to map PostgreSQL database types to Python types
for dynamic GraphQL type generation.
"""

from datetime import date, datetime
from decimal import Decimal
from typing import Any, List
from uuid import UUID


class TypeMapper:
    """Map PostgreSQL types to Python types."""

    # PostgreSQL → Python type mapping
    PG_TO_PYTHON = {
        "uuid": UUID,
        "text": str,
        "character varying": str,
        "varchar": str,
        "char": str,
        "integer": int,
        "int": int,
        "int4": int,
        "bigint": int,
        "int8": int,
        "smallint": int,
        "int2": int,
        "boolean": bool,
        "bool": bool,
        "timestamp with time zone": datetime,
        "timestamptz": datetime,
        "timestamp without time zone": datetime,
        "timestamp": datetime,
        "date": date,
        "jsonb": dict,
        "json": dict,
        "numeric": Decimal,
        "decimal": Decimal,
        "double precision": float,
        "float8": float,
        "real": float,
        "float4": float,
        # Array types
        "text[]": List[str],
        "integer[]": List[int],
        "uuid[]": List[UUID],
        # Custom types (extensible)
    }

    def pg_type_to_python(self, pg_type: str, nullable: bool = False) -> Any:
        """Map PostgreSQL type to Python type.

        Args:
            pg_type: PostgreSQL type name (e.g., "text", "integer")
            nullable: Whether the column is nullable

        Returns:
            Python type (with Optional[] if nullable)
        """
        # Normalize type name
        pg_type_clean = pg_type.lower().strip()

        # Handle array types
        if pg_type_clean.endswith("[]"):
            base_type = pg_type_clean[:-2]
            element_type = self.PG_TO_PYTHON.get(base_type, str)
            python_type = List[element_type]
        else:
            python_type = self.PG_TO_PYTHON.get(pg_type_clean, str)

        # Add Optional if nullable
        if nullable:
            from typing import Optional

            return Optional[python_type]
        return python_type

    def register_custom_type(self, pg_type: str, python_type: type) -> None:
        """Register custom type mapping."""
        self.PG_TO_PYTHON[pg_type.lower()] = python_type
