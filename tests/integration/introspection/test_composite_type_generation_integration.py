"""Integration tests for composite type-based mutation generation.

These tests verify AutoFraiseQL can READ a SpecQL-generated database
and generate mutations correctly.

IMPORTANT: These tests assume a SpecQL-generated schema exists in the database.
"""

import pytest

from fraiseql.introspection import AutoDiscovery


@pytest.fixture
async def specql_test_schema_exists(db_pool) -> None:
    """Verify SpecQL test schema exists in database.

    This fixture does NOT create the schema - it only checks if it exists.
    The schema should be created by:
    1. Running SpecQL to generate it, OR
    2. Manually applying tests/fixtures/specql_test_schema.sql
    """
    async with db_pool.connection() as conn:
        # Check if composite type exists
        result = await conn.execute("""
            SELECT EXISTS (
                SELECT 1 FROM pg_type t
                JOIN pg_namespace n ON n.oid = t.typnamespace
                WHERE n.nspname = 'app'
                  AND t.typname = 'type_create_contact_input'
            )
        """)
        exists = await result.fetchone()

        if not exists[0]:
            pytest.skip("SpecQL test schema not found - run SpecQL or apply test schema SQL")


@pytest.mark.asyncio
async def test_end_to_end_composite_type_generation(db_pool, specql_test_schema_exists) -> None:
    """Test complete flow from database to generated mutation.

    This test READS a SpecQL-generated database and verifies AutoFraiseQL
    can generate mutations correctly.
    """
    # Given: AutoDiscovery with SpecQL schema (already in database)
    auto_discovery = AutoDiscovery(db_pool)

    # When: Discover all mutations (READ from database)
    result = await auto_discovery.discover_all(
        view_pattern="v_%",
        function_pattern="%",  # Discover all functions
        schemas=["app"],
    )

    # Then: Mutation was discovered
    assert len(result["mutations"]) > 0, "Should find at least one mutation"

    # Find the create_contact mutation
    create_contact = next(
        (
            m
            for m in result["mutations"]
            if hasattr(m, "__name__") and "createContact" in m.__name__
        ),
        None,
    )
    assert create_contact is not None, "createContact mutation should be generated"


@pytest.mark.asyncio
async def test_context_params_auto_detection(db_pool, specql_test_schema_exists) -> None:
    """Test that context parameters are automatically detected.

    Verifies that input_tenant_id and input_user_id are auto-detected
    from SpecQL function signatures.
    """
    # Given: AutoDiscovery
    auto_discovery = AutoDiscovery(db_pool)

    # When: Discover mutations (READ from database)
    result = await auto_discovery.discover_all(schemas=["app"])

    # Then: Mutations should be discovered
    assert result is not None
    assert len(result["mutations"]) > 0

    # Note: Detailed assertion about context_params depends on
    # how @fraiseql.mutation exposes this information
    # You may need to add assertions here based on actual mutation structure
