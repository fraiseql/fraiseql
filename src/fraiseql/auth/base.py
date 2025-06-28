"""Base authentication interfaces for FraiseQL."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


@dataclass
class UserContext:
    """User context passed to GraphQL resolvers.

    This class represents the authenticated user's context that is available
    in all GraphQL resolver functions. It contains user identification,
    authorization data, and custom metadata.

    Attributes:
        user_id: Unique identifier for the user.
        email: User's email address (optional).
        name: User's display name (optional).
        roles: List of roles assigned to the user.
        permissions: List of permissions granted to the user.
        metadata: Additional custom data about the user.

    Example:
        ```python
        @query
        async def get_my_profile(info: Info) -> User:
            user_context = info.context["user"]
            if not user_context:
                raise AuthenticationError("Not authenticated")

            return await get_user_by_id(user_context.user_id)

        @requires_permission("posts:write")
        async def create_post(info: Info, title: str, content: str) -> Post:
            # Permission check is handled by decorator
            user_context = info.context["user"]
            return await create_post_for_user(
                user_id=user_context.user_id,
                title=title,
                content=content
            )
        ```
    """

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
    """Abstract base class for authentication providers.

    This abstract class defines the interface that all authentication providers
    must implement. FraiseQL supports multiple authentication providers (Auth0,
    JWT, custom) by implementing this interface.

    Subclasses must implement:
    - validate_token: Validate and decode authentication tokens
    - get_user_context: Extract user context from token payload
    - get_jwks_url: Provide JWKS URL for JWT validation (optional)

    Example implementation:
        ```python
        class CustomAuthProvider(AuthProvider):
            async def validate_token(self, token: str) -> dict[str, Any]:
                # Validate token and return payload
                payload = jwt.decode(token, self.secret, algorithms=["HS256"])
                return payload

            async def get_user_context(self, payload: dict[str, Any]) -> UserContext:
                return UserContext(
                    user_id=payload["sub"],
                    email=payload.get("email"),
                    roles=payload.get("roles", []),
                    permissions=payload.get("permissions", [])
                )
        ```
    """

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
        msg = "Token refresh not supported by this provider"
        raise NotImplementedError(msg)

    async def revoke_token(self, token: str) -> None:
        """Revoke a token.

        Args:
            token: The token to revoke

        Raises:
            NotImplementedError: If provider doesn't support revocation
        """
        msg = "Token revocation not supported by this provider"
        raise NotImplementedError(msg)


class AuthenticationError(Exception):
    """Base exception for authentication errors."""


class TokenExpiredError(AuthenticationError):
    """Token has expired."""


class InvalidTokenError(AuthenticationError):
    """Token is invalid or malformed."""


class InsufficientPermissionsError(AuthenticationError):
    """User lacks required permissions."""
