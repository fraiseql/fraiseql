"""Tests for AsyncFraiseQLClient."""

import httpx
import pytest

from fraiseql.async_client import AsyncFraiseQLClient
from fraiseql.errors import (
    AuthenticationError,
    GraphQLError,
    NetworkError,
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
async def test_http_500_raises_httpx_error():
    def handler(request):
        return httpx.Response(500)

    async with AsyncFraiseQLClient(
        "http://test/graphql",
        client=httpx.AsyncClient(transport=_mock_transport(handler)),
    ) as client:
        with pytest.raises(httpx.HTTPStatusError):
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
