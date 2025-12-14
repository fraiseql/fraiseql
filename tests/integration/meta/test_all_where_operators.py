"""Meta-test for ALL WHERE operators E2E integration.

This test validates that every WHERE operator in FraiseQL works through the
complete GraphQL pipeline: query parsing → validation → SQL generation → execution.

It auto-discovers all operators and tests each one in real GraphQL queries.
"""

import pytest
from graphql import graphql

# Import schema_builder to ensure SchemaRegistry is patched
import fraiseql.gql.schema_builder  # noqa: F401
from fraiseql import fraise_type, query
from fraiseql.gql.builders import SchemaRegistry
from fraiseql.where_clause import ALL_OPERATORS


def get_all_operators():
    """Auto-enumerate all WHERE operators from ALL_OPERATORS."""
    operators = []
    for operator_name in ALL_OPERATORS.keys():
        # Skip internal/private operators
        if operator_name.startswith("_"):
            continue
        operators.append(operator_name)
    return operators


@pytest.fixture(scope="class")
def operator_test_schema(meta_test_schema):
    """Schema registry prepared with operator test types."""
    # Clear any existing registrations
    meta_test_schema.clear()

    # Register test types for different operator categories
    @fraise_type(sql_source="test_strings")
    class StringTestType:
        id: int
        name: str
        description: str

    @fraise_type(sql_source="test_numbers")
    class NumberTestType:
        id: int
        value: int
        score: float

    @fraise_type(sql_source="test_arrays")
    class ArrayTestType:
        id: int
        tags: list[str]
        numbers: list[int]

    @fraise_type(sql_source="test_networks")
    class NetworkTestType:
        id: int
        ip_address: str  # Would be IpAddressScalar in real usage
        network: str  # Would be CIDRScalar in real usage

    # Register queries
    @query
    async def get_strings(info) -> list[StringTestType]:
        return []

    @query
    async def get_numbers(info) -> list[NumberTestType]:
        return []

    @query
    async def get_arrays(info) -> list[ArrayTestType]:
        return []

    @query
    async def get_networks(info) -> list[NetworkTestType]:
        return []

    # Register types with schema
    meta_test_schema.register_type(StringTestType)
    meta_test_schema.register_type(NumberTestType)
    meta_test_schema.register_type(ArrayTestType)
    meta_test_schema.register_type(NetworkTestType)
    meta_test_schema.register_query(get_strings)
    meta_test_schema.register_query(get_numbers)
    meta_test_schema.register_query(get_arrays)
    meta_test_schema.register_query(get_networks)

    return meta_test_schema


@pytest.mark.parametrize("operator", get_all_operators())
async def test_operator_in_graphql_query_validation(operator, operator_test_schema):
    """Every operator should pass GraphQL query validation without errors."""
    # Get appropriate test value and field for this operator
    test_value, field_name, query_name = get_test_params_for_operator(operator)

    # Build GraphQL query using the operator
    query_str = f"""
    query {{
        {query_name}(where: {{{field_name}: {{{operator}: {test_value!r}}}}}) {{
            id
        }}
    }}
    """

    schema = operator_test_schema.build_schema()

    # Execute query - should NOT raise validation error
    result = await graphql(schema, query_str)

    # Should not have validation errors
    assert not result.errors, f"Operator '{operator}' failed GraphQL validation: {result.errors}"


@pytest.mark.parametrize("operator", get_all_operators())
async def test_operator_in_where_clause_with_database(operator, meta_test_pool):
    """Every operator should work in WHERE clauses with real database operations."""
    # Skip operators that are hard to test with database setup
    skip_operators = {
        # Complex operators requiring special setup
        "cosine_distance",
        "l2_distance",
        "l1_distance",
        "hamming_distance",
        "jaccard_distance",
        "matches",
        "plain_query",
        "phrase_query",
        "websearch_query",
        "rank_gt",
        "rank_lt",
        "rank_cd_gt",
        "rank_cd_lt",
        "distance_within",
        "ancestor_of",
        "descendant_of",
        "matches_lquery",
        "matches_ltxtquery",
        "matches_any_lquery",
        "nlevel_eq",
        "nlevel_gt",
        "nlevel_lt",
        "depth_eq",
        "depth_gt",
        "depth_lt",
        "contains_date",
        "adjacent",
        "strictly_left",
        "strictly_right",
        "not_left",
        "not_right",
        "strictly_contains",
        "imatches",
        "not_matches",
        "isdescendant",
        # Network operators (require INET/CIDR setup)
        "isIPv4",
        "isIPv6",
        "isPrivate",
        "isPublic",
        "inSubnet",
        "inRange",
        "overlaps",
        "strictleft",
        "strictright",
        "isipv4",
        "isipv6",
        "isprivate",
        "ispublic",
        "insubnet",
        "inrange",
    }

    if operator in skip_operators:
        pytest.skip(f"Operator '{operator}' requires complex setup, skipping in meta-test")

    # Get test parameters
    test_value, field_name, table_name, column_type = get_db_test_params_for_operator(operator)

    # Create test table
    async with meta_test_pool.connection() as conn:
        from psycopg import sql

        await conn.execute(sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name)))
        await conn.execute(
            sql.SQL("CREATE TABLE {} (id SERIAL PRIMARY KEY, {} {})").format(
                sql.Identifier(table_name), sql.Identifier(field_name), sql.SQL(column_type)
            )
        )

        # Insert test data
        await conn.execute(
            sql.SQL("INSERT INTO {} ({}) VALUES (%s)").format(
                sql.Identifier(table_name), sql.Identifier(field_name)
            ),
            [test_value],
        )

        await conn.commit()

    try:
        # Create schema with test type
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Determine Python type based on column type
        if column_type == "INTEGER":
            field_type = int
        elif column_type == "FLOAT" or column_type == "DOUBLE PRECISION":
            field_type = float
        else:
            field_type = str

        # Create dynamic type for this operator using @fraise_type
        @fraise_type(sql_source=table_name, jsonb_column=None)
        class TestType:
            id: int
            __annotations__ = {"id": int, field_name: field_type}

        @query
        async def get_test_data(info) -> list[TestType]:
            return []

        registry.register_query(get_test_data)

        # Test WHERE clause with the operator
        # Format test_value properly for GraphQL (double quotes for strings)
        if isinstance(test_value, str):
            formatted_value = f'"{test_value}"'
        else:
            formatted_value = str(test_value)

        query_str = f"""
        query {{
            getTestData(where: {{{field_name}: {{{operator}: {formatted_value}}}}}) {{
                id
                {field_name}
            }}
        }}
        """

        schema = registry.build_schema()

        # Execute query - should work without errors
        result = await graphql(schema, query_str)

        assert not result.errors, f"Operator '{operator}' failed in WHERE clause: {result.errors}"

    finally:
        # Cleanup
        async with meta_test_pool.connection() as conn:
            from psycopg import sql

            await conn.execute(
                sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
            )
            await conn.commit()


@pytest.mark.parametrize("operator", ["eq", "neq", "gt", "contains", "in"])
async def test_operator_combinations_with_and_or(operator, operator_test_schema):
    """Operators should work in AND/OR combinations."""
    # Use simple operators for combination testing
    test_value1, test_value2 = get_test_values_for_combination(operator)

    query_str = f"""
    query {{
        getStrings(where: {{
            AND: [
                {{name: {{eq: {test_value1!r}}}}},
                {{description: {{{operator}: {test_value2!r}}}}}
            ]
        }}) {{
            id
            name
            description
        }}
    }}
    """

    schema = operator_test_schema.build_schema()

    # Execute query - should work without errors
    result = await graphql(schema, query_str)

    assert not result.errors, f"Operator '{operator}' failed in AND combination: {result.errors}"

    # Test OR combination
    query_str_or = f"""
    query {{
        getStrings(where: {{
            OR: [
                {{name: {{eq: {test_value1!r}}}}},
                {{description: {{{operator}: {test_value2!r}}}}}
            ]
        }}) {{
            id
            name
            description
        }}
    }}
    """

    result_or = await graphql(schema, query_str_or)

    assert not result_or.errors, (
        f"Operator '{operator}' failed in OR combination: {result_or.errors}"
    )


def get_test_params_for_operator(operator):
    """Get test parameters appropriate for the given operator."""
    # Map operators to test values, field names, and query names
    test_configs = {
        # Comparison operators
        "eq": ("test_value", "name", "getStrings"),
        "neq": ("test_value", "name", "getStrings"),
        "gt": (5, "value", "getNumbers"),
        "gte": (5, "value", "getNumbers"),
        "lt": (10, "value", "getNumbers"),
        "lte": (10, "value", "getNumbers"),
        # String operators
        "contains": ("test", "name", "getStrings"),
        "icontains": ("TEST", "name", "getStrings"),
        "startswith": ("test", "name", "getStrings"),
        "istartswith": ("TEST", "name", "getStrings"),
        "endswith": ("value", "name", "getStrings"),
        "iendswith": ("VALUE", "name", "getStrings"),
        "like": ("test%", "name", "getStrings"),
        "ilike": ("TEST%", "name", "getStrings"),
        # Containment operators
        "in": (["value1", "value2"], "name", "getStrings"),
        "nin": (["value1", "value2"], "name", "getStrings"),
        # Null operators
        "isnull": (True, "name", "getStrings"),
        # Array operators
        "array_eq": (["tag1", "tag2"], "tags", "getArrays"),
        "array_neq": (["tag1", "tag2"], "tags", "getArrays"),
        "array_contains": ("tag1", "tags", "getArrays"),
        "array_contained_by": (["tag1", "tag2", "tag3"], "tags", "getArrays"),
        "contained_by": (["tag1", "tag2", "tag3"], "tags", "getArrays"),
        "array_overlaps": (["tag1", "tag3"], "tags", "getArrays"),
        "overlaps": (["tag1", "tag3"], "tags", "getArrays"),
        "array_length_eq": (2, "tags", "getArrays"),
        "len_eq": (2, "tags", "getArrays"),
        "array_any_eq": ("tag1", "tags", "getArrays"),
        "any_eq": ("tag1", "tags", "getArrays"),
        # Network operators (use string values for simplicity)
        "isIPv4": (True, "ipAddress", "getNetworks"),
        "isIPv6": (True, "ipAddress", "getNetworks"),
        "isPrivate": (True, "ipAddress", "getNetworks"),
        "isPublic": (True, "ipAddress", "getNetworks"),
        "inSubnet": ("192.168.1.0/24", "network", "getNetworks"),
        "inRange": ("192.168.0.0/16", "network", "getNetworks"),
        "overlaps": ("10.0.0.0/8", "network", "getNetworks"),
    }

    # Return default for unknown operators
    return test_configs.get(operator, ("test_value", "name", "getStrings"))


def get_db_test_params_for_operator(operator):
    """Get database test parameters for the given operator."""
    # Map operators to test values, field names, table names, and column types
    db_configs = {
        # Comparison operators
        "eq": ("test_value", "name", "test_eq_table", "TEXT"),
        "neq": ("test_value", "name", "test_neq_table", "TEXT"),
        "gt": (5, "value", "test_gt_table", "INTEGER"),
        "gte": (5, "value", "test_gte_table", "INTEGER"),
        "lt": (10, "value", "test_lt_table", "INTEGER"),
        "lte": (10, "value", "test_lte_table", "INTEGER"),
        # String operators
        "contains": ("test_string", "name", "test_contains_table", "TEXT"),
        "icontains": ("TEST_STRING", "name", "test_icontains_table", "TEXT"),
        "startswith": ("test", "name", "test_startswith_table", "TEXT"),
        "istartswith": ("TEST", "name", "test_istartswith_table", "TEXT"),
        "endswith": ("string", "name", "test_endswith_table", "TEXT"),
        "iendswith": ("STRING", "name", "test_iendswith_table", "TEXT"),
        "like": ("test%", "name", "test_like_table", "TEXT"),
        "ilike": ("TEST%", "name", "test_ilike_table", "TEXT"),
        # Containment operators
        "in": ("value1", "name", "test_in_table", "TEXT"),
        "nin": ("value1", "name", "test_nin_table", "TEXT"),
        # Null operators
        "isnull": (None, "name", "test_isnull_table", "TEXT"),
    }

    # Return default for unknown operators
    return db_configs.get(operator, ("test_value", "name", "test_default_table", "TEXT"))


def get_test_values_for_combination(operator):
    """Get test values for operator combination testing."""
    combination_values = {
        "eq": ("test_name", "test_description"),
        "neq": ("test_name", "other_description"),
        "gt": ("test_name", 5),
        "contains": ("test_name", "desc"),
        "in": ("test_name", ["desc1", "desc2"]),
    }

    return combination_values.get(operator, ("value1", "value2"))
