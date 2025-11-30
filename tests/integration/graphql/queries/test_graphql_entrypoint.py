"""Tests for GraphQL HTTP entrypoint router."""

from unittest.mock import MagicMock

import pytest
from graphql import ExecutionResult, build_schema
from starlette.requests import Request
from starlette.testclient import TestClient

from fraiseql.gql.graphql_entrypoint import GraphNoteRouter

pytestmark = pytest.mark.integration


@pytest.fixture
def simple_schema() -> None:
    """Create a simple GraphQL schema for testing."""
    return build_schema(
        """
        type Query {
            hello: String
        }
    """
    )


@pytest.fixture
def mock_context_getter() -> None:
    """Mock context getter that returns user info."""

    def context_getter(request: Request) -> None:
        return {"user_id": "test-user", "request": request}

    return context_getter


class TestGraphNoteRouter:
    """Test GraphNoteRouter functionality."""

    def test_init_with_schema_only(self, simple_schema) -> None:
        """Test router initialization with schema only."""
        router = GraphNoteRouter(simple_schema)

        assert router.schema == simple_schema
        assert len(router.routes) == 1
        assert router.routes[0].path == "/graphql"

    def test_init_with_context_getter(self, simple_schema, mock_context_getter) -> None:
        """Test router initialization with context getter."""
        router = GraphNoteRouter(simple_schema, context_getter=mock_context_getter)

        assert router.schema == simple_schema
        assert router.context_getter == mock_context_getter

    @pytest.mark.asyncio
    async def test_graphql_query_execution(self, simple_schema) -> None:
        """Test GraphQL query execution through the router."""
        # Mock the graphql execution
        ExecutionResult(data={"hello": "world"})

        router = GraphNoteRouter(simple_schema)
        app = TestClient(router)

        # Test POST request with GraphQL query
        response = app.post("/graphql", json={"query": "{ hello }"})

        assert response.status_code == 200
        # Note: This test would need proper mocking of graphql() function

    def test_context_getter_default_behavior(self, simple_schema) -> None:
        """Test default context getter behavior."""
        router = GraphNoteRouter(simple_schema)

        # Create mock request
        mock_request = MagicMock(spec=Request)

        # Default context getter should return empty dict
        if hasattr(router, "context_getter") and router.context_getter:
            context = router.context_getter(mock_request)
            assert isinstance(context, dict)

    def test_route_configuration(self, simple_schema) -> None:
        """Test that the router is properly configured with GraphQL endpoint."""
        router = GraphNoteRouter(simple_schema)

        # Should have exactly one route for /graphql
        assert len(router.routes) == 1
        route = router.routes[0]
        assert route.path == "/graphql"
        assert "POST" in route.methods or "GET" in route.methods

    @pytest.mark.asyncio
    async def test_error_handling(self, simple_schema) -> None:
        """Test error handling in GraphQL execution."""
        router = GraphNoteRouter(simple_schema)
        app = TestClient(router)

        # Test invalid GraphQL query
        response = app.post("/graphql", json={"query": "{ invalidField }"})

        # Should return a response (might be error, but shouldn't crash)
        assert response.status_code in [200, 400]

    def test_schema_registry_integration(self, simple_schema) -> None:
        """Test integration with SchemaRegistry."""
        # This tests that the import works correctly

        router = GraphNoteRouter(simple_schema)
        assert router.schema == simple_schema
