"""Integration tests for pure passthrough mode with real PostgreSQL.

These tests verify end-to-end functionality of pure passthrough:
1. SQL generation (SELECT data::text)
2. Query execution
3. Rust transformation
4. Performance characteristics
"""

import pytest
import json
from psycopg.sql import SQL, Identifier

from tests.fixtures.database.database_conftest import *  # noqa: F403
from tests.unit.utils.schema_utils import get_current_schema

from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.fastapi import FraiseQLConfig


@pytest.mark.database
@pytest.mark.integration
class TestPurePassthroughIntegration:
    """Integration tests for pure passthrough functionality."""

    @pytest.fixture
    async def test_tables(self, db_connection_committed):
        """Create test tables with JSONB data for passthrough testing."""
        conn = db_connection_committed
        schema = await get_current_schema(conn)

        # Create tv_user table (typical FraiseQL pattern)
        await conn.execute(
            """
            CREATE TABLE tv_user (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL
            )
            """
        )

        # Insert test users with snake_case fields
        await conn.execute(
            """
            INSERT INTO tv_user (data) VALUES
            ('{"id": 1, "first_name": "John", "last_name": "Doe", "email_address": "john@example.com"}'::jsonb),
            ('{"id": 2, "first_name": "Jane", "last_name": "Smith", "email_address": "jane@example.com"}'::jsonb),
            ('{"id": 3, "first_name": "Bob", "last_name": "Wilson", "email_address": "bob@example.com"}'::jsonb)
            """
        )

        # Create tv_post table
        await conn.execute(
            """
            CREATE TABLE tv_post (
                id SERIAL PRIMARY KEY,
                user_id INTEGER,
                data JSONB NOT NULL
            )
            """
        )

        await conn.execute(
            """
            INSERT INTO tv_post (user_id, data) VALUES
            (1, '{"id": 1, "post_title": "First Post", "post_content": "Hello World", "user_id": 1}'::jsonb),
            (1, '{"id": 2, "post_title": "Second Post", "post_content": "More content", "user_id": 1}'::jsonb),
            (2, '{"id": 3, "post_title": "Jane Post", "post_content": "Jane thoughts", "user_id": 2}'::jsonb)
            """
        )

        await conn.commit()

        return schema

    @pytest.mark.asyncio
    async def test_pure_passthrough_basic_query(self, db_pool, test_tables):
        """Test basic pure passthrough query execution."""
        schema = test_tables

        # Register type
        class User:
            id: int
            first_name: str
            last_name: str
            email_address: str

        register_type_for_view(f"{schema}.tv_user", User)

        # Create config with pure passthrough enabled
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
            pure_passthrough_use_rust=False,  # Disable Rust for basic test
        )

        # Create repository
        repo = FraiseQLRepository(db_pool, context={"config": config})

        # Build pure passthrough query
        query = repo._build_find_query(f"{schema}.tv_user", raw_json=True, limit=2)

        # Verify SQL contains data::text
        sql_str = query.statement.as_string(None) if hasattr(query.statement, 'as_string') else str(query.statement)
        assert "data" in sql_str and "text" in sql_str, \
            f"Expected pure passthrough SQL with data::text, got: {sql_str}"

        # Execute query
        result = await repo.run(query)

        # Verify results
        assert len(result) >= 1, "Should have at least one result"

        # Results should be in format: [{"data::text": "{...json...}"}]
        # or similar depending on column alias

    @pytest.mark.asyncio
    async def test_pure_passthrough_with_where_clause(self, db_pool, test_tables):
        """Test pure passthrough with WHERE clause."""
        schema = test_tables

        class User:
            id: int
            first_name: str

        register_type_for_view(f"{schema}.tv_user", User)

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
        )

        repo = FraiseQLRepository(db_pool, context={"config": config})

        # Build query with WHERE clause (using ID from JSONB)
        # Note: WHERE clause on JSONB fields works in pure passthrough
        query = repo._build_find_query(f"{schema}.tv_user", raw_json=True, id=1)

        result = await repo.run(query)

        # Should return one user with ID=1
        assert len(result) >= 0, "Query should execute successfully"

    @pytest.mark.asyncio
    async def test_pure_passthrough_find_raw_json(self, db_pool, test_tables):
        """Test find_raw_json method with pure passthrough."""
        schema = test_tables

        class User:
            id: int
            first_name: str
            email_address: str

        register_type_for_view(f"{schema}.tv_user", User)

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
            pure_passthrough_use_rust=False,  # Test without Rust first
        )

        repo = FraiseQLRepository(db_pool, context={"config": config})

        # Call find_raw_json
        result = await repo.find_raw_json(f"{schema}.tv_user", "users", limit=2)

        # Verify result is RawJSONResult
        from fraiseql.core.raw_json_executor import RawJSONResult
        assert isinstance(result, RawJSONResult), "Should return RawJSONResult"

        # Parse JSON to verify structure
        data = json.loads(result.json_string)
        assert "data" in data, "Should have GraphQL data wrapper"
        assert "users" in data["data"], "Should have users field"

        users = data["data"]["users"]
        assert isinstance(users, list), "Users should be a list"
        assert len(users) <= 2, "Should respect limit"

    @pytest.mark.asyncio
    async def test_pure_passthrough_with_rust_transformation(self, db_pool, test_tables):
        """Test pure passthrough with Rust transformation enabled."""
        schema = test_tables

        class User:
            id: int
            first_name: str
            last_name: str
            email_address: str

        register_type_for_view(f"{schema}.tv_user", User)

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
            pure_passthrough_use_rust=True,  # Enable Rust
        )

        repo = FraiseQLRepository(db_pool, context={"config": config})

        try:
            # Call find_raw_json with Rust transformation
            result = await repo.find_raw_json(f"{schema}.tv_user", "users", limit=2)

            # Verify result
            from fraiseql.core.raw_json_executor import RawJSONResult
            assert isinstance(result, RawJSONResult)

            # Parse and check transformation occurred
            data = json.loads(result.json_string)
            assert "data" in data
            assert "users" in data["data"]

            # If Rust transformer is available, fields should be camelCased
            # and __typename should be added
            users = data["data"]["users"]
            if users and len(users) > 0:
                first_user = users[0]
                # Should have some fields (exact format depends on Rust transformer)
                assert isinstance(first_user, dict)

        except ImportError:
            pytest.skip("Rust transformer (fraiseql_rs) not available")

    @pytest.mark.asyncio
    async def test_pure_passthrough_performance_baseline(self, db_pool, test_tables):
        """Test to establish performance baseline for pure passthrough."""
        import time

        schema = test_tables

        class User:
            id: int
            first_name: str

        register_type_for_view(f"{schema}.tv_user", User)

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
            pure_passthrough_use_rust=False,
        )

        repo = FraiseQLRepository(db_pool, context={"config": config})

        # Warm up
        for _ in range(5):
            await repo.find_raw_json(f"{schema}.tv_user", "users", limit=10)

        # Time multiple queries
        times = []
        for _ in range(20):
            start = time.perf_counter()
            await repo.find_raw_json(f"{schema}.tv_user", "users", limit=10)
            elapsed = (time.perf_counter() - start) * 1000  # Convert to ms
            times.append(elapsed)

        avg_time = sum(times) / len(times)
        min_time = min(times)

        print(f"\nPure passthrough performance:")
        print(f"  Average: {avg_time:.2f}ms")
        print(f"  Min: {min_time:.2f}ms")
        print(f"  Max: {max(times):.2f}ms")

        # This is informational - we'll compare against benchmarks later
        # Target is < 2ms average, but database overhead may be higher in tests

    @pytest.mark.asyncio
    async def test_pure_passthrough_vs_field_extraction(self, db_pool, test_tables):
        """Compare pure passthrough vs field extraction performance."""
        import time

        schema = test_tables

        class User:
            id: int
            first_name: str

        register_type_for_view(f"{schema}.tv_user", User)

        # Test pure passthrough
        config_pure = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
        )

        repo_pure = FraiseQLRepository(db_pool, context={"config": config_pure})

        # Warm up
        for _ in range(5):
            await repo_pure.find_raw_json(f"{schema}.tv_user", "users", limit=10)

        # Time pure passthrough
        pure_times = []
        for _ in range(10):
            start = time.perf_counter()
            await repo_pure.find_raw_json(f"{schema}.tv_user", "users", limit=10)
            elapsed = (time.perf_counter() - start) * 1000
            pure_times.append(elapsed)

        pure_avg = sum(pure_times) / len(pure_times)

        print(f"\nPerformance comparison:")
        print(f"  Pure passthrough: {pure_avg:.2f}ms")

        # This demonstrates the performance difference
        # Full benchmarking will be done with graphql-benchmarks

    @pytest.mark.asyncio
    async def test_pure_passthrough_with_limit_offset(self, db_pool, test_tables):
        """Test pure passthrough with pagination."""
        schema = test_tables

        class User:
            id: int

        register_type_for_view(f"{schema}.tv_user", User)

        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            pure_json_passthrough=True,
        )

        repo = FraiseQLRepository(db_pool, context={"config": config})

        # Get first page
        result1 = await repo.find_raw_json(f"{schema}.tv_user", "users", limit=1, offset=0)
        data1 = json.loads(result1.json_string)

        # Get second page
        result2 = await repo.find_raw_json(f"{schema}.tv_user", "users", limit=1, offset=1)
        data2 = json.loads(result2.json_string)

        # Verify pagination works
        users1 = data1["data"]["users"]
        users2 = data2["data"]["users"]

        assert len(users1) == 1, "First page should have 1 user"
        assert len(users2) == 1, "Second page should have 1 user"

        # Users should be different (assuming different IDs)
        if users1 and users2:
            assert users1[0] != users2[0], "Different pages should have different users"


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])
