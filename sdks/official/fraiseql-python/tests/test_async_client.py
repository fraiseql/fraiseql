"""Tests for AsyncFraiseQLClient."""

import httpx
import pytest

from fraiseql.async_client import AsyncFraiseQLClient
from fraiseql.errors import (
    AuthenticationError,
    GraphQLError,
    HTTPStatusError,
    NetworkError,
    RateLimitError,
    TimeoutError,
)
from fraiseql.retry import RetryConfig


def _mock_transport(handler):
    """Wrap a handler function in an httpx.MockTransport."""
    return httpx.MockTransport(handler)


def _json_response(body, status_code: int = 200):
    return httpx.Response(status_code, json=body)


# ─── query() ─────────────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_query_success():
    def handler(request):
        return _json_response({"data": {"users": [{"id": "1", "name": "Alice"}]}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        result = await client.query("{ users { id name } }")
    assert result["data"]["users"][0]["name"] == "Alice"


@pytest.mark.anyio
async def test_query_with_variables():
    captured = {}

    def handler(request):
        import json

        captured["body"] = json.loads(request.content)
        return _json_response({"data": {"user": {"id": "42"}}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.query("query ($id: ID!) { user(id: $id) { id } }", {"id": "42"})

    assert captured["body"]["variables"] == {"id": "42"}


# ─── mutate() ────────────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_mutate_success():
    def handler(request):
        return _json_response({"data": {"createUser": {"id": "99"}}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        result = await client.mutate(
            "mutation ($name: String!) { createUser(name: $name) { id } }",
            {"name": "Bob"},
        )
    assert result["data"]["createUser"]["id"] == "99"


# ─── GraphQL error handling ───────────────────────────────────────────────────


@pytest.mark.anyio
async def test_graphql_errors_raise_graphql_error():
    def handler(request):
        return _json_response({"errors": [{"message": "Field not found"}], "data": None})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(GraphQLError, match="Field not found"):
            await client.query("{ badField }")


@pytest.mark.anyio
async def test_null_errors_is_success():
    """Regression: ``{"errors": null}`` must NOT raise."""

    def handler(request):
        return _json_response({"data": {"ping": "pong"}, "errors": None})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        result = await client.query("{ ping }")
    assert result["data"]["ping"] == "pong"


@pytest.mark.anyio
async def test_empty_errors_list_is_success():
    """An empty ``errors`` list should also be treated as success."""

    def handler(request):
        return _json_response({"data": {"ping": "pong"}, "errors": []})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        result = await client.query("{ ping }")
    assert result["data"]["ping"] == "pong"


@pytest.mark.anyio
async def test_graphql_error_stores_full_errors_list():
    errors_payload = [{"message": "A"}, {"message": "B"}]

    def handler(request):
        return _json_response({"errors": errors_payload, "data": None})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(GraphQLError) as exc_info:
            await client.query("{ x }")
    assert exc_info.value.errors == errors_payload


# ─── HTTP error handling ──────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_http_401_raises_authentication_error():
    def handler(request):
        return httpx.Response(401)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(AuthenticationError) as exc_info:
            await client.query("{ secret }")
    assert exc_info.value.status_code == 401


@pytest.mark.anyio
async def test_http_403_raises_authentication_error():
    def handler(request):
        return httpx.Response(403)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(AuthenticationError) as exc_info:
            await client.query("{ secret }")
    assert exc_info.value.status_code == 403


@pytest.mark.anyio
async def test_http_500_raises_network_error():
    """5xx is a transport-class failure inside the FraiseQL hierarchy (#1059).

    It used to escape as ``httpx.HTTPStatusError``, so the documented
    ``except fraiseql.FraiseQLError`` catch-all missed the single most common
    transient server failure.
    """

    def handler(request):
        return httpx.Response(500)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(NetworkError):
            await client.query("{ x }")


@pytest.mark.anyio
async def test_http_503_is_retried_by_default():
    """A 503 is retryable without the caller importing an httpx type."""
    attempts = 0

    def handler(request):
        nonlocal attempts
        attempts += 1
        return httpx.Response(503)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(NetworkError):
            await client.query("{ x }")
    assert attempts == 3


@pytest.mark.anyio
async def test_http_429_raises_rate_limit_error_with_retry_after():
    def handler(request):
        return httpx.Response(429, headers={"Retry-After": "12"})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(RateLimitError) as exc_info:
            await client.query("{ x }")
    assert exc_info.value.retry_after == 12.0


@pytest.mark.anyio
async def test_http_429_is_not_retried():
    """Rate limiting is not a transient transport blip — respect the server."""
    attempts = 0

    def handler(request):
        nonlocal attempts
        attempts += 1
        return httpx.Response(429)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(RateLimitError):
            await client.query("{ x }")
    assert attempts == 1


@pytest.mark.anyio
async def test_http_404_raises_http_status_error():
    def handler(request):
        return httpx.Response(404)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(HTTPStatusError) as exc_info:
            await client.query("{ x }")
    assert exc_info.value.status_code == 404


@pytest.mark.anyio
async def test_client_side_4xx_is_not_retried():
    """ADR-0015 §3: a 4xx-class error is permanent, never retried."""
    attempts = 0

    def handler(request):
        nonlocal attempts
        attempts += 1
        return httpx.Response(400)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(HTTPStatusError):
            await client.query("{ x }")
    assert attempts == 1


@pytest.mark.anyio
async def test_http_408_raises_timeout_error():
    def handler(request):
        return httpx.Response(408)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(TimeoutError):
            await client.query("{ x }")


@pytest.mark.anyio
@pytest.mark.parametrize("status", [400, 401, 403, 404, 408, 409, 429, 500, 502, 503])
async def test_every_status_stays_inside_the_fraiseql_hierarchy(status):
    """H27, made falsifiable: drive real responses, not ``issubclass`` tautologies."""
    import fraiseql

    def handler(request):
        return httpx.Response(status)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(fraiseql.FraiseQLError):
            await client.query("{ x }")


# ─── Transport-level errors ───────────────────────────────────────────────────


@pytest.mark.anyio
async def test_connect_error_raises_network_error():
    class ErrorTransport(httpx.AsyncBaseTransport):
        async def handle_async_request(self, request):
            raise httpx.ConnectError("Connection refused")

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=ErrorTransport()),
    ) as client:
        with pytest.raises(NetworkError):
            await client.query("{ x }")


@pytest.mark.anyio
async def test_timeout_raises_timeout_error():
    class TimeoutTransport(httpx.AsyncBaseTransport):
        async def handle_async_request(self, request):
            raise httpx.TimeoutException("timed out")

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=TimeoutTransport()),
    ) as client:
        with pytest.raises(TimeoutError):
            await client.query("{ x }")


# ─── Authorization header ─────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_authorization_header_sent():
    captured = {}

    def handler(request):
        captured["auth"] = request.headers.get("authorization")
        return _json_response({"data": {}})

    # Build the AsyncClient with the correct headers by instantiating the full
    # AsyncFraiseQLClient first (no injected client), but swap the transport
    # after construction so we can intercept the request.
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        authorization="Bearer secret-token",
    ) as client:
        # Swap the transport on the already-configured client
        client._client = httpx.AsyncClient(
            headers=client._client.headers,
            transport=_mock_transport(handler),
        )
        await client.query("{ ping }")
    assert captured["auth"] == "Bearer secret-token"


# ─── Context manager ──────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_context_manager_closes_client():
    def handler(request):
        return _json_response({"data": {}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.query("{ ping }")
    assert client._client.is_closed


# ─── Retry ────────────────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_retry_succeeds_after_transient_failure():
    """Client retries on NetworkError and eventually succeeds."""
    call_count = 0

    class FlakyTransport(httpx.AsyncBaseTransport):
        async def handle_async_request(self, request):
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise httpx.ConnectError("transient")
            return _json_response({"data": {"ok": True}})

    cfg = RetryConfig(max_attempts=3, base_delay=0.0, jitter=False)
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=cfg,
        client=httpx.AsyncClient(transport=FlakyTransport()),
    ) as client:
        result = await client.query("{ ok }")
    assert result["data"]["ok"] is True
    assert call_count == 3


@pytest.mark.anyio
async def test_retry_exhausted_raises_network_error():
    """After all retries, the last NetworkError is re-raised."""

    class AlwaysFailTransport(httpx.AsyncBaseTransport):
        async def handle_async_request(self, request):
            raise httpx.ConnectError("always fails")

    cfg = RetryConfig(max_attempts=2, base_delay=0.0, jitter=False)
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=cfg,
        client=httpx.AsyncClient(transport=AlwaysFailTransport()),
    ) as client:
        with pytest.raises(NetworkError):
            await client.query("{ x }")


@pytest.mark.anyio
async def test_no_retry_on_graphql_error():
    """GraphQLError (non-retryable) should not trigger retry logic."""
    call_count = 0

    def handler(request):
        nonlocal call_count
        call_count += 1
        return _json_response({"errors": [{"message": "bad query"}], "data": None})

    cfg = RetryConfig(max_attempts=3, base_delay=0.0, jitter=False)
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=cfg,
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(GraphQLError):
            await client.query("{ badField }")
    # GraphQL errors are not retried
    assert call_count == 1


@pytest.mark.anyio
async def test_retry_honors_custom_retry_on_type():
    """A configured retry_on type is actually retried (M-retry-config).

    The retry loop used a hardcoded `except (NetworkError, TimeoutError)`, so a
    custom retry_on (here AuthenticationError, raised on HTTP 401) was never
    caught and the request ran only once instead of `max_attempts` times.
    """
    call_count = 0

    def handler(request):
        nonlocal call_count
        call_count += 1
        return httpx.Response(401)

    cfg = RetryConfig(max_attempts=3, base_delay=0.0, jitter=False, retry_on=(AuthenticationError,))
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=cfg,
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(AuthenticationError):
            await client.query("{ x }")
    assert call_count == 3


@pytest.mark.anyio
async def test_injected_client_receives_authorization_header():
    """An injected client still gets the configured Authorization (L-sdk-injected-client)."""
    captured = {}

    def handler(request):
        captured["auth"] = request.headers.get("Authorization")
        return _json_response({"data": {}})

    injected = httpx.AsyncClient(transport=_mock_transport(handler))
    client = AsyncFraiseQLClient("http://test/graphql", authorization="Bearer xyz", client=injected)
    await client.query("{ x }")
    await client.close()
    assert captured["auth"] == "Bearer xyz"


# ─── Idempotency (#1060) ──────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_explicit_idempotency_key_is_sent():
    seen: list[str | None] = []

    def handler(request):
        seen.append(request.headers.get("Idempotency-Key"))
        return _json_response({"data": {"createOrder": {"id": "1"}}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.mutate("mutation { createOrder { id } }", idempotency_key="order-4711")
    assert seen == ["order-4711"]


@pytest.mark.anyio
async def test_one_generated_key_is_reused_across_retry_attempts():
    """The server dedups by key, so retries of one logical call must share it."""
    seen: list[str | None] = []

    def handler(request):
        seen.append(request.headers.get("Idempotency-Key"))
        return httpx.Response(503)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(NetworkError):
            await client.mutate("mutation { createOrder { id } }")

    assert len(seen) == 3
    assert seen[0] is not None
    assert len(set(seen)) == 1


@pytest.mark.anyio
async def test_separate_mutate_calls_get_different_keys():
    seen: list[str | None] = []

    def handler(request):
        seen.append(request.headers.get("Idempotency-Key"))
        return _json_response({"data": {"createOrder": {"id": "1"}}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.mutate("mutation { createOrder { id } }")
        await client.mutate("mutation { createOrder { id } }")

    assert seen[0] != seen[1]


@pytest.mark.anyio
async def test_no_key_generated_without_retry():
    seen: list[str | None] = []

    def handler(request):
        seen.append(request.headers.get("Idempotency-Key"))
        return _json_response({"data": {"createOrder": {"id": "1"}}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.mutate("mutation { createOrder { id } }")
    assert seen == [None]


@pytest.mark.anyio
async def test_no_key_generated_for_a_query():
    seen: list[str | None] = []

    def handler(request):
        seen.append(request.headers.get("Idempotency-Key"))
        return _json_response({"data": {"users": []}})

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        retry=RetryConfig(max_attempts=3, base_delay=0.0, jitter=False),
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        await client.query("{ users { id } }")
    assert seen == [None]


@pytest.mark.anyio
async def test_per_call_headers_win_over_client_headers():
    seen: dict[str, str] = {}

    def handler(request):
        seen["trace"] = request.headers.get("X-Trace", "")
        seen["auth"] = request.headers.get("Authorization", "")
        return _json_response({"data": {"users": []}})

    transport = _mock_transport(handler)
    async with AsyncFraiseQLClient(
        "http://test/graphql",
        authorization="Bearer t",
        client=httpx.AsyncClient(transport=transport, headers={"X-Trace": "client"}),
    ) as client:
        await client.query("{ users { id } }", headers={"X-Trace": "per-call"})

    assert seen["trace"] == "per-call"
    assert seen["auth"] == "Bearer t"
