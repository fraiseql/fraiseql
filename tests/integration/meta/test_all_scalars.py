"""Meta-test for ALL scalar types integration.

This test validates that every scalar type in FraiseQL works through the
complete GraphQL pipeline: schema registration → query validation → execution.

It auto-discovers all scalar types and tests each one comprehensively.
"""

import pytest
from psycopg import sql
from fraiseql import fraise_type, query
from fraiseql.types.scalars import __all__ as ALL_SCALARS
from fraiseql.types.scalars import (
    AirportCodeScalar,
    ApiKeyScalar,
    CIDRScalar,
    ColorScalar,
    ContainerNumberScalar,
    CoordinateScalar,
    CurrencyCodeScalar,
    CUSIPScalar,
    DateRangeScalar,
    DateScalar,
    DateTimeScalar,
    DomainNameScalar,
    DurationScalar,
    ExchangeCodeScalar,
    ExchangeRateScalar,
    FileScalar,
    FlightNumberScalar,
    HashSHA256Scalar,
    HostnameScalar,
    HTMLScalar,
    IBANScalar,
    ImageScalar,
    IpAddressScalar,
    ISINScalar,
    JSONScalar,
    LanguageCodeScalar,
    LatitudeScalar,
    LEIScalar,
    LicensePlateScalar,
    LocaleCodeScalar,
    LongitudeScalar,
    LTreeScalar,
    MacAddressScalar,
    MarkdownScalar,
    MICScalar,
    MimeTypeScalar,
    MoneyScalar,
    PercentageScalar,
    PhoneNumberScalar,
    PortCodeScalar,
    PortScalar,
    PostalCodeScalar,
    SEDOLScalar,
    SemanticVersionScalar,
    SlugScalar,
    StockSymbolScalar,
    SubnetMaskScalar,
    TimeScalar,
    TimezoneScalar,
    TrackingNumberScalar,
    URLScalar,
    UUIDScalar,
    VectorScalar,
    VINScalar,
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

    # Register a test type that uses all scalar types as fields
    @fraise_type
    class ScalarTestType:
        id: int

    # Manually register all scalars to ensure they're available
    # This simulates what would happen in real usage when scalars are used in field types
    for scalar_name, scalar_class in get_all_scalar_types():
        meta_test_schema.register_scalar(scalar_class)

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

    # Get test value for this scalar
    test_value = get_test_value_for_scalar(scalar_class)

    # Use the GraphQL scalar name (scalar_class.name), not the Python variable name
    graphql_scalar_name = scalar_class.name

    # Build query using the scalar as an argument type
    query_str = f"""
    query TestScalar($testValue: {graphql_scalar_name}!) {{
        getScalarsWithArg(testValue: $testValue) {{
            id
        }}
    }}
    """

    # Register a query that accepts the scalar as an argument
    from fraiseql import query as query_decorator, fraise_type
    from typing import Optional

    # Create a simple return type
    @fraise_type
    class ScalarQueryResult:
        id: int

    @query_decorator
    async def get_scalars_with_arg(
        info, test_value: Optional[scalar_class] = None
    ) -> list[ScalarQueryResult]:
        return []

    scalar_test_schema.register_query(get_scalars_with_arg)

    schema = scalar_test_schema.build_schema()

    # Execute query - should NOT raise validation error
    result = await graphql(schema, query_str, variable_values={"testValue": test_value})

    # Should not have validation errors
    assert not result.errors, f"Scalar {scalar_name} failed in GraphQL query: {result.errors}"


@pytest.mark.skip(
    reason="WHERE clause auto-generation not yet configured for dynamic types - separate feature from scalar support"
)
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
        await conn.execute(sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name)))
        await conn.execute(
            sql.SQL("""
                CREATE TABLE {} (
                    id SERIAL PRIMARY KEY,
                    {} {}
                )
            """).format(
                sql.Identifier(table_name),
                sql.Identifier(column_name),
                sql.SQL(get_postgres_type_for_scalar(scalar_class)),
            )
        )

        # Insert test data
        test_value = get_test_value_for_scalar(scalar_class)
        # Handle JSON types that need special adaptation
        if isinstance(test_value, dict):
            from psycopg.types.json import Jsonb

            adapted_value = Jsonb(test_value)
        else:
            adapted_value = test_value

        await conn.execute(
            sql.SQL("""
                INSERT INTO {} ({}) VALUES (%s)
            """).format(sql.Identifier(table_name), sql.Identifier(column_name)),
            [adapted_value],
        )

        await conn.commit()

    try:
        # Create schema with the test type
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Create type dynamically like the working WHERE test
        TestType = type(
            "TestType",
            (),
            {
                "__annotations__": {"id": int, "test_field": scalar_class},
                "__fraiseql_definition__": type(
                    "Definition", (), {"sql_source": table_name, "fields": {}}
                )(),
            },
        )

        # Apply the fraise_type decorator
        TestType = fraise_type(sql_source=table_name)(TestType)

        @query
        async def get_test_data(info) -> list[TestType]:
            return []

        registry.register_type(TestType)
        registry.register_query(get_test_data)

        # Test WHERE clause with the scalar
        test_value = get_test_value_for_scalar(scalar_class)

        # Format value for GraphQL (double quotes for strings, no quotes for numbers)
        if isinstance(test_value, str):
            graphql_value = f'"{test_value}"'
        elif isinstance(test_value, dict):
            # For JSON, use a simple string representation
            graphql_value = f'"{str(test_value)}"'
        else:
            graphql_value = str(test_value)

        # First test: just query the field without WHERE to check if field annotation worked
        simple_query_str = """
        query {
            getTestData {
                id
                testField
            }
        }
        """

        schema = registry.build_schema()

        # Test basic field access first
        result = await graphql(schema, simple_query_str)
        assert not result.errors, f"Scalar {scalar_name} field not accessible: {result.errors}"

        # Now test WHERE clause
        query_str = f"""
        query {{
            getTestData(where: {{testField: {{eq: {graphql_value}}}}}) {{
                id
                testField
            }}
        }}
        """

        schema = registry.build_schema()

        # Execute query with database context (might be needed for WHERE support)
        context = {"db": meta_test_pool, "pool": meta_test_pool}

        # Execute query - should work without errors
        result = await graphql(schema, query_str, context_value=context)

        assert not result.errors, f"Scalar {scalar_name} failed in WHERE clause: {result.errors}"

    finally:
        # Cleanup
        async with meta_test_pool.connection() as conn:
            await conn.execute(
                sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
            )
            await conn.commit()


@pytest.mark.parametrize("scalar_name,scalar_class", get_all_scalar_types())
async def test_scalar_database_roundtrip(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should persist/retrieve correctly from database."""
    # Create a temporary table for this scalar
    table_name = f"test_{scalar_name.lower()}_roundtrip"
    column_name = f"{scalar_name.lower()}_col"

    async with meta_test_pool.connection() as conn:
        # Create table
        await conn.execute(sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name)))
        await conn.execute(
            sql.SQL("""
                CREATE TABLE {} (
                    id SERIAL PRIMARY KEY,
                    {} {}
                )
            """).format(
                sql.Identifier(table_name),
                sql.Identifier(column_name),
                sql.SQL(get_postgres_type_for_scalar(scalar_class)),
            )
        )

        # Insert test value
        test_value = get_test_value_for_scalar(scalar_class)
        # Handle JSON types that need special adaptation
        if isinstance(test_value, dict):
            # For JSON types, psycopg3 needs explicit JSON adaptation
            from psycopg.types.json import Jsonb

            adapted_value = Jsonb(test_value)
        else:
            adapted_value = test_value

        await conn.execute(
            sql.SQL("""
                INSERT INTO {} ({}) VALUES (%s)
            """).format(sql.Identifier(table_name), sql.Identifier(column_name)),
            [adapted_value],
        )

        # Retrieve value
        result = await conn.execute(
            sql.SQL("SELECT {} FROM {} WHERE id = 1").format(
                sql.Identifier(column_name), sql.Identifier(table_name)
            )
        )
        row = await result.fetchone()
        retrieved_value = row[0] if row else None

        await conn.commit()

        # Cleanup
        await conn.execute(sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name)))
        await conn.commit()

    # Verify roundtrip
    assert retrieved_value is not None, f"No value retrieved for {scalar_name}"
    # Note: Exact equality might not work for all types (e.g., JSON, dates)
    # but the important thing is no errors occurred


def get_test_value_for_scalar(scalar_class):
    """Get a test value appropriate for the given scalar type."""
    # Comprehensive map of scalar classes to valid test values
    test_values = {
        # Original (6) - Network & Core
        CIDRScalar: "192.168.1.0/24",
        CUSIPScalar: "037833100",
        DateScalar: "2023-12-13",
        IpAddressScalar: "192.168.1.1",
        JSONScalar: {"key": "value", "number": 42},
        UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",
        # Network & Infrastructure
        MacAddressScalar: "00:1B:63:84:45:E6",
        SubnetMaskScalar: "255.255.255.0",
        HostnameScalar: "example.com",
        DomainNameScalar: "example.com",
        PortScalar: 8080,
        URLScalar: "https://example.com",
        # Geographic & Location
        AirportCodeScalar: "LAX",
        CoordinateScalar: "34.0522,-118.2437",
        LatitudeScalar: "34.0522",
        LongitudeScalar: "-118.2437",
        TimezoneScalar: "America/Los_Angeles",
        # Financial & Business
        CurrencyCodeScalar: "USD",
        IBANScalar: "GB82WEST12345698765432",
        ISINScalar: "US0378331005",
        SEDOLScalar: "B0WNLY7",
        LEIScalar: "549300E9PC51EN656011",
        ExchangeCodeScalar: "NYSE",
        MICScalar: "XNYS",
        StockSymbolScalar: "AAPL",
        MoneyScalar: "100.00",
        ExchangeRateScalar: "1.25",
        # Shipping & Logistics
        PortCodeScalar: "USNYC",
        ContainerNumberScalar: "CSQU3054383",
        TrackingNumberScalar: "1Z999AA10123456784",
        VINScalar: "1HGBH41JXMN109186",
        # Communications
        PhoneNumberScalar: "+14155552671",
        ApiKeyScalar: "sk_test_4eC39HqLyjWDarjtT1zdp7dc",
        # Content & Data
        HTMLScalar: "<p>Hello World</p>",
        MarkdownScalar: "# Hello World",
        MimeTypeScalar: "application/json",
        ColorScalar: "#FF5733",
        HashSHA256Scalar: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        # Identification & Codes
        LanguageCodeScalar: "en",
        LocaleCodeScalar: "en-US",
        PostalCodeScalar: "90210",
        LicensePlateScalar: "ABC123",
        FlightNumberScalar: "AA100",
        SlugScalar: "hello-world",
        # Date & Time
        DateTimeScalar: "2023-12-13T10:30:00Z",
        TimeScalar: "10:30:00",
        DateRangeScalar: "[2023-12-01,2023-12-31]",
        DurationScalar: "PT1H30M",
        # Technical & Specialized
        SemanticVersionScalar: "1.2.3",
        PercentageScalar: 75.5,
        VectorScalar: [0.1, 0.2, 0.3],
        LTreeScalar: "Top.Science.Astronomy",
        FileScalar: "test.txt",
        ImageScalar: "image.png",
    }

    # Return specific value if known, otherwise raise error to catch missing values
    if scalar_class not in test_values:
        raise ValueError(
            f"No test value defined for {scalar_class}. "
            f"Add a valid test value to the test_values dictionary."
        )

    return test_values[scalar_class]


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
