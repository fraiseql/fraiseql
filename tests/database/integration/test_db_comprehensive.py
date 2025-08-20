"""Comprehensive database tests for FraiseQL core components.

Fixed version that properly handles database fixtures and test isolation.
"""

from uuid import uuid4

import pytest
from psycopg.sql import SQL

# Import database fixtures
from tests.database_conftest import *  # noqa: F403

from fraiseql.db import DatabaseQuery, FraiseQLRepository


@pytest.fixture
async def setup_test_database(db_pool):
    """Create test tables for repository tests using a dedicated connection."""
    async with db_pool.connection() as conn:
        # Drop existing tables if any
        await conn.execute("DROP VIEW IF EXISTS users_view CASCADE")
        await conn.execute("DROP VIEW IF EXISTS posts_view CASCADE")
        await conn.execute("DROP TABLE IF EXISTS posts CASCADE")
        await conn.execute("DROP TABLE IF EXISTS users CASCADE")
        await conn.execute("DROP FUNCTION IF EXISTS test_function CASCADE")
        await conn.execute("DROP FUNCTION IF EXISTS void_function CASCADE")

        # Create users table
        await conn.execute(
            """
            CREATE TABLE users (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name TEXT,
                data JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """
        )

        # Create posts table
        await conn.execute(
            """
            CREATE TABLE posts (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                title TEXT,
                author_id UUID REFERENCES users(id),
                data JSONB NOT NULL DEFAULT '{}'::jsonb
            )
            """
        )

        # Create views
        await conn.execute(
            """
            CREATE VIEW users_view AS
            SELECT id, name, data FROM users
            """
        )

        await conn.execute(
            """
            CREATE VIEW posts_view AS
            SELECT p.id, p.title, p.data,
                   u.data as author_data
            FROM posts p
            LEFT JOIN users u ON p.author_id = u.id
            """
        )

        # Create test functions
        await conn.execute(
            """
            CREATE OR REPLACE FUNCTION test_function(param jsonb)
            RETURNS jsonb AS $$
            BEGIN
                RETURN jsonb_build_object('id', '123', 'status', 'success', 'param', param);
            END;
            $$ LANGUAGE plpgsql;
            """
        )

        await conn.execute(
            """
            CREATE OR REPLACE FUNCTION void_function(param jsonb)
            RETURNS void AS $$
            BEGIN
                -- Do nothing
            END;
            $$ LANGUAGE plpgsql;
            """
        )

        await conn.commit()

    yield "setup_complete"

    # Cleanup
    async with db_pool.connection() as conn:
        await conn.execute("DROP VIEW IF EXISTS users_view CASCADE")
        await conn.execute("DROP VIEW IF EXISTS posts_view CASCADE")
        await conn.execute("DROP TABLE IF EXISTS posts CASCADE")
        await conn.execute("DROP TABLE IF EXISTS users CASCADE")
        await conn.execute("DROP FUNCTION IF EXISTS test_function CASCADE")
        await conn.execute("DROP FUNCTION IF EXISTS void_function CASCADE")
        await conn.commit()


@pytest.mark.database
class TestFraiseQLRepository:
    """Test FraiseQLRepository class with proper database setup."""

    @pytest.fixture
    async def repository(self, db_pool):
        """Create a repository instance."""
        return FraiseQLRepository(db_pool)

    async def test_repository_initialization(self, db_pool):
        """Test repository initialization."""
        context = {"user": "test", "tenant_id": 123}
        repo = FraiseQLRepository(db_pool, context)

        assert repo._pool == db_pool
        assert repo.context == context

    async def test_find_with_simple_data(self, repository, db_pool, setup_test_database):
        """Test find method with simple data."""
        # Insert test data
        async with db_pool.connection() as conn:
            user1_id = uuid4()
            user2_id = uuid4()
            await conn.execute(
                """INSERT INTO users (id, name, data) VALUES
                   (%s, %s, %s::jsonb),
                   (%s, %s, %s::jsonb)""",
                (
                    user1_id,
                    "Alice",
                    '{"id": "' + str(user1_id) + '", "name": "Alice", "age": 25}',
                    user2_id,
                    "Bob",
                    '{"id": "' + str(user2_id) + '", "name": "Bob", "age": 30}',
                ),
            )
            await conn.commit()

        # Execute find
        results = await repository.find("users_view")

        # Verify
        assert len(results) == 2
        names = [r["name"] for r in results]
        assert "Alice" in names
        assert "Bob" in names

    async def test_find_one(self, repository, db_pool, setup_test_database):
        """Test find_one method."""
        # Insert test data
        async with db_pool.connection() as conn:
            user_id = uuid4()
            await conn.execute(
                """INSERT INTO users (id, name, data) VALUES (%s, %s, %s::jsonb)""",
                (
                    user_id,
                    "Single User",
                    '{"id": "'
                    + str(user_id)
                    + '", "name": "Single User", "email": "single@example.com"}',
                ),
            )
            await conn.commit()

        # Execute find_one
        result = await repository.find_one("users_view", id=user_id)

        # Verify
        assert result is not None
        assert result["name"] == "Single User"
        assert result["data"]["email"] == "single@example.com"

    async def test_find_one_no_result(self, repository, setup_test_database):
        """Test find_one when no result found."""
        # Execute find_one with non-existent ID
        result = await repository.find_one("users_view", id=uuid4())

        # Verify
        assert result is None

    async def test_run_query_with_results(self, repository, db_pool, setup_test_database):
        """Test running a query that returns results."""
        # Insert test data
        async with db_pool.connection() as conn:
            user1_id = uuid4()
            user2_id = uuid4()
            await conn.execute(
                """INSERT INTO users (id, name, data) VALUES
                   (%s, %s, %s::jsonb), (%s, %s, %s::jsonb)""",
                (user1_id, "User 1", '{"age": 25}', user2_id, "User 2", '{"age": 30}'),
            )
            await conn.commit()

        # Create query
        query = DatabaseQuery(statement=SQL("SELECT id, name FROM users ORDER BY name"), params={})

        # Execute
        results = await repository.run(query)

        # Verify
        assert len(results) == 2
        assert results[0]["name"] == "User 1"
        assert results[1]["name"] == "User 2"

    async def test_run_query_without_fetch(self, repository, db_pool, setup_test_database):
        """Test running a query without fetching results."""
        # Create insert query
        query = DatabaseQuery(
            statement=SQL("INSERT INTO users (name, data) VALUES (%(name)s, %(data)s::jsonb)"),
            params={"name": "Test User", "data": '{"test": true}'},
            fetch_result=False,
        )

        # Execute
        results = await repository.run(query)

        # Verify no results
        assert results == []

        # Verify insert worked
        async with db_pool.connection() as conn:
            cursor = await conn.execute(
                "SELECT COUNT(*) FROM users WHERE name = %s", ("Test User",)
            )
            row = await cursor.fetchone()
            assert row[0] == 1

    async def test_execute_function(self, repository, setup_test_database):
        """Test executing a PostgreSQL function."""
        # Execute function with parameters
        result = await repository.execute_function("test_function", {"test_key": "test_value"})

        # Verify result - the function returns a single column named 'test_function'
        assert "test_function" in result
        function_result = result["test_function"]
        assert function_result["id"] == "123"
        assert function_result["status"] == "success"
        assert function_result["param"]["test_key"] == "test_value"

    async def test_execute_function_no_result(self, repository, setup_test_database):
        """Test executing a function that returns no result."""
        # Execute void function
        result = await repository.execute_function("void_function", {"ignored": "param"})

        # Verify result - void functions return a column with empty string
        assert "void_function" in result
        assert result["void_function"] == ""

    async def test_run_in_transaction_success(self, repository, db_pool, setup_test_database):
        """Test running a function in a transaction successfully."""

        # Define test function that inserts data
        async def test_func(conn):
            await conn.execute(
                """INSERT INTO users (name, data) VALUES (%s, %s::jsonb)""",
                ("Transaction User", '{"transactional": true}'),
            )
            # Verify we can query within transaction
            cursor = await conn.execute(
                "SELECT COUNT(*) FROM users WHERE name = %s", ("Transaction User",)
            )
            row = await cursor.fetchone()
            assert row[0] == 1
            return "success"

        # Execute
        result = await repository.run_in_transaction(test_func)

        # Verify
        assert result == "success"

        # Verify data persisted
        async with db_pool.connection() as conn:
            cursor = await conn.execute(
                "SELECT COUNT(*) FROM users WHERE name = %s", ("Transaction User",)
            )
            row = await cursor.fetchone()
            assert row[0] == 1

    async def test_run_in_transaction_rollback(self, repository, db_pool, setup_test_database):
        """Test transaction rollback on error."""

        # Define test function that raises error after insert
        async def test_func(conn):
            await conn.execute(
                """INSERT INTO users (name, data) VALUES (%s, %s::jsonb)""",
                ("Rollback User", '{"should_rollback": true}'),
            )
            raise ValueError("Test error")

        # Execute and expect exception
        with pytest.raises(ValueError) as exc_info:
            await repository.run_in_transaction(test_func)

        assert str(exc_info.value) == "Test error"

        # Verify rollback occurred
        async with db_pool.connection() as conn:
            cursor = await conn.execute(
                "SELECT COUNT(*) FROM users WHERE name = %s", ("Rollback User",)
            )
            row = await cursor.fetchone()
            assert row[0] == 0  # Should not exist due to rollback
