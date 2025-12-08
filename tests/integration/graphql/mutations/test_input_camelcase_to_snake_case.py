"""Integration test for camelCase → snake_case conversion in mutation inputs.

This test verifies that FraiseQL correctly converts GraphQL input field names
(camelCase) to PostgreSQL field names (snake_case) before calling database functions,
ensuring jsonb_populate_record() works correctly with composite types.
"""

import pytest

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
class TestMutationInputCamelCaseConversion:
    """Test that mutation inputs are converted from camelCase to snake_case."""

    @pytest.fixture(scope="class")
    async def setup_test_schema(self, class_db_pool, test_schema, clear_registry_class) -> None:
        """Set up test schema with composite type and function using jsonb_populate_record."""
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

            # Create composite type with snake_case fields (like PostgreSQL convention)
            await conn.execute(
                """
                CREATE TYPE test_price_input AS (
                    contract_id UUID,
                    contract_item_id UUID,
                    start_date DATE,
                    end_date DATE,
                    amount DOUBLE PRECISION,
                    currency TEXT
                )
                """
            )

            # Create test table
            await conn.execute(
                """
                CREATE TABLE test_prices (
                    id TEXT PRIMARY KEY,
                    contract_id UUID NOT NULL,
                    contract_item_id UUID NOT NULL,
                    start_date DATE NOT NULL,
                    end_date DATE NOT NULL,
                    amount DOUBLE PRECISION NOT NULL,
                    currency TEXT NOT NULL,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
                """
            )

            # Create function that uses jsonb_populate_record
            # This is the pattern that fails without input key conversion
            await conn.execute(
                f"""
                CREATE OR REPLACE FUNCTION {test_schema}.create_test_price(input_payload JSONB)
                RETURNS mutation_response AS $$
                DECLARE
                    v_input test_price_input;
                    new_id TEXT;
                    price_data JSONB;
                BEGIN
                    -- This is the critical line that fails without input conversion
                    -- If input_payload has camelCase keys, all fields will be NULL
                    v_input := jsonb_populate_record(NULL::test_price_input, input_payload);

                    -- Verify that fields were populated
                    IF v_input.contract_id IS NULL THEN
                        RETURN ROW(
                            'failed:validation',
                            'contract_id is NULL - input conversion failed',
                            NULL, NULL, NULL, NULL, NULL, NULL
                        )::mutation_response;
                    END IF;

                    new_id := gen_random_uuid()::TEXT;

                    INSERT INTO test_prices (
                        id, contract_id, contract_item_id,
                        start_date, end_date, amount, currency
                    )
                    VALUES (
                        new_id,
                        v_input.contract_id,
                        v_input.contract_item_id,
                        v_input.start_date,
                        v_input.end_date,
                        v_input.amount,
                        v_input.currency
                    )
                    RETURNING to_jsonb(test_prices.*) INTO price_data;

                    RETURN ROW(
                        'success',
                        'Price created successfully',
                        new_id,
                        'TestPrice',
                        price_data,
                        NULL,
                        NULL,
                        NULL
                    )::mutation_response;
                END;
                $$ LANGUAGE plpgsql;
                """
            )

            await conn.commit()

    async def test_mutation_converts_camelcase_input_to_snake_case(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ) -> None:
        """Verify that camelCase input is converted to snake_case before PostgreSQL call."""
        from types import SimpleNamespace

        # Create a mock config with auto_camel_case=True
        config = SimpleNamespace(auto_camel_case=True)

        # Execute mutation with camelCase input (as GraphQL would send)
        from fraiseql.mutations.rust_executor import execute_mutation_rust

        camelcase_input = {
            "contractId": "11111111-1111-1111-1111-111111111111",
            "contractItemId": "22222222-2222-2222-2222-222222222222",
            "startDate": "2025-02-15",
            "endDate": "2025-12-15",
            "amount": 99.99,
            "currency": "EUR",
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.create_test_price",
            input_data=camelcase_input,
            field_name="createTestPrice",
            success_type="CreateTestPriceSuccess",
            error_type="CreateTestPriceError",
            entity_field_name="price",
            entity_type="TestPrice",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify no errors
        assert "errors" not in data or data["errors"] is None

        # Verify success
        mutation_result = data["data"]["createTestPrice"]
        assert mutation_result["__typename"] == "CreateTestPriceSuccess"
        assert mutation_result["message"] == "Price created successfully"

        # CRITICAL: Verify that the price entity was populated correctly
        # If input conversion didn't work, all fields would be NULL and the
        # function would have returned an error
        price = mutation_result["price"]
        assert price["contractId"] == "11111111-1111-1111-1111-111111111111"
        assert price["contractItemId"] == "22222222-2222-2222-2222-222222222222"
        assert price["startDate"] == "2025-02-15"
        assert price["endDate"] == "2025-12-15"
        assert price["amount"] == 99.99
        assert price["currency"] == "EUR"

    async def test_mutation_preserves_snake_case_when_auto_camel_case_false(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ) -> None:
        """Verify that snake_case input is NOT converted when auto_camel_case=False."""
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=False)

        from fraiseql.mutations.rust_executor import execute_mutation_rust

        # Send snake_case input (as if auto_camel_case is disabled)
        snake_case_input = {
            "contract_id": "33333333-3333-3333-3333-333333333333",
            "contract_item_id": "44444444-4444-4444-4444-444444444444",
            "start_date": "2025-03-01",
            "end_date": "2025-11-30",
            "amount": 149.99,
            "currency": "USD",
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.create_test_price",
            input_data=snake_case_input,
            field_name="createTestPrice",
            success_type="CreateTestPriceSuccess",
            error_type="CreateTestPriceError",
            entity_field_name="price",
            entity_type="TestPrice",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify success (snake_case input should work directly)
        mutation_result = data["data"]["createTestPrice"]
        assert mutation_result["__typename"] == "CreateTestPriceSuccess"

        # Verify entity fields remain snake_case (auto_camel_case=False)
        price = mutation_result["price"]
        assert "contract_id" in price
        assert "contract_item_id" in price
        assert price["contract_id"] == "33333333-3333-3333-3333-333333333333"

    async def test_nested_input_objects_are_converted(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ) -> None:
        """Verify that nested objects in input are also converted."""
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=True)

        from fraiseql.mutations.rust_executor import execute_mutation_rust

        # Create function that accepts nested input
        async with db_connection.cursor() as cursor:
            await cursor.execute(
                f"""
                CREATE OR REPLACE FUNCTION {test_schema}.test_nested_input(input_payload JSONB)
                RETURNS mutation_response AS $$
                DECLARE
                    contract_id_value TEXT;
                BEGIN
                    -- Access nested field using snake_case path
                    contract_id_value := input_payload->'nested_data'->>'contract_id';

                    RETURN ROW(
                        'success',
                        'Nested input processed',
                        contract_id_value,
                        'NestedTest',
                        jsonb_build_object('result', contract_id_value),
                        NULL, NULL, NULL
                    )::mutation_response;
                END;
                $$ LANGUAGE plpgsql;
                """
            )
            await db_connection.commit()

        # Send nested camelCase input
        nested_input = {
            "nestedData": {
                "contractId": "55555555-5555-5555-5555-555555555555",
                "startDate": "2025-01-01",
            }
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.test_nested_input",
            input_data=nested_input,
            field_name="testNestedInput",
            success_type="TestNestedSuccess",
            error_type="TestNestedError",
            entity_field_name="result",
            entity_type="NestedTest",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        data = result.to_json()
        mutation_result = data["data"]["testNestedInput"]

        # Verify nested conversion worked
        assert mutation_result["__typename"] == "TestNestedSuccess"
        # The function returns entity_id as the id field in the response
        assert mutation_result["id"] == "55555555-5555-5555-5555-555555555555"
