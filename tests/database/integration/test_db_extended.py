"""Extended tests for database functionality to improve coverage."""

import os
from unittest.mock import patch

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.db import DatabaseQuery, FraiseQLRepository, _type_registry, register_type_for_view

# Import database fixtures for this database test
from tests.database_conftest import *  # noqa: F403


@pytest.mark.database
class TestDatabaseQuery:
    """Test DatabaseQuery dataclass."""

    def test_database_query_creation(self):
        """Test creating DatabaseQuery instances."""
        # With SQL object
        sql = (SQL("SELECT * FROM users"),)
        params = {"id": 123}

        query = DatabaseQuery(sql, params)
        assert query.statement is sql
        assert query.params == params
        assert query.fetch_result is True  # Default

    def test_database_query_no_fetch(self):
        """Test DatabaseQuery with fetch_result=False."""
        composed = Composed([SQL("INSERT INTO users"), SQL(" VALUES "), SQL("($1)")])
        params = {"name": "test"}

        query = DatabaseQuery(composed, params, fetch_result=False)
        assert query.statement is composed
        assert query.params == params
        assert query.fetch_result is False

    def test_database_query_empty_params(self):
        """Test DatabaseQuery with empty parameters."""
        sql = (SQL("SELECT 1"),)
        query = DatabaseQuery(sql, {})

        assert query.statement is sql
        assert query.params == {}
        assert query.fetch_result is True


@pytest.mark.database
class TestTypeRegistry:
    """Test type registration functionality."""

    def setup_method(self):
        """Clear registry before each test."""
        _type_registry.clear()

    def test_register_type_for_view(self):
        """Test registering a type for a view."""

        class User:
            pass

        register_type_for_view("user_view", User)
        assert _type_registry["user_view"] is User

    def test_register_multiple_types(self):
        """Test registering multiple types."""

        class User:
            pass

        class Post:
            pass

        register_type_for_view("users", User)
        register_type_for_view("posts", Post)

        assert _type_registry["users"] is User
        assert _type_registry["posts"] is Post

    def test_register_type_overwrite(self):
        """Test overwriting a registered type."""

        class OldUser:
            pass

        class NewUser:
            pass

        register_type_for_view("users", OldUser)
        register_type_for_view("users", NewUser)

        assert _type_registry["users"] is NewUser


@pytest.mark.database
class TestFraiseQLRepository:
    """Test FraiseQLRepository functionality."""

    @pytest.fixture
    async def test_tables(self, db_pool):
        """Create test tables for repository tests."""
        async with db_pool.connection() as conn:
            # Drop existing tables to ensure clean state
            await conn.execute("DROP TABLE IF EXISTS test_posts CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_users CASCADE")

            await conn.execute(
                """
                CREATE TABLE test_users (
                    id SERIAL PRIMARY KEY,
                    name TEXT,
                    email TEXT,
                    data JSONB DEFAULT '{}'
                )
            """
            )

            await conn.execute(
                """
                CREATE TABLE test_posts (
                    id SERIAL PRIMARY KEY,
                    user_id INTEGER REFERENCES test_users(id),
                    title TEXT,
                    data JSONB DEFAULT '{}'
                )
            """
            )

            await conn.commit()

        yield "test_tables_created"

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP TABLE IF EXISTS test_posts CASCADE")
            await conn.execute("DROP TABLE IF EXISTS test_users CASCADE")
            await conn.commit()

    @pytest.fixture
    def repository(self, db_pool):
        """Create a repository with database pool."""
        return FraiseQLRepository(db_pool)

    def test_repository_initialization(self, db_pool):
        """Test repository initialization."""
        repo = FraiseQLRepository(db_pool)
        assert repo._pool is db_pool
        assert repo.context == {}
        assert repo.mode in ["development", "production"]

    def test_repository_with_context(self, db_pool):
        """Test repository initialization with context."""
        context = {"user_id": 123, "tenant": "test"}
        repo = FraiseQLRepository(db_pool, context)

        assert repo._pool is db_pool
        assert repo.context == context

    @pytest.mark.asyncio
    async def test_run_query_with_results(self, repository, db_pool, test_tables):
        """Test running a query that returns results."""
        # Insert test data
        async with db_pool.connection() as conn:
            await conn.execute(
                """INSERT INTO test_users (name, email) VALUES (%s, %s), (%s, %s)""",
                ("Alice", "alice@example.com", "Bob", "bob@example.com"),
            )
            await conn.commit()

        # Create and run query
        query = DatabaseQuery(
            SQL("SELECT id, name FROM test_users ORDER BY id"), {}, fetch_result=True
        )

        result = await repository.run(query)

        assert len(result) == 2
        assert result[0]["name"] == "Alice"
        assert result[1]["name"] == "Bob"

    @pytest.mark.asyncio
    async def test_run_query_no_results(self, repository, db_pool, test_tables):
        """Test running a query that doesn't fetch results."""
        # Create and run insert query
        query = DatabaseQuery(
            SQL("INSERT INTO test_users (name, email) VALUES (%(name)s, %(email)s)"),
            {"name": "Charlie", "email": "charlie@example.com"},
            fetch_result=False,
        )

        result = await repository.run(query)

        assert result == []

        # Verify data was inserted
        async with db_pool.connection() as conn:
            cursor = await conn.execute("SELECT name FROM test_users WHERE name = %s", ("Charlie",))
            row = await cursor.fetchone()
            assert row[0] == "Charlie"

    @pytest.mark.asyncio
    async def test_run_query_exception(self, repository):
        """Test handling exceptions during query execution."""
        # Create query with invalid SQL
        query = DatabaseQuery(SQL("SELECT * FROM non_existent_table"), {})

        with pytest.raises(Exception) as exc_info:
            await repository.run(query)

        assert "relation" in str(exc_info.value).lower()
        assert "does not exist" in str(exc_info.value).lower()

    @pytest.mark.asyncio
    async def test_run_in_transaction_success(self, repository, test_tables):
        """Test successful transaction execution."""

        # Function to run in transaction
        async def test_func(conn, value):
            await conn.execute(
                """INSERT INTO test_users (name, email) VALUES (%s, %s)""",
                (f"User {value}", f"user{value}@example.com"),
            )
            cursor = await conn.execute("SELECT COUNT(*) FROM test_users")
            row = await cursor.fetchone()
            return row[0]

        result = await repository.run_in_transaction(test_func, 42)
        assert result >= 1  # At least one row inserted

    @pytest.mark.asyncio
    async def test_run_in_transaction_with_kwargs(self, repository, test_tables):
        """Test transaction with keyword arguments."""

        async def test_func(conn, a, b=None):
            # Simple function that uses kwargs
            await conn.execute("SELECT 1")  # Just to use the connection
            return f"a={a}, b={b}"

        result = await repository.run_in_transaction(test_func, 1, b=2)
        assert result == "a=1, b=2"

    @pytest.mark.asyncio
    async def test_run_in_transaction_exception(self, repository, db_pool, test_tables):
        """Test transaction rollback on exception."""

        async def failing_func(conn):
            await conn.execute("INSERT INTO test_users (name) VALUES (%s)", ("Should Rollback",))
            raise ValueError("Transaction failed")

        with pytest.raises(ValueError, match="Transaction failed"):
            await repository.run_in_transaction(failing_func)

        # Verify rollback occurred
        async with db_pool.connection() as conn:
            cursor = await conn.execute(
                "SELECT COUNT(*) FROM test_users WHERE name = %s", ("Should Rollback",)
            )
            row = await cursor.fetchone()
            assert row[0] == 0

    def test_determine_mode_development(self, db_pool):
        """Test mode determination in development."""
        with patch.dict(os.environ, {"FRAISEQL_ENV": "development"}):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "development"

    def test_determine_mode_production(self, db_pool):
        """Test mode determination in production."""
        with patch.dict(os.environ, {"FRAISEQL_ENV": "production"}):
            repo = FraiseQLRepository(db_pool)
            assert repo.mode == "production"

    def test_determine_mode_default(self, db_pool):
        """Test default mode determination."""
        with patch.dict(os.environ, {}, clear=True):
            repo = FraiseQLRepository(db_pool)
            # Should default to production
            assert repo.mode == "production"

    # Note: Tests for private methods like _instantiate_recursive, _get_type_for_data
    # and _convert_value have been removed as these methods don't exist in the
    # current implementation. The functionality they would test is covered by
    # the public API tests above.

    def test_mode_property(self, repository):
        """Test mode property access."""
        assert hasattr(repository, "mode")
        assert repository.mode in ["development", "production"]

    @pytest.mark.asyncio
    async def test_context_access(self, repository):
        """Test context access and modification."""
        # Initially empty
        assert repository.context == {}

        # Can be modified
        repository.context["user_id"] = 123
        assert repository.context["user_id"] == 123

        # Context persists
        repository.context.update({"tenant": "test", "role": "admin"})
        assert len(repository.context) == 3


@pytest.mark.database
class TestRepositoryEdgeCases:
    """Test edge cases and error conditions."""

    @pytest.fixture
    async def test_tables(self, db_pool):
        """Create test tables for edge case tests."""
        async with db_pool.connection() as conn:
            # Drop existing table to ensure clean state
            await conn.execute("DROP TABLE IF EXISTS edge_test CASCADE")

            await conn.execute(
                """
                CREATE TABLE edge_test (
                    id SERIAL PRIMARY KEY,
                    status TEXT,
                    data JSONB DEFAULT '{}'
                )
            """
            )
            await conn.commit()

        yield "edge_test_created"

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute("DROP TABLE IF EXISTS edge_test CASCADE")
            await conn.commit()

    @pytest.fixture
    def repository(self, db_pool):
        return FraiseQLRepository(db_pool)

    @pytest.mark.asyncio
    async def test_run_with_complex_composed_sql(self, repository, db_pool, test_tables):
        """Test running queries with complex Composed SQL."""
        # Insert test data
        async with db_pool.connection() as conn:
            await conn.execute(
                """INSERT INTO edge_test (id, status) VALUES (1, 'active'), (2, 'inactive')"""
            )
            await conn.commit()

        # Complex composed SQL
        complex_sql = Composed(
            [
                SQL("SELECT * FROM edge_test WHERE "),
                SQL("id = %(id)s AND "),
                SQL("status = %(status)s"),
            ]
        )

        query = DatabaseQuery(complex_sql, {"id": 1, "status": "active"})
        result = await repository.run(query)

        assert len(result) == 1
        assert result[0]["id"] == 1
        assert result[0]["status"] == "active"

    @pytest.mark.asyncio
    async def test_nested_transaction_calls(self, repository, test_tables):
        """Test nested transaction function calls."""

        async def outer_func(conn, value):
            # Insert a row
            await conn.execute(
                "INSERT INTO edge_test (id, status) VALUES (%s, %s)", (value, f"status_{value}")
            )

            # Inner function that queries the data
            async def inner_func():
                cursor = await conn.execute(
                    "SELECT COUNT(*) FROM edge_test WHERE id = %s", (value,)
                )
                row = await cursor.fetchone()
                return row[0]

            result = await inner_func()
            return result

        result = await repository.run_in_transaction(outer_func, 99)
        assert result == 1  # One row inserted and found

    def test_repository_string_representation(self, repository):
        """Test repository string representation if it exists."""
        # Repository should be representable
        repr_str = repr(repository)
        assert "FraiseQLRepository" in repr_str or "Repository" in str(type(repository))
