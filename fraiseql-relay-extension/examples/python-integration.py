"""
FraiseQL Relay Extension - Python Integration Examples

This file demonstrates how to integrate the PostgreSQL Relay extension
with Python FraiseQL applications.
"""

import asyncio
from typing import Optional
from uuid import UUID, uuid4
from datetime import datetime

import fraiseql
from fraiseql import CQRSRepository
from fraiseql.fastapi import create_fraiseql_router

# Import the Relay integration
from fraiseql_relay_extension.python_integration import (
    enable_relay_support,
    relay_entity,
    RelayIntegration,
    Node,
    GlobalID
)

# =============================================================================
# Example 1: Basic Entity Definitions
# =============================================================================

@fraiseql.type
class User(Node):
    """User entity implementing Relay Node interface."""

    id: UUID  # Global ID (required by Node interface)
    email: str
    name: str
    avatar_url: Optional[str] = None
    is_active: bool = True
    created_at: datetime
    updated_at: datetime

    @classmethod
    def from_dict(cls, data: dict) -> "User":
        """Create User instance from database JSONB data."""
        return cls(
            id=UUID(data["id"]),
            email=data["email"],
            name=data["name"],
            avatar_url=data.get("avatar_url"),
            is_active=data.get("is_active", True),
            created_at=data["created_at"],
            updated_at=data["updated_at"]
        )


@relay_entity(
    entity_name="Post",
    pk_column="pk_post",
    v_table="v_post",
    source_table="tb_post",
    tv_table="tv_post"  # Optional materialized table
)
@fraiseql.type
class Post(Node):
    """Post entity with automatic registration via decorator."""

    id: UUID
    title: str
    content: str
    author_id: UUID  # Reference to User
    slug: str
    is_published: bool = False
    view_count: int = 0
    created_at: datetime
    updated_at: datetime

    @classmethod
    def from_dict(cls, data: dict) -> "Post":
        return cls(
            id=UUID(data["id"]),
            title=data["title"],
            content=data["content"],
            author_id=UUID(data["author_id"]),
            slug=data["slug"],
            is_published=data.get("is_published", False),
            view_count=data.get("view_count", 0),
            created_at=data["created_at"],
            updated_at=data["updated_at"]
        )


@fraiseql.type
class Comment(Node):
    """Comment entity."""

    id: UUID
    post_id: UUID
    author_id: UUID
    content: str
    parent_id: Optional[UUID] = None  # For nested comments
    is_edited: bool = False
    created_at: datetime
    updated_at: datetime

    @classmethod
    def from_dict(cls, data: dict) -> "Comment":
        return cls(
            id=UUID(data["id"]),
            post_id=UUID(data["post_id"]),
            author_id=UUID(data["author_id"]),
            content=data["content"],
            parent_id=UUID(data["parent_id"]) if data.get("parent_id") else None,
            is_edited=data.get("is_edited", False),
            created_at=data["created_at"],
            updated_at=data["updated_at"]
        )


# =============================================================================
# Example 2: Schema Setup with Relay Support
# =============================================================================

async def create_schema_with_relay(db_pool):
    """Create a FraiseQL schema with Relay support enabled."""

    # Define regular FraiseQL queries
    @fraiseql.query
    async def users(info, limit: int = 10) -> list[User]:
        """Get users list."""
        db = info.context["db"]
        results = await db.find("v_user", limit=limit)
        return [User.from_dict(r["data"]) for r in results]

    @fraiseql.query
    async def posts(info, limit: int = 10) -> list[Post]:
        """Get posts list."""
        db = info.context["db"]
        results = await db.find("v_post", limit=limit)
        return [Post.from_dict(r["data"]) for r in results]

    # Create base schema
    schema = fraiseql.build_schema([User, Post, Comment], queries=[users, posts])

    # Enable Relay support - this adds the node(id: UUID!) resolver
    relay = await enable_relay_support(
        schema=schema,
        db_pool=db_pool,
        global_id_format="uuid",  # Use direct UUIDs
        auto_register=True        # Automatically discover entities
    )

    # Manual entity registration with advanced options
    await relay.register_entity_type(
        entity_type=User,
        entity_name="User",
        pk_column="pk_user",
        v_table="v_user",
        source_table="tb_user",
        tv_table="tv_user",                    # Materialized table
        lazy_cache_key_pattern="user:{id}",    # Lazy caching
        identifier_column="email"
    )

    await relay.register_entity_type(
        entity_type=Comment,
        entity_name="Comment",
        pk_column="pk_comment",
        v_table="v_comment",
        source_table="tb_comment"
    )

    return schema, relay


# =============================================================================
# Example 3: Custom Context with Relay Support
# =============================================================================

async def create_context_with_relay(request, relay: RelayIntegration):
    """Create GraphQL context with Relay integration."""

    # Get base context from Relay integration
    context = await relay.create_relay_context(request)

    # Add custom context data
    context.update({
        "user_id": getattr(request.state, "user_id", None),
        "tenant_id": getattr(request.state, "tenant_id", None),
    })

    return context


# =============================================================================
# Example 4: Advanced Node Resolution
# =============================================================================

@fraiseql.query
async def resolve_mixed_entities(info, ids: list[UUID]) -> list[Optional[Node]]:
    """
    Resolve multiple entities of different types by their global IDs.

    This demonstrates the power of Relay's Global Object Identification.
    """
    relay = info.context["relay"]

    # Use batch resolution for performance
    nodes = await relay.resolve_nodes_batch(ids)

    return nodes


@fraiseql.query
async def search_across_all_entities(info, search_term: str) -> list[Node]:
    """
    Search across all registered entity types.

    This leverages the unified v_nodes view.
    """
    db = info.context["db"]

    # Search in the unified nodes view
    # Note: This requires full-text search setup in your views
    query = """
        SELECT * FROM core.v_nodes
        WHERE data::text ILIKE $1
        ORDER BY __typename, created_at DESC
        LIMIT 50
    """

    results = await db.execute_raw(query, [f"%{search_term}%"])

    nodes = []
    for result in results:
        # Use the relay context to resolve each node
        node = await info.context["relay"].resolve_node(result["id"])
        if node:
            nodes.append(node)

    return nodes


# =============================================================================
# Example 5: Performance Optimization
# =============================================================================

class OptimizedNodeLoader:
    """
    DataLoader-style batch loading using the Relay extension.
    """

    def __init__(self, relay_context):
        self.relay = relay_context
        self.pending_loads = {}
        self.batch_timeout = 0.001  # 1ms batch window

    async def load(self, node_id: UUID) -> Optional[Node]:
        """Load a single node with automatic batching."""

        if node_id in self.pending_loads:
            return await self.pending_loads[node_id]

        # Create future for this load
        future = asyncio.Future()
        self.pending_loads[node_id] = future

        # Schedule batch processing
        asyncio.create_task(self._process_batch_soon())

        return await future

    async def _process_batch_soon(self):
        """Process batch after small delay to collect more requests."""
        await asyncio.sleep(self.batch_timeout)

        if not self.pending_loads:
            return

        # Get all pending IDs and futures
        ids = list(self.pending_loads.keys())
        futures = list(self.pending_loads.values())

        # Clear pending loads
        self.pending_loads.clear()

        try:
            # Batch resolve
            nodes = await self.relay.resolve_nodes_batch(ids)

            # Resolve futures
            for future, node in zip(futures, nodes):
                if not future.done():
                    future.set_result(node)

        except Exception as e:
            # Resolve all futures with the exception
            for future in futures:
                if not future.done():
                    future.set_exception(e)


# =============================================================================
# Example 6: FastAPI Integration
# =============================================================================

async def create_fastapi_app_with_relay():
    """Create a FastAPI application with Relay-enabled GraphQL endpoint."""

    from fastapi import FastAPI
    from fraiseql.fastapi import FraiseQLConfig
    import asyncpg

    app = FastAPI(title="FraiseQL Relay Example")

    # Database setup
    db_pool = await asyncpg.create_pool(
        "postgresql://user:password@localhost/database"
    )

    # Create schema with Relay
    schema, relay = await create_schema_with_relay(db_pool)

    # Custom context factory
    async def get_context(request):
        return await create_context_with_relay(request, relay)

    # Create FraiseQL router with Relay support
    graphql_router = create_fraiseql_router(
        schema=schema,
        context_getter=get_context,
        config=FraiseQLConfig(
            database_pool=db_pool,
            enable_playground=True
        )
    )

    app.include_router(graphql_router, prefix="/graphql")

    # Health check endpoint
    @app.get("/relay/health")
    async def relay_health():
        """Check Relay extension health."""
        return await relay.get_health_status()

    # Entity registry endpoint
    @app.get("/relay/entities")
    async def list_entities():
        """List registered entities."""
        context = await relay._ensure_context()
        return await context.get_registered_entities()

    return app


# =============================================================================
# Example 7: Testing Utilities
# =============================================================================

async def test_node_resolution():
    """Test node resolution functionality."""

    # Mock database pool (replace with real connection)
    from unittest.mock import AsyncMock

    mock_pool = AsyncMock()

    # Create relay integration
    relay = RelayIntegration(mock_pool, global_id_format="uuid")

    # Test node resolution
    test_id = uuid4()
    node = await relay._ensure_context().resolve_node(test_id)

    print(f"Resolved node: {node}")

    # Test batch resolution
    test_ids = [uuid4() for _ in range(5)]
    nodes = await relay._ensure_context().resolve_nodes_batch(test_ids)

    print(f"Batch resolved {len(nodes)} nodes")


# =============================================================================
# Example 8: Migration Helper
# =============================================================================

async def migrate_existing_schema_to_relay(existing_schema, db_pool):
    """
    Helper to migrate an existing FraiseQL schema to use Relay.

    This shows how to add Relay support to an existing application.
    """

    print("Migrating schema to Relay...")

    # Enable Relay support
    relay = await enable_relay_support(
        schema=existing_schema,
        db_pool=db_pool,
        auto_register=True
    )

    # Get health status
    health = await relay.get_health_status()
    print(f"Relay extension status: {health}")

    # List discovered entities
    context = await relay._ensure_context()
    entities = await context.get_registered_entities()

    print(f"Registered entities:")
    for entity in entities:
        print(f"  - {entity['entity_name']} ({entity['graphql_type']})")

    # Test node resolution
    try:
        # Get first entity ID for testing
        if entities:
            # This would need actual data in your database
            print("Testing node resolution...")
            # test_result = await context.resolve_node(some_real_uuid)

    except Exception as e:
        print(f"Node resolution test failed: {e}")

    print("Migration completed successfully!")
    return relay


# =============================================================================
# Example Usage
# =============================================================================

if __name__ == "__main__":
    async def main():
        """Run example."""

        # Test basic functionality
        await test_node_resolution()

        # Create FastAPI app
        app = await create_fastapi_app_with_relay()
        print("FastAPI app with Relay support created!")

        # Note: In a real application, you would run the app with:
        # uvicorn main:app --reload

    # Run the example
    asyncio.run(main())
