"""Unit tests for FastAPI Router RustResponseBytes pass-through.

Phase 6: TDD Cycle 6.1 - FastAPI Router Integration

These tests verify that the FastAPI create_graphql_router correctly handles
RustResponseBytes returned by execute_graphql() in the fallback execution path.
"""

import pytest
from unittest.mock import AsyncMock, Mock
from fastapi import Request
from fastapi.testclient import TestClient

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.fastapi.routers import create_graphql_router
from fraiseql.fastapi.config import FraiseQLConfig
from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString, ExecutionResult


@pytest.mark.asyncio
async def test_fastapi_router_handles_rustresponsebytes():
    """Test that FastAPI router returns Response with bytes for RustResponseBytes.

    This test verifies Phase 6 implementation:
    - FastAPI router uses execute_graphql() which returns RustResponseBytes
    - Router detects RustResponseBytes and returns it as HTTP Response
    - Content-Type is application/json
    - Status code is 200
    """
    # Create a mock RustResponseBytes
    mock_response_bytes = b'{"data":{"hello":"world"}}'
    rust_response = RustResponseBytes(mock_response_bytes)

    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            name="Query",
            fields={
                "hello": GraphQLField(GraphQLString)
            }
        )
    )

    # Create router with unified_executor_enabled=False to force fallback path
    config = FraiseQLConfig(
        database_url="postgresql://test:test@localhost:5432/test",
        environment="production",
        unified_executor_enabled=False,  # Force fallback to execute_graphql()
    )
    router = create_graphql_router(schema=schema, config=config)

    # Mock execute_graphql to return RustResponseBytes
    import fraiseql.fastapi.routers as router_module
    original_execute_graphql = router_module.execute_graphql

    async def mock_execute_graphql(*args, **kwargs):
        return rust_response

    router_module.execute_graphql = mock_execute_graphql

    try:
        # Create test client
        from fastapi import FastAPI
        app = FastAPI()
        app.include_router(router)
        client = TestClient(app)

        # Make request
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )

        # Verify response
        assert response.status_code == 200, f"Expected 200, got {response.status_code}"
        assert response.headers["content-type"] == "application/json", (
            f"Expected application/json, got {response.headers['content-type']}"
        )
        assert response.content == mock_response_bytes, (
            f"Expected bytes to match, got {response.content}"
        )

    finally:
        # Restore original
        router_module.execute_graphql = original_execute_graphql


@pytest.mark.asyncio
async def test_fastapi_router_handles_normal_executionresult():
    """Test that FastAPI router still handles normal ExecutionResult correctly.

    This verifies backwards compatibility - normal GraphQL execution should work.
    """
    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            name="Query",
            fields={
                "hello": GraphQLField(GraphQLString)
            }
        )
    )

    # Create router with unified_executor_enabled=False to force fallback path
    config = FraiseQLConfig(
        database_url="postgresql://test:test@localhost:5432/test",
        environment="production",
        unified_executor_enabled=False,
    )
    router = create_graphql_router(schema=schema, config=config)

    # Mock execute_graphql to return ExecutionResult
    import fraiseql.fastapi.routers as router_module
    original_execute_graphql = router_module.execute_graphql

    async def mock_execute_graphql(*args, **kwargs):
        return ExecutionResult(data={"hello": "world"}, errors=None)

    router_module.execute_graphql = mock_execute_graphql

    try:
        # Create test client
        from fastapi import FastAPI
        app = FastAPI()
        app.include_router(router)
        client = TestClient(app)

        # Make request
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )

        # Verify response
        assert response.status_code == 200, f"Expected 200, got {response.status_code}"

        # Parse JSON response
        data = response.json()
        assert "data" in data, f"Expected 'data' in response: {data}"
        assert data["data"]["hello"] == "world", f"Expected hello=world: {data}"

    finally:
        # Restore original
        router_module.execute_graphql = original_execute_graphql


@pytest.mark.asyncio
async def test_fastapi_router_handles_errors_in_executionresult():
    """Test that FastAPI router handles errors correctly.

    This verifies error handling - errors should still be returned properly.
    """
    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(
            name="Query",
            fields={
                "hello": GraphQLField(GraphQLString)
            }
        )
    )

    # Create router with unified_executor_enabled=False to force fallback path
    config = FraiseQLConfig(
        database_url="postgresql://test:test@localhost:5432/test",
        environment="production",
        unified_executor_enabled=False,
    )
    router = create_graphql_router(schema=schema, config=config)

    # Mock execute_graphql to return ExecutionResult with errors
    import fraiseql.fastapi.routers as router_module
    original_execute_graphql = router_module.execute_graphql

    from graphql import GraphQLError

    async def mock_execute_graphql(*args, **kwargs):
        return ExecutionResult(
            data=None,
            errors=[GraphQLError("Test error")]
        )

    router_module.execute_graphql = mock_execute_graphql

    try:
        # Create test client
        from fastapi import FastAPI
        app = FastAPI()
        app.include_router(router)
        client = TestClient(app)

        # Make request
        response = client.post(
            "/graphql",
            json={"query": "{ hello }"}
        )

        # Verify response - FastAPI may return 200 even with GraphQL errors
        # Parse JSON response
        data = response.json()
        assert "errors" in data, f"Expected 'errors' in response: {data}"
        assert len(data["errors"]) > 0, f"Expected at least one error: {data}"

    finally:
        # Restore original
        router_module.execute_graphql = original_execute_graphql
