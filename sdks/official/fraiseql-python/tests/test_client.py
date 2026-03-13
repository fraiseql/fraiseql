"""Tests for FraiseQLClient."""

import httpx
import pytest

from fraiseql.client import (
    FraiseQLAuthError,
    FraiseQLClient,
    FraiseQLDatabaseError,
    FraiseQLError,
    FraiseQLRateLimitError,
    FraiseQLUnsupportedError,
)


def _mock_transport(handler):
    """Create an httpx.MockTransport from a handler function."""
    return httpx.MockTransport(handler)


def _json_response(body, status_code=200):
    return httpx.Response(status_code, json=body)


@pytest.mark.anyio
async def test_execute_success():
    def handler(request):
        return _json_response({"data": {"users": [{"id": "1", "name": "Alice"}]}})

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    result = await client.execute("{ users { id name } }")
    assert result["data"]["users"][0]["name"] == "Alice"
    await client.close()


@pytest.mark.anyio
async def test_graphql_errors_raise_fraiseql_error():
    def handler(request):
        return _json_response(
            {"errors": [{"message": "Something went wrong", "extensions": {"code": "SOME_CODE"}}]}
        )

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    with pytest.raises(FraiseQLError, match="Something went wrong"):
        await client.execute("{ bad }")
    await client.close()


@pytest.mark.anyio
async def test_unauthenticated_raises_auth_error():
    def handler(request):
        return _json_response(
            {
                "errors": [
                    {"message": "Not authenticated", "extensions": {"code": "UNAUTHENTICATED"}}
                ]
            }
        )

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    with pytest.raises(FraiseQLAuthError, match="Not authenticated"):
        await client.execute("{ secret }")
    await client.close()


@pytest.mark.anyio
async def test_unsupported_raises_unsupported_error():
    def handler(request):
        return _json_response(
            {
                "errors": [
                    {
                        "message": "Not supported on SQLite",
                        "extensions": {"code": "UNSUPPORTED_OPERATION", "backend": "sqlite"},
                    }
                ]
            }
        )

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    with pytest.raises(FraiseQLUnsupportedError) as exc_info:
        await client.execute("mutation { doThing }")
    assert exc_info.value.backend == "sqlite"
    await client.close()


@pytest.mark.anyio
async def test_rate_limited_raises_rate_limit_error():
    def handler(request):
        return _json_response(
            {"errors": [{"message": "Rate limit exceeded", "extensions": {"code": "RATE_LIMITED"}}]}
        )

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    with pytest.raises(FraiseQLRateLimitError, match="Rate limit exceeded"):
        await client.execute("{ users { id } }")
    await client.close()


@pytest.mark.anyio
async def test_database_error():
    def handler(request):
        return _json_response(
            {
                "errors": [
                    {"message": "Connection refused", "extensions": {"code": "DATABASE_ERROR"}}
                ]
            }
        )

    client = FraiseQLClient("http://test/graphql")
    client._client = httpx.AsyncClient(transport=_mock_transport(handler))
    with pytest.raises(FraiseQLDatabaseError, match="Connection refused"):
        await client.execute("{ users { id } }")
    await client.close()


@pytest.mark.anyio
async def test_auth_token_header():
    def handler(request):
        assert request.headers["authorization"] == "Bearer my-token"
        return _json_response({"data": {}})

    client = FraiseQLClient("http://test/graphql", auth_token="my-token")
    client._client = httpx.AsyncClient(
        transport=_mock_transport(handler),
        headers=client._client.headers,
    )
    await client.execute("{ ping }")
    await client.close()


@pytest.mark.anyio
async def test_api_key_header():
    def handler(request):
        assert request.headers["x-api-key"] == "key-123"
        return _json_response({"data": {}})

    client = FraiseQLClient("http://test/graphql", api_key="key-123")
    client._client = httpx.AsyncClient(
        transport=_mock_transport(handler),
        headers=client._client.headers,
    )
    await client.execute("{ ping }")
    await client.close()


@pytest.mark.anyio
async def test_context_manager_closes_client():
    def handler(request):
        return _json_response({"data": {}})

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        await client.execute("{ ping }")
    # After __aexit__, client should be closed
    assert client._client.is_closed
