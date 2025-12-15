"""GraphQL-specific test fixtures.

These fixtures provide utilities for testing GraphQL schema generation
and query execution. They follow the explicit pattern from
test_graphql_query_execution_complete.py.

Fixture hierarchy:
- gql_mock_pool: Creates mock pool wrapping db_connection
- gql_context: GraphQL context dict with FraiseQLRepository
- setup_graphql_table: Factory to create JSONB tables/views
- seed_graphql_data: Factory to seed JSONB data
"""

from contextlib import asynccontextmanager
from typing import Any
from unittest.mock import MagicMock

import pytest
import pytest_asyncio


@pytest.fixture
def gql_mock_pool(db_connection):
    """Create a mock pool that wraps db_connection for FraiseQLRepository.

    This follows the pattern from test_graphql_query_execution_complete.py.

    Usage:
        def test_something(gql_mock_pool):
            repo = FraiseQLRepository(pool=gql_mock_pool)
    """
    mock_pool = MagicMock()

    @asynccontextmanager
    async def mock_connection():
        yield db_connection

    mock_pool.connection = mock_connection
    return mock_pool


@pytest.fixture
def gql_context(gql_mock_pool):
    """Create GraphQL context dict with FraiseQLRepository.

    Usage:
        async def test_query(gql_context):
            result = await execute_graphql(schema, query, context_value=gql_context)
    """
    from fraiseql.db import FraiseQLRepository

    return {"db": FraiseQLRepository(pool=gql_mock_pool)}


@pytest_asyncio.fixture
async def setup_graphql_table(db_connection, clear_registry):
    """Factory fixture to create JSONB-backed tables and views.

    Creates:
    - tb_{name}: Table with id (UUID) and data (JSONB)
    - v_{name}: View selecting id and data

    Usage:
        async def test_something(setup_graphql_table):
            await setup_graphql_table("users")
            # Creates tb_users and v_users
    """

    async def _setup(table_name: str, extra_columns: str | None = None):
        columns = "id UUID PRIMARY KEY DEFAULT gen_random_uuid(), data JSONB NOT NULL"
        if extra_columns:
            columns = f"{columns}, {extra_columns}"

        await db_connection.execute(f"""
            DROP TABLE IF EXISTS tb_{table_name} CASCADE;
            DROP VIEW IF EXISTS v_{table_name} CASCADE;

            CREATE TABLE tb_{table_name} ({columns});

            CREATE VIEW v_{table_name} AS
            SELECT id, data FROM tb_{table_name};
        """)

    return _setup


@pytest_asyncio.fixture
async def seed_graphql_data(db_connection):
    """Factory fixture to seed JSONB data into tables.

    Usage:
        async def test_something(setup_graphql_table, seed_graphql_data):
            await setup_graphql_table("users")
            await seed_graphql_data("tb_users", [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25},
            ])
    """
    import json

    async def _seed(table_name: str, records: list[dict[str, Any]]):
        for record in records:
            json_str = json.dumps(record).replace("'", "''")
            await db_connection.execute(f"""
                INSERT INTO {table_name} (data) VALUES ('{json_str}'::jsonb)
            """)

    return _seed
