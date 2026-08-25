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


class RateLimitError(FraiseQLError):
    """HTTP 429 response from the server.

    Not retried by default: the server is asking for less traffic, not
    reporting a transient blip.  ``retry_after`` carries the ``Retry-After``
    header in seconds when the server sent one.

    Example::

        try:
            result = await client.query("{ users { id } }")
        except RateLimitError as exc:
            await asyncio.sleep(exc.retry_after or 60)
    """

    def __init__(self, retry_after: float | None = None) -> None:
        self.retry_after = retry_after
        super().__init__("Rate limit exceeded")


class HTTPStatusError(FraiseQLError):
    """A non-2xx response that is permanent rather than transient.

    Covers the 4xx statuses the client does not classify more specifically
    (401/403 → :class:`AuthenticationError`, 408 → :class:`TimeoutError`,
    429 → :class:`RateLimitError`).  Per ADR-0015 §3 a 4xx-class response
    means the request itself was rejected, so it is **not** retried.

    Distinct from :class:`httpx.HTTPStatusError`: this one is a
    :class:`FraiseQLError`, so the documented catch-all sees it.
    """

    def __init__(self, status_code: int) -> None:
        self.status_code = status_code
        super().__init__(f"HTTP {status_code}")


# ── HTTP status classification (shared by both clients) ───────────────────────

_HTTP_ERROR_FLOOR = 400
_HTTP_UNAUTHORIZED = 401
_HTTP_FORBIDDEN = 403
_HTTP_REQUEST_TIMEOUT = 408
_HTTP_TOO_MANY_REQUESTS = 429
_HTTP_SERVER_ERROR_FLOOR = 500


def _parse_retry_after(value: str | None) -> float | None:
    """Parse a ``Retry-After`` header expressed in seconds.

    The HTTP-date form is not decoded; callers get ``None`` rather than a
    wrong number.
    """
    if value is None:
        return None
    try:
        return float(value.strip())
    except ValueError:
        return None


def raise_for_status(status_code: int, retry_after: str | None = None) -> None:
    """Raise the :class:`FraiseQLError` corresponding to an HTTP status.

    Every non-2xx response lands inside the FraiseQL hierarchy, so the
    documented ``except fraiseql.FraiseQLError`` catch-all cannot be bypassed
    by a transport-level status (#1059).  The split also decides retryability,
    because the default ``retry_on`` is ``(NetworkError, TimeoutError)``:

    ============  ==============================  =========
    Status        Raises                          Retried
    ============  ==============================  =========
    401, 403      :class:`AuthenticationError`    no
    408           :class:`TimeoutError`           yes
    429           :class:`RateLimitError`         no
    other 4xx     :class:`HTTPStatusError`        no
    5xx           :class:`NetworkError`           yes
    ============  ==============================  =========

    Treating 4xx as permanent follows ADR-0015 §3; 5xx and 408 are the
    transient classes worth another attempt.

    Args:
        status_code: HTTP status of the response.
        retry_after: Raw ``Retry-After`` header value, when present.

    Raises:
        AuthenticationError: On 401 or 403.
        TimeoutError: On 408.
        RateLimitError: On 429.
        HTTPStatusError: On any other 4xx.
        NetworkError: On any 5xx.
    """
    if status_code < _HTTP_ERROR_FLOOR:
        return
    if status_code in (_HTTP_UNAUTHORIZED, _HTTP_FORBIDDEN):
        raise AuthenticationError(status_code)
    if status_code == _HTTP_REQUEST_TIMEOUT:
        msg = "Server reported a request timeout (HTTP 408)"
        raise TimeoutError(msg)
    if status_code == _HTTP_TOO_MANY_REQUESTS:
        raise RateLimitError(_parse_retry_after(retry_after))
    if status_code >= _HTTP_SERVER_ERROR_FLOOR:
        raise NetworkError(f"HTTP {status_code}")
    raise HTTPStatusError(status_code)


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
