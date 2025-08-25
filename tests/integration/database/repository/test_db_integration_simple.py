"""Integration tests for FraiseQLRepository with real PostgreSQL.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
Each test runs in its own committed schema that is cleaned up automatically.
"""

import asyncio

import pytest
from psycopg.sql import SQL, Composed, Identifier

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403
from tests.unit.utils.schema_utils import (
    SchemaQualifiedQueryBuilder,
    build_select_query,
    get_current_schema,
)

from fraiseql.db import DatabaseQuery, FraiseQLRepository


@pytest.mark.database
class TestFraiseQLRepositoryIntegration:
    """Integration test suite for FraiseQLRepository with real database."""

    @pytest.fixture
    async def test_data(self, db_connection_committed):
        """Create test tables and data with committed changes."""
        conn = db_connection_committed
        schema = await get_current_schema(conn)

        # Create users table
        await conn.execute(
            """
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """
        )

        # Insert test data
        await conn.execute(
            """
            INSERT INTO users (data) VALUES
            ('{"name": "John Doe", "email": "john@example.com", "active": true}'::jsonb),
            ('{"name": "Jane Smith", "email": "jane@example.com", "active": true}'::jsonb),
            ('{"name": "Bob Wilson", "email": "bob@example.com", "active": false}'::jsonb)
        """
        )

        # Create posts table
        await conn.execute(
            """
            CREATE TABLE posts (
                id SERIAL PRIMARY KEY,
                user_id INTEGER REFERENCES users(id),
                data JSONB NOT NULL DEFAULT '{}'::jsonb,
                published_at TIMESTAMP
            )
        """
        )

        await conn.execute(
            """
            INSERT INTO posts (user_id, data, published_at) VALUES
            (1, '{"title": "First Post", "content": "Hello World"}'::jsonb, '2024-01-01'),
            (1, '{"title": "Second Post", "content": "More content"}'::jsonb, '2024-01-02'),
            (2, '{"title": "Jane''s Post", "content": "Jane''s thoughts"}'::jsonb, NULL)
        """
        )

        # Commit the changes so they're visible to other connections
        await conn.commit()

        # Return the schema name for use in queries
        return schema

    @pytest.mark.asyncio
    async def test_run_simple_query(self, db_pool, test_data) -> None:
        """Test running a simple SQL query."""
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        # Example using the new query builder utility
        statement = (
            SchemaQualifiedQueryBuilder(schema)
            .select("id", "data->>'name' as name")
            .from_table("users")
            .order_by("id")
            .build()
        )

        query = DatabaseQuery(statement=statement, params={}, fetch_result=True)
        result = await repository.run(query)

        # Assertions
        assert len(result) == 3
        assert result[0]["name"] == "John Doe"
        assert result[1]["name"] == "Jane Smith"
        assert result[2]["name"] == "Bob Wilson"

    @pytest.mark.asyncio
    async def test_run_query_with_params(self, db_pool, test_data) -> None:
        """Test running a query with parameters."""
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                """SELECT id, data->>'email' as email FROM {}.users """
                """WHERE data->>'email' = %(email)s"""
            ).format(Identifier(schema)),
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=Composed(
                [
                    SQL("SELECT id, data FROM "),
                    Identifier(schema, "users"),
                    SQL(" WHERE (data->>'active')::boolean = %(active)s"),
                ]
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                """INSERT INTO {}.users (data) VALUES (%(data)s::jsonb) RETURNING id, data"""
            ).format(Identifier(schema)),
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        # Update Bob's status to active
        update_query = DatabaseQuery(
            statement=Composed(
                [
                    SQL("UPDATE "),
                    Identifier(schema, "users"),
                    SQL(" SET data = jsonb_set(data, '{active}', 'true') "),
                    SQL("WHERE data->>'name' = %(name)s"),
                ]
            ),
            params={"name": "Bob Wilson"},
            fetch_result=False,
        )
        await repository.run(update_query)

        # Verify the update
        verify_query = DatabaseQuery(
            statement=SQL("SELECT data FROM {}.users WHERE data->>'name' = %(name)s").format(
                Identifier(schema)
            ),
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        # Delete inactive users
        delete_query = DatabaseQuery(
            statement=SQL("DELETE FROM {}.users WHERE NOT (data->>'active')::boolean").format(
                Identifier(schema)
            ),
            params={},
            fetch_result=False,
        )
        await repository.run(delete_query)

        # Verify deletion
        verify_query = DatabaseQuery(
            statement=SQL("SELECT COUNT(*) as count FROM {}.users").format(Identifier(schema)),
            params={},
            fetch_result=True,
        )
        result = await repository.run(verify_query)

        # Assertions
        assert result[0]["count"] == 2  # Only active users remain

    @pytest.mark.asyncio
    async def test_run_join_query(self, db_pool, test_data) -> None:
        """Test running a JOIN query."""
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)
        query = DatabaseQuery(
            statement=SQL(
                """
                SELECT
                    u.data->>'name' as user_name,
                    p.data->>'title' as post_title,
                    p.published_at
                FROM {0}.users u
                JOIN {0}.posts p ON u.id = p.user_id
                WHERE p.published_at IS NOT NULL
                ORDER BY p.published_at
            """
            ).format(Identifier(schema)),
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
    async def test_transaction_behavior(self, db_pool, db_connection_committed) -> None:
        """Test transaction behavior with the unified container system."""
        conn = db_connection_committed
        schema = await get_current_schema(conn)
        repository = FraiseQLRepository(pool=db_pool)

        # Create minimal test table within our transaction
        await conn.execute(
            """
            CREATE TABLE test_tx (
                id SERIAL PRIMARY KEY,
                value TEXT
            )
        """
        )

        # Insert data that will be visible within this test
        await conn.execute("INSERT INTO test_tx (value) VALUES ('test_value')")

        # Commit so it's visible to the pool connections
        await conn.commit()

        # Verify data is visible
        query = DatabaseQuery(
            statement=SQL("SELECT * FROM {}.test_tx").format(Identifier(schema)),
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        # Test @> operator (contains)
        contains_query = DatabaseQuery(
            statement=SQL("SELECT * FROM {}.users WHERE data @> %(filter)s::jsonb").format(
                Identifier(schema)
            ),
            params={"filter": '{"active": true}'},
            fetch_result=True,
        )
        active_users = await repository.run(contains_query)

        # Test ? operator (key exists)
        has_email_query = DatabaseQuery(
            statement=SQL("SELECT * FROM {}.users WHERE data ? 'email'").format(Identifier(schema)),
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        # Example using build_select_query utility for complex queries with GROUP BY
        statement = build_select_query(
            schema=schema,
            table="users",
            columns=[
                """(data->>'active')::boolean as active""",
                """COUNT(*) as count""",
                """jsonb_agg(data->>'name') as names""",
            ],
            group_by=["(data->>'active')::boolean"],
        )

        query = DatabaseQuery(statement=statement, params={}, fetch_result=True)
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
        schema = test_data  # test_data fixture now returns the schema name
        repository = FraiseQLRepository(pool=db_pool)

        async def run_query(email: str):
            query = DatabaseQuery(
                statement=SQL("SELECT * FROM {}.users WHERE data->>'email' = %(email)s").format(
                    Identifier(schema)
                ),
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
            statement=SQL("SELECT * FROM nonexistent_table"), params={}, fetch_result=True
        )

        with pytest.raises(Exception) as exc_info:
            await repository.run(invalid_query)

        # Should be a database error
        assert "nonexistent_table" in str(exc_info.value)
