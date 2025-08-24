"""Authentication fixtures for FraiseQL testing.

This module provides comprehensive authentication testing utilities including:
- Mock authentication contexts with various permission levels
- JWT token generation and validation helpers
- Request mocking with authentication headers
- User role simulation (admin, user, guest)
- Auth0 integration testing utilities
- CSRF token handling

These fixtures support testing of both authentication flows and
authorization enforcement across the FraiseQL system.
"""

import time
from datetime import UTC, datetime, timedelta
from typing import Any, Dict, List, Optional
from unittest.mock import Mock
from uuid import uuid4

import jwt
import pytest
from fastapi import Request

# Test JWT configuration
TEST_JWT_SECRET = "test-jwt-secret-key-for-fraiseql-testing-only"
TEST_JWT_ALGORITHM = "HS256"


@pytest.fixture
def jwt_secret():
    """JWT secret for testing."""
    return TEST_JWT_SECRET


@pytest.fixture
def jwt_algorithm():
    """JWT algorithm for testing."""
    return TEST_JWT_ALGORITHM


def create_test_token(
    user_id: str,
    email: str,
    role: str = "user",
    permissions: Optional[List[str]] = None,
    expires_in: int = 3600,
    **extra_claims,
) -> str:
    """Create a test JWT token.

    Args:
        user_id: User identifier
        email: User email
        role: User role (admin, user, guest)
        permissions: List of permissions
        expires_in: Token expiration in seconds
        **extra_claims: Additional JWT claims

    Returns:
        str: Encoded JWT token
    """
    if permissions is None:
        permissions = {
            "admin": ["read", "write", "delete", "admin"],
            "user": ["read", "write"],
            "guest": ["read"],
        }.get(role, ["read"])

    now = datetime.now(UTC)
    payload = {
        "sub": user_id,
        "email": email,
        "role": role,
        "permissions": permissions,
        "iat": int(now.timestamp()),
        "exp": int((now + timedelta(seconds=expires_in)).timestamp()),
        "iss": "fraiseql-test",
        "aud": "fraiseql-api",
        **extra_claims,
    }

    return jwt.encode(payload, TEST_JWT_SECRET, algorithm=TEST_JWT_ALGORITHM)


@pytest.fixture
def test_token_factory():
    """Factory for creating test JWT tokens."""
    return create_test_token


# User context fixtures
@pytest.fixture
def admin_user():
    """Admin user data."""
    return {
        "id": f"admin_{uuid4().hex[:8]}",
        "email": "admin@fraiseql.test",
        "username": "test_admin",
        "role": "admin",
        "permissions": ["read", "write", "delete", "admin"],
        "is_active": True,
        "is_admin": True,
    }


@pytest.fixture
def regular_user():
    """Regular user data."""
    return {
        "id": f"user_{uuid4().hex[:8]}",
        "email": "user@fraiseql.test",
        "username": "test_user",
        "role": "user",
        "permissions": ["read", "write"],
        "is_active": True,
        "is_admin": False,
    }


@pytest.fixture
def guest_user():
    """Guest user data."""
    return {
        "id": f"guest_{uuid4().hex[:8]}",
        "email": "guest@fraiseql.test",
        "username": "test_guest",
        "role": "guest",
        "permissions": ["read"],
        "is_active": True,
        "is_admin": False,
    }


@pytest.fixture
def inactive_user():
    """Inactive user data."""
    return {
        "id": f"inactive_{uuid4().hex[:8]}",
        "email": "inactive@fraiseql.test",
        "username": "inactive_user",
        "role": "user",
        "permissions": [],
        "is_active": False,
        "is_admin": False,
    }


# Token fixtures
@pytest.fixture
def admin_token(admin_user, test_token_factory):
    """JWT token for admin user."""
    return test_token_factory(
        user_id=admin_user["id"],
        email=admin_user["email"],
        role=admin_user["role"],
        permissions=admin_user["permissions"],
    )


@pytest.fixture
def user_token(regular_user, test_token_factory):
    """JWT token for regular user."""
    return test_token_factory(
        user_id=regular_user["id"],
        email=regular_user["email"],
        role=regular_user["role"],
        permissions=regular_user["permissions"],
    )


@pytest.fixture
def guest_token(guest_user, test_token_factory):
    """JWT token for guest user."""
    return test_token_factory(
        user_id=guest_user["id"],
        email=guest_user["email"],
        role=guest_user["role"],
        permissions=guest_user["permissions"],
    )


@pytest.fixture
def expired_token(regular_user, test_token_factory):
    """Expired JWT token."""
    return test_token_factory(
        user_id=regular_user["id"],
        email=regular_user["email"],
        role=regular_user["role"],
        permissions=regular_user["permissions"],
        expires_in=-3600,  # Expired 1 hour ago
    )


# Request mocking fixtures
@pytest.fixture
def mock_request_factory():
    """Factory for creating mock requests."""

    def create_request(
        method: str = "POST",
        url: str = "https://api.fraiseql.test/graphql",
        headers: Optional[Dict[str, str]] = None,
        user: Optional[Dict[str, Any]] = None,
        token: Optional[str] = None,
        **kwargs,
    ) -> Mock:
        """Create a mock request.

        Args:
            method: HTTP method
            url: Request URL
            headers: HTTP headers
            user: User data
            token: JWT token
            **kwargs: Additional request attributes

        Returns:
            Mock: Mock request object
        """
        request = Mock(spec=Request)
        request.method = method
        request.url = Mock()
        request.url.path = "/graphql"
        request.url.scheme = "https"

        # Set up headers
        request.headers = headers or {}
        if token:
            request.headers["Authorization"] = f"Bearer {token}"

        # Set up user
        request.user = user

        # Set up other attributes
        request.cookies = kwargs.get("cookies", {})
        request.state = Mock()
        request.state.session_id = kwargs.get("session_id")

        # Add any additional attributes
        for key, value in kwargs.items():
            if not hasattr(request, key):
                setattr(request, key, value)

        return request

    return create_request


@pytest.fixture
def authenticated_request(mock_request_factory, regular_user, user_token):
    """Mock authenticated request with regular user."""
    return mock_request_factory(user=regular_user, token=user_token, method="POST")


@pytest.fixture
def admin_request(mock_request_factory, admin_user, admin_token):
    """Mock authenticated request with admin user."""
    return mock_request_factory(user=admin_user, token=admin_token, method="POST")


@pytest.fixture
def unauthenticated_request(mock_request_factory):
    """Mock unauthenticated request."""
    return mock_request_factory(method="POST")


# GraphQL context fixtures
@pytest.fixture
def graphql_context_factory():
    """Factory for creating GraphQL contexts."""

    def create_context(
        request: Optional[Mock] = None,
        user: Optional[Dict[str, Any]] = None,
        db: Optional[Any] = None,
        **extra_context,
    ) -> Dict[str, Any]:
        """Create GraphQL execution context.

        Args:
            request: HTTP request object
            user: User data
            db: Database connection/repository
            **extra_context: Additional context data

        Returns:
            Dict: GraphQL context
        """
        context = {"request": request, "user": user, "db": db, **extra_context}

        # Add user info to context if available
        if user:
            context.update(
                {
                    "user_id": user.get("id"),
                    "user_role": user.get("role"),
                    "user_permissions": user.get("permissions", []),
                    "is_authenticated": True,
                    "is_admin": user.get("is_admin", False),
                }
            )
        else:
            context.update(
                {
                    "user_id": None,
                    "user_role": None,
                    "user_permissions": [],
                    "is_authenticated": False,
                    "is_admin": False,
                }
            )

        return context

    return create_context


@pytest.fixture
def admin_context(graphql_context_factory, admin_request, admin_user):
    """GraphQL context for admin user."""
    return graphql_context_factory(request=admin_request, user=admin_user)


@pytest.fixture
def user_context(graphql_context_factory, authenticated_request, regular_user):
    """GraphQL context for regular user."""
    return graphql_context_factory(request=authenticated_request, user=regular_user)


@pytest.fixture
def guest_context(graphql_context_factory, guest_user):
    """GraphQL context for guest user."""
    return graphql_context_factory(user=guest_user)


@pytest.fixture
def anonymous_context(graphql_context_factory):
    """GraphQL context for anonymous user."""
    return graphql_context_factory()


# CSRF fixtures
@pytest.fixture
def csrf_token():
    """CSRF token for testing."""
    return f"csrf_{uuid4().hex[:16]}"


@pytest.fixture
def csrf_request(mock_request_factory, csrf_token):
    """Request with CSRF token."""
    return mock_request_factory(
        method="POST", headers={"X-CSRF-Token": csrf_token}, cookies={"csrf_token": csrf_token}
    )


# Auth0 integration fixtures
@pytest.fixture
def auth0_config():
    """Auth0 configuration for testing."""
    return {
        "domain": "fraiseql-test.auth0.com",
        "client_id": "test_client_id_123",
        "client_secret": "test_client_secret_456",
        "audience": "https://api.fraiseql.test",
        "algorithms": ["RS256"],
    }


@pytest.fixture
def auth0_user():
    """Auth0 user profile."""
    return {
        "sub": "auth0|123456789",
        "nickname": "testuser",
        "name": "Test User",
        "picture": "https://avatar.example.com/test.jpg",
        "email": "test@example.com",
        "email_verified": True,
        "iss": "https://fraiseql-test.auth0.com/",
        "aud": "test_client_id_123",
        "iat": int(time.time()),
        "exp": int(time.time() + 3600),
    }


# Permission testing utilities
def has_permission(context: Dict[str, Any], permission: str) -> bool:
    """Check if context has specific permission.

    Args:
        context: GraphQL context
        permission: Permission to check

    Returns:
        bool: True if permission exists
    """
    return permission in context.get("user_permissions", [])


def is_admin(context: Dict[str, Any]) -> bool:
    """Check if context represents admin user.

    Args:
        context: GraphQL context

    Returns:
        bool: True if admin user
    """
    return context.get("is_admin", False)


def is_authenticated(context: Dict[str, Any]) -> bool:
    """Check if context represents authenticated user.

    Args:
        context: GraphQL context

    Returns:
        bool: True if authenticated
    """
    return context.get("is_authenticated", False)
