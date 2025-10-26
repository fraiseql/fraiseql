"""View metadata caching for query optimization."""

import asyncio
import time
from dataclasses import dataclass
from typing import Optional

from fraiseql.cqrs.repository import CQRSRepository


@dataclass
class ColumnInfo:
    """Database column information."""

    name: str
    data_type: str
    is_nullable: bool
    is_jsonb: bool
    jsonb_structure: Optional[dict[str, any]] = None


@dataclass
class ViewInfo:
    """Database view information."""

    view_name: str
    columns: dict[str, ColumnInfo]
    primary_key: Optional[str] = None
    indexes: list[str] = None
    last_analyzed: float = 0


class ViewMetadataCache:
    """Caches database view metadata for query optimization."""

    def __init__(self, ttl: int = 3600) -> None:
        """Initialize cache.

        Args:
            ttl: Time-to-live for cached metadata in seconds
        """
        self.ttl = ttl
        self.cache: dict[str, ViewInfo] = {}
        self._lock = asyncio.Lock()

    async def get_view_structure(self, view_name: str, db: CQRSRepository) -> Optional[ViewInfo]:
        """Get cached view structure.

        Args:
            view_name: Name of the database view
            db: Database repository

        Returns:
            ViewInfo if found and valid, None otherwise
        """
        # Check cache
        if view_name in self.cache:
            info = self.cache[view_name]
            if time.time() - info.last_analyzed < self.ttl:
                return info

        # Refresh cache
        async with self._lock:
            # Double-check after acquiring lock
            if view_name in self.cache:
                info = self.cache[view_name]
                if time.time() - info.last_analyzed < self.ttl:
                    return info

            # Query view structure
            info = await self._query_view_structure(view_name, db)
            if info:
                self.cache[view_name] = info

            return info

    async def _query_view_structure(self, view_name: str, db: CQRSRepository) -> Optional[ViewInfo]:
        """Query view structure from information_schema.

        Args:
            view_name: Name of the view
            db: Database repository

        Returns:
            ViewInfo if view exists, None otherwise
        """
        # Split schema and view name
        parts = view_name.split(".")
        if len(parts) == 2:
            schema_name, view_name = parts
        else:
            schema_name = "public"

        # Query column information
        column_query = """
        SELECT
            column_name,
            data_type,
            is_nullable,
            column_default,
            udt_name
        FROM information_schema.columns
        WHERE table_schema = %(schema)s
        AND table_name = %(view)s
        ORDER BY ordinal_position
        """

        columns_result = await db.fetch(column_query, {"schema": schema_name, "view": view_name})

        if not columns_result:
            return None

        # Build column info
        columns = {}
        for row in columns_result:
            col_info = ColumnInfo(
                name=row["column_name"],
                data_type=row["data_type"],
                is_nullable=row["is_nullable"] == "YES",
                is_jsonb=row["udt_name"] == "jsonb",
            )

            # Analyze JSONB structure if needed
            if col_info.is_jsonb:
                structure = await self._analyze_jsonb_column(
                    f"{schema_name}.{view_name}", col_info.name, db
                )
                col_info.jsonb_structure = structure

            columns[col_info.name] = col_info

        # Get primary key
        pk_query = """
        SELECT kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
            ON tc.constraint_name = kcu.constraint_name
        WHERE tc.table_schema = %(schema)s
        AND tc.table_name = %(view)s
        AND tc.constraint_type = 'PRIMARY KEY'
        """

        pk_result = await db.fetch(pk_query, {"schema": schema_name, "view": view_name})

        primary_key = pk_result[0]["column_name"] if pk_result else None

        # Get indexes
        index_query = """
        SELECT indexname
        FROM pg_indexes
        WHERE schemaname = %(schema)s
        AND tablename = %(view)s
        """

        index_result = await db.fetch(index_query, {"schema": schema_name, "view": view_name})

        indexes = [row["indexname"] for row in index_result]

        return ViewInfo(
            view_name=f"{schema_name}.{view_name}",
            columns=columns,
            primary_key=primary_key,
            indexes=indexes,
            last_analyzed=time.time(),
        )

    async def _analyze_jsonb_column(
        self, table_name: str, column_name: str, db: CQRSRepository
    ) -> Optional[dict[str, any]]:
        """Analyze JSONB column structure.

        Args:
            table_name: Full table/view name
            column_name: JSONB column name
            db: Database repository

        Returns:
            Dictionary describing JSONB structure
        """
        # Sample the JSONB structure
        sample_query = f"""
        SELECT
            jsonb_object_keys({column_name}) as key,
            jsonb_typeof({column_name}->jsonb_object_keys({column_name})) as type
        FROM {table_name}
        WHERE {column_name} IS NOT NULL
        LIMIT 100
        """

        try:
            result = await db.fetch(sample_query)

            # Aggregate key types
            structure = {}
            for row in result:
                key = row["key"]
                type_name = row["type"]

                if key not in structure:
                    structure[key] = {"types": set()}

                structure[key]["types"].add(type_name)

            # Convert sets to lists for JSON serialization
            for value in structure.values():
                value["types"] = list(value["types"])

            return structure

        except Exception:
            # If analysis fails, return None
            return None

    def get_jsonb_paths(self, view_name: str, field_name: str) -> Optional[str]:
        """Get JSONB path for a field.

        Args:
            view_name: View name
            field_name: Field name to map

        Returns:
            JSONB path if found
        """
        if view_name not in self.cache:
            return None

        view_info = self.cache[view_name]

        # Check if field is a direct column
        if field_name in view_info.columns:
            return field_name

        # Check JSONB columns for the field
        for col_name, col_info in view_info.columns.items():
            if (
                col_info.is_jsonb
                and col_info.jsonb_structure
                and field_name in col_info.jsonb_structure
            ):
                return f"{col_name}->'{field_name}'"

        return None

    def clear(self) -> None:
        """Clear the cache."""
        self.cache.clear()

    def invalidate(self, view_name: str) -> None:
        """Invalidate cache for specific view.

        Args:
            view_name: View to invalidate
        """
        self.cache.pop(view_name, None)
