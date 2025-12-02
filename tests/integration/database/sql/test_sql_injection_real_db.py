"""Real database SQL injection prevention tests.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
Each test runs in its own transaction that is rolled back automatically.

Tests SQL injection prevention with actual database execution
to ensure parameterization works correctly in practice.
"""

import json

import pytest
import pytest_asyncio
from psycopg.sql import SQL, Identifier

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.sql.where_generator import safe_create_where_type

pytestmark = [pytest.mark.integration, pytest.mark.database]


@fraiseql.type
class User:
    id: int
    name: str
    email: str
    role: str = "user"
    active: bool = True


@pytest.mark.database
class TestSQLInjectionPrevention:
    """Test SQL injection prevention with real database execution."""

    @pytest_asyncio.fixture(scope="class")
    async def test_users(self, class_db_pool, test_schema, clear_registry_class):
        """Create users table and test data within committed schema."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")

            # Create users table
            await conn.execute(
                """
                CREATE TABLE users (
                    id SERIAL PRIMARY KEY,
                    data JSONB NOT NULL DEFAULT '{}'
                )
                """
            )

            # Insert test data
            test_users = [
                {"name": "Admin", "email": "admin@example.com", "role": "admin", "active": True},
                {"name": "User1", "email": "user1@example.com", "role": "user", "active": True},
                {"name": "User2", "email": "user2@example.com", "role": "user", "active": False},
            ]

            for user_data in test_users:
                await conn.execute(
                    """INSERT INTO users (data) VALUES (%s::jsonb)""", (json.dumps(user_data),)
                )

            # Commit the data so it's visible to other connections from the pool
            await conn.commit()

        # Return the schema name for use in queries
        return test_schema

    @pytest.mark.asyncio
    async def test_sql_injection_in_string_fields(
        self, class_db_pool, test_schema, test_users
    ) -> None:
        """Test SQL injection attempts in string fields."""
        schema = test_users
        repo = FraiseQLRepository(class_db_pool)
        UserWhere = safe_create_where_type(User)

        # Various SQL injection attempts
        malicious_inputs = [
            "'; DROP TABLE users; --",
            "' OR data->>'role' = 'admin' --",
            "'); DELETE FROM users WHERE true; --",
            "' UNION SELECT * FROM users WHERE data->>'role' = 'admin' --",
            "admin'/*",
            "admin' OR '1'='1",
        ]

        for malicious in malicious_inputs:
            # Try injection via name field
            where = UserWhere(name={"eq": malicious})
            sql_where = where.to_sql()

            # Build the complete SQL query
            query = DatabaseQuery(
                statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
                + sql_where,
                params={},
                fetch_result=True,
            )

            # This should execute safely without SQL injection
            results = await repo.run(query)

            # Verify no unauthorized access - should return empty or only exact matches
            assert len(results) == 0, f"Injection attempt succeeded with: {malicious}"

            # Verify table still exists and has correct row count
            count_query = DatabaseQuery(
                statement=SQL("SELECT COUNT(*) FROM {}.users").format(Identifier(schema)),
                params={},
                fetch_result=True,
            )
            count_result = await repo.run(count_query)
            assert count_result[0]["count"] == 3, (
                f"Table corrupted after injection attempt: {malicious}"
            )

    @pytest.mark.asyncio
    async def test_sql_injection_in_list_operations(
        self, class_db_pool, test_schema, test_users
    ) -> None:
        """Test SQL injection in IN/NOT IN operations."""
        schema = test_users
        repo = FraiseQLRepository(class_db_pool)
        UserWhere = safe_create_where_type(User)

        # Try injection via IN operator
        malicious_list = ["user", "admin'; DROP TABLE users; --"]

        where = UserWhere(role={"in": malicious_list})
        sql_where = where.to_sql()

        query = DatabaseQuery(
            statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
            + sql_where,
            params={},
            fetch_result=True,
        )

        results = await repo.run(query)

        # Should only match exact values, not execute injection
        assert all(r["data"]["role"] in ["user", "admin"] for r in results), (
            """IN operator allowed injection"""
        )

        # Verify table integrity
        count_query = DatabaseQuery(
            statement=SQL("SELECT COUNT(*) FROM {}.users").format(Identifier(schema)),
            params={},
            fetch_result=True,
        )
        count_result = await repo.run(count_query)
        assert count_result[0]["count"] == 3, "Table corrupted via IN operator injection"

    @pytest.mark.asyncio
    async def test_sql_injection_with_special_characters(
        self, class_db_pool, test_schema, test_users
    ) -> None:
        """Test handling of special characters that could be used in injections."""
        schema = test_users
        repo = FraiseQLRepository(class_db_pool)
        UserWhere = safe_create_where_type(User)

        # Special characters that might be used in injection attempts
        special_inputs = [
            "user\\'; DROP TABLE users; --",  # Backslash
            "user`; DROP TABLE users; --",  # Backtick
            'user"; DROP TABLE users; --',  # Double quote
            "user\n; DROP TABLE users; --",  # Newline
            "user\r\n; DROP TABLE users; --",  # CRLF
            "user\x00; DROP TABLE users; --",  # Null byte
            "user/*comment*/name",  # SQL comment
        ]

        for special in special_inputs:
            # Should handle special characters safely
            try:
                # Use eq operator instead of contains for string comparison
                where = UserWhere(name={"eq": special})
                sql_where = where.to_sql()

                query = DatabaseQuery(
                    statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
                    + sql_where,
                    params={},
                    fetch_result=True,
                )

                results = await repo.run(query)
                assert len(results) == 0, f"Special character injection with: {special!r}"
            except Exception as e:
                # Null bytes cause PostgreSQL to raise DataError, which is expected
                if "\x00" in special and "NUL" in str(e):
                    # This is expected behavior - PostgreSQL rejects null bytes
                    pass
                else:
                    raise

            # Verify database integrity
            count_query = DatabaseQuery(
                statement=SQL("SELECT COUNT(*) FROM {}.users").format(Identifier(schema)),
                params={},
                fetch_result=True,
            )
            count_result = await repo.run(count_query)
            assert count_result[0]["count"] == 3, (
                f"Database corrupted with special character: {special!r}"
            )

    @pytest.mark.asyncio
    async def test_verify_parameterization(self, class_db_pool, test_schema, test_users) -> None:
        """Verify that queries are properly parameterized."""
        schema = test_users
        repo = FraiseQLRepository(class_db_pool)
        UserWhere = safe_create_where_type(User)

        # Create a query with potential injection
        where = UserWhere(
            name={"eq": "Admin'; DROP TABLE users; --"},
            role={"in": ["admin", "user'; DELETE FROM users; --"]},
        )

        sql_where = where.to_sql()
        query = DatabaseQuery(
            statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
            + sql_where,
            params={},
            fetch_result=True,
        )

        # Execute query
        results = await repo.run(query)

        # Query should execute safely with no results
        assert len(results) == 0

        # Verify parameterization by checking that the table still exists
        # and has the correct structure
        table_check_query = DatabaseQuery(
            statement=SQL(
                """
                SELECT COUNT(*)
                FROM information_schema.columns
                WHERE table_name = 'users' AND table_schema = %(schema)s
            """
            ),
            params={"schema": schema},
            fetch_result=True,
        )
        table_check_result = await repo.run(table_check_query)
        assert table_check_result[0]["count"] == 2, "Table structure was modified"

    @pytest.mark.asyncio
    async def test_actual_database_execution(self, class_db_pool, test_schema, test_users) -> None:
        """Real integration test that executes against database.

        This replaces the placeholder test in the original SQL injection
        prevention tests with actual database execution.
        """
        schema = test_users
        repo = FraiseQLRepository(class_db_pool)
        UserWhere = safe_create_where_type(User)

        # Test that normal queries work correctly
        where_normal = UserWhere(name={"eq": "Admin"})
        sql_where = where_normal.to_sql()
        query_normal = DatabaseQuery(
            statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
            + sql_where,
            params={},
            fetch_result=True,
        )

        results_normal = await repo.run(query_normal)
        assert len(results_normal) == 1
        assert results_normal[0]["data"]["name"] == "Admin"

        # Test that injection attempts fail
        where_injection = UserWhere(name={"eq": "'; DROP TABLE users; --"})
        sql_where_injection = where_injection.to_sql()
        query_injection = DatabaseQuery(
            statement=SQL("SELECT id, data FROM {}.users WHERE ").format(Identifier(schema))
            + sql_where_injection,
            params={},
            fetch_result=True,
        )

        results_injection = await repo.run(query_injection)
        assert len(results_injection) == 0

        # Verify database is intact
        verify_query = DatabaseQuery(
            statement=SQL(
                """
                SELECT EXISTS (
                    SELECT FROM information_schema.tables
                    WHERE table_name = 'users' AND table_schema = %(schema)s
                )
            """
            ),
            params={"schema": schema},
            fetch_result=True,
        )
        verify_result = await repo.run(verify_query)
        assert verify_result[0]["exists"] is True, "Table was dropped via SQL injection"
