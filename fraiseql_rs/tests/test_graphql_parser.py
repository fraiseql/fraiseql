"""Tests for Rust GraphQL parser."""

import pytest
from fraiseql.core.graphql_parser import RustGraphQLParser


@pytest.fixture
def parser():
    return RustGraphQLParser()


@pytest.mark.asyncio
async def test_parse_simple_query(parser):
    """Test parsing a simple query."""
    query = "query { users { id name } }"
    result = await parser.parse(query)

    assert result.operation_type == "query"
    assert result.root_field == "users"
    assert len(result.selections) == 1
    assert result.selections[0].name == "users"
    assert len(result.selections[0].nested_fields) == 2


@pytest.mark.asyncio
async def test_parse_query_with_arguments(parser):
    """Test parsing query with WHERE argument."""
    query = """
        query {
            users(where: {status: "active"}, limit: 10) {
                id
                name
            }
        }
    """
    result = await parser.parse(query)

    users_field = result.selections[0]
    assert len(users_field.arguments) == 2
    assert users_field.arguments[0].name == "where"
    assert users_field.arguments[1].name == "limit"


@pytest.mark.asyncio
async def test_parse_nested_fields(parser):
    """Test parsing nested field selection."""
    query = """
        query {
            users {
                id
                equipment {
                    name
                    status
                }
            }
        }
    """
    result = await parser.parse(query)

    users_field = result.selections[0]
    # Should have id and equipment fields
    assert len(users_field.nested_fields) == 2

    equipment_field = next(f for f in users_field.nested_fields if f.name == "equipment")
    assert len(equipment_field.nested_fields) == 2


@pytest.mark.asyncio
async def test_parse_mutation(parser):
    """Test parsing mutation."""
    query = """
        mutation {
            createUser(input: {name: "John"}) {
                id
                name
            }
        }
    """
    result = await parser.parse(query)

    assert result.operation_type == "mutation"
    assert result.root_field == "createUser"


@pytest.mark.asyncio
async def test_parse_with_variables(parser):
    """Test parsing query with variables."""
    query = """
        query GetUsers($where: UserWhere!) {
            users(where: $where) {
                id
            }
        }
    """
    result = await parser.parse(query)

    assert len(result.variables) == 1
    assert result.variables[0].name == "where"
    assert result.variables[0].var_type == "UserWhere!"


@pytest.mark.asyncio
async def test_parse_invalid_query(parser):
    """Test parsing invalid query raises error."""
    with pytest.raises(SyntaxError):
        await parser.parse("this is not graphql syntax")


@pytest.mark.asyncio
async def test_query_signature(parser):
    """Test query signature generation for caching."""
    query = "query { users { id } }"
    result = await parser.parse(query)

    sig = result.signature()
    assert "query" in sig
    assert "users" in sig


@pytest.mark.asyncio
async def test_is_cacheable(parser):
    """Test cacheable detection."""
    # Query without variables is cacheable
    query1 = "query { users { id } }"
    result1 = await parser.parse(query1)
    assert result1.is_cacheable()

    # Query with variables is not cacheable
    query2 = "query GetUsers($where: UserWhere!) { users(where: $where) { id } }"
    result2 = await parser.parse(query2)
    assert not result2.is_cacheable()


@pytest.mark.asyncio
async def test_parse_integer_arguments(parser):
    """Test parsing integer arguments (verifies unsafe Number conversion)."""
    query = """
        query {
            users(limit: 42, offset: 100) {
                id
            }
        }
    """
    result = await parser.parse(query)

    users_field = result.selections[0]
    assert len(users_field.arguments) == 2

    # Verify integer serialization works correctly
    assert users_field.arguments[0].name == "limit"
    assert users_field.arguments[0].value_type == "int"
    assert users_field.arguments[0].value_json == "42"

    assert users_field.arguments[1].name == "offset"
    assert users_field.arguments[1].value_type == "int"
    assert users_field.arguments[1].value_json == "100"


@pytest.mark.asyncio
async def test_inline_fragment_error(parser):
    """Test that inline fragments return proper error."""
    query = """
        query {
            users {
                id
                ... on Admin {
                    permissions
                }
            }
        }
    """
    with pytest.raises(SyntaxError) as exc_info:
        await parser.parse(query)

    assert "Inline fragments not yet supported" in str(exc_info.value)


@pytest.mark.asyncio
async def test_fragment_spread_error(parser):
    """Test that fragment spreads return proper error."""
    query = """
        query {
            users {
                ...UserFields
            }
        }
    """
    with pytest.raises(SyntaxError) as exc_info:
        await parser.parse(query)

    assert "Fragment spreads not yet supported" in str(exc_info.value)
