"""Test GraphQL playground tool configuration."""

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


# Simple test type to satisfy schema requirements
@fraiseql.type
class User:
    id: int
    name: str


@fraiseql.type
class QueryRoot:
    """Simple query root for testing."""

    test_field: str = fraiseql.fraise_field(description="Test field", purpose="output")

    def resolve_test_field(self, info) -> str:
        return "test_value"


def test_graphiql_default(clear_registry) -> None:
    """Test that GraphiQL is the default playground tool."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test", types=[User, QueryRoot], production=False
    )

    with TestClient(app) as client:
        response = client.get("/graphql")
        assert response.status_code == 200
        assert "GraphiQL" in response.text
        assert "graphiql.min.js" in response.text
        assert "Apollo Sandbox" not in response.text


def test_apollo_sandbox_config(clear_registry) -> None:
    """Test Apollo Sandbox configuration."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[User, QueryRoot],
        production=False,
        config=FraiseQLConfig(
            database_url="postgresql://localhost/test", playground_tool="apollo-sandbox"
        ),
    )

    with TestClient(app) as client:
        response = client.get("/graphql")
        assert response.status_code == 200
        assert "Apollo Sandbox" in response.text
        assert "embeddable-sandbox.cdn.apollographql.com" in response.text
        assert "graphiql.min.js" not in response.text


def test_graphiql_explicit_config(clear_registry) -> None:
    """Test explicit GraphiQL configuration."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[User, QueryRoot],
        production=False,
        config=FraiseQLConfig(
            database_url="postgresql://localhost/test", playground_tool="graphiql"
        ),
    )

    with TestClient(app) as client:
        response = client.get("/graphql")
        assert response.status_code == 200
        assert "GraphiQL" in response.text
        assert "graphiql.min.js" in response.text


def test_playground_disabled_in_production(clear_registry) -> None:
    """Test that playground is disabled in production."""
    app = create_fraiseql_app(
        database_url="postgresql://localhost/test",
        types=[User, QueryRoot],
        production=True,
        lifespan=noop_lifespan,
    )

    with TestClient(app) as client:
        # In production, playground is disabled and GET endpoint is not registered
        response = client.get("/graphql")
        assert response.status_code == 404  # Not found in production


def test_playground_tool_env_var(monkeypatch) -> None:
    """Test playground tool configuration via environment variable."""
    monkeypatch.setenv("FRAISEQL_DATABASE_URL", "postgresql://localhost/test")
    monkeypatch.setenv("FRAISEQL_PLAYGROUND_TOOL", "apollo-sandbox")

    config = FraiseQLConfig()
    assert config.playground_tool == "apollo-sandbox"


def test_invalid_playground_tool() -> None:
    """Test that invalid playground tool raises error."""
    with pytest.raises(ValueError, match="playground_tool"):
        FraiseQLConfig(database_url="postgresql://localhost/test", playground_tool="invalid-tool")
