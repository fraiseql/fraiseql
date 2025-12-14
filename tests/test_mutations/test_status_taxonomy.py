"""Test status taxonomy in Python → Rust → Python flow."""

import pytest


@pytest.mark.asyncio
async def test_validation_error_detected(db_connection, clear_registry):
    """Test that validation: prefix is detected as error."""
    # Create mutation_response type
    await db_connection.execute("""
        DO $$ BEGIN
            CREATE TYPE mutation_response AS (
                status          text,
                message         text,
                entity_id       text,
                entity_type     text,
                entity          jsonb,
                updated_fields  text[],
                cascade         jsonb,
                metadata        jsonb
            );
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
    """)

    # Create test function that returns validation error
    await db_connection.execute("""
        CREATE FUNCTION test_validation_error(input_data JSONB)
        RETURNS mutation_response AS $$
        BEGIN
            RETURN (
                'failed:validation_error',
                'Invalid email format',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_response;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_validation_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    # Execute via Rust path
    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_validation_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    # Should return error type
    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "validation" in response["data"]["testMutation"]["status"]


@pytest.mark.asyncio
async def test_conflict_error_detected(db_connection, clear_registry):
    """Test that conflict: prefix is detected as error."""
    # Create mutation_response type
    await db_connection.execute("""
        DO $$ BEGIN
            CREATE TYPE mutation_response AS (
                status          text,
                message         text,
                entity_id       text,
                entity_type     text,
                entity          jsonb,
                updated_fields  text[],
                cascade         jsonb,
                metadata        jsonb
            );
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
    """)

    await db_connection.execute("""
        CREATE FUNCTION test_conflict_error(input_data JSONB)
        RETURNS mutation_response AS $$
        BEGIN
            RETURN (
                'conflict:duplicate_email',
                'Email already exists',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_response;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_conflict_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_conflict_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "conflict:" in response["data"]["testMutation"]["status"]


@pytest.mark.asyncio
async def test_noop_returns_success_type(db_connection, clear_registry):
    """Test that noop: prefix returns success type (not error)."""
    # Create mutation_response type
    await db_connection.execute("""
        DO $$ BEGIN
            CREATE TYPE mutation_response AS (
                status          text,
                message         text,
                entity_id       text,
                entity_type     text,
                entity          jsonb,
                updated_fields  text[],
                cascade         jsonb,
                metadata        jsonb
            );
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
    """)

    await db_connection.execute("""
        CREATE FUNCTION test_noop_status(input_data JSONB)
        RETURNS mutation_response AS $$
        BEGIN
            RETURN (
                'noop:duplicate',
                'Already exists',
                '123',
                'TestEntity',
                '{"id": "123"}'::jsonb,
                NULL,
                NULL,
                NULL
            )::mutation_response;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_noop_status")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_noop_status",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    # v1.8.0: noop returns ERROR type with code 422
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert response["data"]["testMutation"]["code"] == 422
    assert response["data"]["testMutation"]["status"].startswith("noop:")


@pytest.mark.asyncio
async def test_timeout_error_detected(db_connection, clear_registry):
    """Test that timeout: prefix is detected as error."""
    # Create mutation_response type
    await db_connection.execute("""
        DO $$ BEGIN
            CREATE TYPE mutation_response AS (
                status          text,
                message         text,
                entity_id       text,
                entity_type     text,
                entity          jsonb,
                updated_fields  text[],
                cascade         jsonb,
                metadata        jsonb
            );
        EXCEPTION
            WHEN duplicate_object THEN null;
        END $$;
    """)

    await db_connection.execute("""
        CREATE FUNCTION test_timeout_error(input_data JSONB)
        RETURNS mutation_response AS $$
        BEGIN
            RETURN (
                'timeout:database_query',
                'Query exceeded timeout',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_response;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_timeout_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_timeout_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "timeout:" in response["data"]["testMutation"]["status"]
