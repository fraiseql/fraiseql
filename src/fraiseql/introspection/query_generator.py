"""Query generation for AutoFraiseQL.
This module provides the QueryGenerator class that creates standard GraphQL
queries (find_one, find_all, connection) for auto-discovered types.
"""

import logging
from typing import Any, Callable
from uuid import UUID

from .metadata_parser import TypeAnnotation

logger = logging.getLogger(__name__)

class QueryGenerator:
    """Generate standard queries for auto-discovered types."""

    def generate_queries_for_type(
        self, type_class: Any, view_name: str, schema_name: str, annotation: TypeAnnotation
    ) -> list[Callable]:
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
        queries.append(
            self._generate_find_one_query(type_class, view_name, schema_name)
        )

        # 2. Generate find_all query
        queries.append(
            self._generate_find_all_query(type_class, view_name, schema_name)
        )

        # 3. Generate connection query (optional, for Relay)
        if annotation.filter_config:
            queries.append(
                self._generate_connection_query(type_class, view_name, schema_name)
            )

        return queries

    def _generate_find_one_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> Callable:
        """Generate find_one(id) query."""
        # Create query function dynamically
        async def find_one_impl(info: Any, id: UUID) -> Any | None:
            """Get a single item by ID."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"
            result = await db.find_one(sql_source, where={"id": id})
            return result

        # Apply @query decorator
        from fraiseql import query
        decorated_query = query(
            name=f"find_{type_class.__name__.lower()}_by_id",
            returns=type_class,
            nullable=True
        )(find_one_impl)

        return decorated_query

    def _generate_find_all_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> Callable:
        """Generate find_all query."""
        async def find_all_impl(
            info: Any,
            where: dict[str, Any] | None = None,
            order_by: list[str] | None = None,
            limit: int | None = None,
            offset: int | None = None
        ) -> list[Any]:
            """Get multiple items."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"
            results = await db.find_all(
                sql_source,
                where=where,
                order_by=order_by,
                limit=limit,
                offset=offset
            )
            return results

        from fraiseql import query
        decorated_query = query(
            name=f"all_{type_class.__name__.lower()}s",
            returns=list[type_class]
        )(find_all_impl)

        return decorated_query

    def _generate_connection_query(
        self, type_class: Any, view_name: str, schema_name: str
    ) -> Callable:
        """Generate connection query for Relay pagination."""
        async def connection_impl(
            info: Any,
            first: int | None = None,
            after: str | None = None,
            where: dict[str, Any] | None = None
        ) -> Any:
            """Get items with Relay pagination."""
            db = info.context["db"]
            sql_source = f"{schema_name}.{view_name}"
            # This would use a Relay-compliant connection helper
            return None

        from fraiseql import query
        decorated_query = query(
            name=f"{type_class.__name__.lower()}_connection",
            returns=Any  # Connection type would be generated
        )(connection_impl)

        return decorated_query
