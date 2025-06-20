"""TurboRouter implementation for high-performance query execution.

TurboRouter bypasses GraphQL parsing and validation for registered queries
by directly executing pre-validated SQL templates.
"""

import hashlib
from collections import OrderedDict
from dataclasses import dataclass
from typing import Any


@dataclass
class TurboQuery:
    """Represents a pre-validated GraphQL query with its SQL template."""

    graphql_query: str
    sql_template: str
    param_mapping: dict[str, str]  # GraphQL variable path -> SQL parameter name
    operation_name: str | None = None

    def map_variables(self, graphql_variables: dict[str, Any]) -> dict[str, Any]:
        """Map GraphQL variables to SQL parameters.

        Args:
            graphql_variables: Variables from GraphQL request

        Returns:
            Dictionary of SQL parameter names to values
        """
        sql_params = {}

        for gql_path, sql_param in self.param_mapping.items():
            # Handle nested variable paths like "filters.name"
            value = graphql_variables
            for part in gql_path.split("."):
                if isinstance(value, dict) and part in value:
                    value = value[part]
                else:
                    value = None
                    break

            sql_params[sql_param] = value

        return sql_params


class TurboRegistry:
    """Registry for TurboRouter queries with LRU eviction."""

    def __init__(self, max_size: int = 1000) -> None:
        """Initialize the registry.

        Args:
            max_size: Maximum number of queries to cache
        """
        self.max_size = max_size
        self._queries: OrderedDict[str, TurboQuery] = OrderedDict()

    def hash_query(self, query: str) -> str:
        """Generate a normalized hash for a GraphQL query.

        Args:
            query: GraphQL query string

        Returns:
            Hex string hash of the normalized query
        """
        # Normalize whitespace
        normalized = " ".join(query.split())

        # Use SHA-256 for consistent hashing
        return hashlib.sha256(normalized.encode("utf-8")).hexdigest()

    def register(self, turbo_query: TurboQuery) -> str:
        """Register a TurboQuery for fast execution.

        Args:
            turbo_query: The TurboQuery to register

        Returns:
            The hash of the registered query
        """
        query_hash = self.hash_query(turbo_query.graphql_query)

        # Move to end if already exists (LRU behavior)
        if query_hash in self._queries:
            self._queries.move_to_end(query_hash)
        else:
            # Add new query
            self._queries[query_hash] = turbo_query

            # Evict oldest if over limit
            if len(self._queries) > self.max_size:
                self._queries.popitem(last=False)

        return query_hash

    def get(self, query: str) -> TurboQuery | None:
        """Get a registered TurboQuery by GraphQL query string.

        Args:
            query: GraphQL query string

        Returns:
            TurboQuery if registered, None otherwise
        """
        query_hash = self.hash_query(query)

        if query_hash in self._queries:
            # Move to end for LRU
            self._queries.move_to_end(query_hash)
            return self._queries[query_hash]

        return None

    def clear(self) -> None:
        """Clear all registered queries."""
        self._queries.clear()

    def __len__(self) -> int:
        """Return the number of registered queries."""
        return len(self._queries)


class TurboRouter:
    """High-performance router for registered GraphQL queries."""

    def __init__(self, registry: TurboRegistry) -> None:
        """Initialize the router with a registry.

        Args:
            registry: TurboRegistry containing registered queries
        """
        self.registry = registry

    async def execute(
        self,
        query: str,
        variables: dict[str, Any],
        context: dict[str, Any],
    ) -> dict[str, Any] | None:
        """Execute a query using the turbo path if registered.

        Args:
            query: GraphQL query string
            variables: GraphQL variables
            context: Request context (must contain 'db')

        Returns:
            Query result if executed via turbo path, None otherwise
        """
        # Look up the query in the registry
        turbo_query = self.registry.get(query)
        if turbo_query is None:
            return None

        # Get database from context
        db = context.get("db")
        if db is None:
            msg = "Database connection not found in context"
            raise ValueError(msg)

        # Map GraphQL variables to SQL parameters
        sql_params = turbo_query.map_variables(variables)

        # Execute the SQL directly
        result = await db.fetch(turbo_query.sql_template, sql_params)

        # Extract the result
        if result and len(result) > 0:
            # Assume the SQL returns a 'result' column with the formatted data
            row = result[0]
            if "result" in row:
                # Handle both single object and array results
                data = row["result"]

                # Determine the root field name from the query
                # This is a simplified approach - in production we'd parse the query
                import re

                match = re.search(r"{\s*(\w+)", query)
                if match:
                    root_field = match.group(1)
                    return {"data": {root_field: data}}

                return {"data": data}

        return {"data": None}
