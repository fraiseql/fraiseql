"""FraiseQL Relay Integration

Main integration layer that connects FraiseQL GraphQL schemas with the
PostgreSQL Relay extension for high-performance Global Object Identification.
"""

from typing import Any, Dict, Optional, Type

import fraiseql
from fraiseql import CQRSRepository

from .types import GlobalID, GlobalIDConverter, Node, RelayContext, RelayNode


class RelayIntegration:
    """Main class for integrating FraiseQL with the PostgreSQL Relay extension.

    Provides seamless GraphQL Relay specification compliance with high-performance
    PostgreSQL-native node resolution.
    """

    def __init__(self, db_pool, global_id_format: str = "uuid"):
        """Initialize Relay integration.

        Args:
            db_pool: Database connection pool
            global_id_format: Either "uuid" (direct UUIDs) or "base64" (encoded)
        """
        self.db_pool = db_pool
        self.global_id_format = global_id_format
        self.context = None  # Will be initialized with first connection
        self._schema_modified = False

    async def _ensure_context(self) -> RelayContext:
        """Ensure RelayContext is initialized with a database connection."""
        if not self.context:
            # Get a connection from the pool
            async with self.db_pool.acquire() as conn:
                repo = CQRSRepository(conn)
                self.context = RelayContext(repo)
        return self.context

    async def add_node_resolver(self, schema) -> None:
        """Add the node resolver to an existing FraiseQL schema.

        This adds the `node(id: UUID!): Node` query that's required by Relay.
        """

        @fraiseql.query
        async def node(info, id: GlobalID) -> Optional[RelayNode]:
            """Relay Node interface resolver.

            Resolves any object by its global ID using the PostgreSQL extension.
            """
            context = await self._ensure_context()
            return await context.resolve_node(id)

        # Add the resolver to the schema
        if hasattr(schema, "add_query"):
            schema.add_query(node)
        else:
            # Fallback for different schema types
            schema.node = node

        self._schema_modified = True

    async def register_entity_type(
        self,
        entity_type: Type[Node],
        entity_name: str,
        pk_column: str,
        v_table: str,
        source_table: str,
        tv_table: Optional[str] = None,
        mv_table: Optional[str] = None,
        turbo_function: Optional[str] = None,
        lazy_cache_key_pattern: Optional[str] = None,
        **kwargs,
    ) -> None:
        """Register an entity type with the PostgreSQL extension.

        Args:
            entity_type: Python class implementing the Node interface
            entity_name: Internal entity name (e.g., 'User')
            pk_column: Primary key column name (e.g., 'pk_user')
            v_table: Real-time view name (e.g., 'v_user')
            source_table: Command side table (e.g., 'tb_user')
            tv_table: Materialized table name (optional)
            mv_table: Materialized view name (optional)
            turbo_function: TurboRouter function name (optional)
            lazy_cache_key_pattern: Lazy cache key pattern (optional)
        """
        context = await self._ensure_context()

        # Determine GraphQL type name
        graphql_type = getattr(entity_type, "__name__", entity_name)

        await context.register_entity(
            entity_name=entity_name,
            graphql_type=graphql_type,
            python_type=entity_type,
            pk_column=pk_column,
            v_table=v_table,
            source_table=source_table,
            tv_table=tv_table,
            mv_table=mv_table,
            turbo_function=turbo_function,
            lazy_cache_key_pattern=lazy_cache_key_pattern,
            **kwargs,
        )

    async def auto_register_entities(self, schema) -> int:
        """Automatically discover and register entities from a FraiseQL schema.

        Scans the schema for types implementing the Node interface and
        attempts to auto-register them with sensible defaults.

        Returns:
            Number of entities registered
        """
        from .discovery import EntityDiscovery

        discovery = EntityDiscovery(self.db_pool)
        entities = await discovery.discover_from_schema(schema)

        registered_count = 0
        for entity_info in entities:
            await self.register_entity_type(**entity_info)
            registered_count += 1

        return registered_count

    async def create_relay_context(self, request) -> Dict[str, Any]:
        """Create a GraphQL context with Relay support.

        This should be used as the context_getter for FraiseQL applications.
        """
        context = await self._ensure_context()

        # Standard FraiseQL context
        base_context = {
            "db": context.db,
            "repo": context.db,  # Alias for backward compatibility
            "request": request,
        }

        # Add Relay-specific context
        base_context.update(
            {
                "relay": context,
                "node_resolver": context.resolve_node,
                "batch_node_resolver": context.resolve_nodes_batch,
            }
        )

        # Add user/tenant info if available
        if hasattr(request, "state"):
            if hasattr(request.state, "user"):
                base_context["user"] = request.state.user
            if hasattr(request.state, "tenant_id"):
                base_context["tenant_id"] = request.state.tenant_id

        return base_context

    async def get_health_status(self) -> Dict[str, Any]:
        """Get health status of the PostgreSQL Relay extension."""
        context = await self._ensure_context()
        return await context.get_extension_health()

    async def refresh_views(self) -> bool:
        """Refresh all materialized views and the v_nodes view."""
        context = await self._ensure_context()
        return await context.refresh_nodes_view()

    def create_global_id_encoder(self):
        """Create a Global ID encoder/decoder for the configured format."""
        if self.global_id_format == "base64":
            return GlobalIDConverter()
        # Direct UUID format - no encoding needed
        return None


async def enable_relay_support(
    schema, db_pool, global_id_format: str = "uuid", auto_register: bool = True
) -> RelayIntegration:
    """Enable Relay support for an existing FraiseQL schema.

    This is the main entry point for adding Relay specification compliance
    to a FraiseQL application.

    Args:
        schema: FraiseQL GraphQL schema
        db_pool: Database connection pool
        global_id_format: "uuid" for direct UUIDs, "base64" for encoded IDs
        auto_register: Whether to automatically discover and register entities

    Returns:
        RelayIntegration instance for further configuration

    Example:
        ```python
        from fraiseql.extensions.relay import enable_relay_support

        # Enable Relay support
        relay = await enable_relay_support(schema, db_pool)

        # Manually register specific entities
        await relay.register_entity_type(
            User, "User", "pk_user", "v_user", "tb_user",
            tv_table="tv_user", turbo_function="turbo.fn_get_users"
        )
        ```
    """
    relay = RelayIntegration(db_pool, global_id_format)

    # Add the node resolver to the schema
    await relay.add_node_resolver(schema)

    # Auto-register entities if requested
    if auto_register:
        registered_count = await relay.auto_register_entities(schema)
        print(f"FraiseQL Relay: Auto-registered {registered_count} entities")

    # Check extension health
    try:
        health = await relay.get_health_status()
        print(f"FraiseQL Relay: Extension status = {health.get('status', 'unknown')}")
    except Exception as e:
        print(f"FraiseQL Relay: Warning - Extension health check failed: {e}")

    return relay


# Decorator for easy entity registration
def relay_entity(entity_name: str, pk_column: str, v_table: str, source_table: str, **kwargs):
    """Decorator to mark a FraiseQL type as a Relay entity.

    Example:
        ```python
        @relay_entity("User", "pk_user", "v_user", "tb_user", tv_table="tv_user")
        @fraiseql.type
        class User:
            id: UUID
            name: str
            email: str
        ```
    """

    def decorator(cls):
        # Store metadata on the class for later registration
        cls._relay_entity_info = {
            "entity_name": entity_name,
            "pk_column": pk_column,
            "v_table": v_table,
            "source_table": source_table,
            **kwargs,
        }

        # Ensure the class implements Node interface
        if not hasattr(cls, "id"):
            raise TypeError(f"Relay entity {cls.__name__} must have an 'id' field")

        return cls

    return decorator


# Context manager for batch operations
class RelayBatchContext:
    """Context manager for efficient batch node resolution.

    Example:
        ```python
        async with RelayBatchContext(relay) as batch:
            user = await batch.resolve_node(user_id)
            post = await batch.resolve_node(post_id)
            comment = await batch.resolve_node(comment_id)
        # All nodes resolved in a single database call
        ```
    """

    def __init__(self, relay_integration: RelayIntegration):
        self.relay = relay_integration
        self.pending_ids = []
        self.resolved_nodes = {}
        self.context = None

    async def __aenter__(self):
        self.context = await self.relay._ensure_context()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.pending_ids:
            # Resolve all pending IDs in batch
            nodes = await self.context.resolve_nodes_batch(self.pending_ids)
            for node_id, node in zip(self.pending_ids, nodes, strict=False):
                self.resolved_nodes[node_id] = node

    async def resolve_node(self, node_id: GlobalID) -> Optional[Node]:
        """Queue a node for batch resolution."""
        if node_id in self.resolved_nodes:
            return self.resolved_nodes[node_id]

        if node_id not in self.pending_ids:
            self.pending_ids.append(node_id)

        # Return placeholder for now - actual resolution happens on context exit
        return None
