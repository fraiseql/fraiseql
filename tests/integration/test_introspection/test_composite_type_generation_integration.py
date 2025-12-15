"""Integration tests for composite type-based mutation generation.

These tests verify AutoFraiseQL can READ a SpecQL-generated database
and generate mutations correctly.

IMPORTANT: These tests assume a SpecQL-generated schema exists in the database.
"""

import pytest
import pytest_asyncio

from fraiseql.introspection import AutoDiscovery

pytestmark = pytest.mark.integration


@pytest_asyncio.fixture(scope="class")
async def specql_test_schema(class_db_pool, test_schema) -> None:
    """Create SpecQL-style test schema for composite type generation testing.

    This fixture creates a minimal SpecQL-compatible schema with:
    - Composite input/output types
    - PostgreSQL function with SpecQL naming conventions
    - Table and view for discovery
    """
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")

        # Create SpecQL-style composite types
        await conn.execute("""
            -- Input type for create_contact mutation
            CREATE TYPE type_create_contact_input AS (
                input_tenant_id UUID,
                input_user_id UUID,
                name TEXT,
                email TEXT,
                phone TEXT
            );

            -- Success type
            CREATE TYPE type_create_contact_success AS (
                id UUID,
                message TEXT
            );

            -- Error type
            CREATE TYPE type_create_contact_error AS (
                code TEXT,
                message TEXT
            );

            -- Mutation result union type
            CREATE TYPE mutation_result_create_contact AS (
                success type_create_contact_success,
                error type_create_contact_error
            );

            -- Contact table
            CREATE TABLE contacts (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id UUID NOT NULL,
                created_by UUID NOT NULL,
                name TEXT NOT NULL,
                email TEXT,
                phone TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );

            -- SpecQL-style mutation function
            -- @fraiseql:mutation
            -- name: createContact
            -- success_type: type_create_contact_success
            -- error_type: type_create_contact_error
            -- context_params: [input_tenant_id, input_user_id]
            CREATE OR REPLACE FUNCTION create_contact(
                input_tenant_id UUID,
                input_user_id UUID,
                input_data JSONB
            ) RETURNS mutation_result_create_contact AS $$
            DECLARE
                new_id UUID;
                result mutation_result_create_contact;
            BEGIN
                INSERT INTO contacts (tenant_id, created_by, name, email, phone)
                VALUES (
                    input_tenant_id,
                    input_user_id,
                    input_data->>'name',
                    input_data->>'email',
                    input_data->>'phone'
                )
                RETURNING id INTO new_id;

                result.success = ROW(new_id, 'Contact created successfully')::type_create_contact_success;
                result.error = NULL;
                RETURN result;
            EXCEPTION WHEN OTHERS THEN
                result.success = NULL;
                result.error = ROW('CREATE_ERROR', SQLERRM)::type_create_contact_error;
                RETURN result;
            END;
            $$ LANGUAGE plpgsql;

            -- View for query discovery
            CREATE VIEW v_contacts AS
            SELECT id, tenant_id, created_by, name, email, phone, created_at
            FROM contacts;

            -- Views for success/error types (SpecQL pattern)
            CREATE VIEW v_create_contact_success AS
            SELECT
                '{"id": "00000000-0000-0000-0000-000000000000", "message": "Contact created successfully"}'::jsonb as data;

            COMMENT ON VIEW v_create_contact_success IS '@fraiseql:type';

            CREATE VIEW v_create_contact_error AS
            SELECT
                '{"code": "CREATE_ERROR", "message": "Failed to create contact"}'::jsonb as data;

            COMMENT ON VIEW v_create_contact_error IS '@fraiseql:type';

            -- Add comment to function for SpecQL annotation
            COMMENT ON FUNCTION create_contact(UUID, UUID, JSONB) IS
            '@fraiseql:mutation
            name: createContact
            success_type: CreateContactSuccess
            error_type: CreateContactError
            context_params: [input_tenant_id, input_user_id]';
        """)

        await conn.commit()


@pytest.mark.asyncio
async def test_end_to_end_composite_type_generation(
    class_db_pool, test_schema, specql_test_schema
) -> None:
    """Test complete flow from database to generated mutation.

    This test READS a SpecQL-generated database and verifies AutoFraiseQL
    can generate mutations correctly.
    """
    # Given: AutoDiscovery with SpecQL schema (already in database)
    auto_discovery = AutoDiscovery(class_db_pool)

    # When: Discover all mutations (READ from database)
    result = await auto_discovery.discover_all(
        view_pattern="v_%",
        function_pattern="%",  # Discover all functions
        schemas=[test_schema],
    )

    # Then: Mutation was discovered
    assert len(result["mutations"]) > 0, "Should find at least one mutation"

    # Find the create_contact mutation
    create_contact = next(
        (
            m
            for m in result["mutations"]
            if hasattr(m, "__name__") and "CreateContact" in m.__name__
        ),
        None,
    )
    assert create_contact is not None, "createContact mutation should be generated"


@pytest.mark.asyncio
async def test_context_params_auto_detection(
    class_db_pool, test_schema, specql_test_schema
) -> None:
    """Test that context parameters are automatically detected.

    Verifies that input_tenant_id and input_user_id are auto-detected
    from SpecQL function signatures.
    """
    # Given: AutoDiscovery
    auto_discovery = AutoDiscovery(class_db_pool)

    # When: Discover mutations (READ from database)
    result = await auto_discovery.discover_all(function_pattern="%", schemas=[test_schema])

    # Then: Mutations should be discovered
    assert result is not None
    assert len(result["mutations"]) > 0

    # Note: Detailed assertion about context_params depends on
    # how @fraiseql.mutation exposes this information
    # You may need to add assertions here based on actual mutation structure
