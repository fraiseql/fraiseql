"""Integration test to verify query timeout fix works with actual database operations."""

import pytest
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import FraiseQLRepository
from fraiseql.gql import FraiseQLGQL, fraise_type


@fraise_type
class Gateway:
    """A test gateway type."""

    id: str
    ip_address: str
    name: str | None = None


@pytest.mark.database
@pytest.mark.asyncio
async def test_find_one_with_timeout_integration(db_connection_committed):
    """Test that find_one works with query timeout in a real database environment."""
    conn = db_connection_committed

    # Create gateway table and view
    await conn.execute("""
        CREATE TABLE IF NOT EXISTS gateways (
            id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            name TEXT
        )
    """)

    await conn.execute("""
        CREATE OR REPLACE VIEW gateway_view AS
        SELECT 
            id,
            jsonb_build_object(
                'id', id,
                'ip_address', ip_address,
                'name', name
            ) as data
        FROM gateways
    """)

    # Insert test data
    await conn.execute("""
        INSERT INTO gateways (id, ip_address, name)
        VALUES ('test-gateway-1', '192.168.1.1', 'Test Gateway')
    """)

    await conn.commit()

    # Create a pool with the connection
    # Note: In a real scenario, this would be a proper pool
    class MockPool:
        def connection(self):
            class ConnContext:
                async def __aenter__(self):
                    return conn

                async def __aexit__(self, *args):
                    pass

            return ConnContext()

    mock_pool = MockPool()

    # Create repository with query timeout
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 30})

    # Test find_one with timeout
    result = await repo.find_one("gateway_view", id="test-gateway-1")

    assert result is not None
    assert result["id"] == "test-gateway-1"
    assert result["ip_address"] == "192.168.1.1"
    assert result["name"] == "Test Gateway"

    # Test with non-existent ID
    result = await repo.find_one("gateway_view", id="non-existent")
    assert result is None


@pytest.mark.database
@pytest.mark.asyncio
async def test_graphql_query_with_timeout(db_connection_committed):
    """Test that GraphQL queries work correctly with query timeout."""
    conn = db_connection_committed

    # Create gateway table and view
    await conn.execute("""
        CREATE TABLE IF NOT EXISTS gateways (
            id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            name TEXT
        )
    """)

    await conn.execute("""
        CREATE OR REPLACE VIEW gateway_view AS
        SELECT 
            id,
            jsonb_build_object(
                'id', id,
                'ip_address', ip_address,
                'name', name
            ) as data
        FROM gateways
    """)

    # Insert test data
    await conn.execute("""
        INSERT INTO gateways (id, ip_address, name)
        VALUES 
            ('gw-1', '10.0.0.1', 'Gateway 1'),
            ('gw-2', '10.0.0.2', 'Gateway 2')
    """)

    await conn.commit()

    # Create FraiseQL instance
    fraiseql = FraiseQLGQL()

    # Define query
    @fraiseql.query
    async def gateway(info, id: str) -> Gateway | None:
        db: FraiseQLRepository = info.context["db"]
        result = await db.find_one("gateway_view", id=id)
        return result

    @fraiseql.query
    async def gateways(info) -> list[Gateway]:
        db: FraiseQLRepository = info.context["db"]
        results = await db.find("gateway_view")
        return results

    # Create context with timeout
    class MockPool:
        def connection(self):
            class ConnContext:
                async def __aenter__(self):
                    return conn

                async def __aexit__(self, *args):
                    pass

            return ConnContext()

    mock_pool = MockPool()
    repo = FraiseQLRepository(mock_pool, context={"query_timeout": 30})

    # Execute GraphQL query
    query = """
        query GetGateway($id: String!) {
            gateway(id: $id) {
                id
                ipAddress
                name
            }
        }
    """

    result = await fraiseql.run(query, {"id": "gw-1"}, context={"db": repo})

    assert result.data is not None
    assert result.data["gateway"]["id"] == "gw-1"
    assert result.data["gateway"]["ipAddress"] == "10.0.0.1"
    assert result.data["gateway"]["name"] == "Gateway 1"

    # Test list query
    list_query = """
        query ListGateways {
            gateways {
                id
                ipAddress
            }
        }
    """

    result = await fraiseql.run(list_query, context={"db": repo})

    assert result.data is not None
    assert len(result.data["gateways"]) == 2
    assert any(gw["id"] == "gw-1" for gw in result.data["gateways"])
    assert any(gw["id"] == "gw-2" for gw in result.data["gateways"])