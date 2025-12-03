"""Test GET /graphql endpoint behavior."""

import json
from contextlib import asynccontextmanager

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

pytestmark = pytest.mark.integration


@pytest.mark.unit
@asynccontextmanager
async def noop_lifespan(app: FastAPI) -> None:
    """No-op lifespan for tests that don't need a database."""
    yield


# Define query functions
@fraiseql.query
def hello(info, name: str = "World") -> str:
    """Simple hello query."""
    return f"Hello, {name}!"


@fraiseql.query
def add(info, a: int, b: int) -> int:
    """Add two numbers."""
    return a + b


class TestGraphQLGetEndpoint:
    """Test GET /graphql endpoint functionality."""

    @pytest.fixture
    def app_dev(self, clear_registry) -> None:
        """Create test app in development mode."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="development",
            enable_introspection=True,
            enable_playground=True,
            playground_tool="graphiql",
        )
        return create_fraiseql_app(config=config, queries=[hello, add], lifespan=noop_lifespan)

    @pytest.fixture
    def app_dev_apollo(self, clear_registry) -> None:
        """Create test app with Apollo Sandbox."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="development",
            enable_introspection=True,
            enable_playground=True,
            playground_tool="apollo-sandbox",
        )
        return create_fraiseql_app(config=config, queries=[hello, add], lifespan=noop_lifespan)

    @pytest.fixture
    def app_prod(self, clear_registry) -> None:
        """Create test app in production mode."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="production",
            enable_introspection=False,
            enable_playground=False,
        )
        return create_fraiseql_app(config=config, queries=[hello, add], lifespan=noop_lifespan)

    def test_get_graphql_no_query_serves_playground(self, app_dev) -> None:
        """Test GET /graphql without query serves GraphiQL playground."""
        with TestClient(app_dev) as client:
            response = client.get("/graphql")
            assert response.status_code == 200
            assert response.headers["content-type"] == "text/html; charset=utf-8"
            assert "GraphiQL" in response.text
            assert '<div id="graphiql">' in response.text

    def test_get_graphql_no_query_serves_apollo_sandbox(self, app_dev_apollo) -> None:
        """Test GET /graphql without query serves Apollo Sandbox."""
        with TestClient(app_dev_apollo) as client:
            response = client.get("/graphql")
            assert response.status_code == 200
            assert response.headers["content-type"] == "text/html; charset=utf-8"
            assert "Apollo Sandbox" in response.text
            assert '<div id="sandbox"' in response.text

    def test_get_graphql_with_query(self, app_dev) -> None:
        """Test GET /graphql with query parameter executes query."""
        with TestClient(app_dev) as client:
            query = "{ hello }"
            response = client.get(f"/graphql?query={query}")
            assert response.status_code == 200
            result = response.json()
            assert result["data"] == {"hello": "Hello, World!"}

    def test_get_graphql_with_query_and_variables(self, app_dev) -> None:
        """Test GET /graphql with query and variables."""
        with TestClient(app_dev) as client:
            query = "query($name: String!) { hello(name: $name) }"
            variables = json.dumps({"name": "Alice"})
            response = client.get(f"/graphql?query={query}&variables={variables}")
            assert response.status_code == 200
            result = response.json()
            assert result["data"] == {"hello": "Hello, Alice!"}

    def test_get_graphql_with_operation_name(self, app_dev) -> None:
        """Test GET /graphql with operationName."""
        with TestClient(app_dev) as client:
            query = "query HelloQuery { hello } query AddQuery { add(a: 1, b: 2) }"
            response = client.get(f"/graphql?query={query}&operationName=AddQuery")
            assert response.status_code == 200
            result = response.json()
            assert result["data"] == {"add": 3}

    def test_get_graphql_invalid_json_variables(self, app_dev) -> None:
        """Test GET /graphql with invalid JSON in variables."""
        with TestClient(app_dev) as client:
            query = "query($name: String!) { hello(name: $name) }"
            variables = "not-valid-json"
            response = client.get(f"/graphql?query={query}&variables={variables}")
            assert response.status_code == 400
            assert "Invalid JSON in variables parameter" in response.json()["detail"]

    def test_get_graphql_complex_query(self, app_dev) -> None:
        """Test GET /graphql with more complex query."""
        with TestClient(app_dev) as client:
            query = "{ result: add(a: 5, b: 7) }"
            response = client.get(f"/graphql?query={query}")
            assert response.status_code == 200
            result = response.json()
            assert result["data"] == {"result": 12}

    def test_get_graphql_malformed_query(self, app_dev) -> None:
        """Test GET /graphql with malformed query."""
        with TestClient(app_dev) as client:
            query = "{ hello("  # Missing closing parenthesis
            response = client.get(f"/graphql?query={query}")
            assert response.status_code == 200  # GraphQL returns 200 with errors
            assert "errors" in response.json()

    def test_get_graphql_production_no_endpoint(self, app_prod) -> None:
        """Test GET /graphql is not available in production."""
        with TestClient(app_prod) as client:
            response = client.get("/graphql")
            assert response.status_code == 404  # Not Found in production

    def test_playground_endpoint_removed(self, app_dev) -> None:
        """Test /playground endpoint no longer exists."""
        with TestClient(app_dev) as client:
            response = client.get("/playground")
            assert response.status_code == 404

    def test_get_graphql_url_encoded_query(self, app_dev) -> None:
        """Test GET /graphql with URL encoded query."""
        with TestClient(app_dev) as client:
            query = '{ hello(name: "Test User") }'
            # The test client will handle URL encoding
            response = client.get("/graphql", params={"query": query})
            assert response.status_code == 200
            result = response.json()
            assert result["data"] == {"hello": "Hello, Test User!"}

    def test_get_graphql_empty_query(self, app_dev) -> None:
        """Test GET /graphql with empty query parameter."""
        with TestClient(app_dev) as client:
            response = client.get("/graphql?query=")
            assert response.status_code == 200
            assert "errors" in response.json()  # GraphQL validation error
