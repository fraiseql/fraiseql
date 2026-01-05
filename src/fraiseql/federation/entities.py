"""Auto-generated _entities resolver for Apollo Federation.

The _entities query is how the Apollo Gateway resolves entity references
across subgraphs. This module provides EntitiesResolver which:

1. Accepts entity representations (references) from the gateway
2. Batches them by type for efficient queries
3. Executes queries against CQRS query-side tables (tv_*)
4. Returns resolved entities

Performance targets:
- Single entity: < 2ms
- Batch (100 entities): < 50ms
- Uses GIN-indexed JSONB from CQRS views
"""

from dataclasses import dataclass
from typing import Any

from .decorators import get_entity_metadata, get_entity_registry


@dataclass
class EntityResolutionRequest:
    """A request to resolve a single entity reference.

    Attributes:
        __typename: GraphQL type name
        [key_field]: The key field value (e.g., id: "123")
    """

    typename: str
    key_value: Any
    key_field: str


class EntitiesResolver:
    """Auto-generated resolver for Apollo Federation _entities query.

    Resolves entity references from the gateway using CQRS query-side tables.

    Example:
        >>> from fraiseql.federation import entity, EntitiesResolver
        >>>
        >>> @entity
        ... class User:
        ...     id: str
        ...     name: str
        >>>
        >>> resolver = EntitiesResolver()
        >>> # Get representations from Apollo Gateway
        >>> representations = [
        ...     {"__typename": "User", "id": "123"},
        ...     {"__typename": "User", "id": "456"},
        ... ]
        >>> # Resolve them
        >>> entities = await resolver.resolve(representations, db_pool)
    """

    def __init__(self):
        """Initialize the entities resolver.

        Registering the resolver registers it with the federation framework.
        """
        self.entity_registry = get_entity_registry()

    def _parse_representation(self, rep: dict[str, Any]) -> EntityResolutionRequest:
        """Parse a federation representation into a resolution request.

        Args:
            rep: Representation from Apollo Gateway with __typename and key

        Returns:
            EntityResolutionRequest with type, key field, and value

        Raises:
            ValueError: If representation is invalid
        """
        typename = rep.get("__typename")
        if not typename:
            raise ValueError("Missing __typename in representation")

        metadata = get_entity_metadata(typename)
        if not metadata:
            raise ValueError(f"Unknown entity type: {typename}")

        # Get key field from metadata
        key_field = metadata.resolved_key
        if isinstance(key_field, list):
            # Composite key - not yet supported in Federation Lite
            # Will be added in Federation Standard
            raise NotImplementedError(
                f"Composite keys not yet supported for {typename}. Use Federation Standard mode.",
            )

        key_value = rep.get(key_field)
        if key_value is None:
            raise ValueError(f"Missing key field '{key_field}' in {typename} representation")

        return EntityResolutionRequest(
            typename=typename,
            key_field=key_field,
            key_value=key_value,
        )

    def _build_queries(self, requests: list[EntityResolutionRequest]) -> dict[str, tuple]:
        """Build database queries grouped by entity type.

        Batches requests by type to minimize database round-trips.

        Args:
            requests: List of resolution requests

        Returns:
            Dict mapping typename to (table_name, key_field, [key_values])
        """
        queries: dict[str, tuple] = {}

        for req in requests:
            if req.typename not in queries:
                metadata = get_entity_metadata(req.typename)
                queries[req.typename] = {
                    "table_name": f"tv_{req.typename.lower()}",  # CQRS query table
                    "key_field": req.key_field,
                    "key_values": [],
                    "metadata": metadata,
                }

            queries[req.typename]["key_values"].append(req.key_value)

        return queries

    async def resolve(
        self,
        representations: list[dict[str, Any]],
        db_pool: Any,
    ) -> list[dict[str, Any] | None]:
        """Resolve entity references from Apollo Gateway.

        Executes efficient batched queries against CQRS query-side tables.

        Args:
            representations: List of entity references from gateway with __typename and key
            db_pool: Database connection pool

        Returns:
            List of resolved entity data (JSONB from tv_* tables) in same order as input

        Raises:
            ValueError: If any representation is invalid
        """
        # Parse all representations first
        requests = [self._parse_representation(rep) for rep in representations]

        # Build batched queries
        queries = self._build_queries(requests)

        # Execute queries and collect results
        results_by_key: dict[tuple, dict[str, Any]] = {}

        async with db_pool.acquire() as conn:
            for typename, query_info in queries.items():
                table_name = query_info["table_name"]
                key_field = query_info["key_field"]
                key_values = query_info["key_values"]

                if not key_values:
                    continue

                # Build SQL: SELECT data FROM tv_user WHERE id IN ($1, $2, ...)
                placeholders = ", ".join(f"${i}" for i in range(1, len(key_values) + 1))
                sql = (
                    f"SELECT {key_field}, data FROM {table_name} "
                    f"WHERE {key_field} IN ({placeholders})"
                )

                try:
                    rows = await conn.fetch(sql, *key_values)
                except Exception as e:
                    # Table might not exist - return None for these entities
                    # In production, log this error
                    import logging

                    logger = logging.getLogger(__name__)
                    logger.warning(f"Failed to fetch entities for {typename}: {e}")
                    for key_value in key_values:
                        results_by_key[(typename, key_value)] = None
                    continue

                # Cache results by (typename, key_value)
                for row in rows:
                    key_value = row[key_field]
                    entity_data = dict(row["data"]) if row["data"] else {}

                    # Ensure __typename is set for gateway
                    entity_data["__typename"] = typename

                    results_by_key[(typename, key_value)] = entity_data

        # Return results in original order
        resolved = []
        for req in requests:
            entity = results_by_key.get((req.typename, req.key_value))
            resolved.append(entity)

        return resolved

    def get_supported_types(self) -> list[str]:
        """Get list of supported entity types.

        Returns:
            List of entity type names that this resolver can handle
        """
        return list(self.entity_registry.keys())

    def get_key_field(self, typename: str) -> str | None:
        """Get the key field for an entity type.

        Args:
            typename: GraphQL type name

        Returns:
            Key field name, or None if type not registered
        """
        metadata = get_entity_metadata(typename)
        if metadata and isinstance(metadata.resolved_key, str):
            return metadata.resolved_key
        return None
