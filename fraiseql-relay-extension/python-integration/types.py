"""FraiseQL Relay Types

GraphQL types and interfaces for Relay specification compliance.
"""

from typing import Any, Dict, Optional, Protocol, Union, runtime_checkable
from uuid import UUID

import fraiseql

# Global ID type alias for clarity
GlobalID = Union[UUID, str]


@runtime_checkable
class Node(Protocol):
    """GraphQL Node interface for Relay Global Object Identification.

    All entities that can be refetched by global ID should implement this interface.
    """

    id: GlobalID  # The global identifier for this object

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Node":
        """Create instance from dictionary data."""
        ...


@fraiseql.interface
class RelayNode:
    """FraiseQL implementation of the Relay Node interface.

    This provides the base interface that all Relay-compatible entities should implement.
    """

    id: GlobalID

    @staticmethod
    def resolve_type(obj: Any, info: Any) -> str:
        """Resolve the concrete GraphQL type for a node object.

        This is called by GraphQL to determine which concrete type
        to use when returning a Node interface result.
        """
        if hasattr(obj, "__typename"):
            return obj.__typename
        if hasattr(obj, "_typename"):
            return obj._typename
        # Fallback to class name
        return obj.__class__.__name__


@fraiseql.type
class NodeResult:
    """Result type for node resolution operations.

    Used internally by the extension to return node data with metadata.
    """

    typename: str
    data: fraiseql.JSON
    entity_name: str
    source_used: Optional[str] = None


class RelayContext:
    """Context object for Relay operations.

    Provides access to the PostgreSQL extension functions and maintains
    state for node resolution and entity management.
    """

    def __init__(self, db_connection):
        self.db = db_connection
        self._type_registry: Dict[str, type] = {}
        self._entity_registry: Dict[str, Dict[str, Any]] = {}

    def register_type(self, typename: str, python_type: type) -> None:
        """Register a Python type for dynamic node resolution."""
        self._type_registry[typename] = python_type

    def register_entity(
        self,
        entity_name: str,
        graphql_type: str,
        python_type: type,
        pk_column: str,
        v_table: str,
        source_table: str,
        **kwargs,
    ) -> None:
        """Register an entity with both the PostgreSQL extension and Python type system."""
        # Register in PostgreSQL extension
        self.db.execute_function(
            "core.register_entity",
            {
                "p_entity_name": entity_name,
                "p_graphql_type": graphql_type,
                "p_pk_column": pk_column,
                "p_v_table": v_table,
                "p_source_table": source_table,
                **kwargs,
            },
        )

        # Register Python type for resolution
        self.register_type(graphql_type, python_type)

        # Store entity metadata
        self._entity_registry[entity_name] = {
            "graphql_type": graphql_type,
            "python_type": python_type,
            "pk_column": pk_column,
            "v_table": v_table,
            "source_table": source_table,
            **kwargs,
        }

    async def resolve_node(self, node_id: GlobalID) -> Optional[Node]:
        """Resolve a node by its global ID using the PostgreSQL extension.

        This uses the high-performance C implementation when available,
        falling back to the SQL implementation.
        """
        try:
            # Try C implementation first
            result = await self.db.execute_function(
                "core.fraiseql_resolve_node_fast", {"node_id": node_id}
            )
        except Exception:
            # Fallback to SQL implementation
            result = await self.db.execute_function("core.resolve_node_smart", {"node_id": node_id})

        if not result:
            return None

        typename = result.get("__typename")
        data = result.get("data", {})

        # Resolve Python type
        python_type = self._type_registry.get(typename)
        if not python_type:
            raise ValueError(f"No Python type registered for GraphQL type: {typename}")

        # Create instance
        if hasattr(python_type, "from_dict"):
            return python_type.from_dict(data)
        # Fallback: try to create instance directly
        return python_type(**data)

    async def resolve_nodes_batch(self, node_ids: list[GlobalID]) -> list[Optional[Node]]:
        """Batch resolve multiple nodes for performance.

        Uses the C batch resolution function when available.
        """
        try:
            results = await self.db.execute_function(
                "core.fraiseql_resolve_nodes_batch", {"node_ids": node_ids}
            )
        except Exception:
            # Fallback to individual resolution
            nodes = []
            for node_id in node_ids:
                node = await self.resolve_node(node_id)
                nodes.append(node)
            return nodes

        nodes = []
        node_map = {result["id"]: result for result in results}

        for node_id in node_ids:
            result = node_map.get(node_id)
            if result:
                typename = result.get("__typename")
                data = result.get("data", {})

                python_type = self._type_registry.get(typename)
                if python_type and hasattr(python_type, "from_dict"):
                    nodes.append(python_type.from_dict(data))
                else:
                    nodes.append(None)
            else:
                nodes.append(None)

        return nodes

    async def get_registered_entities(self) -> list[Dict[str, Any]]:
        """Get all registered entities from the PostgreSQL extension."""
        return await self.db.execute_function("core.list_registered_entities")

    async def get_extension_health(self) -> Dict[str, Any]:
        """Check the health status of the PostgreSQL extension."""
        return await self.db.execute_function("core.fraiseql_relay_health")

    async def refresh_nodes_view(self) -> bool:
        """Refresh the v_nodes view using the C implementation when available."""
        try:
            return await self.db.execute_function("core.fraiseql_refresh_nodes_view_fast")
        except Exception:
            # Fallback to SQL implementation
            await self.db.execute_function("core.refresh_v_nodes_view")
            return True


# Type converters for different Global ID strategies
class GlobalIDConverter:
    """Utilities for converting between different Global ID formats."""

    @staticmethod
    async def encode_global_id(db_connection, typename: str, local_id: UUID) -> str:
        """Encode a Global ID using the PostgreSQL extension."""
        return await db_connection.execute_function(
            "core.fraiseql_encode_global_id", {"typename": typename, "local_id": local_id}
        )

    @staticmethod
    async def decode_global_id(db_connection, global_id: str) -> Dict[str, Any]:
        """Decode a Global ID using the PostgreSQL extension."""
        return await db_connection.execute_function(
            "core.fraiseql_decode_global_id", {"global_id": global_id}
        )

    @staticmethod
    def is_uuid_format(global_id: GlobalID) -> bool:
        """Check if a Global ID is in direct UUID format."""
        if isinstance(global_id, UUID):
            return True
        if isinstance(global_id, str):
            try:
                UUID(global_id)
                return True
            except ValueError:
                return False
        return False
