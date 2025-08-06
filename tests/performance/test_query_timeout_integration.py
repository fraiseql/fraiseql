"""Integration test to verify query timeout fix works with actual database operations."""

import pytest

import fraiseql
from fraiseql import query
from fraiseql.db import FraiseQLRepository


@fraiseql.type
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
    await conn.execute(
        """
        CREATE TABLE IF NOT EXISTS gateways (
            id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            name TEXT
        )
    """
    )

    await conn.execute(
        """
        CREATE OR REPLACE VIEW gateway_view AS
        SELECT
            id,
            jsonb_build_object(
                'id', id,
                'ip_address', ip_address,
                'name', name
            ) as data
        FROM gateways
    """
    )

    # Insert test data
    await conn.execute(
        """
        INSERT INTO gateways (id, ip_address, name)
        VALUES ('test-gateway-1', '192.168.1.1', 'Test Gateway')
    """
    )

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
    # In development mode, find_one returns the data from the 'data' JSONB column
    if isinstance(result, dict) and "data" in result:
        # Raw row from database
        data = result["data"]
        assert data["id"] == "test-gateway-1"
        assert data["ip_address"] == "192.168.1.1"
        assert data["name"] == "Test Gateway"
    else:
        # Parsed object
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
    await conn.execute(
        """
        CREATE TABLE IF NOT EXISTS gateways (
            id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            name TEXT
        )
    """
    )

    await conn.execute(
        """
        CREATE OR REPLACE VIEW gateway_view AS
        SELECT
            id,
            jsonb_build_object(
                'id', id,
                'ip_address', ip_address,
                'name', name
            ) as data
        FROM gateways
    """
    )

    # Insert test data
    await conn.execute(
        """
        INSERT INTO gateways (id, ip_address, name)
        VALUES
            ('gw-1', '10.0.0.1', 'Gateway 1'),
            ('gw-2', '10.0.0.2', 'Gateway 2')
    """
    )

    await conn.commit()

    # Define queries
    @query
    async def gateway(info, id: str) -> Gateway | None:
        db: FraiseQLRepository = info.context["db"]
        result = await db.find_one("gateway_view", id=id)
        if result:
            # The result might be nested in 'data' field from the view
            if "data" in result and isinstance(result["data"], dict):
                gateway_data = result["data"]
            else:
                gateway_data = result
            # Convert ipAddress to ip_address for the Gateway type
            if "ipAddress" in gateway_data and "ip_address" not in gateway_data:
                gateway_data["ip_address"] = gateway_data.pop("ipAddress")
            return Gateway(**gateway_data)
        return None

    @query
    async def gateways(info) -> list[Gateway]:
        db: FraiseQLRepository = info.context["db"]
        results = await db.find("gateway_view")
        gateways_list = []
        for r in results:
            if "data" in r and isinstance(r["data"], dict):
                gateway_data = r["data"]
            else:
                gateway_data = r
            # Convert ipAddress to ip_address for the Gateway type
            if "ipAddress" in gateway_data and "ip_address" not in gateway_data:
                gateway_data["ip_address"] = gateway_data.pop("ipAddress")
            gateways_list.append(Gateway(**gateway_data))
        return gateways_list

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

    # Build schema
    from graphql import execute

    from fraiseql.gql.schema_builder import build_fraiseql_schema

    schema = build_fraiseql_schema(query_types=[gateway, gateways])

    # Execute GraphQL query
    query_str = """
        query GetGateway($id: String!) {
            gateway(id: $id) {
                id
                ipAddress
                name
            }
        }
    """
    from graphql import parse

    document = parse(query_str)
    result = await execute(
        schema, document, variable_values={"id": "gw-1"}, context_value={"db": repo}
    )

    assert result.errors is None
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
    list_document = parse(list_query)
    result = await execute(schema, list_document, context_value={"db": repo})

    assert result.errors is None
    assert result.data is not None
    assert len(result.data["gateways"]) == 2
    assert any(gw["id"] == "gw-1" for gw in result.data["gateways"])
    assert any(gw["id"] == "gw-2" for gw in result.data["gateways"])
