"""Test for ORDER BY bug reported in printoptim_backend."""

import uuid

import pytest

import fraiseql
from fraiseql.sql import OrderDirection, create_graphql_order_by_input
from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

# Import database fixtures
from tests.database_conftest import *  # noqa: F403


# Test type
@fraiseql.type
class Organization:
    id: uuid.UUID
    name: str
    identifier: str
    is_active: bool = True


# Create OrderBy input
OrganizationOrderBy = create_graphql_order_by_input(Organization)


class TestGraphQLOrderByBug:
    """Test ORDER BY conversion from GraphQL input to SQL."""

    def test_order_by_dict_conversion(self):
        """Test that a dict representing OrderBy input is properly converted."""
        # Simulate what GraphQL would pass - a dict with field: direction mappings
        order_by_dict = {"name": OrderDirection.DESC, "identifier": OrderDirection.ASC}

        # The issue is that this dict doesn't have _to_sql_order_by method
        assert not hasattr(order_by_dict, "_to_sql_order_by")

        # Test direct dict conversion (the fix)
        sql_order_by = _convert_order_by_input_to_sql(order_by_dict)
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2

        # Check SQL generation
        sql_string = sql_order_by.to_sql().as_string(None)
        assert "ORDER BY" in sql_string
        assert "data ->> 'name' DESC" in sql_string
        assert "data ->> 'identifier' ASC" in sql_string

    def test_graphql_string_enum_handling(self):
        """Test handling of GraphQL enum values as strings."""
        # GraphQL might pass enum values as strings
        order_by_dict = {
            "name": "DESC",  # String instead of enum
            "identifier": "ASC",
        }

        # Test that string enums are handled
        sql_order_by = _convert_order_by_input_to_sql(order_by_dict)
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2

        # Verify directions were properly converted
        assert sql_order_by.instructions[0].field == "name"
        assert sql_order_by.instructions[0].direction == "desc"
        assert sql_order_by.instructions[1].field == "identifier"
        assert sql_order_by.instructions[1].direction == "asc"

    def test_nested_dict_order_by(self):
        """Test nested object ordering with dict input."""
        # Test nested ordering
        order_by_dict = {"name": "ASC", "department": {"name": "DESC", "location": {"city": "ASC"}}}

        sql_order_by = _convert_order_by_input_to_sql(order_by_dict)
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 3

        # Check nested paths
        fields = [(i.field, i.direction) for i in sql_order_by.instructions]
        assert ("name", "asc") in fields
        assert ("department.name", "desc") in fields
        assert ("department.location.city", "asc") in fields

    def test_mixed_enum_and_string_values(self):
        """Test mixing enum and string values."""
        order_by_dict = {
            "name": OrderDirection.DESC,  # Enum
            "identifier": "ASC",  # String
        }

        sql_order_by = _convert_order_by_input_to_sql(order_by_dict)
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2

        # Both should work
        assert sql_order_by.instructions[0].direction == "desc"
        assert sql_order_by.instructions[1].direction == "asc"

    def test_none_values_ignored(self):
        """Test that None values are properly ignored."""
        order_by_dict = {
            "name": "DESC",
            "identifier": None,  # Should be ignored
            "is_active": "ASC",
        }

        sql_order_by = _convert_order_by_input_to_sql(order_by_dict)
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2  # Only 2, not 3

        # Only non-None values should be included
        fields = [i.field for i in sql_order_by.instructions]
        assert "name" in fields
        assert "is_active" in fields
        assert "identifier" not in fields

    def test_db_build_query_with_dict_order_by(self):
        """Test that db._build_find_query properly handles dict OrderBy inputs."""
        from fraiseql.db import FraiseQLRepository

        # Create a repository (pool can be None for query building)
        repo = FraiseQLRepository(pool=None)

        # Test with dict order_by (simulating GraphQL input)
        order_by_dict = {"name": "DESC"}

        # Build the query
        query = repo._build_find_query("organization_view", order_by=order_by_dict, limit=10)

        # Verify SQL was generated correctly
        sql_string = query.statement.as_string(None)
        assert "SELECT * FROM" in sql_string
        assert "organization_view" in sql_string
        assert "ORDER BY" in sql_string
        assert "data ->> 'name' DESC" in sql_string
        assert "LIMIT 10" in sql_string

    @pytest.mark.asyncio
    @pytest.mark.database
    async def test_order_by_end_to_end(self, db_connection_committed):
        """Test end-to-end ORDER BY with real database."""
        from fraiseql.db import FraiseQLRepository
        from tests.utils.schema_utils import get_current_schema

        # Get schema for this test
        schema = await get_current_schema(db_connection_committed)

        # Create a simple test table with JSONB data
        await db_connection_committed.execute(
            """
            CREATE TABLE test_orders (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                data JSONB NOT NULL
            )
        """
        )

        # Insert test data
        await db_connection_committed.execute(
            """
            INSERT INTO test_orders (data) VALUES
            (%s::jsonb),
            (%s::jsonb),
            (%s::jsonb)
        """,
            (
                '{"name": "Charlie", "priority": 1}',
                '{"name": "Alice", "priority": 3}',
                '{"name": "Bob", "priority": 2}',
            ),
        )

        # Commit data
        await db_connection_committed.commit()

        # Create a mock pool that uses the same connection
        class MockPool:
            def __init__(self, conn):
                self.conn = conn

            def connection(self):
                # Context manager that yields the connection
                class ConnContext:
                    def __init__(self, conn):
                        self.conn = conn

                    async def __aenter__(self):
                        return self.conn

                    async def __aexit__(self, *args):
                        pass

                return ConnContext(self.conn)

        # Create repository with mock pool
        mock_pool = MockPool(db_connection_committed)
        repo = FraiseQLRepository(mock_pool)

        # Test ORDER BY with dict input
        order_by_dict = {"name": "ASC"}
        results = await repo.find("test_orders", order_by=order_by_dict)

        # Verify ordering
        assert len(results) == 3
        # Debug: print the first result to see structure
        print(f"First result: {results[0]}")

        # FraiseQL might return data in different structure
        # Check if data is nested
        if "data" in results[0]:
            # Data is nested in 'data' field
            assert results[0]["data"]["name"] == "Alice"
            assert results[1]["data"]["name"] == "Bob"
            assert results[2]["data"]["name"] == "Charlie"
        else:
            # Data is flattened
            assert results[0]["name"] == "Alice"
            assert results[1]["name"] == "Bob"
            assert results[2]["name"] == "Charlie"

        # Test multiple field ordering
        order_by_dict = {"priority": "DESC", "name": "ASC"}
        results = await repo.find("test_orders", order_by=order_by_dict)

        # Verify ordering by priority DESC
        if "data" in results[0]:
            assert results[0]["data"]["priority"] == 3  # Alice
            assert results[1]["data"]["priority"] == 2  # Bob
            assert results[2]["data"]["priority"] == 1  # Charlie
        else:
            assert results[0]["priority"] == 3  # Alice
            assert results[1]["priority"] == 2  # Bob
            assert results[2]["priority"] == 1  # Charlie
