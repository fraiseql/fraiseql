"""Integration tests for FraiseQLRepository with real PostgreSQL.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
Each test runs in its own transaction that is rolled back automatically.
"""

import asyncio

import pytest
from psycopg.sql import SQL, Composed, Identifier

from fraiseql.db import DatabaseQuery, FraiseQLRepository


@pytest.mark.database
class TestFraiseQLRepositoryIntegration:
    """Integration test suite for FraiseQLRepository with real database."""

    @pytest.fixture
    async def test_data(self, db_connection):
        """Create test tables and data within the test transaction."""
        # Create users table
        await db_connection.execute(
            """
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """,
        )

        # Insert test data
        await db_connection.execute(
            """
            INSERT INTO users (data) VALUES
            ('{"name": "John Doe", "email": "john@example.com", "active": true}'::jsonb),
            ('{"name": "Jane Smith", "email": "jane@example.com", "active": true}'::jsonb),
            ('{"name": "Bob Wilson", "email": "bob@example.com", "active": false}'::jsonb)
        """,
        )

        # Create posts table
        await db_connection.execute(
            """
            CREATE TABLE posts (
                id SERIAL PRIMARY KEY,
                user_id INTEGER REFERENCES users(id),
                data JSONB NOT NULL DEFAULT '{}'::jsonb,
                published_at TIMESTAMP
            )
        """,
        )

        await db_connection.execute(
            """
            INSERT INTO posts (user_id, data, published_at) VALUES
            (1, '{"title": "First Post", "content": "Hello World"}'::jsonb, '2024-01-01'),
            (1, '{"title": "Second Post", "content": "More content"}'::jsonb, '2024-01-02'),
            (2, '{"title": "Jane''s Post", "content": "Jane''s thoughts"}'::jsonb, NULL)
        """,
        )

        # No commit needed - transaction will be rolled back after test

    @pytest.mark.asyncio
    async def test_run_simple_query(self, db_pool, test_data) -> None:
        """Test running a simple SQL query."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL("SELECT id, data->>'name' as name FROM users ORDER BY id"),
            params={},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 3
        assert result[0]["name"] == "John Doe"
        assert result[1]["name"] == "Jane Smith"
        assert result[2]["name"] == "Bob Wilson"

    @pytest.mark.asyncio
    async def test_run_query_with_params(self, db_pool, test_data) -> None:
        """Test running a query with parameters."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                "SELECT id, data->>'email' as email FROM users WHERE data->>'email' = %(email)s",
            ),
            params={"email": "jane@example.com"},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 1
        assert result[0]["email"] == "jane@example.com"

    @pytest.mark.asyncio
    async def test_run_composed_query(self, db_pool, test_data) -> None:
        """Test running a Composed SQL query."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=Composed(
                [
                    SQL("SELECT id, data FROM "),
                    Identifier("users"),
                    SQL(" WHERE (data->>'active')::boolean = %(active)s"),
                ],
            ),
            params={"active": True},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 2
        active_names = [r["data"]["name"] for r in result]
        assert "John Doe" in active_names
        assert "Jane Smith" in active_names

    @pytest.mark.asyncio
    async def test_run_insert_returning(self, db_pool, test_data) -> None:
        """Test running an INSERT with RETURNING clause."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL("INSERT INTO users (data) VALUES (%(data)s::jsonb) RETURNING id, data"),
            params={"data": '{"name": "New User", "email": "new@example.com", "active": true}'},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 1
        assert result[0]["data"]["name"] == "New User"
        assert isinstance(result[0]["id"], int)

    @pytest.mark.asyncio
    async def test_run_update_query(self, db_pool, test_data) -> None:
        """Test running an UPDATE query."""
        repository = FraiseQLRepository(pool=db_pool)

        # Update Bob's status to active
        update_query = DatabaseQuery(
            statement=SQL(
                "UPDATE users SET data = jsonb_set(data, '{active}', 'true') "
                "WHERE data->>'name' = %(name)s",
            ),
            params={"name": "Bob Wilson"},
            fetch_result=False,
        )
        await repository.run(update_query)

        # Verify the update
        verify_query = DatabaseQuery(
            statement=SQL("SELECT data FROM users WHERE data->>'name' = %(name)s"),
            params={"name": "Bob Wilson"},
            fetch_result=True,
        )
        result = await repository.run(verify_query)

        # Assertions
        assert len(result) == 1
        assert result[0]["data"]["active"] is True

    @pytest.mark.asyncio
    async def test_run_delete_query(self, db_pool, test_data) -> None:
        """Test running a DELETE query."""
        repository = FraiseQLRepository(pool=db_pool)

        # Delete inactive users
        delete_query = DatabaseQuery(
            statement=SQL("DELETE FROM users WHERE NOT (data->>'active')::boolean"),
            params={},
            fetch_result=False,
        )
        await repository.run(delete_query)

        # Verify deletion
        verify_query = DatabaseQuery(
            statement=SQL("SELECT COUNT(*) as count FROM users"),
            params={},
            fetch_result=True,
        )
        result = await repository.run(verify_query)

        # Assertions
        assert result[0]["count"] == 2  # Only active users remain

    @pytest.mark.asyncio
    async def test_run_join_query(self, db_pool, test_data) -> None:
        """Test running a JOIN query."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                """
                SELECT
                    u.data->>'name' as user_name,
                    p.data->>'title' as post_title,
                    p.published_at
                FROM users u
                JOIN posts p ON u.id = p.user_id
                WHERE p.published_at IS NOT NULL
                ORDER BY p.published_at
            """,
            ),
            params={},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 2
        assert result[0]["user_name"] == "John Doe"
        assert result[0]["post_title"] == "First Post"
        assert result[1]["post_title"] == "Second Post"

    @pytest.mark.asyncio
    async def test_transaction_behavior(self, db_pool, db_connection) -> None:
        """Test transaction behavior with the unified container system."""
        repository = FraiseQLRepository(pool=db_pool)

        # Create minimal test table within our transaction
        await db_connection.execute(
            """
            CREATE TABLE test_tx (
                id SERIAL PRIMARY KEY,
                value TEXT
            )
        """,
        )

        # Insert data that will be visible within this test
        await db_connection.execute("INSERT INTO test_tx (value) VALUES ('test_value')")

        # Verify data is visible
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM test_tx"),
            params={},
            fetch_result=True,
        )
        result = await repository.run(query)

        assert len(result) == 1
        assert result[0]["value"] == "test_value"

        # After this test, the transaction will be rolled back
        # and the table will not exist for other tests

    @pytest.mark.asyncio
    async def test_jsonb_operators(self, db_pool, test_data) -> None:
        """Test JSONB operators in queries."""
        repository = FraiseQLRepository(pool=db_pool)

        # Test @> operator (contains)
        contains_query = DatabaseQuery(
            statement=SQL("SELECT * FROM users WHERE data @> %(filter)s::jsonb"),
            params={"filter": '{"active": true}'},
            fetch_result=True,
        )
        active_users = await repository.run(contains_query)

        # Test ? operator (key exists)
        has_email_query = DatabaseQuery(
            statement=SQL("SELECT * FROM users WHERE data ? 'email'"),
            params={},
            fetch_result=True,
        )
        users_with_email = await repository.run(has_email_query)

        # Assertions
        assert len(active_users) == 2
        assert len(users_with_email) == 3

    @pytest.mark.asyncio
    async def test_aggregate_query(self, db_pool, test_data) -> None:
        """Test aggregate functions with JSONB."""
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                """
                SELECT
                    (data->>'active')::boolean as active,
                    COUNT(*) as count,
                    jsonb_agg(data->>'name') as names
                FROM users
                GROUP BY (data->>'active')::boolean
            """,
            ),
            params={},
            fetch_result=True,
        )
        result = await repository.run(query)

        # Assertions
        assert len(result) == 2

        active_group = next(r for r in result if r["active"] is True)
        inactive_group = next(r for r in result if r["active"] is False)

        assert active_group["count"] == 2
        assert inactive_group["count"] == 1
        assert "Bob Wilson" in inactive_group["names"]

    @pytest.mark.asyncio
    async def test_connection_pool_concurrency(self, db_pool, test_data) -> None:
        """Test concurrent queries using the connection pool."""
        repository = FraiseQLRepository(pool=db_pool)

        async def run_query(email: str):
            query = DatabaseQuery(
                statement=SQL("SELECT * FROM users WHERE data->>'email' = %(email)s"),
                params={"email": email},
                fetch_result=True,
            )
            return await repository.run(query)

        # Run multiple queries concurrently
        results = await asyncio.gather(
            run_query("john@example.com"),
            run_query("jane@example.com"),
            run_query("bob@example.com"),
            run_query("nonexistent@example.com"),
        )

        # Assertions
        assert len(results[0]) == 1  # John
        assert len(results[1]) == 1  # Jane
        assert len(results[2]) == 1  # Bob
        assert len(results[3]) == 0  # Nonexistent

    @pytest.mark.asyncio
    async def test_error_handling(self, db_pool) -> None:
        """Test error handling in repository."""
        repository = FraiseQLRepository(pool=db_pool)

        # Test with invalid SQL
        invalid_query = DatabaseQuery(
            statement=SQL("SELECT * FROM nonexistent_table"),
            params={},
            fetch_result=True,
        )

        with pytest.raises(Exception) as exc_info:
            await repository.run(invalid_query)

        # Should be a database error
        assert "nonexistent_table" in str(exc_info.value)
