"""Test that authentication fixtures work correctly."""

import pytest

pytestmark = pytest.mark.integration


@pytest.mark.unit
def test_mock_auth_context(mock_auth_context) -> None:
    """Test mock auth context fixture."""
    assert mock_auth_context["user"]["id"] == "123"
    assert mock_auth_context["user"]["email"] == "test@example.com"
    assert mock_auth_context["user"]["role"] == "admin"
    assert mock_auth_context["token"] == "test_token"
    assert "read" in mock_auth_context["permissions"]
    assert "write" in mock_auth_context["permissions"]


def test_mock_request_with_auth(mock_request_with_auth) -> None:
    """Test mock request with auth fixture."""
    assert "Authorization" in mock_request_with_auth.headers
    assert "Bearer test_token" in mock_request_with_auth.headers["Authorization"]
    assert mock_request_with_auth.user["id"] == "123"
    assert hasattr(mock_request_with_auth, "cookies")
    assert hasattr(mock_request_with_auth.state, "session_id")


def test_authenticated_request(authenticated_request) -> None:
    """Test authenticated request fixture."""
    assert authenticated_request.user.id == "test_user_123"
    assert authenticated_request.user.is_authenticated is True
    assert "read" in authenticated_request.user.permissions
    assert "Bearer test_token_123" in authenticated_request.headers["Authorization"]


def test_auth_context(auth_context) -> None:
    """Test auth context fixture."""
    assert auth_context["request"] is not None
    assert auth_context["user"].id == "test_user_123"
    assert auth_context["auth_token"] == "Bearer test_token_123"


def test_admin_context(admin_context) -> None:
    """Test admin context fixture."""
    assert admin_context["user"]["id"] == "admin_123"
    assert admin_context["is_admin"] is True
    assert "admin" in admin_context["permissions"]


def test_user_context(user_context) -> None:
    """Test regular user context fixture."""
    assert user_context["user"]["id"] == "user_456"
    assert user_context["is_admin"] is False
    assert "read" in user_context["permissions"]
    assert "admin" not in user_context["permissions"]


def test_unauthenticated_context(unauthenticated_context) -> None:
    """Test unauthenticated context fixture."""
    assert unauthenticated_context == {}


def test_mock_csrf_request(mock_csrf_request) -> None:
    """Test mock CSRF request fixture."""
    assert mock_csrf_request.headers["X-CSRF-Token"] == "test_csrf_token"
    assert mock_csrf_request.cookies["csrf_token"] == "test_csrf_token"
    assert mock_csrf_request.method == "POST"
    assert mock_csrf_request.url.path == "/graphql"
