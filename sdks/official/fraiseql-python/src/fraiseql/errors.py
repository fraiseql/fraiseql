"""FraiseQL errors and exceptions.

Provides a hierarchy of typed exceptions for both schema authoring
errors and client-side GraphQL/network errors.
"""

from __future__ import annotations

from typing import Any


class FraiseQLError(Exception):
    """Base class for all FraiseQL SDK errors."""


class GraphQLError(FraiseQLError):
    """One or more errors returned in the GraphQL response ``errors`` array.

    Example::

        try:
            result = await client.query("{ users { id } }")
        except GraphQLError as exc:
            print(exc.errors)  # list of raw GraphQL error dicts
    """

    def __init__(self, errors: list[dict[str, Any]]) -> None:
        self.errors = errors
        message = errors[0].get("message", "GraphQL error") if errors else "GraphQL error"
        super().__init__(message)


class NetworkError(FraiseQLError):
    """Transport-level error (connection refused, timeout, DNS failure)."""


class TimeoutError(NetworkError):
    """The request exceeded the configured timeout."""


class AuthenticationError(FraiseQLError):
    """401/403 response from the server.

    Example::

        try:
            result = await client.query("{ secret }")
        except AuthenticationError as exc:
            print(exc.status_code)  # 401 or 403
    """

    def __init__(self, status_code: int) -> None:
        self.status_code = status_code
        super().__init__(f"Authentication failed (HTTP {status_code})")


class FederationValidationError(ValueError):
    """Exception raised when federation schema validation fails.

    Raised when decorators detect invalid federation metadata,
    such as non-existent key fields, circular dependencies, or incorrect
    directive usage.
    """
