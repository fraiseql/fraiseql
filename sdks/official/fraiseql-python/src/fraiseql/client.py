"""FraiseQL async HTTP client.

Provides a typed async client for executing GraphQL queries against a FraiseQL server.

Example:
    ```python
    async with FraiseQLClient("http://localhost:8080/graphql") as client:
        result = await client.execute("{ users { id name } }")
        print(result["data"]["users"])
    ```
"""

from __future__ import annotations

from typing import Any

import httpx


class FraiseQLError(Exception):
    """Base error for FraiseQL GraphQL errors."""

    def __init__(self, message: str, errors: list[dict[str, Any]] | None = None) -> None:
        super().__init__(message)
        self.errors = errors or []


class FraiseQLAuthError(FraiseQLError):
    """Raised when the server returns an authentication error."""


class FraiseQLUnsupportedError(FraiseQLError):
    """Raised when the server returns an unsupported operation error."""

    def __init__(
        self,
        message: str,
        errors: list[dict[str, Any]] | None = None,
        backend: str | None = None,
    ) -> None:
        super().__init__(message, errors)
        self.backend = backend


class FraiseQLRateLimitError(FraiseQLError):
    """Raised when the server returns a rate limit error."""


class FraiseQLDatabaseError(FraiseQLError):
    """Raised when the server returns a database error."""


def _classify_error(errors: list[dict[str, Any]]) -> FraiseQLError:
    """Map GraphQL error codes to specific exception types."""
    if not errors:
        return FraiseQLError("Unknown error")

    first = errors[0]
    message = first.get("message", "Unknown error")
    extensions = first.get("extensions", {})
    code = extensions.get("code", "")

    if code in ("UNAUTHENTICATED", "UNAUTHORIZED"):
        return FraiseQLAuthError(message, errors)
    if code == "UNSUPPORTED_OPERATION":
        backend = extensions.get("backend")
        return FraiseQLUnsupportedError(message, errors, backend=backend)
    if code == "RATE_LIMITED":
        return FraiseQLRateLimitError(message, errors)
    if code in ("DATABASE_ERROR", "INTERNAL_ERROR"):
        return FraiseQLDatabaseError(message, errors)
    return FraiseQLError(message, errors)


class FraiseQLClient:
    """Async HTTP client for FraiseQL GraphQL servers.

    Args:
        url: GraphQL endpoint URL.
        auth_token: Bearer token for Authorization header.
        api_key: API key for X-API-Key header.
        timeout: Request timeout in seconds.
        verify_ssl: Whether to verify SSL certificates.
    """

    def __init__(
        self,
        url: str,
        *,
        auth_token: str | None = None,
        api_key: str | None = None,
        timeout: float = 30.0,
        verify_ssl: bool = True,
    ) -> None:
        self.url = url
        headers: dict[str, str] = {"Content-Type": "application/json"}
        if auth_token:
            headers["Authorization"] = f"Bearer {auth_token}"
        if api_key:
            headers["X-API-Key"] = api_key
        self._client = httpx.AsyncClient(
            headers=headers,
            timeout=timeout,
            verify=verify_ssl,
        )

    async def execute(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> dict[str, Any]:
        """Execute a GraphQL query.

        Args:
            query: GraphQL query string.
            variables: Optional query variables.
            operation_name: Optional operation name.

        Returns:
            The full GraphQL response dict (with "data" and optional "errors" keys).

        Raises:
            FraiseQLError: On GraphQL-level errors.
            httpx.HTTPError: On network/transport errors.
        """
        payload: dict[str, Any] = {"query": query}
        if variables is not None:
            payload["variables"] = variables
        if operation_name is not None:
            payload["operationName"] = operation_name

        resp = await self._client.post(self.url, json=payload)
        resp.raise_for_status()
        body: dict[str, Any] = resp.json()

        if body.get("errors"):
            raise _classify_error(body["errors"])

        return body

    async def introspect(self) -> dict[str, Any]:
        """Run an introspection query and return the schema description.

        Returns:
            The introspection result containing type information.
        """
        query = """
        query IntrospectionQuery {
          __schema {
            queryType { name }
            mutationType { name }
            types {
              kind
              name
              description
              fields(includeDeprecated: true) {
                name
                description
                args {
                  name
                  description
                  type { kind name ofType { kind name ofType { kind name } } }
                  defaultValue
                }
                type { kind name ofType { kind name ofType { kind name } } }
              }
            }
          }
        }
        """
        return await self.execute(query)

    async def __aenter__(self) -> FraiseQLClient:
        return self

    async def __aexit__(self, *_: object) -> None:
        await self._client.aclose()

    async def close(self) -> None:
        """Close the underlying HTTP client."""
        await self._client.aclose()
