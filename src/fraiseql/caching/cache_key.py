"""Cache key generation for FraiseQL queries.

This module provides utilities for generating consistent cache keys
from query parameters, ensuring proper serialization of complex types.
"""

import hashlib
import json
from datetime import date, datetime
from typing import Any
from uuid import UUID


class CacheKeyBuilder:
    """Builds consistent cache keys from query parameters."""

    def __init__(self, prefix: str = "fraiseql") -> None:
        """Initialize cache key builder.

        Args:
            prefix: Global prefix for all cache keys
        """
        self.prefix = prefix

    def build_key(
        self,
        query_name: str,
        tenant_id: Any | None = None,
        filters: dict[str, Any] | None = None,
        order_by: list[tuple[str, str]] | None = None,
        limit: int | None = None,
        offset: int | None = None,
        **kwargs: Any,
    ) -> str:
        """Build a cache key from query parameters.

        Args:
            query_name: Name of the query/view
            tenant_id: Tenant ID for multi-tenant isolation (CRITICAL for security!)
            filters: Filter conditions
            order_by: Order by clauses
            limit: Result limit
            offset: Result offset
            **kwargs: Additional parameters

        Returns:
            A consistent cache key string with tenant isolation

        Security Note:
            Including tenant_id in the cache key prevents cross-tenant cache poisoning.
            Without this, Tenant A could access Tenant B's cached data!
        """
        parts = [self.prefix]

        # Add tenant_id as second component for tenant isolation
        if tenant_id is not None:
            parts.append(str(tenant_id))

        parts.append(query_name)

        # Add filters to key
        if filters:
            filter_parts = []
            for field, value in sorted(filters.items()):
                filter_str = self._serialize_filter(field, value)
                filter_parts.append(filter_str)
            parts.extend(filter_parts)

        # Add order by
        if order_by:
            # Convert to tuples if needed to avoid unpacking errors
            order_by_tuples = self._convert_order_by_to_tuples(order_by)
            if order_by_tuples:
                for field, direction in order_by_tuples:
                    parts.append(f"order:{field}:{direction}")

        # Add pagination
        if limit is not None:
            parts.append(f"limit:{limit}")
        if offset is not None:
            parts.append(f"offset:{offset}")

        # Add any extra kwargs
        for key, value in sorted(kwargs.items()):
            parts.append(f"{key}:{self._serialize_value(value)}")

        # Join with colons
        return ":".join(parts)

    def _serialize_filter(self, field: str, value: Any) -> str:
        """Serialize a filter condition.

        Args:
            field: Field name
            value: Filter value or dict of operators

        Returns:
            Serialized filter string
        """
        if isinstance(value, dict):
            # Handle operator dict like {"gte": 10, "lte": 100}
            parts = [field]
            for op, val in sorted(value.items()):
                parts.append(f"{op}:{self._serialize_value(val)}")
            return ":".join(parts)
        # Simple equality
        return f"{field}:{self._serialize_value(value)}"

    def _convert_order_by_to_tuples(self, order_by: Any) -> list[tuple[str, str]] | None:
        """Convert any OrderBy format to list of tuples.

        Args:
            order_by: OrderBy in any format (GraphQL dicts, tuples, OrderBySet)

        Returns:
            List of (field, direction) tuples or None
        """
        if not order_by:
            return None

        # Already a list of tuples
        if isinstance(order_by, list) and order_by and isinstance(order_by[0], tuple):
            return order_by

        # GraphQL format - convert using FraiseQL
        if isinstance(order_by, (list, dict)):
            try:
                from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

                order_by_set = _convert_order_by_input_to_sql(order_by)
                if order_by_set:
                    return [(instr.field, instr.direction) for instr in order_by_set.instructions]
            except ImportError:
                pass

        # OrderBySet object
        if hasattr(order_by, "instructions"):
            return [(instr.field, instr.direction) for instr in order_by.instructions]

        return None

    def _serialize_value(self, value: Any) -> str:
        """Serialize a value for use in cache key.

        Args:
            value: Value to serialize

        Returns:
            String representation of value
        """
        if isinstance(value, UUID):
            return str(value)
        if isinstance(value, (datetime, date)):
            return value.isoformat()
        if isinstance(value, (list, tuple)):
            # For lists, create a hash to keep key length manageable
            if all(isinstance(v, str) for v in value):
                return ",".join(sorted(value))
            # For complex lists, use a hash
            content = json.dumps(value, sort_keys=True, default=str)
            return hashlib.md5(content.encode()).hexdigest()[:8]
        if isinstance(value, bool):
            return str(value).lower()
        if value is None:
            return "null"
        return str(value)

    def build_mutation_pattern(self, table_name: str) -> str:
        """Build a pattern for invalidating cache after mutations.

        Args:
            table_name: Name of the table/view that was mutated

        Returns:
            A pattern string for cache invalidation
        """
        return f"{self.prefix}:{table_name}:*"
