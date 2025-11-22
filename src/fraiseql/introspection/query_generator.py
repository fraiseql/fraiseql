"""Query generation for AutoFraiseQL.

This module provides the QueryGenerator class that creates standard GraphQL
queries (find_one, find_all, connection) for auto-discovered types.
"""

import logging
from typing import Any, Optional
from uuid import UUID

from .metadata_parser import TypeAnnotation

logger = logging.getLogger(__name__)


class QueryGenerator:
    """Generate standard queries for auto-discovered types."""

    def generate_queries_for_type(
        self, type_class: Any, view_name: str, schema_name: str, annotation: TypeAnnotation
    ) -> list[callable]:
        """Generate standard queries for a type.

        Generates:
        1. find_one(id) → Single item by UUID
        2. find_all(where, order_by, limit, offset) → List
        3. connection(first, after, where) → Relay pagination (optional)

        Args:
            type_class: The generated @type class
            view_name: Database view name
            schema_name: Database schema name
            annotation: Parsed @fraiseql:type annotation

        Returns:
            List of decorated query functions
        """
        queries = []

        # 1. Generate find_one query
        queries.append(self._generate_find_one_query(type_class, view_name, schema_name))

        # 2. Generate find_all query
        queries.append(self._generate_find_all_query(type_class, view_name, schema_name))

        # 3. Generate connection query (optional, for Relay)
        if annotation.filter_config:
            queries.append(self._generate_connection_query(type_class, view_name, schema_name))

        return queries

    def _generate_find_one_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> callable:
        """Generate find_one(id) query."""

        # Create query function dynamically
        async def find_one_impl(info: Any, id: UUID) -> Optional[Any]:
            """Get a single item by ID."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"
            result = await db.find_one(sql_source, where={"id": id})
            return result

        # Rename function
        type_name = type_class.__name__
        find_one_impl.__name__ = type_name[0].lower() + type_name[1:]
        find_one_impl.__qualname__ = find_one_impl.__name__

        # Apply @query decorator
        from fraiseql import query

        return query(find_one_impl)

    def _generate_find_all_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> callable:
        """Generate find_all(where, order_by, ...) query."""

        async def find_all_impl(
            info: Any,
            where: Optional[dict] = None,
            order_by: Optional[dict] = None,
            limit: Optional[int] = None,
            offset: Optional[int] = None,
        ) -> list[Any]:
            """Get all items with optional filtering."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"
            results = await db.find(
                sql_source, where=where, order_by=order_by, limit=limit, offset=offset
            )
            return results

        # Rename function
        type_name = type_class.__name__
        plural_name = type_name[0].lower() + type_name[1:] + "s"
        find_all_impl.__name__ = plural_name
        find_all_impl.__qualname__ = plural_name

        # Apply @query decorator
        from fraiseql import query

        return query(find_all_impl)

    def _generate_connection_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> callable:
        """Generate Relay connection query (optional)."""

        # For now, return a basic connection implementation
        # In a full implementation, this would use Relay connection spec
        async def connection_impl(
            info: Any,
            first: Optional[int] = None,
            after: Optional[str] = None,
            where: Optional[dict] = None,
        ) -> dict:
            """Relay-style connection with pagination."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"

            # Basic implementation - would need proper Relay connection logic
            results = await db.find(
                sql_source, where=where, limit=first, offset=int(after) if after else 0
            )

            return {
                "edges": [{"node": item, "cursor": str(i)} for i, item in enumerate(results)],
                "pageInfo": {
                    "hasNextPage": len(results) == first if first else False,
                    "endCursor": str(len(results) - 1) if results else None,
                },
            }

        # Rename function
        type_name = type_class.__name__
        connection_name = type_name[0].lower() + type_name[1:] + "Connection"
        connection_impl.__name__ = connection_name
        connection_impl.__qualname__ = connection_name

        # Apply @query decorator
        from fraiseql import query

        return query(connection_impl)
