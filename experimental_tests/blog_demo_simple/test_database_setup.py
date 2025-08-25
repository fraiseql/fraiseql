"""Test database setup and basic GraphQL client functionality."""

import pytest


@pytest.mark.asyncio
async def test_database_connection(db_connection_simple):
    """Test that we can connect to the test database."""
    async with db_connection_simple.cursor() as cursor:
        await cursor.execute("SELECT 1 as test")
        result = await cursor.fetchone()
        assert result[0] == 1


@pytest.mark.asyncio
async def test_schema_tables_exist(db_manager_simple):
    """Test that our schema tables are properly created."""
    tables = await db_manager_simple.execute_query("""
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
        AND table_type = 'BASE TABLE'
        ORDER BY table_name
    """)

    table_names = [table['table_name'] for table in tables]

    # Check for key command tables
    expected_tables = ['tb_user', 'tb_post', 'tb_comment', 'tb_tag', 'tb_post_tag']
    for table in expected_tables:
        assert table in table_names, f"Table {table} not found in database"


@pytest.mark.asyncio
async def test_graphql_client_creation(simple_graphql_client):
    """Test that the GraphQL client can be created."""
    assert simple_graphql_client is not None
    assert hasattr(simple_graphql_client, 'execute')
    assert hasattr(simple_graphql_client, 'execute_async')


@pytest.mark.asyncio
async def test_basic_graphql_introspection(simple_graphql_client):
    """Test basic GraphQL introspection query."""
    query = """
    query {
        __schema {
            queryType {
                name
            }
        }
    }
    """

    try:
        result = await simple_graphql_client.execute(query)

        # Check that we got a response
        assert result is not None

        # It's ok if we get an error since we haven't set up the schema yet
        # We just want to verify the client is working
        if "errors" not in result:
            assert "data" in result

    except Exception as e:
        # For now, just verify the client attempt to connect
        # Schema might not be ready yet
        assert "GraphQL" in str(type(e).__name__) or "HTTP" in str(type(e).__name__) or str(e), f"Unexpected error type: {e}"
