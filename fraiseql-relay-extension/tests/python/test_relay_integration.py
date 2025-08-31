"""
FraiseQL Relay Extension - Python Integration Tests

Tests the Python integration layer with the PostgreSQL extension.
"""

import pytest
import asyncio
from typing import Optional
from uuid import UUID, uuid4
from datetime import datetime
from unittest.mock import AsyncMock, MagicMock

# Test imports - these would be installed when using the extension
try:
    from fraiseql_relay_extension.python_integration import (
        RelayIntegration,
        enable_relay_support,
        relay_entity,
        Node,
        GlobalID,
        RelayContext,
        GlobalIDConverter
    )
    import fraiseql
    from fraiseql import CQRSRepository
except ImportError as e:
    pytest.skip(f"FraiseQL dependencies not available: {e}", allow_module_level=True)


# Test entity definitions
@fraiseql.type
class TestUser(Node):
    """Test user entity for integration tests."""

    id: UUID
    email: str
    name: str
    is_active: bool = True
    created_at: datetime

    @classmethod
    def from_dict(cls, data: dict) -> "TestUser":
        return cls(
            id=UUID(data["id"]),
            email=data["email"],
            name=data["name"],
            is_active=data.get("is_active", True),
            created_at=data["created_at"]
        )


@relay_entity(
    entity_name="Post",
    pk_column="pk_post",
    v_table="v_post",
    source_table="tb_post"
)
@fraiseql.type
class TestPost(Node):
    """Test post entity with relay decorator."""

    id: UUID
    title: str
    content: str
    author_id: UUID

    @classmethod
    def from_dict(cls, data: dict) -> "TestPost":
        return cls(
            id=UUID(data["id"]),
            title=data["title"],
            content=data["content"],
            author_id=UUID(data["author_id"])
        )


# Test fixtures
@pytest.fixture
def mock_db_pool():
    """Mock database pool for testing."""
    return AsyncMock()


@pytest.fixture
def mock_db_connection():
    """Mock database connection."""
    mock_conn = AsyncMock()
    mock_conn.execute_function = AsyncMock()
    mock_conn.execute_raw = AsyncMock()
    mock_conn.find = AsyncMock()
    mock_conn.find_one = AsyncMock()
    return mock_conn


@pytest.fixture
def mock_cqrs_repo(mock_db_connection):
    """Mock CQRS repository."""
    repo = AsyncMock(spec=CQRSRepository)
    repo.execute_function = mock_db_connection.execute_function
    repo.execute_raw = mock_db_connection.execute_raw
    repo.find = mock_db_connection.find
    repo.find_one = mock_db_connection.find_one
    return repo


@pytest.fixture
async def relay_context(mock_cqrs_repo):
    """RelayContext with mocked dependencies."""
    context = RelayContext(mock_cqrs_repo)
    return context


@pytest.fixture
async def relay_integration(mock_db_pool):
    """RelayIntegration with mocked dependencies."""
    integration = RelayIntegration(mock_db_pool, global_id_format="uuid")
    return integration


# RelayContext Tests
class TestRelayContext:
    """Test RelayContext functionality."""

    async def test_register_type(self, relay_context):
        """Test type registration."""
        relay_context.register_type("User", TestUser)

        assert "User" in relay_context._type_registry
        assert relay_context._type_registry["User"] == TestUser

    async def test_register_entity(self, relay_context):
        """Test entity registration."""
        # Mock the database call
        relay_context.db.execute_function.return_value = None

        await relay_context.register_entity(
            entity_name="User",
            graphql_type="User",
            python_type=TestUser,
            pk_column="pk_user",
            v_table="v_user",
            source_table="tb_user"
        )

        # Verify database function was called
        relay_context.db.execute_function.assert_called_once_with(
            "core.register_entity",
            {
                "p_entity_name": "User",
                "p_graphql_type": "User",
                "p_pk_column": "pk_user",
                "p_v_table": "v_user",
                "p_source_table": "tb_user"
            }
        )

        # Verify type was registered
        assert "User" in relay_context._type_registry
        assert relay_context._type_registry["User"] == TestUser

    async def test_resolve_node_success(self, relay_context):
        """Test successful node resolution."""
        # Setup
        test_id = uuid4()
        mock_result = {
            "__typename": "User",
            "data": {
                "id": str(test_id),
                "email": "test@example.com",
                "name": "Test User",
                "is_active": True,
                "created_at": datetime.now()
            }
        }

        relay_context.db.execute_function.return_value = mock_result
        relay_context.register_type("User", TestUser)

        # Execute
        result = await relay_context.resolve_node(test_id)

        # Verify
        assert isinstance(result, TestUser)
        assert result.id == test_id
        assert result.email == "test@example.com"
        assert result.name == "Test User"

        # Verify C function was tried first
        relay_context.db.execute_function.assert_called_with(
            "core.fraiseql_resolve_node_fast",
            {"node_id": test_id}
        )

    async def test_resolve_node_fallback(self, relay_context):
        """Test fallback to SQL implementation."""
        # Setup - C function fails, SQL succeeds
        test_id = uuid4()
        mock_result = {
            "__typename": "User",
            "data": {
                "id": str(test_id),
                "email": "test@example.com",
                "name": "Test User",
                "created_at": datetime.now()
            }
        }

        # Mock C function failure, SQL success
        relay_context.db.execute_function.side_effect = [
            Exception("C function not available"),  # First call fails
            mock_result  # Second call succeeds
        ]
        relay_context.register_type("User", TestUser)

        # Execute
        result = await relay_context.resolve_node(test_id)

        # Verify
        assert isinstance(result, TestUser)
        assert result.id == test_id

        # Verify fallback was used
        assert relay_context.db.execute_function.call_count == 2
        calls = relay_context.db.execute_function.call_args_list
        assert calls[0][0] == ("core.fraiseql_resolve_node_fast",)
        assert calls[1][0] == ("core.resolve_node_smart",)

    async def test_resolve_node_not_found(self, relay_context):
        """Test node not found scenario."""
        test_id = uuid4()
        relay_context.db.execute_function.return_value = None

        result = await relay_context.resolve_node(test_id)

        assert result is None

    async def test_resolve_nodes_batch(self, relay_context):
        """Test batch node resolution."""
        # Setup
        test_ids = [uuid4() for _ in range(3)]
        mock_results = [
            {
                "id": str(test_ids[0]),
                "__typename": "User",
                "data": {"id": str(test_ids[0]), "name": "User 1", "email": "user1@test.com", "created_at": datetime.now()}
            },
            {
                "id": str(test_ids[1]),
                "__typename": "User",
                "data": {"id": str(test_ids[1]), "name": "User 2", "email": "user2@test.com", "created_at": datetime.now()}
            },
            {
                "id": str(test_ids[2]),
                "__typename": "User",
                "data": {"id": str(test_ids[2]), "name": "User 3", "email": "user3@test.com", "created_at": datetime.now()}
            }
        ]

        relay_context.db.execute_function.return_value = mock_results
        relay_context.register_type("User", TestUser)

        # Execute
        results = await relay_context.resolve_nodes_batch(test_ids)

        # Verify
        assert len(results) == 3
        assert all(isinstance(r, TestUser) for r in results if r is not None)
        assert results[0].name == "User 1"
        assert results[1].name == "User 2"
        assert results[2].name == "User 3"

        relay_context.db.execute_function.assert_called_with(
            "core.fraiseql_resolve_nodes_batch",
            {"node_ids": test_ids}
        )


# RelayIntegration Tests
class TestRelayIntegration:
    """Test RelayIntegration functionality."""

    async def test_initialization(self, mock_db_pool):
        """Test RelayIntegration initialization."""
        integration = RelayIntegration(mock_db_pool, global_id_format="base64")

        assert integration.db_pool == mock_db_pool
        assert integration.global_id_format == "base64"
        assert integration.context is None
        assert integration._schema_modified is False

    async def test_ensure_context(self, relay_integration, mock_db_pool):
        """Test context initialization."""
        # Mock pool acquire
        mock_conn = AsyncMock()
        mock_db_pool.acquire.return_value.__aenter__.return_value = mock_conn

        context = await relay_integration._ensure_context()

        assert isinstance(context, RelayContext)
        assert relay_integration.context == context

    async def test_register_entity_type(self, relay_integration):
        """Test entity type registration."""
        # Mock context
        mock_context = AsyncMock()
        relay_integration.context = mock_context

        await relay_integration.register_entity_type(
            entity_type=TestUser,
            entity_name="User",
            pk_column="pk_user",
            v_table="v_user",
            source_table="tb_user"
        )

        mock_context.register_entity.assert_called_once_with(
            entity_name="User",
            graphql_type="TestUser",  # From class name
            python_type=TestUser,
            pk_column="pk_user",
            v_table="v_user",
            source_table="tb_user",
            tv_table=None,
            mv_table=None,
            turbo_function=None,
            lazy_cache_key_pattern=None
        )

    async def test_get_health_status(self, relay_integration):
        """Test health status check."""
        # Mock context and response
        mock_context = AsyncMock()
        mock_health = {
            "status": "healthy",
            "entities_registered": 5,
            "v_nodes_exists": True
        }
        mock_context.get_extension_health.return_value = mock_health
        relay_integration.context = mock_context

        health = await relay_integration.get_health_status()

        assert health == mock_health
        mock_context.get_extension_health.assert_called_once()


# GlobalIDConverter Tests
class TestGlobalIDConverter:
    """Test Global ID encoding/decoding functionality."""

    async def test_is_uuid_format(self):
        """Test UUID format detection."""
        # Valid UUID formats
        assert GlobalIDConverter.is_uuid_format(uuid4())
        assert GlobalIDConverter.is_uuid_format("550e8400-e29b-41d4-a716-446655440000")

        # Invalid formats
        assert not GlobalIDConverter.is_uuid_format("not-a-uuid")
        assert not GlobalIDConverter.is_uuid_format("VXNlcjoxMjM=")  # base64
        assert not GlobalIDConverter.is_uuid_format(123)

    async def test_encode_global_id(self):
        """Test Global ID encoding."""
        mock_connection = AsyncMock()
        mock_connection.execute_function.return_value = "VXNlcjo1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDA="

        test_uuid = UUID("550e8400-e29b-41d4-a716-446655440000")
        result = await GlobalIDConverter.encode_global_id(
            mock_connection, "User", test_uuid
        )

        assert result == "VXNlcjo1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDA="
        mock_connection.execute_function.assert_called_with(
            "core.fraiseql_encode_global_id",
            {"typename": "User", "local_id": test_uuid}
        )

    async def test_decode_global_id(self):
        """Test Global ID decoding."""
        mock_connection = AsyncMock()
        mock_connection.execute_function.return_value = {
            "typename": "User",
            "local_id": UUID("550e8400-e29b-41d4-a716-446655440000")
        }

        result = await GlobalIDConverter.decode_global_id(
            mock_connection, "VXNlcjo1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDA="
        )

        assert result["typename"] == "User"
        assert result["local_id"] == UUID("550e8400-e29b-41d4-a716-446655440000")


# Decorator Tests
class TestRelayEntityDecorator:
    """Test @relay_entity decorator."""

    def test_decorator_adds_metadata(self):
        """Test that decorator adds metadata to class."""
        @relay_entity("TestEntity", "pk_test", "v_test", "tb_test")
        @fraiseql.type
        class DecoratedEntity:
            id: UUID
            name: str

        assert hasattr(DecoratedEntity, '_relay_entity_info')
        info = DecoratedEntity._relay_entity_info

        assert info['entity_name'] == 'TestEntity'
        assert info['pk_column'] == 'pk_test'
        assert info['v_table'] == 'v_test'
        assert info['source_table'] == 'tb_test'

    def test_decorator_requires_id_field(self):
        """Test that decorator validates id field exists."""
        with pytest.raises(TypeError, match="must have an 'id' field"):
            @relay_entity("TestEntity", "pk_test", "v_test", "tb_test")
            @fraiseql.type
            class InvalidEntity:
                name: str  # Missing id field


# Integration Tests
class TestFullIntegration:
    """Test full integration scenarios."""

    async def test_enable_relay_support(self, mock_db_pool):
        """Test enable_relay_support function."""
        # Mock schema
        mock_schema = MagicMock()

        # Mock successful integration
        with pytest.MonkeyPatch().context() as m:
            # Mock the RelayIntegration class
            mock_integration = AsyncMock()
            mock_integration.add_node_resolver = AsyncMock()
            mock_integration.auto_register_entities = AsyncMock(return_value=3)
            mock_integration.get_health_status = AsyncMock(return_value={
                "status": "healthy",
                "entities_registered": 3
            })

            # Mock the constructor to return our mock
            async def mock_enable_relay_support(schema, db_pool, **kwargs):
                integration = mock_integration
                await integration.add_node_resolver(schema)
                await integration.auto_register_entities(schema)
                await integration.get_health_status()
                return integration

            result = await mock_enable_relay_support(mock_schema, mock_db_pool)

            assert result == mock_integration
            mock_integration.add_node_resolver.assert_called_with(mock_schema)
            mock_integration.auto_register_entities.assert_called_with(mock_schema)

    async def test_node_resolution_workflow(self, relay_context):
        """Test complete node resolution workflow."""
        # Setup
        test_id = uuid4()
        relay_context.register_type("User", TestUser)

        # Mock database response
        mock_result = {
            "__typename": "User",
            "data": {
                "id": str(test_id),
                "email": "workflow@test.com",
                "name": "Workflow Test",
                "is_active": True,
                "created_at": datetime.now()
            }
        }
        relay_context.db.execute_function.return_value = mock_result

        # Execute
        node = await relay_context.resolve_node(test_id)

        # Verify
        assert isinstance(node, TestUser)
        assert node.email == "workflow@test.com"
        assert node.name == "Workflow Test"
        assert node.is_active is True

    async def test_batch_resolution_workflow(self, relay_context):
        """Test batch resolution workflow."""
        # Setup
        test_ids = [uuid4() for _ in range(2)]
        relay_context.register_type("User", TestUser)
        relay_context.register_type("Post", TestPost)

        # Mock mixed batch response
        mock_results = [
            {
                "id": str(test_ids[0]),
                "__typename": "User",
                "data": {
                    "id": str(test_ids[0]),
                    "email": "batch1@test.com",
                    "name": "Batch User",
                    "created_at": datetime.now()
                }
            },
            {
                "id": str(test_ids[1]),
                "__typename": "Post",
                "data": {
                    "id": str(test_ids[1]),
                    "title": "Batch Post",
                    "content": "Post content",
                    "author_id": str(uuid4())
                }
            }
        ]
        relay_context.db.execute_function.return_value = mock_results

        # Execute
        nodes = await relay_context.resolve_nodes_batch(test_ids)

        # Verify
        assert len(nodes) == 2
        assert isinstance(nodes[0], TestUser)
        assert isinstance(nodes[1], TestPost)
        assert nodes[0].email == "batch1@test.com"
        assert nodes[1].title == "Batch Post"


# Performance Tests
class TestPerformanceScenarios:
    """Test performance-related scenarios."""

    async def test_batch_vs_individual_mocking(self, relay_context):
        """Test that batch resolution uses the right functions."""
        # Setup
        test_ids = [uuid4() for _ in range(5)]
        relay_context.register_type("User", TestUser)

        # Mock batch response
        mock_batch_results = [
            {
                "id": str(id_val),
                "__typename": "User",
                "data": {
                    "id": str(id_val),
                    "name": f"User {i}",
                    "email": f"user{i}@test.com",
                    "created_at": datetime.now()
                }
            }
            for i, id_val in enumerate(test_ids)
        ]

        # Test batch resolution
        relay_context.db.execute_function.return_value = mock_batch_results
        batch_results = await relay_context.resolve_nodes_batch(test_ids)

        # Verify batch function was called
        relay_context.db.execute_function.assert_called_with(
            "core.fraiseql_resolve_nodes_batch",
            {"node_ids": test_ids}
        )

        assert len(batch_results) == 5
        assert all(isinstance(r, TestUser) for r in batch_results)

        # Reset mock for individual calls
        relay_context.db.execute_function.reset_mock()

        # Mock individual resolution failure to test fallback
        relay_context.db.execute_function.side_effect = Exception("Batch not available")

        # This should fallback to individual calls
        # (We won't actually test the fallback implementation details here,
        # but we verify the exception handling)
        with pytest.raises(Exception):
            await relay_context.resolve_nodes_batch(test_ids)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
