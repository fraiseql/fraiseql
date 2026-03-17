"""Async FraiseQL client built on httpx.

Provides ``AsyncFraiseQLClient``, a fully async, retry-capable HTTP client
for executing GraphQL queries against a FraiseQL server.

Example::

    async with AsyncFraiseQLClient("http://localhost:8080/graphql") as client:
        result = await client.query("{ users { id name } }")
        print(result["data"]["users"])
"""

from __future__ import annotations

from typing import Any

import httpx

from fraiseql.errors import (
    AuthenticationError,
    GraphQLError,
    NetworkError,
    TimeoutError,
)
from fraiseql.retry import RetryConfig  # noqa: TC001 — used at runtime for method calls


class AsyncFraiseQLClient:
    """Async HTTP client for FraiseQL GraphQL servers.

    Args:
        url: GraphQL endpoint URL.
        authorization: Optional ``Authorization`` header value (e.g.
            ``"Bearer <token>"``).  When provided it is sent verbatim;
            callers are responsible for including the scheme prefix.
        timeout: Request timeout in seconds (default: ``30.0``).
        retry: Optional :class:`~fraiseql.retry.RetryConfig`.  When omitted
            requests are not retried.
        client: Injectable :class:`httpx.AsyncClient` for testing.  When
            ``None`` (the default) a new client is created internally.
    """

    def __init__(
        self,
        url: str,
        *,
        authorization: str | None = None,
        timeout: float = 30.0,
        retry: RetryConfig | None = None,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self._url = url
        self._retry = retry
        headers: dict[str, str] = {"Content-Type": "application/json"}
        if authorization is not None:
            headers["Authorization"] = authorization
        if client is not None:
            self._client = client
        else:
            self._client = httpx.AsyncClient(headers=headers, timeout=timeout)

    # ─── Public API ───────────────────────────────────────────────────────────

    async def query(
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
            The full GraphQL response dict (``data`` key always present on
            success).

        Raises:
            GraphQLError: When the response contains a non-null ``errors``
                array.
            AuthenticationError: On HTTP 401 or 403.
            TimeoutError: When the request times out.
            NetworkError: On other transport-level failures.
        """
        return await self._execute(query, variables, operation_name)

    async def mutate(
        self,
        mutation: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> dict[str, Any]:
        """Execute a GraphQL mutation.

        Semantically identical to :meth:`query`; provided as a convenience
        to make call sites self-documenting.

        Args:
            mutation: GraphQL mutation string.
            variables: Optional mutation variables.
            operation_name: Optional operation name.

        Returns:
            The full GraphQL response dict.

        Raises:
            GraphQLError: When the response contains a non-null ``errors``
                array.
            AuthenticationError: On HTTP 401 or 403.
            TimeoutError: When the request times out.
            NetworkError: On other transport-level failures.
        """
        return await self._execute(mutation, variables, operation_name)

    async def close(self) -> None:
        """Close the underlying HTTP client and release connections."""
        await self._client.aclose()

    async def __aenter__(self) -> AsyncFraiseQLClient:
        return self

    async def __aexit__(self, *args: object) -> None:
        await self.close()

    # ─── Internal helpers ─────────────────────────────────────────────────────

    async def _execute(
        self,
        document: str,
        variables: dict[str, Any] | None,
        operation_name: str | None = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"query": document}
        if variables is not None:
            payload["variables"] = variables
        if operation_name is not None:
            payload["operationName"] = operation_name

        cfg = self._retry
        max_attempts = cfg.max_attempts if cfg is not None else 1

        last_exc: Exception | None = None
        for attempt in range(max_attempts):
            try:
                return await self._send(payload)
            except (NetworkError, TimeoutError) as exc:  # noqa: PERF203 — try/except in loop is required for retry logic
                last_exc = exc
                if cfg is None or not cfg.should_retry(exc):
                    raise
                if attempt + 1 >= max_attempts:
                    raise
                # Back-off before retrying (anyio works with both asyncio and trio)
                import anyio  # noqa: PLC0415 — deferred to avoid import cost

                await anyio.sleep(cfg.delay_for(attempt))

        # This path is only reachable if max_attempts == 0.
        if last_exc is not None:
            raise last_exc
        msg = "max_attempts must be >= 1"
        raise ValueError(msg)

    async def _send(self, payload: dict[str, Any]) -> dict[str, Any]:
        try:
            resp = await self._client.post(self._url, json=payload)
        except httpx.TimeoutException as exc:
            raise TimeoutError("Request timed out") from exc
        except httpx.RequestError as exc:
            raise NetworkError(str(exc)) from exc

        if resp.status_code in (401, 403):
            raise AuthenticationError(resp.status_code)

        resp.raise_for_status()
        body: dict[str, Any] = resp.json()

        errors = body.get("errors")
        if errors:  # None and [] both falsy → treated as success
            raise GraphQLError(errors)

        return body
