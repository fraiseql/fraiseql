"""Real database SQL injection prevention tests.

Tests SQL injection prevention with actual database execution
to ensure parameterization works correctly in practice.
"""

import json

import pytest
from psycopg.sql import SQL

import fraiseql
from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.sql.where_generator import safe_create_where_type


@fraiseql.type
class User:
    id: int
    name: str
    email: str
    role: str = "user"
    active: bool = True


async def setup_test_users(conn):
    """Create users table and test data."""
    await conn.execute("""
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            data JSONB NOT NULL DEFAULT '{}'
        )
    """)

    # Insert test data
    test_users = [
        {"name": "Admin", "email": "admin@example.com", "role": "admin", "active": True},
        {"name": "User1", "email": "user1@example.com", "role": "user", "active": True},
        {"name": "User2", "email": "user2@example.com", "role": "user", "active": False},
    ]

    for user_data in test_users:
        await conn.execute(
            "INSERT INTO users (data) VALUES (%s::jsonb)",
            (json.dumps(user_data),),
        )

    await conn.commit()


async def cleanup_test_users(conn):
    """Clean up test table."""
    await conn.execute("DROP TABLE IF EXISTS users CASCADE")
    await conn.commit()


@pytest.mark.database
class TestSQLInjectionPrevention:
    """Test SQL injection prevention with real database execution."""

    @pytest.mark.asyncio
    async def test_sql_injection_in_string_fields(self, db_pool):
        """Test SQL injection attempts in string fields."""
        # Setup
        async with db_pool.connection() as conn:
            await setup_test_users(conn)

        repo = FraiseQLRepository(db_pool)
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
                statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where,
                params={},
                fetch_result=True,
            )

            # This should execute safely without SQL injection
            results = await repo.run(query)

            # Verify no unauthorized access - should return empty or only exact matches
            assert len(results) == 0, f"Injection attempt succeeded with: {malicious}"

            # Verify table still exists and has correct row count
            async with db_pool.connection() as conn:
                result = await conn.execute("SELECT COUNT(*) FROM users")
                count = await result.fetchone()
                assert count[0] == 3, f"Table corrupted after injection attempt: {malicious}"

        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_test_users(conn)

    @pytest.mark.asyncio
    async def test_sql_injection_in_list_operations(self, db_pool):
        """Test SQL injection in IN/NOT IN operations."""
        # Setup
        async with db_pool.connection() as conn:
            await setup_test_users(conn)

        repo = FraiseQLRepository(db_pool)
        UserWhere = safe_create_where_type(User)

        # Try injection via IN operator
        malicious_list = ["user", "admin'; DROP TABLE users; --"]

        where = UserWhere(role={"in": malicious_list})
        sql_where = where.to_sql()

        query = DatabaseQuery(
            statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where,
            params={},
            fetch_result=True,
        )

        results = await repo.run(query)

        # Should only match exact values, not execute injection
        assert all(
            r["data"]["role"] in ["user", "admin"]
            for r in results
        ), "IN operator allowed injection"

        # Verify table integrity
        async with db_pool.connection() as conn:
            count_result = await conn.execute("SELECT COUNT(*) FROM users")
            count = await count_result.fetchone()
            assert count[0] == 3, "Table corrupted via IN operator injection"

        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_test_users(conn)

    @pytest.mark.asyncio
    async def test_sql_injection_with_special_characters(self, db_pool):
        """Test handling of special characters that could be used in injections."""
        # Setup
        async with db_pool.connection() as conn:
            await setup_test_users(conn)

        repo = FraiseQLRepository(db_pool)
        UserWhere = safe_create_where_type(User)

        # Special characters that might be used in injection attempts
        special_inputs = [
            "user\\'; DROP TABLE users; --",  # Backslash
            "user`; DROP TABLE users; --",    # Backtick
            'user"; DROP TABLE users; --',   # Double quote
            "user\n; DROP TABLE users; --",   # Newline
            "user\r\n; DROP TABLE users; --", # CRLF
            "user\x00; DROP TABLE users; --", # Null byte
            "user/*comment*/name",            # SQL comment
        ]

        for special in special_inputs:
            # Use eq operator instead of contains for string comparison
            where = UserWhere(name={"eq": special})
            sql_where = where.to_sql()

            query = DatabaseQuery(
                statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where,
                params={},
                fetch_result=True,
            )

            # Should handle special characters safely
            try:
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
            async with db_pool.connection() as conn:
                count_result = await conn.execute("SELECT COUNT(*) FROM users")
                count = await count_result.fetchone()
                assert count[0] == 3, f"Database corrupted with special character: {special!r}"

        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_test_users(conn)

    @pytest.mark.asyncio
    async def test_verify_parameterization(self, db_pool):
        """Verify that queries are properly parameterized."""
        # Setup
        async with db_pool.connection() as conn:
            await setup_test_users(conn)

        repo = FraiseQLRepository(db_pool)
        UserWhere = safe_create_where_type(User)

        # Create a query with potential injection
        where = UserWhere(
            name={"eq": "Admin'; DROP TABLE users; --"},
            role={"in": ["admin", "user'; DELETE FROM users; --"]},
        )

        sql_where = where.to_sql()
        query = DatabaseQuery(
            statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where,
            params={},
            fetch_result=True,
        )

        # Execute query
        results = await repo.run(query)

        # Query should execute safely with no results
        assert len(results) == 0

        # Verify parameterization by checking that the table still exists
        # and has the correct structure
        async with db_pool.connection() as conn:
            table_check_result = await conn.execute("""
                SELECT COUNT(*)
                FROM information_schema.columns
                WHERE table_name = 'users'
            """)
            table_check = await table_check_result.fetchone()
            assert table_check[0] == 2, "Table structure was modified"

        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_test_users(conn)

    @pytest.mark.asyncio
    async def test_actual_database_execution(self, db_pool):
        """Real integration test that executes against database.

        This replaces the placeholder test in the original SQL injection
        prevention tests with actual database execution.
        """
        # Setup
        async with db_pool.connection() as conn:
            await setup_test_users(conn)

        repo = FraiseQLRepository(db_pool)
        UserWhere = safe_create_where_type(User)

        # Test that normal queries work correctly
        where_normal = UserWhere(name={"eq": "Admin"})
        sql_where = where_normal.to_sql()
        query_normal = DatabaseQuery(
            statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where,
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
            statement=SQL("SELECT id, data FROM users WHERE ").format() + sql_where_injection,
            params={},
            fetch_result=True,
        )

        results_injection = await repo.run(query_injection)
        assert len(results_injection) == 0

        # Verify database is intact
        async with db_pool.connection() as conn:
            verify_result = await conn.execute("""
                SELECT EXISTS (
                    SELECT FROM information_schema.tables
                    WHERE table_name = 'users'
                )
            """)
            exists = await verify_result.fetchone()
            assert exists[0] is True, "Table was dropped via SQL injection"

        # Cleanup
        async with db_pool.connection() as conn:
            await cleanup_test_users(conn)
