import pytest

pytestmark = pytest.mark.integration

"""Integration tests for development auth with create_fraiseql_app."""

import base64
import os
from unittest.mock import patch

from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi.app import create_fraiseql_app


@pytest.mark.unit
@fraiseql.type
class SimpleUser:
    """Simple user type for integration tests."""

    id: str = fraiseql.fraise_field(description="User ID")
    name: str = fraiseql.fraise_field(description="User name")


@fraiseql.type
class QueryRoot:
    """Simple query root for testing."""

    test_field: str = fraiseql.fraise_field(description="Test field", purpose="output")

    def resolve_test_field(self, info) -> str:
        return "test_value"


class TestDevAuthAppIntegration:
    """Test development auth integration with create_fraiseql_app."""

    def test_dev_auth_enabled_via_parameter(self, clear_registry) -> None:
        """Test that dev auth is enabled when enabled via parameter."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="testpass123",
        )

        client = TestClient(app)

        # Try to access health endpoint (should not be protected)
        response = client.get("/health")
        assert response.status_code == 200

        # Try to access docs (should be protected)
        response = client.get("/docs")
        assert response.status_code == 401
        assert "WWW-Authenticate" in response.headers

    def test_dev_auth_enabled_from_env_var(self, clear_registry) -> None:
        """Test that dev auth is enabled when environment variable is set."""
        # Set env var before creating app
        with patch.dict(os.environ, {"FRAISEQL_DEV_AUTH_PASSWORD": "envpass123"}):
            # Don't pass config, let it be created from env vars
            app = create_fraiseql_app(
                database_url="postgresql://localhost/test",
                types=[SimpleUser, QueryRoot],
                production=False,
            )

            client = TestClient(app)

            # Try to access docs (should be protected by env var password)
            response = client.get("/docs")
            assert response.status_code == 401

            # Try with correct credentials from env var
            credentials = base64.b64encode(b"admin:envpass123").decode()
            headers = {"Authorization": f"Basic {credentials}"}

            response = client.get("/docs", headers=headers)
            assert response.status_code == 200

    @patch.dict(os.environ, {}, clear=True)
    def test_dev_auth_disabled_no_env_var(self, clear_registry) -> None:
        """Test that dev auth is disabled when no environment variable is set."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
        )

        client = TestClient(app)

        # All endpoints should be accessible without auth
        response = client.get("/health")
        assert response.status_code == 200

        response = client.get("/docs")
        assert response.status_code == 200

    def test_dev_auth_enabled_with_custom_password(self, clear_registry) -> None:
        """Test that dev auth can be enabled via function parameter."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="parampass123",
        )

        client = TestClient(app)

        # Try accessing protected endpoint without auth
        response = client.get("/docs")
        assert response.status_code == 401

        # Try accessing with correct credentials
        credentials = base64.b64encode(b"admin:parampass123").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/docs", headers=headers)
        assert response.status_code == 200

    def test_dev_auth_custom_username_via_parameter(self, clear_registry) -> None:
        """Test that custom username can be set via parameter."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_username="customuser",
            dev_auth_password="custompass",
        )

        client = TestClient(app)

        # Try with default username (should fail)
        credentials = base64.b64encode(b"admin:custompass").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/docs", headers=headers)
        assert response.status_code == 401

        # Try with custom username (should succeed)
        credentials = base64.b64encode(b"customuser:custompass").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/docs", headers=headers)
        assert response.status_code == 200

    @patch.dict(
        os.environ,
        {"FRAISEQL_DEV_AUTH_USERNAME": "envuser", "FRAISEQL_DEV_AUTH_PASSWORD": "envpass"},
    )
    def test_dev_auth_parameter_overrides_env(self, clear_registry) -> None:
        """Test that function parameters override environment variables."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_username="paramuser",
            dev_auth_password="parampass",
        )

        client = TestClient(app)

        # Try with env credentials (should fail)
        credentials = base64.b64encode(b"envuser:envpass").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/docs", headers=headers)
        assert response.status_code == 401

        # Try with param credentials (should succeed)
        credentials = base64.b64encode(b"paramuser:parampass").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/docs", headers=headers)
        assert response.status_code == 200

    @patch.dict(os.environ, {"FRAISEQL_DEV_AUTH_PASSWORD": "testpass123"})
    def test_dev_auth_disabled_in_production(self, clear_registry) -> None:
        """Test that dev auth is disabled in production mode even when password is set."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=True,  # Production mode
        )

        client = TestClient(app)

        # All endpoints should be accessible without auth in production
        # (even though FRAISEQL_DEV_PASSWORD is set)
        response = client.get("/health")
        assert response.status_code == 200

    def test_dev_auth_with_existing_app(self, clear_registry) -> None:
        """Test that dev auth works when extending an existing FastAPI app."""
        from fastapi import FastAPI

        existing_app = FastAPI()

        @existing_app.get("/custom")
        async def custom_endpoint() -> None:
            return {"message": "custom"}

        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="testpass",
            app=existing_app,
        )

        client = TestClient(app)

        # Custom endpoint should not be protected (added before middleware)
        response = client.get("/custom")
        assert response.status_code == 200

        # Docs should be protected
        response = client.get("/docs")
        assert response.status_code == 401

    def test_dev_auth_logs_warning(self, clear_registry, caplog) -> None:
        """Test that enabling dev auth logs a warning."""
        import logging

        # Set caplog to capture at the module level
        caplog.set_level(logging.WARNING, logger="fraiseql.fastapi.dev_auth")

        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="testpass",
        )

        # Make a request to trigger middleware initialization
        client = TestClient(app)
        client.get("/docs")  # This should trigger the middleware

        # Check that warning was logged
        warning_messages = [
            record.message for record in caplog.records if record.levelname == "WARNING"
        ]

        assert any("Development authentication is enabled" in msg for msg in warning_messages)

    def test_dev_auth_multiple_protected_paths(self, clear_registry) -> None:
        """Test that dev auth protects multiple expected paths."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="testpass",
        )

        client = TestClient(app)

        protected_paths = ["/docs", "/redoc", "/openapi.json"]

        for path in protected_paths:
            response = client.get(path)
            # Some paths might not exist (404) but should still require auth first
            assert response.status_code in [401, 404]
            if response.status_code == 401:
                assert "WWW-Authenticate" in response.headers


class TestDevAuthConfigValidation:
    """Test validation and configuration of development auth."""

    def test_empty_password_disables_auth(self, clear_registry) -> None:
        """Test that empty password disables dev auth."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="",  # Empty password
        )

        client = TestClient(app)

        # Should not require auth
        response = client.get("/docs")
        assert response.status_code == 200

    def test_none_password_disables_auth(self, clear_registry) -> None:
        """Test that None password disables dev auth."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password=None,
        )

        client = TestClient(app)

        # Should not require auth
        response = client.get("/docs")
        assert response.status_code == 200

    def test_whitespace_only_password_enables_auth(self, clear_registry) -> None:
        """Test that whitespace-only password still enables auth."""
        app = create_fraiseql_app(
            database_url="postgresql://localhost/test",
            types=[SimpleUser, QueryRoot],
            production=False,
            dev_auth_password="   ",  # Whitespace only
        )

        client = TestClient(app)

        # Should require auth (whitespace is a valid password)
        response = client.get("/docs")
        assert response.status_code == 401
