"""Comprehensive tests for db module to improve coverage."""

import os
from dataclasses import dataclass
from typing import Optional
from unittest.mock import patch
from uuid import UUID, uuid4

import pytest
from psycopg.sql import SQL

# Import database fixtures for this database test
from tests.database_conftest import *  # noqa: F403

from fraiseql.db import (
    DatabaseQuery,
    FraiseQLRepository,
    _type_registry,
    register_type_for_view,
)
from fraiseql.types import fraise_type


@fraise_type
@dataclass
class SimpleType:
    """Simple type for testing."""

    id: UUID
    name: str
    is_active: bool = True


@fraise_type
@dataclass
class NestedType:
    """Type with nested object."""

    id: UUID
    title: str
    author: SimpleType


@fraise_type
@dataclass
class TypeWithOptional:
    """Type with optional fields."""

    id: UUID
    name: str
    description: Optional[str] = None
    nested: Optional[SimpleType] = None


@fraise_type
@dataclass
class TypeWithList:
    """Type with list fields."""

    id: UUID
    tags: list[str]
    items: list[SimpleType]


@pytest.fixture
async def test_tables(db_connection):
    """Create test tables for the entire test suite."""
    # Create users table with JSONB data column
    await db_connection.execute("""
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT,
            data JSONB NOT NULL DEFAULT '{}'::jsonb,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    """)
    
    # Create posts table for nested type testing
    await db_connection.execute("""
        CREATE TABLE IF NOT EXISTS posts (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            title TEXT,
            author_id UUID REFERENCES users(id),
            data JSONB NOT NULL DEFAULT '{}'::jsonb
        )
    """)
    
    # Create views for find/find_one testing
    await db_connection.execute("""
        CREATE OR REPLACE VIEW users_view AS
        SELECT id, name, data FROM users
    """)
    
    await db_connection.execute("""
        CREATE OR REPLACE VIEW posts_view AS
        SELECT p.id, p.title, p.data,
               u.data as author_data
        FROM posts p
        LEFT JOIN users u ON p.author_id = u.id
    """)
    
    # Create test function
    await db_connection.execute("""
        CREATE OR REPLACE FUNCTION test_function(param jsonb)
        RETURNS jsonb AS $$
        BEGIN
            RETURN jsonb_build_object('id', '123', 'status', 'success', 'param', param);
        END;
        $$ LANGUAGE plpgsql;
    """)
    
    # Create void function
    await db_connection.execute("""
        CREATE OR REPLACE FUNCTION void_function(param jsonb)
        RETURNS void AS $$
        BEGIN
            -- Do nothing
        END;
        $$ LANGUAGE plpgsql;
    """)


@pytest.fixture
async def repository(db_pool):
    """Create a repository instance with real pool."""
    return FraiseQLRepository(db_pool)


@pytest.mark.database
class TestDatabaseQuery:
    """Test DatabaseQuery dataclass."""

    def test_database_query_creation(self):
        """Test creating a DatabaseQuery instance."""
        statement = SQL("SELECT * FROM users")
        params = {"id": 123}

        query = DatabaseQuery(statement=statement, params=params)

        assert query.statement == statement
        assert query.params == params
        assert query.fetch_result is True

    def test_database_query_no_fetch(self):
        """Test creating a DatabaseQuery with fetch_result=False."""
        statement = SQL("INSERT INTO users (name) VALUES (%s)")
        params = {"name": "Test"}

        query = DatabaseQuery(statement=statement, params=params, fetch_result=False)

        assert query.fetch_result is False


class TestTypeRegistry:
    """Test type registry functions."""

    def test_register_type_for_view(self):
        """Test registering a type for a view."""
        # Clear registry first
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)

        assert "user_view" in _type_registry
        assert _type_registry["user_view"] == SimpleType

    def test_register_multiple_types(self):
        """Test registering multiple types."""
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)
        register_type_for_view("post_view", NestedType)

        assert len(_type_registry) == 2
        assert _type_registry["user_view"] == SimpleType
        assert _type_registry["post_view"] == NestedType

    def test_override_existing_registration(self):
        """Test overriding an existing type registration."""
        _type_registry.clear()

        register_type_for_view("user_view", SimpleType)
        register_type_for_view("user_view", NestedType)

        assert _type_registry["user_view"] == NestedType


@pytest.mark.database
class TestFraiseQLRepository:
    """Test FraiseQLRepository class."""

    async def test_repository_initialization(self, db_pool):
        """Test repository initialization."""
        context = {"user": "test", "tenant_id": 123}
        repo = FraiseQLRepository(db_pool, context)

        assert repo._pool == db_pool
        assert repo.context == context

    async def test_repository_initialization_no_context(self, db_pool):
        """Test repository initialization without context."""
        repo = FraiseQLRepository(db_pool)

        assert repo.context == {}

    async def test_run_query_with_results(self, repository, db_connection, test_tables):
        """Test running a query that returns results."""
        # Insert test data
        user1_id = uuid4()
        user2_id = uuid4()
        await db_connection.execute(
            "INSERT INTO users (id, name, data) VALUES (%s, %s, %s), (%s, %s, %s)",
            (
                user1_id, "User 1", '{"age": 25}'::JSONB,
                user2_id, "User 2", '{"age": 30}'::JSONB
            )
        )
        
        # Create query
        query = DatabaseQuery(
            statement=SQL("SELECT id, name FROM users ORDER BY name"),
            params={},
        )
        
        # Execute
        results = await repository.run(query)
        
        # Verify
        assert len(results) == 2
        assert results[0]["name"] == "User 1"
        assert results[1]["name"] == "User 2"

    async def test_run_query_without_fetch(self, repository, db_connection, test_tables):
        """Test running a query without fetching results."""
        # Create insert query
        query = DatabaseQuery(
            statement=SQL("INSERT INTO users (name, data) VALUES (%(name)s, %(data)s)"),
            params={"name": "Test User", "data": '{"test": true}'::JSONB},
            fetch_result=False,
        )
        
        # Execute
        results = await repository.run(query)
        
        # Verify no results returned
        assert results == []
        
        # Verify data was inserted
        cursor = await db_connection.execute("SELECT name FROM users WHERE name = %s", ("Test User",))
        row = await cursor.fetchone()
        assert row is not None
        assert row[0] == "Test User"

    async def test_run_query_with_exception(self, repository):
        """Test running a query that raises an exception."""
        # Create query with invalid table
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM invalid_table_that_does_not_exist"),
            params={},
        )
        
        # Execute and expect exception
        with pytest.raises(Exception) as exc_info:
            await repository.run(query)
        
        # Verify it's a database error
        assert "relation" in str(exc_info.value).lower()
        assert "does not exist" in str(exc_info.value).lower()

    async def test_run_in_transaction_success(self, repository, db_connection, test_tables):
        """Test running a function in a transaction successfully."""
        # Define test function that inserts data
        async def test_func(conn):
            await conn.execute(
                "INSERT INTO users (name, data) VALUES (%s, %s)",
                ("Transaction User", '{"transactional": true}'::JSONB)
            )
            # Verify we can query within transaction
            cursor = await conn.execute("SELECT COUNT(*) FROM users WHERE name = %s", ("Transaction User",))
            row = await cursor.fetchone()
            return row[0]

        # Execute
        result = await repository.run_in_transaction(test_func)

        # Verify function returned expected value
        assert result == 1
        
        # Verify data was committed (check in new transaction)
        cursor = await db_connection.execute("SELECT COUNT(*) FROM users WHERE name = %s", ("Transaction User",))
        row = await cursor.fetchone()
        assert row[0] == 1

    async def test_run_in_transaction_rollback(self, repository, db_connection, test_tables):
        """Test transaction rollback on error."""
        # Define test function that raises error after insert
        async def test_func(conn):
            await conn.execute(
                "INSERT INTO users (name, data) VALUES (%s, %s)",
                ("Rollback User", '{"should_rollback": true}'::JSONB)
            )
            raise ValueError("Test error")

        # Execute and expect exception
        with pytest.raises(ValueError, match="Test error"):
            await repository.run_in_transaction(test_func)

        # Verify data was rolled back
        cursor = await db_connection.execute("SELECT COUNT(*) FROM users WHERE name = %s", ("Rollback User",))
        row = await cursor.fetchone()
        assert row[0] == 0  # Should not exist due to rollback

    async def test_execute_function(self, repository, test_tables):
        """Test executing a PostgreSQL function."""
        # Execute function with parameters
        result = await repository.execute_function("test_function", {"test_key": "test_value"})
        
        # Verify result
        assert result["id"] == "123"
        assert result["status"] == "success"
        assert result["param"]["test_key"] == "test_value"

    async def test_execute_function_no_result(self, repository, test_tables):
        """Test executing a function that returns no result."""
        # Execute void function
        result = await repository.execute_function("void_function", {"ignored": "param"})
        
        # Verify empty result
        assert result == {}

    async def test_find_with_simple_data(self, repository, db_connection, test_tables):
        """Test find method with simple data."""
        # Insert test data
        user1_id = uuid4()
        user2_id = uuid4()
        await db_connection.execute(
            """INSERT INTO users (id, name, data) VALUES 
               (%s, %s, %s), 
               (%s, %s, %s)""",
            (
                user1_id, "Alice", '{"id": "' + str(user1_id) + '", "age": 25}'::JSONB,
                user2_id, "Bob", '{"id": "' + str(user2_id) + '", "age": 30}'::JSONB
            )
        )
        
        # Execute find
        results = await repository.find("users_view")
        
        # Verify - in production mode, returns raw dicts
        assert len(results) == 2
        # Results should contain the JSONB data
        names = [r["name"] for r in results]
        assert "Alice" in names
        assert "Bob" in names

    async def test_find_one(self, repository, db_connection, test_tables):
        """Test find_one method."""
        # Insert test data
        user_id = uuid4()
        await db_connection.execute(
            "INSERT INTO users (id, name, data) VALUES (%s, %s, %s)",
            (user_id, "Single User", '{"id": "' + str(user_id) + '", "email": "single@example.com"}'::JSONB)
        )
        
        # Execute find_one
        result = await repository.find_one("users_view", id=user_id)
        
        # Verify
        assert result is not None
        assert result["name"] == "Single User"
        assert result["data"]["email"] == "single@example.com"

    async def test_find_one_no_result(self, repository, test_tables):
        """Test find_one when no result found."""
        # Execute find_one with non-existent ID
        result = await repository.find_one("users_view", id=uuid4())
        
        # Verify
        assert result is None


@pytest.mark.database
class TestRepositoryModes:
    """Test repository mode handling."""

    async def test_development_mode(self, db_pool):
        """Test repository in development mode."""
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "development"

    async def test_production_mode(self, db_pool):
        """Test repository in production mode (default)."""
        with patch.dict(os.environ, {}, clear=True):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "production"

    async def test_mode_from_context(self, db_pool):
        """Test mode determination from context."""
        context = {"mode": "development"}
        repo = FraiseQLRepository(db_pool, context)
        assert repo.mode == "development"


@pytest.mark.database  
class TestRepositoryHelpers:
    """Test repository helper methods."""

    async def test_to_snake_case_table_name(self, repository):
        """Test table name conversion to snake case."""
        # This would be used internally when auto_camel_case is enabled
        from fraiseql.utils.casing import to_snake_case

        assert to_snake_case("UserProfile") == "user_profile"
        assert to_snake_case("APIKey") == "api_key"
        assert to_snake_case("HTTPSConnection") == "https_connection"
