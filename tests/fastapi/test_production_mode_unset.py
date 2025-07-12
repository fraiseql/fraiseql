"""Tests for production mode UNSET handling and error logging."""

import logging
from unittest.mock import Mock, patch

from fastapi.testclient import TestClient
from graphql import build_schema

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.routers import create_production_router
from fraiseql.types.definitions import UNSET


class TestProductionModeUnsetHandling:
    """Test UNSET handling in production mode."""

    def setup_method(self):
        """Set up test fixtures."""
        self.schema = build_schema("""
            type Query {
                hello: String
                user(id: Int!): User
            }
            type User {
                id: Int!
                name: String
                email: String
            }
        """)
        self.config = FraiseQLConfig(database_url="postgresql://test:test@localhost/test")
        self.config.environment = "production"

    @patch("fraiseql.fastapi.routers.graphql")
    def test_production_unset_in_data_cleaned(self, mock_graphql):
        """Test that UNSET values in data are properly cleaned."""
        # Mock GraphQL result with UNSET values
        mock_result = Mock()
        mock_result.data = {
            "user": {
                "id": 123,
                "name": "John",
                "email": UNSET,  # This should be cleaned to None
            },
        }
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        # Mock the context dependency
        mock_context = {"db": Mock()}
        with patch("fraiseql.fastapi.routers.build_graphql_context", return_value=mock_context):
            router = create_production_router(self.schema, self.config)
            client = TestClient(router)

            response = client.post(
                "/graphql",
                json={"query": "{ user(id: 123) { id name email } }"},
            )

            assert response.status_code == 200
            data = response.json()
            assert data["data"]["user"]["email"] is None  # UNSET converted to None
            assert data["data"]["user"]["id"] == 123
            assert data["data"]["user"]["name"] == "John"

    @patch("fraiseql.fastapi.routers.graphql")
    def test_production_unset_in_errors_cleaned(self, mock_graphql):
        """Test that UNSET values in error extensions are cleaned."""
        # Mock GraphQL result with errors containing UNSET
        mock_error = Mock()
        mock_error.message = "Test error"
        mock_error.locations = None
        mock_error.path = None
        mock_error.extensions = {
            "code": "TEST_ERROR",
            "details": UNSET,  # This should be cleaned
            "info": {"valid": True, "invalid": UNSET},
        }

        mock_result = Mock()
        mock_result.data = None
        mock_result.errors = [mock_error]
        mock_graphql.return_value = mock_result

        router = create_production_router(self.schema, self.config)
        client = TestClient(router)

        response = client.post(
            "/graphql",
            json={"query": "{ hello }"},
        )

        assert response.status_code == 200
        data = response.json()
        assert "errors" in data
        # In production mode, error messages are hidden but extensions should be cleaned
        assert data["errors"][0]["message"] == "Internal server error"

    def test_production_exception_logging(self, caplog):
        """Test that exceptions are properly logged in production mode."""
        # Mock the context dependency to avoid database requirement
        mock_context = {"db": Mock()}

        with patch("fraiseql.fastapi.routers.graphql", side_effect=RuntimeError("Test error")):
            with patch("fraiseql.fastapi.routers.build_graphql_context", return_value=mock_context):
                router = create_production_router(self.schema, self.config)
                client = TestClient(router)

                with caplog.at_level(logging.ERROR):
                    response = client.post(
                        "/graphql",
                        json={"query": "{ hello }"},
                    )

                assert response.status_code == 200
                data = response.json()
                assert data["errors"][0]["message"] == "Internal server error"

                # Check that the actual error was logged
                assert "Production GraphQL execution error: Test error" in caplog.text

    def test_production_unset_serialization_error_logging(self, caplog):
        """Test specific logging for UNSET serialization errors."""
        unset_error = TypeError("Object of type Unset is not JSON serializable")

        with patch("fraiseql.fastapi.routers.graphql", side_effect=unset_error):
            router = create_production_router(self.schema, self.config)
            client = TestClient(router)

            with caplog.at_level(logging.ERROR):
                response = client.post(
                    "/graphql",
                    json={
                        "query": "{ user(id: 123) { name } }",
                        "variables": {"userId": 123},
                    },
                )

            assert response.status_code == 200
            data = response.json()
            assert data["errors"][0]["message"] == "Internal server error"

            # Check that UNSET-specific error was logged
            assert "UNSET serialization error in production mode" in caplog.text
            assert "Query: { user(id: 123) { name } }" in caplog.text
            assert "Variables: {'userId': 123}" in caplog.text

    def test_production_long_query_truncation_in_logging(self, caplog):
        """Test that long queries are truncated in error logs."""
        long_query = "{ " + "field " * 100 + "}"  # Very long query
        unset_error = TypeError("Object of type Unset is not JSON serializable")

        with patch("fraiseql.fastapi.routers.graphql", side_effect=unset_error):
            router = create_production_router(self.schema, self.config)
            client = TestClient(router)

            with caplog.at_level(logging.ERROR):
                response = client.post(
                    "/graphql",
                    json={"query": long_query},
                )

            assert response.status_code == 200

            # Check that query was truncated to 200 characters
            logged_query = None
            for record in caplog.records:
                if "UNSET serialization error" in record.message:
                    # Extract the query part from the log message
                    message_parts = record.message.split("Query: ")
                    if len(message_parts) > 1:
                        logged_query = message_parts[1].split(", Variables:")[0]
                        break

            assert logged_query is not None
            assert len(logged_query) <= 200

    @patch("fraiseql.fastapi.routers.graphql")
    def test_production_nested_unset_values_cleaned(self, mock_graphql):
        """Test that nested UNSET values are properly cleaned."""
        # Mock GraphQL result with deeply nested UNSET values
        mock_result = Mock()
        mock_result.data = {
            "users": [
                {
                    "id": 1,
                    "profile": {
                        "name": "John",
                        "email": UNSET,
                        "settings": {
                            "theme": "dark",
                            "notifications": UNSET,
                        },
                    },
                },
                {
                    "id": 2,
                    "profile": UNSET,
                },
            ],
        }
        mock_result.errors = None
        mock_graphql.return_value = mock_result

        router = create_production_router(self.schema, self.config)
        client = TestClient(router)

        response = client.post(
            "/graphql",
            json={
                "query": "{ users { id profile { name email settings { theme notifications } } } }",
            },
        )

        assert response.status_code == 200
        data = response.json()

        users = data["data"]["users"]

        # First user - UNSET values should be None
        assert users[0]["profile"]["email"] is None
        assert users[0]["profile"]["settings"]["notifications"] is None
        assert users[0]["profile"]["name"] == "John"
        assert users[0]["profile"]["settings"]["theme"] == "dark"

        # Second user - UNSET profile should be None
        assert users[1]["profile"] is None
        assert users[1]["id"] == 2

    def test_production_no_variables_in_unset_error_logging(self, caplog):
        """Test UNSET error logging when no variables are provided."""
        unset_error = TypeError("Object of type Unset is not JSON serializable")

        with patch("fraiseql.fastapi.routers.graphql", side_effect=unset_error):
            router = create_production_router(self.schema, self.config)
            client = TestClient(router)

            with caplog.at_level(logging.ERROR):
                response = client.post(
                    "/graphql",
                    json={"query": "{ hello }"},  # No variables
                )

            assert response.status_code == 200

            # Check that Variables: None is logged when no variables provided
            assert "Variables: None" in caplog.text
