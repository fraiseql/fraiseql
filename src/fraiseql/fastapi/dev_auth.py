"""Development authentication middleware for FraiseQL.

This module provides basic HTTP authentication for development environments only.
It should never be used in production.
"""

import logging
import secrets
from collections.abc import Callable

from fastapi import Request, Response, status
from fastapi.security import HTTPBasic, HTTPBasicCredentials
from starlette.middleware.base import BaseHTTPMiddleware

from fraiseql.audit import get_security_logger

logger = logging.getLogger(__name__)


class DevAuthMiddleware(BaseHTTPMiddleware):
    """HTTP Basic Auth middleware for development environments.

    This middleware provides simple username/password authentication
    for protecting GraphQL endpoints during development. It should
    only be used in non-production environments.
    """

    def __init__(self, app, username: str, password: str) -> None:
        """Initialize the development auth middleware.

        Args:
            app: The FastAPI application
            username: Required username for authentication
            password: Required password for authentication
        """
        super().__init__(app)
        self.username = username
        self.password = password
        self.security = HTTPBasic()

        # Log warning that dev auth is enabled
        logger.warning(
            "Development authentication is enabled. "
            "This should NOT be used in production environments.",
        )

    async def dispatch(self, request: Request, call_next: Callable) -> Response:
        """Process the request and handle authentication.

        Args:
            request: The incoming HTTP request
            call_next: The next middleware in the chain

        Returns:
            HTTP response

        Raises:
            HTTPException: If authentication fails
        """
        # Skip auth for non-GraphQL endpoints that might not need protection
        if not self._should_protect_path(request.url.path):
            return await call_next(request)

        # Extract credentials from Authorization header
        credentials = self._extract_credentials(request)

        if not credentials:
            return self._unauthorized_response()

        # Verify credentials
        if not self._verify_credentials(credentials):
            return self._unauthorized_response()

        # Authentication successful, continue
        return await call_next(request)

    def _should_protect_path(self, path: str) -> bool:
        """Determine if a path should be protected by auth.

        Args:
            path: The request path

        Returns:
            True if the path should be protected
        """
        # Protect GraphQL endpoints and playground
        protected_paths = [
            "/graphql",
            "/playground",
            "/graphiql",
            "/docs",
            "/redoc",
            "/openapi.json",
        ]

        return any(path.startswith(protected_path) for protected_path in protected_paths)

    def _extract_credentials(self, request: Request) -> HTTPBasicCredentials | None:
        """Extract HTTP Basic Auth credentials from request.

        Args:
            request: The HTTP request

        Returns:
            Credentials if present, None otherwise
        """
        auth_header = request.headers.get("authorization")
        if not auth_header or not auth_header.startswith("Basic "):
            return None

        try:
            import base64

            # Remove "Basic " prefix and decode
            encoded_credentials = auth_header[6:]
            decoded_bytes = base64.b64decode(encoded_credentials)
            decoded_str = decoded_bytes.decode("utf-8")

            # Split username:password
            if ":" not in decoded_str:
                return None

            username, password = decoded_str.split(":", 1)
            return HTTPBasicCredentials(username=username, password=password)

        except Exception:
            return None

    def _verify_credentials(self, credentials: HTTPBasicCredentials) -> bool:
        """Verify the provided credentials.

        Args:
            credentials: The HTTP Basic Auth credentials

        Returns:
            True if credentials are valid
        """
        # Use constant-time comparison to prevent timing attacks
        username_correct = secrets.compare_digest(credentials.username, self.username)
        password_correct = secrets.compare_digest(credentials.password, self.password)

        result = username_correct and password_correct

        # Log authentication attempt
        security_logger = get_security_logger()
        if result:
            security_logger.log_auth_success(
                user_id=credentials.username,
                metadata={"auth_type": "dev_basic_auth"},
            )
        else:
            security_logger.log_auth_failure(
                reason="Invalid username or password",
                attempted_username=credentials.username,
                metadata={"auth_type": "dev_basic_auth"},
            )

        return result

    def _unauthorized_response(self) -> Response:
        """Create an HTTP 401 Unauthorized response.

        Returns:
            HTTP 401 response with WWW-Authenticate header
        """
        return Response(
            content="Development authentication required",
            status_code=status.HTTP_401_UNAUTHORIZED,
            headers={"WWW-Authenticate": 'Basic realm="FraiseQL Development"'},
        )


def get_dev_auth_credentials() -> tuple[str, str | None]:
    """Get development auth credentials from environment variables.

    Returns:
        Tuple of (username, password). Password is None if not set.
    """
    import os

    username = os.getenv("FRAISEQL_DEV_AUTH_USERNAME", "admin")
    password = os.getenv("FRAISEQL_DEV_AUTH_PASSWORD")

    return username, password


def is_dev_auth_enabled() -> bool:
    """Check if development authentication is enabled.

    Returns:
        True if dev auth password is set in environment
    """
    _, password = get_dev_auth_credentials()
    return password is not None


def create_dev_auth_middleware(
    app,
    username: str | None = None,
    password: str | None = None,
) -> DevAuthMiddleware | None:
    """Create development auth middleware if enabled.

    Args:
        app: The FastAPI application
        username: Override username (defaults to env var or "admin")
        password: Override password (defaults to env var)

    Returns:
        DevAuthMiddleware instance if enabled, None otherwise
    """
    # Get credentials from parameters or environment
    if username is None or password is None:
        env_username, env_password = get_dev_auth_credentials()
        username = username or env_username
        password = password or env_password

    # Only create middleware if password is provided
    if not password:
        return None

    return DevAuthMiddleware(app, username, password)
