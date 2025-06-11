"""Base authentication interfaces for FraiseQL."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


@dataclass
class UserContext:
    """User context passed to GraphQL resolvers."""

    user_id: str
    email: str | None = None
    name: str | None = None
    roles: list[str] = field(default_factory=list)
    permissions: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def has_role(self, role: str) -> bool:
        """Check if user has a specific role."""
        return role in self.roles

    def has_permission(self, permission: str) -> bool:
        """Check if user has a specific permission."""
        return permission in self.permissions

    def has_any_role(self, roles: list[str]) -> bool:
        """Check if user has any of the specified roles."""
        return any(role in self.roles for role in roles)

    def has_any_permission(self, permissions: list[str]) -> bool:
        """Check if user has any of the specified permissions."""
        return any(perm in self.permissions for perm in permissions)

    def has_all_roles(self, roles: list[str]) -> bool:
        """Check if user has all specified roles."""
        return all(role in self.roles for role in roles)

    def has_all_permissions(self, permissions: list[str]) -> bool:
        """Check if user has all specified permissions."""
        return all(perm in self.permissions for perm in permissions)


class AuthProvider(ABC):
    """Abstract base class for authentication providers."""

    @abstractmethod
    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate a token and return the decoded payload.

        Args:
            token: The authentication token to validate

        Returns:
            Decoded token payload

        Raises:
            AuthenticationError: If token is invalid
        """
        pass

    @abstractmethod
    async def get_user_from_token(self, token: str) -> UserContext:
        """Get user context from a token.

        Args:
            token: The authentication token

        Returns:
            UserContext object with user information

        Raises:
            AuthenticationError: If token is invalid or user not found
        """
        pass

    async def refresh_token(self, refresh_token: str) -> tuple[str, str]:
        """Refresh an access token using a refresh token.

        Args:
            refresh_token: The refresh token

        Returns:
            Tuple of (new_access_token, new_refresh_token)

        Raises:
            NotImplementedError: If provider doesn't support refresh
            AuthenticationError: If refresh token is invalid
        """
        raise NotImplementedError("Token refresh not supported by this provider")

    async def revoke_token(self, token: str) -> None:
        """Revoke a token.

        Args:
            token: The token to revoke

        Raises:
            NotImplementedError: If provider doesn't support revocation
        """
        raise NotImplementedError("Token revocation not supported by this provider")


class AuthenticationError(Exception):
    """Base exception for authentication errors."""

    pass


class TokenExpiredError(AuthenticationError):
    """Token has expired."""

    pass


class InvalidTokenError(AuthenticationError):
    """Token is invalid or malformed."""

    pass


class InsufficientPermissionsError(AuthenticationError):
    """User lacks required permissions."""

    pass
