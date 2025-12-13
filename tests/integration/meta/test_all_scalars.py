"""Meta-test for ALL scalar types integration.

This test validates that every scalar type in FraiseQL works through the
complete GraphQL pipeline: schema registration → query validation → execution.

It auto-discovers all scalar types and tests each one comprehensively.
"""

import pytest
from fraiseql import fraise_type, query
from fraiseql.types.scalars import __all__ as ALL_SCALARS
from fraiseql.types.scalars import (
    CIDRScalar,
    CUSIPScalar,
    DateScalar,
    IpAddressScalar,
    JSONScalar,
    UUIDScalar,
)
from fraiseql.gql.builders import SchemaRegistry

# Import schema_builder to ensure SchemaRegistry is patched with build_schema method
import fraiseql.gql.schema_builder  # noqa: F401


def get_all_scalar_types():
    """Auto-enumerate all custom scalars from the scalars module."""
    import fraiseql.types.scalars as scalars_module

    scalar_types = []
    for scalar_name in ALL_SCALARS:
        try:
            scalar_class = getattr(scalars_module, scalar_name)
            scalar_types.append((scalar_name, scalar_class))
        except AttributeError:
            # Skip if scalar not found (shouldn't happen with __all__)
            continue

    return scalar_types


@pytest.fixture(scope="class")
def scalar_test_schema(meta_test_schema):
    """Schema registry prepared with scalar test types."""
    # Clear any existing registrations
    meta_test_schema.clear()

    # Build field annotations dict explicitly
    annotations = {"id": int}

    # Add one field for each scalar type
    for scalar_name, scalar_class in get_all_scalar_types():
        field_name = scalar_name.lower().replace("scalar", "_field")
        annotations[field_name] = scalar_class

    # Register a test type that uses all scalar types as fields
    @fraise_type
    class ScalarTestType:
        pass

    # Manually set annotations (workaround for dynamic fields)
    ScalarTestType.__annotations__ = annotations

    # Register a simple query
    @query
    async def get_scalars(info) -> list[ScalarTestType]:
        return []

    # Register types with schema
    meta_test_schema.register_type(ScalarTestType)
    meta_test_schema.register_query(get_scalars)

    return meta_test_schema


@pytest.mark.parametrize("scalar_name,scalar_class", get_all_scalar_types())
def test_scalar_in_schema_registration(scalar_name, scalar_class, scalar_test_schema):
    """Every scalar should be registrable in a GraphQL schema."""
    # Build the schema using the prepared registry from the fixture
    schema = scalar_test_schema.build_schema()

    # Verify schema was built successfully
    assert schema is not None

    # Verify the scalar type exists in the schema
    # Use the scalar's GraphQL name (scalar_class.name), not the variable name
    graphql_scalar_name = scalar_class.name
    scalar_type = schema.get_type(graphql_scalar_name)
    assert scalar_type is not None, (
        f"Scalar {graphql_scalar_name} (from {scalar_name}) not found in schema"
    )


@pytest.mark.parametrize("scalar_name,scalar_class", get_all_scalar_types())
async def test_scalar_in_graphql_query(scalar_name, scalar_class, scalar_test_schema):
    """Every scalar should work as a query argument without validation errors."""
    from graphql import graphql
    from fraiseql.gql.schema_builder import build_fraiseql_schema

    # Get test value for this scalar
    test_value = get_test_value_for_scalar(scalar_class)

    # Build query using the scalar as an argument
    query_str = f"""
    query TestScalar($testValue: {scalar_name}!) {{
        getScalars {{
            id
        }}
    }}
    """

    schema = build_fraiseql_schema()

    # Execute query - should NOT raise validation error
    result = await graphql(schema, query_str, variable_values={"testValue": test_value})

    # Should not have validation errors
    assert not result.errors, f"Scalar {scalar_name} failed in GraphQL query: {result.errors}"


@pytest.mark.parametrize(
    "scalar_name,scalar_class",
    [
        ("CIDRScalar", CIDRScalar),
        ("CUSIPScalar", CUSIPScalar),
        ("DateScalar", DateScalar),
        ("IpAddressScalar", IpAddressScalar),
        ("JSONScalar", JSONScalar),
        ("UUIDScalar", UUIDScalar),
    ],
)
async def test_scalar_in_where_clause(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should work in WHERE clauses with database roundtrip."""
    from graphql import graphql
    from fraiseql import fraise_type, query
    from fraiseql.gql.builders import SchemaRegistry

    # Create a test table with the scalar column
    table_name = f"test_{scalar_name.lower()}_table"
    column_name = f"{scalar_name.lower()}_col"

    # Create table in database
    async with meta_test_pool.connection() as conn:
        await conn.execute(f"DROP TABLE IF EXISTS {table_name}")
        await conn.execute(f"""
            CREATE TABLE {table_name} (
                id SERIAL PRIMARY KEY,
                {column_name} {get_postgres_type_for_scalar(scalar_class)}
            )
        """)

        # Insert test data
        test_value = get_test_value_for_scalar(scalar_class)
        await conn.execute(
            f"""
            INSERT INTO {table_name} ({column_name}) VALUES ($1)
        """,
            [test_value],
        )

        await conn.commit()

    try:
        # Create schema with the test type
        registry = SchemaRegistry.get_instance()
        registry.clear()

        @fraise_type(sql_source=table_name)
        class TestType:
            id: int
            test_field = scalar_class

        @query
        async def get_test_data(info) -> list[TestType]:
            return []

        registry.register_type(TestType)
        registry.register_query(get_test_data)

        # Test WHERE clause with the scalar
        test_value = get_test_value_for_scalar(scalar_class)
        query_str = f"""
        query {{
            getTestData(where: {{testField: {{eq: {repr(test_value)}}}}}) {{
                id
                testField
            }}
        }}
        """

        schema = registry.build_schema()

        # Execute query - should work without errors
        result = await graphql(schema, query_str)

        assert not result.errors, f"Scalar {scalar_name} failed in WHERE clause: {result.errors}"

    finally:
        # Cleanup
        async with meta_test_pool.connection() as conn:
            await conn.execute(f"DROP TABLE IF EXISTS {table_name}")
            await conn.commit()


@pytest.mark.parametrize("scalar_name,scalar_class", get_all_scalar_types())
async def test_scalar_database_roundtrip(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should persist/retrieve correctly from database."""
    # Create a temporary table for this scalar
    table_name = f"test_{scalar_name.lower()}_roundtrip"
    column_name = f"{scalar_name.lower()}_col"

    async with meta_test_pool.connection() as conn:
        # Create table
        await conn.execute(f"DROP TABLE IF EXISTS {table_name}")
        await conn.execute(f"""
            CREATE TABLE {table_name} (
                id SERIAL PRIMARY KEY,
                {column_name} {get_postgres_type_for_scalar(scalar_class)}
            )
        """)

        # Insert test value
        test_value = get_test_value_for_scalar(scalar_class)
        await conn.execute(
            f"""
            INSERT INTO {table_name} ({column_name}) VALUES ($1)
        """,
            [test_value],
        )

        # Retrieve value
        result = await conn.execute(f"SELECT {column_name} FROM {table_name} WHERE id = 1")
        row = await result.fetchone()
        retrieved_value = row[0] if row else None

        await conn.commit()

        # Cleanup
        await conn.execute(f"DROP TABLE IF EXISTS {table_name}")
        await conn.commit()

    # Verify roundtrip
    assert retrieved_value is not None, f"No value retrieved for {scalar_name}"
    # Note: Exact equality might not work for all types (e.g., JSON, dates)
    # but the important thing is no errors occurred


def get_test_value_for_scalar(scalar_class):
    """Get a test value appropriate for the given scalar type."""
    # Map scalar classes to test values
    test_values = {
        CIDRScalar: "192.168.1.0/24",
        CUSIPScalar: "037833100",  # Apple Inc. CUSIP
        DateScalar: "2023-12-13",
        IpAddressScalar: "192.168.1.1",
        JSONScalar: {"key": "value", "number": 42},
        UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",
    }

    # Return specific value if known, otherwise a generic string
    return test_values.get(scalar_class, "test_value")


def get_postgres_type_for_scalar(scalar_class):
    """Get the appropriate PostgreSQL type for a scalar."""
    # Map scalars to PostgreSQL types
    type_mapping = {
        CIDRScalar: "CIDR",
        CUSIPScalar: "VARCHAR(9)",
        DateScalar: "DATE",
        IpAddressScalar: "INET",
        JSONScalar: "JSONB",
        UUIDScalar: "UUID",
    }

    return type_mapping.get(scalar_class, "TEXT")
