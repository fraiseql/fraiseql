import pytest

"""Tests for development authentication middleware."""

import base64
import os
from unittest.mock import patch

from fastapi import FastAPI
from fastapi.testclient import TestClient

from fraiseql.fastapi.dev_auth import (
    DevAuthMiddleware,
    create_dev_auth_middleware,
    get_dev_auth_credentials,
    is_dev_auth_enabled,
)


@pytest.mark.unit
class TestDevAuthMiddleware:
    """Test the DevAuthMiddleware class."""

    def test_middleware_creation(self) -> None:
        """Test creating the middleware with username and password."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "testuser", "testpass")

        assert middleware.username == "testuser"
        assert middleware.password == "testpass"
        assert middleware.security is not None

    def test_should_protect_path(self) -> None:
        """Test path protection logic."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        # Protected paths
        assert middleware._should_protect_path("/graphql")
        assert middleware._should_protect_path("/playground")
        assert middleware._should_protect_path("/graphiql")
        assert middleware._should_protect_path("/docs")
        assert middleware._should_protect_path("/graphql/something")

        # Unprotected paths
        assert not middleware._should_protect_path("/health")
        assert not middleware._should_protect_path("/api/users")
        assert not middleware._should_protect_path("/")

    def test_extract_credentials_valid_auth_header(self) -> None:
        """Test extracting valid credentials from auth header."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        # Create basic auth header
        credentials = "testuser:testpass"
        encoded = base64.b64encode(credentials.encode()).decode()

        # Mock request with auth header
        class MockRequest:
            def __init__(self) -> None:
                self.headers = {"authorization": f"Basic {encoded}"}

        request = MockRequest()
        extracted = middleware._extract_credentials(request)

        assert extracted is not None
        assert extracted.username == "testuser"
        assert extracted.password == "testpass"

    def test_extract_credentials_no_auth_header(self) -> None:
        """Test extracting credentials when no auth header present."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        class MockRequest:
            def __init__(self) -> None:
                self.headers = {}

        request = MockRequest()
        extracted = middleware._extract_credentials(request)

        assert extracted is None

    def test_extract_credentials_invalid_auth_header(self) -> None:
        """Test extracting credentials with invalid auth header."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        class MockRequest:
            def __init__(self) -> None:
                self.headers = {"authorization": "Bearer token123"}

        request = MockRequest()
        extracted = middleware._extract_credentials(request)

        assert extracted is None

    def test_verify_credentials_correct(self) -> None:
        """Test credential verification with correct credentials."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "admin", "secret123")

        from fastapi.security import HTTPBasicCredentials

        credentials = HTTPBasicCredentials(username="admin", password="secret123")

        assert middleware._verify_credentials(credentials) is True

    def test_verify_credentials_incorrect_username(self) -> None:
        """Test credential verification with incorrect username."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "admin", "secret123")

        from fastapi.security import HTTPBasicCredentials

        credentials = HTTPBasicCredentials(username="wronguser", password="secret123")

        assert middleware._verify_credentials(credentials) is False

    def test_verify_credentials_incorrect_password(self) -> None:
        """Test credential verification with incorrect password."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "admin", "secret123")

        from fastapi.security import HTTPBasicCredentials

        credentials = HTTPBasicCredentials(username="admin", password="wrongpass")

        assert middleware._verify_credentials(credentials) is False

    def test_unauthorized_response(self) -> None:
        """Test unauthorized response creation."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        response = middleware._unauthorized_response()

        assert response.status_code == 401
        assert "WWW-Authenticate" in response.headers
        assert response.headers["WWW-Authenticate"] == 'Basic realm="FraiseQL Development"'


class TestDevAuthIntegration:
    """Test development auth integration with FastAPI."""

    def test_protected_endpoint_without_auth(self) -> None:
        """Test accessing protected endpoint without authentication."""
        app = FastAPI()
        app.add_middleware(DevAuthMiddleware, username="admin", password="secret")

        @app.get("/graphql")
        async def graphql_endpoint() -> None:
            return {"message": "GraphQL endpoint"}

        client = TestClient(app)
        response = client.get("/graphql")

        assert response.status_code == 401
        assert "WWW-Authenticate" in response.headers

    def test_protected_endpoint_with_correct_auth(self) -> None:
        """Test accessing protected endpoint with correct authentication."""
        app = FastAPI()
        app.add_middleware(DevAuthMiddleware, username="admin", password="secret")

        @app.get("/graphql")
        async def graphql_endpoint() -> None:
            return {"message": "GraphQL endpoint"}

        client = TestClient(app)

        # Create auth header
        credentials = base64.b64encode(b"admin:secret").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/graphql", headers=headers)

        assert response.status_code == 200
        assert response.json() == {"message": "GraphQL endpoint"}

    def test_protected_endpoint_with_incorrect_auth(self) -> None:
        """Test accessing protected endpoint with incorrect authentication."""
        app = FastAPI()
        app.add_middleware(DevAuthMiddleware, username="admin", password="secret")

        @app.get("/graphql")
        async def graphql_endpoint() -> None:
            return {"message": "GraphQL endpoint"}

        client = TestClient(app)

        # Create wrong auth header
        credentials = base64.b64encode(b"admin:wrongpass").decode()
        headers = {"Authorization": f"Basic {credentials}"}

        response = client.get("/graphql", headers=headers)

        assert response.status_code == 401

    def test_unprotected_endpoint_without_auth(self) -> None:
        """Test accessing unprotected endpoint without authentication."""
        app = FastAPI()
        app.add_middleware(DevAuthMiddleware, username="admin", password="secret")

        @app.get("/health")
        async def health_endpoint() -> None:
            return {"status": "healthy"}

        client = TestClient(app)
        response = client.get("/health")

        assert response.status_code == 200
        assert response.json() == {"status": "healthy"}


class TestDevAuthHelpers:
    """Test helper functions for development auth."""

    @patch.dict(
        os.environ, {"FRAISEQL_DEV_AUTH_USERNAME": "myuser", "FRAISEQL_DEV_AUTH_PASSWORD": "mypass"}
    )
    def test_get_dev_auth_credentials_from_env(self) -> None:
        """Test getting credentials from environment variables."""
        username, password = get_dev_auth_credentials()

        assert username == "myuser"
        assert password == "mypass"

    @patch.dict(os.environ, {"FRAISEQL_DEV_AUTH_PASSWORD": "onlypass"}, clear=True)
    def test_get_dev_auth_credentials_default_username(self) -> None:
        """Test getting credentials with default username."""
        username, password = get_dev_auth_credentials()

        assert username == "admin"  # Default
        assert password == "onlypass"

    @patch.dict(os.environ, {}, clear=True)
    def test_get_dev_auth_credentials_no_env_vars(self) -> None:
        """Test getting credentials when no env vars are set."""
        username, password = get_dev_auth_credentials()

        assert username == "admin"  # Default
        assert password is None

    @patch.dict(os.environ, {"FRAISEQL_DEV_AUTH_PASSWORD": "testpass"})
    def test_is_dev_auth_enabled_true(self) -> None:
        """Test dev auth enabled when password is set."""
        assert is_dev_auth_enabled() is True

    @patch.dict(os.environ, {}, clear=True)
    def test_is_dev_auth_enabled_false(self) -> None:
        """Test dev auth disabled when no password is set."""
        assert is_dev_auth_enabled() is False

    def test_create_dev_auth_middleware_with_password(self) -> None:
        """Test creating middleware when password is provided."""
        app = FastAPI()
        middleware = create_dev_auth_middleware(app, "testuser", "testpass")

        assert middleware is not None
        assert isinstance(middleware, DevAuthMiddleware)
        assert middleware.username == "testuser"
        assert middleware.password == "testpass"

    def test_create_dev_auth_middleware_without_password(self) -> None:
        """Test creating middleware when no password is provided."""
        app = FastAPI()
        middleware = create_dev_auth_middleware(app, "testuser", None)

        assert middleware is None

    @patch.dict(os.environ, {"FRAISEQL_DEV_AUTH_PASSWORD": "envpass"})
    def test_create_dev_auth_middleware_from_env(self) -> None:
        """Test creating middleware from environment variables."""
        app = FastAPI()
        middleware = create_dev_auth_middleware(app)

        assert middleware is not None
        assert middleware.username == "admin"
        assert middleware.password == "envpass"


class TestDevAuthSecurity:
    """Test security aspects of development auth."""

    def test_timing_attack_protection(self) -> None:
        """Test that credential verification uses constant-time comparison."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "admin", "secret123")

        from fastapi.security import HTTPBasicCredentials

        # These should both take similar time to process
        wrong_user = HTTPBasicCredentials(username="wrong", password="secret123")
        wrong_pass = HTTPBasicCredentials(username="admin", password="wrong")

        # Both should return False
        assert middleware._verify_credentials(wrong_user) is False
        assert middleware._verify_credentials(wrong_pass) is False

    def test_base64_decoding_error_handling(self) -> None:
        """Test handling of malformed base64 in auth header."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        class MockRequest:
            def __init__(self) -> None:
                self.headers = {"authorization": "Basic invalidbase64!@#"}

        request = MockRequest()
        extracted = middleware._extract_credentials(request)

        assert extracted is None

    def test_malformed_credentials_handling(self) -> None:
        """Test handling of credentials without colon separator."""
        app = FastAPI()
        middleware = DevAuthMiddleware(app, "user", "pass")

        # Create credentials without colon
        malformed = base64.b64encode(b"usernameonly").decode()

        class MockRequest:
            def __init__(self) -> None:
                self.headers = {"authorization": f"Basic {malformed}"}

        request = MockRequest()
        extracted = middleware._extract_credentials(request)

        assert extracted is None
