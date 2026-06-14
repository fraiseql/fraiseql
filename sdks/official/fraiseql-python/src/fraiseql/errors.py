"""FraiseQL errors and exceptions.

This module is the single source of the FraiseQL SDK error hierarchy. Both the
synchronous :class:`fraiseql.FraiseQLClient` and the asynchronous
:class:`fraiseql.AsyncFraiseQLClient` raise subclasses of the one
:class:`FraiseQLError` base defined here, so the documented catch-all works for
either client::

    try:
        result = await client.query("{ users { id } }")
    except fraiseql.FraiseQLError as exc:
        ...  # catches GraphQLError, NetworkError, TimeoutError,
             # AuthenticationError, and every sync-client error too

Error classification differs by client, by design:

* The **async** client classifies by transport: HTTP 401/403 →
  :class:`AuthenticationError`, timeouts → :class:`TimeoutError`, other
  transport failures → :class:`NetworkError`, and a non-empty GraphQL ``errors``
  array → :class:`GraphQLError`.
* The **sync** client classifies by the GraphQL ``extensions.code`` of the first
  error → :class:`FraiseQLAuthError` / :class:`FraiseQLUnsupportedError` /
  :class:`FraiseQLRateLimitError` / :class:`FraiseQLDatabaseError`.

Both sets are subclasses of :class:`FraiseQLError`.
"""

from __future__ import annotations

from typing import Any


class FraiseQLError(Exception):
    """Base class for all FraiseQL SDK errors.

    Args:
        message: Human-readable error message.
        errors: Raw GraphQL error dicts, when the error originated from a
            GraphQL ``errors`` array. Empty for transport-level errors.
    """

    def __init__(self, message: str = "", errors: list[dict[str, Any]] | None = None) -> None:
        super().__init__(message)
        self.errors = errors or []


# ── Async client errors (classified by transport) ─────────────────────────────


class GraphQLError(FraiseQLError):
    """One or more errors returned in the GraphQL response ``errors`` array.

    Example::

        try:
            result = await client.query("{ users { id } }")
        except GraphQLError as exc:
            print(exc.errors)  # list of raw GraphQL error dicts
    """

    def __init__(self, errors: list[dict[str, Any]]) -> None:
        message = errors[0].get("message", "GraphQL error") if errors else "GraphQL error"
        super().__init__(message, errors)


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


# ── Sync client errors (classified by GraphQL ``extensions.code``) ─────────────


class FraiseQLAuthError(FraiseQLError):
    """``UNAUTHENTICATED`` / ``UNAUTHORIZED`` extensions code from the sync client."""


class FraiseQLUnsupportedError(FraiseQLError):
    """``UNSUPPORTED_OPERATION`` extensions code from the sync client."""

    def __init__(
        self,
        message: str,
        errors: list[dict[str, Any]] | None = None,
        backend: str | None = None,
    ) -> None:
        super().__init__(message, errors)
        self.backend = backend


class FraiseQLRateLimitError(FraiseQLError):
    """``RATE_LIMITED`` extensions code from the sync client."""


class FraiseQLDatabaseError(FraiseQLError):
    """``DATABASE_ERROR`` / ``INTERNAL_ERROR`` extensions code from the sync client."""


# ── Schema-authoring errors ────────────────────────────────────────────────────


class FederationValidationError(ValueError):
    """Exception raised when federation schema validation fails.

    Raised when decorators detect invalid federation metadata,
    such as non-existent key fields, circular dependencies, or incorrect
    directive usage.
    """
