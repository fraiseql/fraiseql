"""Comprehensive tests for CQRS repository module to improve coverage."""

from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import UUID, uuid4

import pytest
from psycopg import AsyncConnection
from psycopg.rows import dict_row
from psycopg.sql import SQL, Composed, Identifier

from fraiseql.cqrs.executor import CQRSExecutor
from fraiseql.cqrs.repository import CQRSRepository


@pytest.fixture
def mock_connection():
    """Create a mock async database connection."""
    conn = AsyncMock(spec=AsyncConnection)
    conn.execute = AsyncMock()
    conn.fetchone = AsyncMock()
    conn.fetchall = AsyncMock()
    return conn


@pytest.fixture
def mock_executor():
    """Create a mock CQRS executor."""
    executor = AsyncMock(spec=CQRSExecutor)
    return executor


@pytest.fixture
async def repository(mock_connection):
    """Create a repository instance with mock connection."""
    return CQRSRepository(mock_connection)


class TestCQRSRepositoryCommands:
    """Test command methods (write operations)."""

    async def test_create_entity(self, repository, mock_connection):
        """Test creating an entity via SQL function."""
        # Mock the executor
        test_id = uuid4()
        expected_result = {
            "id": test_id,
            "name": "Test User",
            "email": "test@example.com"
        }
        
        with patch.object(repository.executor, 'execute_function', return_value=expected_result) as mock_exec:
            result = await repository.create("user", {
                "name": "Test User",
                "email": "test@example.com"
            })
            
            mock_exec.assert_called_once_with(
                "fn_create_user",
                {"name": "Test User", "email": "test@example.com"}
            )
            assert result == expected_result

    async def test_update_entity(self, repository):
        """Test updating an entity via SQL function."""
        test_id = uuid4()
        update_data = {
            "id": test_id,
            "name": "Updated User"
        }
        expected_result = {
            "id": test_id,
            "name": "Updated User",
            "email": "test@example.com"
        }
        
        with patch.object(repository.executor, 'execute_function', return_value=expected_result) as mock_exec:
            result = await repository.update("user", update_data)
            
            mock_exec.assert_called_once_with("fn_update_user", update_data)
            assert result == expected_result

    async def test_delete_entity(self, repository):
        """Test deleting an entity via SQL function."""
        test_id = uuid4()
        expected_result = {"id": test_id, "deleted": True}
        
        with patch.object(repository.executor, 'execute_function', return_value=expected_result) as mock_exec:
            result = await repository.delete("user", test_id)
            
            mock_exec.assert_called_once_with("fn_delete_user", {"id": test_id})
            assert result == expected_result

    async def test_execute_custom_function(self, repository):
        """Test executing a custom SQL function."""
        function_result = {"status": "success", "count": 5}
        
        with patch.object(repository.executor, 'execute_function', return_value=function_result) as mock_exec:
            result = await repository.execute_function("custom_function", {"param": "value"})
            
            mock_exec.assert_called_once_with("custom_function", {"param": "value"})
            assert result == function_result


class TestCQRSRepositoryQueries:
    """Test query methods (read operations)."""

    async def test_find_by_id(self, repository):
        """Test finding entity by ID."""
        test_id = uuid4()
        expected_data = {
            "data": {
                "id": str(test_id),
                "name": "Test User",
                "email": "test@example.com"
            }
        }
        
        with patch.object(repository.executor, 'execute_query', return_value=[expected_data]) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: UUID
                name: str
                email: str
            
            result = await repository.find_by_id(User, test_id)
            
            # Should query the appropriate view
            mock_exec.assert_called_once()
            call_args = mock_exec.call_args[0]
            assert "vw_user" in str(call_args[0])  # View name
            assert str(test_id) in str(call_args[1])  # ID in where clause

    async def test_find_by_id_not_found(self, repository):
        """Test finding entity by ID when not found."""
        test_id = uuid4()
        
        with patch.object(repository.executor, 'execute_query', return_value=[]) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: UUID
                name: str
            
            result = await repository.find_by_id(User, test_id)
            assert result is None

    async def test_list_entities(self, repository):
        """Test listing entities with pagination."""
        expected_data = [
            {"data": {"id": "1", "name": "User 1"}},
            {"data": {"id": "2", "name": "User 2"}}
        ]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_data) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
                name: str
            
            results = await repository.list(User, limit=10, offset=0)
            
            mock_exec.assert_called_once()
            call_args = mock_exec.call_args[0]
            query_str = str(call_args[0])
            
            assert "vw_user" in query_str
            assert "LIMIT" in query_str
            assert "OFFSET" in query_str

    async def test_list_with_filtering(self, repository):
        """Test listing entities with where clause."""
        expected_data = [{"data": {"id": "1", "name": "Active User"}}]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_data) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
                name: str
                status: str
            
            results = await repository.list(
                User,
                where={"status": {"eq": "active"}},
                limit=10
            )
            
            mock_exec.assert_called_once()
            # Where clause should be applied

    async def test_list_with_ordering(self, repository):
        """Test listing entities with order by."""
        expected_data = []
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_data) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
                created_at: str
            
            results = await repository.list(
                User,
                order_by=[("created_at", "DESC")],
                limit=10
            )
            
            mock_exec.assert_called_once()
            call_args = mock_exec.call_args[0]
            query_str = str(call_args[0])
            
            assert "ORDER BY" in query_str

    async def test_find_by_view(self, repository):
        """Test finding by custom view."""
        expected_data = [
            {"data": {"id": "1", "email": "user@example.com"}}
        ]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_data) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
                email: str
            
            results = await repository.find_by_view(
                "vw_active_users",
                User,
                where={"email": {"like": "%@example.com"}},
                limit=5
            )
            
            mock_exec.assert_called_once()
            call_args = mock_exec.call_args[0]
            assert "vw_active_users" in str(call_args[0])

    async def test_execute_raw_query(self, repository):
        """Test executing raw SQL query."""
        expected_data = [
            {"count": 10, "status": "active"}
        ]
        
        query = SQL("SELECT COUNT(*) as count, status FROM users GROUP BY status")
        params = None
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_data) as mock_exec:
            results = await repository.execute_query(query, params)
            
            mock_exec.assert_called_once_with(query, params)
            assert results == expected_data


class TestCQRSRepositoryRelationships:
    """Test relationship loading methods."""

    async def test_load_one_to_many(self, repository):
        """Test loading one-to-many relationship."""
        parent = {"id": "1", "name": "Parent"}
        expected_children = [
            {"data": {"id": "10", "parent_id": "1", "name": "Child 1"}},
            {"data": {"id": "11", "parent_id": "1", "name": "Child 2"}}
        ]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_children) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class Child:
                id: str
                parent_id: str
                name: str
            
            result = await repository.load_one_to_many(
                parent,
                "children",
                Child,
                "parent_id"
            )
            
            assert result["children"] == [
                {"id": "10", "parent_id": "1", "name": "Child 1"},
                {"id": "11", "parent_id": "1", "name": "Child 2"}
            ]

    async def test_load_many_to_many(self, repository):
        """Test loading many-to-many relationship."""
        entity = {"id": "1", "name": "Entity"}
        expected_related = [
            {"data": {"id": "20", "name": "Related 1"}},
            {"data": {"id": "21", "name": "Related 2"}}
        ]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_related) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class RelatedEntity:
                id: str
                name: str
            
            result = await repository.load_many_to_many(
                entity,
                "related_items",
                RelatedEntity,
                "entity_related_mapping",
                "entity_id",
                "related_id"
            )
            
            assert len(result["related_items"]) == 2


class TestCQRSRepositoryBatchOperations:
    """Test batch operations."""

    async def test_batch_create(self, repository):
        """Test batch creating entities."""
        inputs = [
            {"name": "User 1", "email": "user1@example.com"},
            {"name": "User 2", "email": "user2@example.com"}
        ]
        expected_results = [
            {"id": "1", "name": "User 1", "email": "user1@example.com"},
            {"id": "2", "name": "User 2", "email": "user2@example.com"}
        ]
        
        with patch.object(repository.executor, 'execute_function') as mock_exec:
            mock_exec.side_effect = expected_results
            
            results = await repository.batch_create("user", inputs)
            
            assert len(results) == 2
            assert mock_exec.call_count == 2

    async def test_batch_update(self, repository):
        """Test batch updating entities."""
        updates = [
            {"id": "1", "name": "Updated 1"},
            {"id": "2", "name": "Updated 2"}
        ]
        expected_results = [
            {"id": "1", "name": "Updated 1", "email": "user1@example.com"},
            {"id": "2", "name": "Updated 2", "email": "user2@example.com"}
        ]
        
        with patch.object(repository.executor, 'execute_function') as mock_exec:
            mock_exec.side_effect = expected_results
            
            results = await repository.batch_update("user", updates)
            
            assert len(results) == 2
            assert mock_exec.call_count == 2

    async def test_batch_delete(self, repository):
        """Test batch deleting entities."""
        ids = [uuid4(), uuid4(), uuid4()]
        expected_results = [
            {"id": str(ids[0]), "deleted": True},
            {"id": str(ids[1]), "deleted": True},
            {"id": str(ids[2]), "deleted": True}
        ]
        
        with patch.object(repository.executor, 'execute_function') as mock_exec:
            mock_exec.side_effect = expected_results
            
            results = await repository.batch_delete("user", ids)
            
            assert len(results) == 3
            assert all(r["deleted"] for r in results)


class TestCQRSRepositoryTransactions:
    """Test transaction handling."""

    async def test_with_transaction(self, mock_connection):
        """Test executing operations within a transaction."""
        # Mock transaction context
        transaction = AsyncMock()
        mock_connection.transaction.return_value.__aenter__.return_value = transaction
        
        repository = CQRSRepository(mock_connection)
        
        async with repository.transaction():
            # Perform operations
            with patch.object(repository.executor, 'execute_function') as mock_exec:
                await repository.create("user", {"name": "Test"})
                mock_exec.assert_called_once()
        
        # Transaction should be used
        mock_connection.transaction.assert_called_once()


class TestCQRSRepositoryUtilities:
    """Test utility methods."""

    def test_get_view_name(self):
        """Test view name generation from entity type."""
        from fraiseql.types import fraise_type
        
        @fraise_type
        class UserProfile:
            id: str
        
        repo = CQRSRepository(MagicMock())
        view_name = repo._get_view_name(UserProfile)
        assert view_name == "vw_user_profile"

    def test_get_function_name(self):
        """Test function name generation."""
        repo = CQRSRepository(MagicMock())
        
        assert repo._get_function_name("create", "user") == "fn_create_user"
        assert repo._get_function_name("update", "user_profile") == "fn_update_user_profile"

    async def test_count_entities(self, repository):
        """Test counting entities."""
        expected_count = [{"count": 42}]
        
        with patch.object(repository.executor, 'execute_query', return_value=expected_count) as mock_exec:
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
            
            count = await repository.count(User, where={"is_active": {"eq": True}})
            
            assert count == 42
            # Should use COUNT(*) query
            call_args = mock_exec.call_args[0]
            assert "COUNT(*)" in str(call_args[0])

    async def test_exists(self, repository):
        """Test checking entity existence."""
        test_id = uuid4()
        
        # Entity exists
        with patch.object(repository.executor, 'execute_query', return_value=[{"exists": True}]):
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: UUID
            
            exists = await repository.exists(User, test_id)
            assert exists is True
        
        # Entity doesn't exist
        with patch.object(repository.executor, 'execute_query', return_value=[{"exists": False}]):
            exists = await repository.exists(User, test_id)
            assert exists is False


class TestCQRSRepositoryErrorHandling:
    """Test error handling in repository."""

    async def test_handle_missing_function(self, repository):
        """Test handling when SQL function doesn't exist."""
        with patch.object(repository.executor, 'execute_function') as mock_exec:
            mock_exec.side_effect = Exception("function fn_create_invalid does not exist")
            
            with pytest.raises(Exception, match="function.*does not exist"):
                await repository.create("invalid", {"data": "test"})

    async def test_handle_query_error(self, repository):
        """Test handling query execution errors."""
        with patch.object(repository.executor, 'execute_query') as mock_exec:
            mock_exec.side_effect = Exception("relation does not exist")
            
            from fraiseql.types import fraise_type
            
            @fraise_type
            class User:
                id: str
            
            with pytest.raises(Exception, match="relation does not exist"):
                await repository.list(User)