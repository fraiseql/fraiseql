# Extracted from: docs/advanced/authentication.md
# Block number: 1
from abc import ABC, abstractmethod
from typing import Any


class AuthProvider(ABC):
    """Abstract base for authentication providers."""

    @abstractmethod
    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate token and return decoded payload.

        Raises:
            TokenExpiredError: If token has expired
            InvalidTokenError: If token is invalid
        """

    @abstractmethod
    async def get_user_from_token(self, token: str) -> UserContext:
        """Extract UserContext from validated token."""

    async def refresh_token(self, refresh_token: str) -> tuple[str, str]:
        """Optional: Refresh access token.

        Returns:
            Tuple of (new_access_token, new_refresh_token)
        """
        raise NotImplementedError("Token refresh not supported")

    async def revoke_token(self, token: str) -> None:
        """Optional: Revoke a token."""
        raise NotImplementedError("Token revocation not supported")
