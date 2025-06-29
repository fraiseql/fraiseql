"""Auth0 authentication provider for FraiseQL."""

from typing import Any

import httpx
import jwt
from jwt import PyJWKClient

from fraiseql.audit import SecurityEventSeverity, SecurityEventType, get_security_logger
from fraiseql.auth.base import (
    AuthenticationError,
    AuthProvider,
    InvalidTokenError,
    TokenExpiredError,
    UserContext,
)


class Auth0Provider(AuthProvider):
    """Auth0 authentication provider implementation.

    This provider integrates FraiseQL with Auth0 for authentication and authorization.
    It validates JWT tokens issued by Auth0, extracts user information, and manages
    roles and permissions from Auth0's authorization extension or custom claims.

    Features:
    - Automatic JWKS (JSON Web Key Set) fetching and caching
    - Token validation with RS256 algorithm by default
    - Role and permission extraction from Auth0 claims
    - Configurable token expiration and audience validation
    - Thread-safe token validation

    Example:
        ```python
        from fraiseql.auth import Auth0Provider
        from fraiseql.fastapi import create_fraiseql_app

        # Configure Auth0 provider
        auth_provider = Auth0Provider(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
            cache_jwks=True
        )

        # Create app with authentication
        app = create_fraiseql_app(
            types=[User, Post],
            auth_provider=auth_provider
        )
        ```

    Note:
        Requires Auth0 application configuration with:
        - API created in Auth0 dashboard
        - Proper audience (api_identifier) configured
        - Authorization extension or custom rules for roles/permissions
    """

    def __init__(
        self,
        domain: str,
        api_identifier: str,
        algorithms: list[str] | None = None,
        *,
        cache_jwks: bool = True,
    ) -> None:
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

    async def close(self) -> None:
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
            return jwt.decode(
                token,
                signing_key.key,
                algorithms=self.algorithms,
                audience=self.api_identifier,
                issuer=self.issuer,
            )

        except jwt.ExpiredSignatureError as e:
            msg = "Token has expired"
            security_logger = get_security_logger()
            from fraiseql.audit import SecurityEvent

            security_logger.log_event(
                SecurityEvent(
                    event_type=SecurityEventType.AUTH_TOKEN_EXPIRED,
                    severity=SecurityEventSeverity.WARNING,
                    reason=msg,
                ),
            )
            raise TokenExpiredError(msg) from e
        except jwt.InvalidTokenError as e:
            msg = f"Invalid token: {e!s}"
            security_logger = get_security_logger()
            from fraiseql.audit import SecurityEvent

            security_logger.log_event(
                SecurityEvent(
                    event_type=SecurityEventType.AUTH_TOKEN_INVALID,
                    severity=SecurityEventSeverity.WARNING,
                    reason=msg,
                ),
            )
            raise InvalidTokenError(msg) from e
        except Exception as e:
            msg = f"Token validation failed: {e!s}"
            security_logger = get_security_logger()
            security_logger.log_auth_failure(
                reason=msg,
                metadata={"error_type": type(e).__name__},
            )
            raise AuthenticationError(msg) from e

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

        # Log successful authentication
        security_logger = get_security_logger()
        security_logger.log_auth_success(
            user_id=user_id,
            user_email=email,
            metadata={
                "provider": "auth0",
                "roles": roles,
                "permissions_count": len(permissions),
            },
        )

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

        Raises:
            AuthenticationError: If the request fails or returns an error
        """
        try:
            client = await self.http_client

            response = await client.get(
                f"https://{self.domain}/api/v2/users/{user_id}",
                headers={"Authorization": f"Bearer {access_token}"},
            )

            http_ok = 200
            if response.status_code != http_ok:
                msg = f"Failed to fetch user profile: {response.text}"
                raise AuthenticationError(msg)

            return response.json()
        except httpx.HTTPError as e:
            msg = f"Failed to fetch user profile: {e}"
            raise AuthenticationError(msg) from e

    async def get_user_roles(self, user_id: str, access_token: str) -> list[dict[str, Any]]:
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

        http_ok = 200
        if response.status_code != http_ok:
            msg = f"Failed to fetch user roles: {response.text}"
            raise AuthenticationError(msg)

        return response.json()

    async def get_user_permissions(self, user_id: str, access_token: str) -> list[dict[str, Any]]:
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

        http_ok = 200
        if response.status_code != http_ok:
            msg = f"Failed to fetch user permissions: {response.text}"
            raise AuthenticationError(msg)

        return response.json()


class Auth0Config:
    """Configuration for Auth0 integration.

    This class holds all configuration parameters needed to integrate with Auth0.
    It can be used to configure the Auth0Provider or passed to FraiseQLConfig
    for automatic setup.

    Attributes:
        domain: Your Auth0 tenant domain (e.g., "myapp.auth0.com").
        api_identifier: The API identifier/audience from Auth0 dashboard.
        client_id: Auth0 application client ID (optional, for Management API).
        client_secret: Auth0 application client secret (optional, for Management API).
        algorithms: List of allowed JWT algorithms (defaults to ["RS256"]).

    Example:
        ```python
        # Basic configuration
        auth_config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com"
        )

        # With Management API access
        auth_config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
            client_id="your_client_id",
            client_secret="your_client_secret"
        )

        # Use with FraiseQL
        config = FraiseQLConfig(
            database_url="postgresql://...",
            auth_provider="auth0",
            auth0_domain=auth_config.domain,
            auth0_api_identifier=auth_config.api_identifier
        )
        ```
    """

    def __init__(
        self,
        domain: str,
        api_identifier: str,
        client_id: str | None = None,
        client_secret: str | None = None,
        algorithms: list[str] | None = None,
    ) -> None:
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
