"""Integration test for unified camelCase conversion path.

This test verifies that mutations and queries both use the same Rust-based
camelCase conversion, ensuring consistency across all GraphQL responses.
"""

import pytest

from fraiseql.mutations.rust_executor import execute_mutation_rust

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
class TestUnifiedCamelCase:
    """Test that mutations use Rust camelCase conversion consistently."""

    @pytest.fixture(scope="class")
    async def setup_test_schema(self, class_db_pool, test_schema, clear_registry_class):
        """Set up test schema with a simple table and function."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")

            # Create mutation_response type
            await conn.execute(
                """
                CREATE TYPE mutation_response AS (
                    status TEXT,
                    message TEXT,
                    entity_id TEXT,
                    entity_type TEXT,
                    entity JSONB,
                    updated_fields TEXT[],
                    cascade JSONB,
                    metadata JSONB
                )
                """
            )

            # Create test table
            await conn.execute(
                """
                CREATE TABLE test_servers (
                    id TEXT PRIMARY KEY,
                    ip_address TEXT NOT NULL,
                    dns_server_name TEXT,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
                """
            )

            # Create mutation function that returns mutation_response
            await conn.execute(
                """
                CREATE OR REPLACE FUNCTION create_test_server(input_data JSONB)
                RETURNS mutation_response AS $$
                DECLARE
                    new_id TEXT;
                    server_data JSONB;
                BEGIN
                    new_id := gen_random_uuid()::TEXT;

                    INSERT INTO test_servers (id, ip_address, dns_server_name)
                    VALUES (
                        new_id,
                        input_data->>'ip_address',
                        input_data->>'dns_server_name'
                    )
                    RETURNING to_jsonb(test_servers.*) INTO server_data;

                    RETURN ROW(
                        'success',
                        'Test server created',
                        new_id,
                        'TestServer',
                        server_data,
                        NULL,
                        NULL,
                        NULL
                    )::mutation_response;
                END;
                $$ LANGUAGE plpgsql;
                """
            )

            await conn.commit()

    async def test_mutation_with_auto_camel_case_true(
        self, db_connection, setup_test_schema, clear_registry
    ):
        """Verify mutation returns camelCase field names when auto_camel_case=True."""
        # Create a mock config with auto_camel_case=True
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=True)

        # Execute mutation directly via rust_executor
        result = await execute_mutation_rust(
            conn=db_connection,
            function_name="create_test_server",
            input_data={"ip_address": "192.168.1.1", "dns_server_name": "test-dns-1"},
            field_name="createTestServer",
            success_type="CreateTestServerSuccess",
            error_type="CreateTestServerError",
            entity_field_name="test_server",  # Pass snake_case - Rust should convert
            entity_type="TestServer",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify no errors
        assert "errors" not in data or data["errors"] is None

        # Verify structure
        assert "data" in data
        assert "createTestServer" in data["data"]
        mutation_result = data["data"]["createTestServer"]

        # Verify Success type
        assert mutation_result["__typename"] == "CreateTestServerSuccess"

        # CRITICAL: Verify entity_field_name was converted to camelCase
        assert "testServer" in mutation_result, "Entity field should be camelCase"
        assert "test_server" not in mutation_result, "Entity field should NOT be snake_case"

        # Verify entity fields are camelCase
        test_server = mutation_result["testServer"]
        assert "id" in test_server
        assert "ipAddress" in test_server, "Field should be camelCase"
        assert "ip_address" not in test_server, "Field should NOT be snake_case"
        assert "dnsServerName" in test_server, "Field should be camelCase"
        assert "dns_server_name" not in test_server, "Field should NOT be snake_case"

        # Verify values
        assert test_server["ipAddress"] == "192.168.1.1"
        assert test_server["dnsServerName"] == "test-dns-1"

    async def test_mutation_with_auto_camel_case_false(
        self, db_connection, setup_test_schema, clear_registry
    ):
        """Verify mutation preserves snake_case when auto_camel_case=False."""
        # Create a mock config with auto_camel_case=False
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=False)

        # Execute mutation directly via rust_executor
        result = await execute_mutation_rust(
            conn=db_connection,
            function_name="create_test_server",
            input_data={"ip_address": "192.168.1.2", "dns_server_name": "test-dns-2"},
            field_name="createTestServer",
            success_type="CreateTestServerSuccess",
            error_type="CreateTestServerError",
            entity_field_name="test_server",  # Pass snake_case
            entity_type="TestServer",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify no errors
        assert "errors" not in data or data["errors"] is None

        # Verify structure
        assert "data" in data
        mutation_result = data["data"]["createTestServer"]

        # CRITICAL: Verify entity_field_name was NOT converted (stays snake_case)
        assert "test_server" in mutation_result, (
            "Entity field should remain snake_case when auto_camel_case=False"
        )
        assert "testServer" not in mutation_result, (
            "Entity field should NOT be camelCase when auto_camel_case=False"
        )

        # Verify entity fields remain snake_case
        test_server = mutation_result["test_server"]
        assert "ip_address" in test_server, "Field should remain snake_case"
        assert "ipAddress" not in test_server, "Field should NOT be camelCase"
        assert "dns_server_name" in test_server, "Field should remain snake_case"
        assert "dnsServerName" not in test_server, "Field should NOT be camelCase"

        # Verify values
        assert test_server["ip_address"] == "192.168.1.2"
        assert test_server["dns_server_name"] == "test-dns-2"

    async def test_mutation_default_config_uses_camel_case(
        self, db_connection, setup_test_schema, clear_registry
    ):
        """Verify mutation defaults to camelCase when no config provided."""
        # Execute mutation without config (should default to auto_camel_case=True)
        result = await execute_mutation_rust(
            conn=db_connection,
            function_name="create_test_server",
            input_data={"ip_address": "192.168.1.3", "dns_server_name": "test-dns-3"},
            field_name="createTestServer",
            success_type="CreateTestServerSuccess",
            error_type="CreateTestServerError",
            entity_field_name="test_server",
            entity_type="TestServer",
            context_args=None,
            cascade_selections=None,
            config=None,  # No config - should default to True
        )

        # Parse response
        data = result.to_json()

        # Verify structure
        mutation_result = data["data"]["createTestServer"]

        # Should be camelCase by default
        assert "testServer" in mutation_result, "Default should be auto_camel_case=True"
        test_server = mutation_result["testServer"]
        assert "ipAddress" in test_server, "Default should convert to camelCase"
        assert "dnsServerName" in test_server, "Default should convert to camelCase"
