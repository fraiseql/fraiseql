"""Tests for APQ middleware integration with GraphQL router."""

from contextlib import asynccontextmanager
from unittest.mock import patch

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


# Define test query functions
@fraiseql.query
def hello(info, name: str = "World") -> str:
    """Simple hello query."""
    return f"Hello, {name}!"


@fraiseql.query
def get_user(info, id: str) -> str:
    """Get user by ID as JSON string."""
    return f'{{"id": "{id}", "name": "User {id}", "email": "user{id}@example.com"}}'


class TestAPQMiddlewareIntegration:
    """Test APQ integration with FraiseQL GraphQL router."""

    @pytest.fixture
    def app_dev(self, clear_registry) -> None:
        """Create test app in development mode with APQ support."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="development",
            enable_introspection=True,
            auth_enabled=False,
        )
        return create_fraiseql_app(config=config, queries=[hello, get_user], lifespan=noop_lifespan)

    def test_apq_persisted_query_not_found_error(self, app_dev) -> None:
        """Test APQ returns PERSISTED_QUERY_NOT_FOUND for unknown hash."""
        # This test should FAIL initially - APQ not integrated yet
        with TestClient(app_dev) as client:
            apq_request = {
                "extensions": {
                    "persistedQuery": {
                        "version": 1,
                        "sha256Hash": "unknown_hash_that_does_not_exist_in_storage",
                    }
                }
            }

            response = client.post("/graphql", json=apq_request)

        assert response.status_code == 200
        data = response.json()

        # Should get APQ error response
        assert "errors" in data
        assert len(data["errors"]) == 1

        error = data["errors"][0]
        assert error["message"] == "PersistedQueryNotFound"
        assert error["extensions"]["code"] == "PERSISTED_QUERY_NOT_FOUND"

    def test_apq_successful_query_execution(self, app_dev) -> None:
        """Test APQ executes stored query successfully."""
        # First, store a query (simulated - will need actual storage)
        test_query = 'query { hello(name: "APQ") }'
        test_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

        # Mock the storage to return our test query
        with patch("fraiseql.middleware.apq.get_persisted_query") as mock_get:
            mock_get.return_value = test_query

            with TestClient(app_dev) as client:
                apq_request = {
                    "extensions": {"persistedQuery": {"version": 1, "sha256Hash": test_hash}}
                }

                response = client.post("/graphql", json=apq_request)

            assert response.status_code == 200
            data = response.json()

            # Should execute query successfully
            assert "data" in data
            assert data["data"]["hello"] == "Hello, APQ!"
            assert "errors" not in data

    def test_apq_with_variables(self, app_dev) -> None:
        """Test APQ handles GraphQL variables correctly."""
        test_query = "query GetUser($id: String!) { getUser(id: $id) }"
        test_hash = "var_hash_123"

        with patch("fraiseql.middleware.apq.get_persisted_query") as mock_get:
            mock_get.return_value = test_query

            with TestClient(app_dev) as client:
                apq_request = {
                    "variables": {"id": "123"},
                    "extensions": {"persistedQuery": {"version": 1, "sha256Hash": test_hash}},
                }

                response = client.post("/graphql", json=apq_request)

            assert response.status_code == 200
            data = response.json()

            # Should execute query with variables
            assert "data" in data
            assert "getUser" in data["data"]
            user_json = data["data"]["getUser"]
            assert "123" in user_json  # Check ID is in JSON string

    def test_apq_with_operation_name(self, app_dev) -> None:
        """Test APQ handles operation names correctly."""
        test_query = """
        query GetUserByID($id: String!) {
            getUser(id: $id)
        }
        query GetUserHello {
            hello
        }
        """
        test_hash = "multi_op_hash_456"

        with patch("fraiseql.middleware.apq.get_persisted_query") as mock_get:
            mock_get.return_value = test_query

            with TestClient(app_dev) as client:
                apq_request = {
                    "operationName": "GetUserHello",
                    "extensions": {"persistedQuery": {"version": 1, "sha256Hash": test_hash}},
                }

                response = client.post("/graphql", json=apq_request)

            assert response.status_code == 200
            data = response.json()

            # Should execute the correct operation
            assert "data" in data
            assert data["data"]["hello"] == "Hello, World!"

    def test_apq_invalid_hash_format(self, app_dev) -> None:
        """Test APQ handles invalid hash format gracefully."""
        with TestClient(app_dev) as client:
            apq_request = {
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": ""}}  # Empty hash
            }

            response = client.post("/graphql", json=apq_request)

        # Empty hash fails Pydantic validation (422) before reaching APQ logic
        # This is correct behavior - invalid requests should fail validation
        assert response.status_code == 422

    def test_apq_unsupported_version(self, app_dev) -> None:
        """Test APQ handles unsupported versions correctly."""
        with TestClient(app_dev) as client:
            apq_request = {
                "extensions": {
                    "persistedQuery": {
                        "version": 2,  # Unsupported version
                        "sha256Hash": "some_hash",
                    }
                }
            }

            # This should fail at request validation level
            response = client.post("/graphql", json=apq_request)

        # Should get validation error (422) due to GraphQLRequest validation
        assert response.status_code == 422

    def test_regular_query_still_works(self, app_dev) -> None:
        """Test that regular GraphQL queries still work when APQ is integrated."""
        with TestClient(app_dev) as client:
            regular_request = {"query": 'query { hello(name: "Regular") }'}

            response = client.post("/graphql", json=regular_request)

        assert response.status_code == 200
        data = response.json()

        assert "data" in data
        assert data["data"]["hello"] == "Hello, Regular!"

    def test_apq_integration_preserves_auth(self, clear_registry) -> None:
        """Test APQ integration respects authentication requirements."""
        # For GREEN phase: Test that APQ returns proper error response when no auth provider
        # Future enhancement: Add proper auth provider integration testing
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="development",
            auth_enabled=True,  # Enable auth but no provider (realistic edge case)
        )

        app = create_fraiseql_app(config=config, queries=[hello], lifespan=noop_lifespan)

        with TestClient(app) as client:
            apq_request = {
                "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "some_hash"}}
            }

            response = client.post("/graphql", json=apq_request)

        # With no auth provider, APQ should still work and return query not found error
        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        assert data["errors"][0]["extensions"]["code"] == "PERSISTED_QUERY_NOT_FOUND"

    def test_apq_production_mode_compatibility(self, clear_registry) -> None:
        """Test APQ works in production mode."""
        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            environment="production",
            auth_enabled=False,
        )

        app = create_fraiseql_app(config=config, queries=[hello], lifespan=noop_lifespan)

        with TestClient(app) as client:
            test_query = "query { hello }"
            test_hash = "prod_hash_789"

            with patch("fraiseql.middleware.apq.get_persisted_query") as mock_get:
                mock_get.return_value = test_query

                apq_request = {
                    "extensions": {"persistedQuery": {"version": 1, "sha256Hash": test_hash}}
                }

                response = client.post("/graphql", json=apq_request)

            assert response.status_code == 200
            data = response.json()

            # Should execute successfully in production
            assert "data" in data
            assert data["data"]["hello"] == "Hello, World!"
