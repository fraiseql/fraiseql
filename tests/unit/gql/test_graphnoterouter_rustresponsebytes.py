"""Unit tests for GraphNoteRouter RustResponseBytes pass-through.

Phase 2: TDD Cycle 2.1 - HTTP Layer Integration

These tests verify that GraphNoteRouter correctly handles RustResponseBytes
returned by execute_graphql() and passes them through as HTTP response bytes.
"""

from unittest.mock import AsyncMock, Mock

import pytest
from graphql import ExecutionResult, GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString
from starlette.requests import Request

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.gql.graphql_entrypoint import GraphNoteRouter


@pytest.mark.asyncio
async def test_graphnoterouter_handles_rustresponsebytes() -> None:
    """Test that GraphNoteRouter returns Response with bytes for RustResponseBytes.

    This test verifies Phase 2 implementation:
    - GraphNoteRouter uses execute_graphql() which returns RustResponseBytes
    - GraphNoteRouter detects RustResponseBytes and returns it as HTTP Response
    - Content-Type is application/json
    - Status code is 200
    """
    # Create a mock RustResponseBytes
    mock_response_bytes = b'{"data":{"hello":"world"}}'
    rust_response = RustResponseBytes(mock_response_bytes)

    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(name="Query", fields={"hello": GraphQLField(GraphQLString)})
    )

    # Create router
    router = GraphNoteRouter(schema=schema)

    # Mock execute_graphql to return RustResponseBytes
    # We need to patch it where it's used
    import fraiseql.gql.graphql_entrypoint as gql_module

    original_execute_graphql = gql_module.execute_graphql

    async def mock_execute_graphql(*args, **kwargs) -> None:
        return rust_response

    gql_module.execute_graphql = mock_execute_graphql

    try:
        # Create mock request
        mock_request = Mock(spec=Request)
        mock_request.method = "POST"
        mock_request.json = AsyncMock(return_value={"query": "{ hello }"})

        # Call handle_graphql
        response = await router.handle_graphql(mock_request)

        # Verify response
        assert response.status_code == 200, f"Expected 200, got {response.status_code}"
        assert response.media_type == "application/json", (
            f"Expected application/json, got {response.media_type}"
        )
        assert response.body == mock_response_bytes, f"Expected bytes to match, got {response.body}"

    finally:
        # Restore original
        gql_module.execute_graphql = original_execute_graphql


@pytest.mark.asyncio
async def test_graphnoterouter_handles_normal_executionresult() -> None:
    """Test that GraphNoteRouter still handles normal ExecutionResult correctly.

    This verifies backwards compatibility - normal GraphQL execution should work.
    """
    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(name="Query", fields={"hello": GraphQLField(GraphQLString)})
    )

    # Create router
    router = GraphNoteRouter(schema=schema)

    # Mock execute_graphql to return ExecutionResult
    import fraiseql.gql.graphql_entrypoint as gql_module

    original_execute_graphql = gql_module.execute_graphql

    async def mock_execute_graphql(*args, **kwargs) -> None:
        return ExecutionResult(data={"hello": "world"}, errors=None)

    gql_module.execute_graphql = mock_execute_graphql

    try:
        # Create mock request
        mock_request = Mock(spec=Request)
        mock_request.method = "POST"
        mock_request.json = AsyncMock(return_value={"query": "{ hello }"})

        # Call handle_graphql
        response = await router.handle_graphql(mock_request)

        # Verify response
        assert response.status_code == 200, f"Expected 200, got {response.status_code}"

        # Parse JSON response
        import json

        data = json.loads(response.body)
        assert "data" in data, f"Expected 'data' in response: {data}"
        assert data["data"]["hello"] == "world", f"Expected hello=world: {data}"

    finally:
        # Restore original
        gql_module.execute_graphql = original_execute_graphql


@pytest.mark.asyncio
async def test_graphnoterouter_handles_errors_in_executionresult() -> None:
    """Test that GraphNoteRouter handles errors correctly.

    This verifies error handling - errors should return 400 status.
    """
    # Create a simple schema
    schema = GraphQLSchema(
        query=GraphQLObjectType(name="Query", fields={"hello": GraphQLField(GraphQLString)})
    )

    # Create router
    router = GraphNoteRouter(schema=schema)

    # Mock execute_graphql to return ExecutionResult with errors
    import fraiseql.gql.graphql_entrypoint as gql_module

    original_execute_graphql = gql_module.execute_graphql

    from graphql import GraphQLError

    async def mock_execute_graphql(*args, **kwargs) -> None:
        return ExecutionResult(data=None, errors=[GraphQLError("Test error")])

    gql_module.execute_graphql = mock_execute_graphql

    try:
        # Create mock request
        mock_request = Mock(spec=Request)
        mock_request.method = "POST"
        mock_request.json = AsyncMock(return_value={"query": "{ hello }"})

        # Call handle_graphql
        response = await router.handle_graphql(mock_request)

        # Verify response
        assert response.status_code == 400, f"Expected 400 for errors, got {response.status_code}"

        # Parse JSON response
        import json

        data = json.loads(response.body)
        assert "errors" in data, f"Expected 'errors' in response: {data}"
        assert len(data["errors"]) > 0, f"Expected at least one error: {data}"

    finally:
        # Restore original
        gql_module.execute_graphql = original_execute_graphql
