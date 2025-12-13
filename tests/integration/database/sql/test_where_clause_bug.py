"""Tests for WHERE clause generation fix in repository.

This test module demonstrates and tests the fix for the WHERE clause
generation bug where GraphQL filters were ignored in SQL queries.
The bug has been fixed by integrating proper WHERE clause generation.
"""

import pytest

from fraiseql.cqrs.repository import CQRSRepository

pytestmark = [pytest.mark.integration, pytest.mark.database]


@pytest.mark.asyncio
class TestWhereClauseHandling:
    """Test cases demonstrating the WHERE clause generation fix."""

    async def test_simple_string_filter_works(self, class_db_pool, test_schema) -> None:
        """Test that string filters with operators now work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

        # Create test data
        await conn.execute(
            """
            CREATE TEMP TABLE test_entities (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                data JSONB
            );
            INSERT INTO test_entities (data) VALUES
            ('{"name": "router-01", "type": "network"}'),
            ('{"name": "switch-02", "type": "network"}'),
            ('{"name": "server-03", "type": "compute"}');

            CREATE TEMP VIEW v_test_entities AS
            SELECT id, data FROM test_entities;
        """
        )

        # This should filter results but currently returns all 3
        results = await repo.select_from_json_view(
            "v_test_entities", where={"name": {"contains": "router"}}
        )

        # FIXED: This now works correctly with WHERE clause generation
        assert len(results) == 1, f"Expected 1 result, got {len(results)}"
        assert "router" in results[0]["name"]

    async def test_network_address_filter_works(self, class_db_pool, test_schema) -> None:
        """Test that network address filters now work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

        # Create test data with IP addresses - use unique table name to avoid conflicts
        import uuid

        table_suffix = str(uuid.uuid4()).replace("-", "_")[:8]
        table_name = f"test_network_devices_{table_suffix}"
        view_name = f"v_test_network_devices_{table_suffix}"

        await conn.execute(
            f"""
            CREATE TEMP TABLE {table_name} (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                data JSONB
            );
            INSERT INTO {table_name} (data) VALUES
            ('{{"hostname": "router-01", "ipAddress": "192.168.1.1"}}'),
            ('{{"hostname": "router-02", "ipAddress": "10.0.0.1"}}'),
            ('{{"hostname": "server-01", "ipAddress": "8.8.8.8"}}');

            CREATE TEMP VIEW {view_name} AS
            SELECT id, data FROM {table_name};
        """
        )

        # Test private IP filter - should return only private IPs
        results = await repo.select_from_json_view(
            view_name, where={"ipAddress": {"isPrivate": True}}
        )

        # FIXED: This now works correctly with network address filtering
        assert len(results) == 2, f"Expected 2 private IPs, got {len(results)}"

    async def test_multiple_operators_work(self, class_db_pool, test_schema) -> None:
        """Test that multiple operators in WHERE clause now work correctly."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

        # Create test data
        await conn.execute(
            """
            CREATE TEMP TABLE test_items (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                data JSONB
            );
            INSERT INTO test_items (data) VALUES
            ('{"name": "item-01", "price": 100, "category": "electronics"}'),
            ('{"name": "item-02", "price": 200, "category": "electronics"}'),
            ('{"name": "item-03", "price": 150, "category": "books"}');

            CREATE TEMP VIEW v_test_items AS
            SELECT id, data FROM test_items;
        """
        )

        # Test multiple conditions - should return electronics items >= 150
        results = await repo.select_from_json_view(
            "v_test_items", where={"category": {"eq": "electronics"}, "price": {"gte": 150}}
        )

        # FIXED: This now works correctly with multiple operator filtering
        assert len(results) == 1, f"Expected 1 result, got {len(results)}"
        assert results[0]["name"] == "item-02"

    async def test_working_simple_equality(self, class_db_pool, test_schema) -> None:
        """Test that simple equality still works (this should pass)."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            repo = CQRSRepository(conn)

        # Create test data
        await conn.execute(
            """
            CREATE TEMP TABLE test_simple (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                data JSONB
            );
            INSERT INTO test_simple (data) VALUES
            ('{"status": "active"}'),
            ('{"status": "inactive"}');

            CREATE TEMP VIEW v_test_simple AS
            SELECT id, data FROM test_simple;
        """
        )

        # Test simple string equality (current implementation)
        results = await repo.select_from_json_view(
            "v_test_simple",
            where={"status": "active"},  # Simple key-value, not operator dict
        )

        # This should work with current implementation
        assert len(results) == 1
        assert results[0]["status"] == "active"

    @pytest.mark.asyncio
    async def test_expected_where_clause_generation(self) -> None:
        """Test what the WHERE clause generation should produce."""
        # This test documents the expected WHERE clause generation behavior

        test_cases = [
            # Test case: string contains
            {
                "where": {"name": {"contains": "router"}},
                "expected_sql": "data->>'name' ILIKE %s",
                "expected_params": ["%router%"],
            },
            # Test case: network address isPrivate
            {
                "where": {"ipAddress": {"isPrivate": True}},
                "expected_sql": "inet(data->>'ipAddress') << '10.0.0.0/8'::inet OR inet(data->>'ipAddress') << '172.16.0.0/12'::inet OR inet(data->>'ipAddress') << '192.168.0.0/16'::inet",
                "expected_params": [],
            },
            # Test case: multiple conditions
            {
                "where": {"category": {"eq": "electronics"}, "price": {"gte": 150}},
                "expected_sql": "data->>'category' = %s AND (data->>'price')::numeric >= %s",
                "expected_params": ["electronics", 150],
            },
        ]

        # This test will be implemented once we have the fix
        # For now, just document the expected behavior
        for _case in test_cases:
            # TODO: Implement actual WHERE clause generation testing
            # when the fix is ready
            pass
