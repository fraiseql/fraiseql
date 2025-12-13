"""Fixtures for meta-integration tests.

Meta-tests validate that ALL components of a category work through the
complete pipeline. They auto-enumerate components and test each one.
"""

import pytest
import psycopg_pool
from fraiseql.gql.builders import SchemaRegistry


@pytest.fixture(scope="class")
async def meta_test_pool(postgres_url):
    """Dedicated pool for meta-tests."""
    pool = psycopg_pool.AsyncConnectionPool(
        postgres_url,
        min_size=1,
        max_size=3,
        timeout=30,
        open=False,
    )
    await pool.open()
    await pool.wait()
    yield pool
    await pool.close()


@pytest.fixture(scope="class")
async def meta_test_schema():
    """Get the schema registry for meta-tests."""
    registry = SchemaRegistry.get_instance()
    return registry
