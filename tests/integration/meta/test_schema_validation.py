"""Tests for schema validation infrastructure.

Validates that core testing infrastructure components work correctly,
including database connection pools and schema registries.
"""

import pytest


@pytest.mark.asyncio
class TestSchemaValidationInfrastructure:
    """Test core schema validation infrastructure components."""

    async def test_meta_test_pool_fixture_works(self, meta_test_pool):
        """meta_test_pool fixture should provide a working connection pool."""
        # Should be an AsyncConnectionPool
        assert meta_test_pool is not None
        assert hasattr(meta_test_pool, "connection")

        # Should be able to get a connection
        async with meta_test_pool.connection() as conn:
            result = await conn.execute("SELECT 1 as test")
            row = await result.fetchone()
            assert row[0] == 1

    async def test_meta_test_schema_fixture_works(self, meta_test_schema):
        """meta_test_schema fixture should provide schema registry."""
        assert meta_test_schema is not None
        # Should be a SchemaRegistry instance
        from fraiseql.gql.builders import SchemaRegistry

        assert isinstance(meta_test_schema, SchemaRegistry)
