"""Auth0 authentication provider for FraiseQL."""

from typing import Any

import httpx
import jwt
from jwt import PyJWKClient

from fraiseql.auth.base import (
    AuthenticationError,
    AuthProvider,
    InvalidTokenError,
    TokenExpiredError,
    UserContext,
)


class Auth0Provider(AuthProvider):
    """Auth0 authentication provider."""

    def __init__(
        self,
        domain: str,
        api_identifier: str,
        algorithms: list[str] | None = None,
        cache_jwks: bool = True,
    ):
        """Initialize Auth0 provider.

        Args:
            domain: Auth0 domain (e.g., "myapp.auth0.com")
            api_identifier: API identifier/audience
            algorithms: Allowed algorithms (defaults to ["RS256"])
            cache_jwks: Whether to cache JWKS keys
        """
        self.domain = domain
        self.api_identifier = api_identifier
        self.algorithms = algorithms or ["RS256"]
        self.issuer = f"https://{domain}/"
        self.jwks_uri = f"https://{domain}/.well-known/jwks.json"

        # Initialize JWKS client with caching
        self.jwks_client = PyJWKClient(
            self.jwks_uri,
            cache_keys=cache_jwks,
            lifespan=3600,  # Cache for 1 hour
        )

        # HTTP client for Auth0 API calls
        self._http_client: httpx.AsyncClient | None = None

    @property
    async def http_client(self) -> httpx.AsyncClient:
        """Get or create HTTP client."""
        if self._http_client is None:
            self._http_client = httpx.AsyncClient()
        return self._http_client

    async def close(self):
        """Close HTTP client."""
        if self._http_client:
            await self._http_client.aclose()
            self._http_client = None

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate an Auth0 JWT token.

        Args:
            token: JWT token to validate

        Returns:
            Decoded token payload

        Raises:
            TokenExpiredError: If token has expired
            InvalidTokenError: If token is invalid
        """
        try:
            # Get signing key from JWKS
            signing_key = self.jwks_client.get_signing_key_from_jwt(token)

            # Decode and verify token
            payload = jwt.decode(
                token,
                signing_key.key,
                algorithms=self.algorithms,
                audience=self.api_identifier,
                issuer=self.issuer,
            )

            return payload

        except jwt.ExpiredSignatureError as e:
            raise TokenExpiredError("Token has expired") from e
        except jwt.InvalidTokenError as e:
            raise InvalidTokenError(f"Invalid token: {e!s}") from e
        except Exception as e:
            raise AuthenticationError(f"Token validation failed: {e!s}") from e

    async def get_user_from_token(self, token: str) -> UserContext:
        """Get user context from Auth0 token.

        Args:
            token: JWT token

        Returns:
            UserContext with user information

        Raises:
            AuthenticationError: If token is invalid or user not found
        """
        # Validate token and get payload
        payload = await self.validate_token(token)

        # Extract user information
        user_id = payload.get("sub", "")
        email = payload.get("email")
        name = payload.get("name")

        # Extract roles and permissions
        # Auth0 custom claims are typically namespaced
        namespace = f"https://{self.api_identifier}/"
        roles = payload.get(f"{namespace}roles", [])
        permissions = payload.get("permissions", [])

        # If permissions not in token, might need to fetch from Auth0
        if not permissions and payload.get("scope"):
            # Parse scope string into permissions
            permissions = payload["scope"].split()

        # Build metadata from remaining claims
        metadata = {
            k: v
            for k, v in payload.items()
            if k
            not in [
                "sub",
                "email",
                "name",
                "permissions",
                "scope",
                "aud",
                "iss",
                "iat",
                "exp",
            ]
        }

        return UserContext(
            user_id=user_id,
            email=email,
            name=name,
            roles=roles,
            permissions=permissions,
            metadata=metadata,
        )

    async def get_user_profile(self, user_id: str, access_token: str) -> dict[str, Any]:
        """Fetch full user profile from Auth0 Management API.

        Args:
            user_id: Auth0 user ID
            access_token: Management API access token

        Returns:
            User profile data
        """
        client = await self.http_client

        response = await client.get(
            f"https://{self.domain}/api/v2/users/{user_id}",
            headers={"Authorization": f"Bearer {access_token}"},
        )

        if response.status_code != 200:
            raise AuthenticationError(f"Failed to fetch user profile: {response.text}")

        return response.json()

    async def get_user_roles(
        self, user_id: str, access_token: str
    ) -> list[dict[str, Any]]:
        """Fetch user roles from Auth0 Management API.

        Args:
            user_id: Auth0 user ID
            access_token: Management API access token

        Returns:
            List of user roles
        """
        client = await self.http_client

        response = await client.get(
            f"https://{self.domain}/api/v2/users/{user_id}/roles",
            headers={"Authorization": f"Bearer {access_token}"},
        )

        if response.status_code != 200:
            raise AuthenticationError(f"Failed to fetch user roles: {response.text}")

        return response.json()

    async def get_user_permissions(
        self, user_id: str, access_token: str
    ) -> list[dict[str, Any]]:
        """Fetch user permissions from Auth0 Management API.

        Args:
            user_id: Auth0 user ID
            access_token: Management API access token

        Returns:
            List of user permissions
        """
        client = await self.http_client

        response = await client.get(
            f"https://{self.domain}/api/v2/users/{user_id}/permissions",
            headers={"Authorization": f"Bearer {access_token}"},
        )

        if response.status_code != 200:
            raise AuthenticationError(
                f"Failed to fetch user permissions: {response.text}"
            )

        return response.json()


class Auth0Config:
    """Configuration for Auth0 integration."""

    def __init__(
        self,
        domain: str,
        api_identifier: str,
        client_id: str | None = None,
        client_secret: str | None = None,
        algorithms: list[str] | None = None,
    ):
        """Initialize Auth0 configuration.

        Args:
            domain: Auth0 domain
            api_identifier: API identifier/audience
            client_id: Client ID for Management API access
            client_secret: Client secret for Management API access
            algorithms: Allowed JWT algorithms
        """
        self.domain = domain
        self.api_identifier = api_identifier
        self.client_id = client_id
        self.client_secret = client_secret
        self.algorithms = algorithms or ["RS256"]

    def create_provider(self) -> Auth0Provider:
        """Create an Auth0Provider instance from this config."""
        return Auth0Provider(
            domain=self.domain,
            api_identifier=self.api_identifier,
            algorithms=self.algorithms,
        )
