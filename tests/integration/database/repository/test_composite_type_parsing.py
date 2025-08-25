"""Test PostgreSQL composite type parsing in FraiseQL.

This test reproduces the issue where PostgreSQL composite types
are returned as string representations instead of structured objects,
preventing ALWAYS_DATA_CONFIG from working properly.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.db import DatabaseQuery, FraiseQLRepository
from fraiseql.fastapi.app import create_db_pool


class TestCompositeTypeParsing:
    """Test that PostgreSQL composite types are properly parsed."""

    @pytest.mark.asyncio
    async def test_composite_type_returned_as_string_without_registration(self, postgres_url):
        """RED TEST: This test should FAIL initially.

        Demonstrates the problem: PostgreSQL composite types are returned
        as string representations instead of structured objects.
        """
        # Create a pool without composite type registration (current behavior)
        pool = await create_db_pool(postgres_url)
        repo = FraiseQLRepository(pool=pool)

        try:
            # Create the composite type and function for testing
            await self._setup_test_schema(repo)

            # Call a function that returns a composite type
            result = await repo.execute_function(
                "test_schema.test_mutation_function",
                {
                    "entity_id": "123e4567-e89b-12d3-a456-426614174000",
                    "status": "noop:not_found",
                    "message": "Test message",
                },
            )

            # The problem: result should be a structured object but comes back as string

            # This assertion should FAIL because the composite type isn't parsed
            # The result will be something like: "('123...',{},'noop:not_found','Test message',null,{})"
            assert isinstance(result, dict), f"Expected dict, got {type(result)}"

            # Analyze the actual result structure

            # The function name might not be included in the result for single composite types
            # Let's check if the composite type fields are directly in the result
            if "status" in result:
                # Direct composite type fields - this means it's working!
                status = result["status"]
                assert status == "noop:not_found", f"Expected 'noop:not_found', got {status}"
            else:
                # Check if it's wrapped under function name
                mutation_result = result.get("test_mutation_function")
                if mutation_result is not None:
                    if isinstance(mutation_result, (tuple, list)):
                        status = mutation_result[2] if len(mutation_result) > 2 else None
                        assert status == "noop:not_found", (
                            f"Expected 'noop:not_found', got {status}"
                        )
                    elif isinstance(mutation_result, dict) and "status" in mutation_result:
                        status = mutation_result["status"]
                        assert status == "noop:not_found", (
                            f"Expected 'noop:not_found', got {status}"
                        )
                    elif hasattr(mutation_result, "status"):
                        assert mutation_result.status == "noop:not_found"
                    else:
                        pytest.fail(
                            f"Cannot access status from: {type(mutation_result)} = {mutation_result}"
                        )
                else:
                    pytest.fail(f"No composite result found in: {result}")

        finally:
            await pool.close()

    async def _setup_test_schema(self, repo: FraiseQLRepository):
        """Set up test schema with composite type and function."""
        # Create test schema
        await repo.run(
            DatabaseQuery(
                statement=SQL("CREATE SCHEMA IF NOT EXISTS test_schema;"),
                params={},
                fetch_result=False,
            )
        )

        # Create a composite type similar to FraiseQL's app.mutation_result
        await repo.run(
            DatabaseQuery(
                statement=SQL("""
                DROP TYPE IF EXISTS test_schema.test_mutation_result CASCADE;
                CREATE TYPE test_schema.test_mutation_result AS (
                    id UUID,
                    updated_fields TEXT[],
                    status TEXT,
                    message TEXT,
                    object_data JSONB,
                    extra_metadata JSONB
                );
            """),
                params={},
                fetch_result=False,
            )
        )

        # Create a function that returns the composite type
        await repo.run(
            DatabaseQuery(
                statement=SQL("""
                CREATE OR REPLACE FUNCTION test_schema.test_mutation_function(
                    input_data JSONB
                ) RETURNS test_schema.test_mutation_result
                LANGUAGE plpgsql AS $$
                DECLARE
                    result test_schema.test_mutation_result;
                BEGIN
                    result := (
                        (input_data->>'entity_id')::UUID,
                        ARRAY['test_field'],
                        input_data->>'status',
                        input_data->>'message',
                        '{"test": "data"}'::JSONB,
                        '{"extra": true}'::JSONB
                    );

                    RETURN result;
                END;
                $$;
            """),
                params={},
                fetch_result=False,
            )
        )

    @pytest.mark.asyncio
    async def test_composite_type_parsing_for_always_data_config(self, postgres_url):
        """RED TEST: This test should FAIL initially.

        Tests the specific use case for ALWAYS_DATA_CONFIG:
        - Function returns composite type with status field
        - ALWAYS_DATA_CONFIG needs to check if status contains error indicators
        - Current problem: status field is not accessible
        """
        pool = await create_db_pool(postgres_url)
        repo = FraiseQLRepository(pool=pool)

        try:
            await self._setup_test_schema(repo)

            # Test various status types that ALWAYS_DATA_CONFIG needs to detect
            test_cases = [
                {"status": "success", "expected_is_error": False},
                {"status": "noop:not_found", "expected_is_error": True},
                {"status": "error:validation_failed", "expected_is_error": True},
                {"status": "noop:no_changes", "expected_is_error": True},
            ]

            for case in test_cases:
                result = await repo.execute_function(
                    "test_schema.test_mutation_function",
                    {
                        "entity_id": "123e4567-e89b-12d3-a456-426614174000",
                        "status": case["status"],
                        "message": f"Test {case['status']}",
                    },
                )

                # The PostgreSQL composite type is returned directly as a dict
                # The key test: can ALWAYS_DATA_CONFIG logic work?
                if isinstance(result, dict) and "status" in result:
                    status = result["status"]
                else:
                    pytest.fail(
                        f"Cannot access status field from result: {type(result)} = {result}"
                    )

                # Simulate ALWAYS_DATA_CONFIG logic
                is_error = status and ("noop:" in status or "error:" in status)

                assert is_error == case["expected_is_error"], (
                    f"ALWAYS_DATA_CONFIG logic failed for status '{case['status']}': "
                    f"expected is_error={case['expected_is_error']}, got {is_error}"
                )

        finally:
            await pool.close()

    @pytest.mark.asyncio
    async def test_multiple_composite_types_registration(self, postgres_url):
        """RED TEST: This test should FAIL initially.

        Tests that FraiseQL can handle multiple composite types in one database,
        which is common in real applications like FraiseQL Backend.
        """
        pool = await create_db_pool(postgres_url)
        repo = FraiseQLRepository(pool=pool)

        try:
            # Create multiple composite types
            await repo.run(
                DatabaseQuery(
                    statement=SQL("""
                    CREATE SCHEMA IF NOT EXISTS test_schema;

                    DROP TYPE IF EXISTS test_schema.validation_result CASCADE;
                    CREATE TYPE test_schema.validation_result AS (
                        is_valid BOOLEAN,
                        errors TEXT[],
                        warnings TEXT[]
                    );

                    DROP TYPE IF EXISTS test_schema.audit_result CASCADE;
                    CREATE TYPE test_schema.audit_result AS (
                        action TEXT,
                        timestamp TIMESTAMPTZ,
                        metadata JSONB
                    );
                """),
                    params={},
                    fetch_result=False,
                )
            )

            # Create functions returning different composite types
            await repo.run(
                DatabaseQuery(
                    statement=SQL("""
                    CREATE OR REPLACE FUNCTION test_schema.validate_data(input JSONB)
                    RETURNS test_schema.validation_result
                    LANGUAGE plpgsql AS $$
                    BEGIN
                        RETURN (false, ARRAY['field_required'], ARRAY['deprecated_field']);
                    END;
                    $$;

                    CREATE OR REPLACE FUNCTION test_schema.audit_action(input JSONB)
                    RETURNS test_schema.audit_result
                    LANGUAGE plpgsql AS $$
                    BEGIN
                        RETURN ROW('CREATE'::TEXT, NOW(), '{"user_id": "test"}'::JSONB)::test_schema.audit_result;
                    END;
                    $$;
                """),
                    params={},
                    fetch_result=False,
                )
            )

            # Test validation_result composite type
            validation_result = await repo.execute_function(
                "test_schema.validate_data", {"test": "data"}
            )

            # PostgreSQL composite type should be returned as a dict
            if isinstance(validation_result, dict) and "is_valid" in validation_result:
                is_valid = validation_result["is_valid"]
                errors = validation_result["errors"]
                assert is_valid is False
                assert "field_required" in errors
            else:
                pytest.fail(
                    f"validation_result not parsed correctly: {type(validation_result)} = {validation_result}"
                )

            # Test audit_result composite type
            audit_result = await repo.execute_function(
                "test_schema.audit_action", {"input": {"action": "test"}}
            )

            # PostgreSQL composite type should be returned as a dict
            if isinstance(audit_result, dict) and "action" in audit_result:
                action = audit_result["action"]
                assert action == "CREATE"
            else:
                pytest.fail(
                    f"audit_result not parsed correctly: {type(audit_result)} = {audit_result}"
                )

        finally:
            await pool.close()
