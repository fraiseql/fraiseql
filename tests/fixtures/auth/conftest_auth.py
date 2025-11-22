"""Common authentication fixtures for tests."""

from unittest.mock import Mock

import pytest


@pytest.fixture
def mock_auth_context() -> None:
    """Mock authentication context with user, token, and permissions."""
    return {
        "user": {"id": "123", "email": "test@example.com", "role": "admin"},
        "token": "test_token",
        "permissions": ["read", "write", "delete"],
    }


@pytest.fixture
def mock_request_with_auth(mock_auth_context) -> None:
    """Mock request with authentication headers and user."""
    request = Mock()
    request.headers = {"Authorization": f"Bearer {mock_auth_context['token']}"}
    request.user = mock_auth_context["user"]
    request.cookies = {}
    request.state = Mock()
    request.state.session_id = None
    return request


@pytest.fixture
def authenticated_request() -> None:
    """Mock authenticated request."""
    request = Mock()
    request.user = Mock()
    request.user.id = "test_user_123"
    request.user.is_authenticated = True
    request.user.permissions = ["read", "write"]
    request.headers = {"Authorization": "Bearer test_token_123"}
    request.cookies = {}
    request.state = Mock()
    request.state.session_id = None
    return request


@pytest.fixture
def auth_context(authenticated_request) -> None:
    """Authentication context for GraphQL resolvers."""
    return {
        "request": authenticated_request,
        "user": authenticated_request.user,
        "auth_token": "Bearer test_token_123",
    }


@pytest.fixture
def admin_context() -> None:
    """Admin user context."""
    return {
        "user": {"id": "admin_123", "email": "admin@example.com", "role": "admin"},
        "is_admin": True,
        "permissions": ["read", "write", "delete", "admin"],
    }


@pytest.fixture
def user_context() -> None:
    """Regular user context."""
    return {
        "user": {"id": "user_456", "email": "user@example.com", "role": "user"},
        "is_admin": False,
        "permissions": ["read"],
    }


@pytest.fixture
def unauthenticated_context() -> None:
    """Unauthenticated context."""
    return {}


@pytest.fixture
def mock_csrf_request() -> None:
    """Mock request with CSRF token."""
    request = Mock()
    request.headers = {"X-CSRF-Token": "test_csrf_token"}
    request.cookies = {"csrf_token": "test_csrf_token"}
    request.method = "POST"
    request.url = Mock()
    request.url.path = "/graphql"
    request.state = Mock()
    request.state.session_id = None
    return request
