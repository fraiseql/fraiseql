"""SDK Error Envelope Parity Tests.

Verifies that the FraiseQL Python SDK correctly parses all variants of the
GraphQL error envelope format:

  {
    "errors": [
      {
        "message": "...",
        "locations": [{"line": 1, "column": 5}],
        "path": ["users", 0, "id"],
        "extensions": {"code": "...", ...}
      }
    ],
    "data": null | {...}
  }

These tests use httpx.MockTransport so no server is required.

Reference: https://spec.graphql.org/October2021/#sec-Errors
"""

import httpx
import pytest

from fraiseql.client import (
    FraiseQLAuthError,
    FraiseQLClient,
    FraiseQLError,
)


def _mock_transport(handler):
    return httpx.MockTransport(handler)


def _json_response(body, status_code: int = 200):
    return httpx.Response(status_code, json=body)


# ─── Single error ─────────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_single_error_is_raised():
    """A single error in the envelope raises FraiseQLError with correct message."""

    def handler(request):
        return _json_response(
            {
                "errors": [{"message": "Field 'nonexistentField' not found"}],
                "data": None,
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError, match="nonexistentField"):
            await client.execute("{ nonexistentField }")


@pytest.mark.anyio
async def test_single_error_with_locations():
    """Error envelope includes locations — client raises without losing the message."""

    def handler(request):
        return _json_response(
            {
                "errors": [
                    {
                        "message": "Syntax error: unexpected token",
                        "locations": [{"line": 1, "column": 9}],
                    }
                ],
                "data": None,
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError, match="Syntax error"):
            await client.execute("{ invalid }")


@pytest.mark.anyio
async def test_single_error_with_path():
    """Error envelope includes a path field — client raises with correct message."""

    def handler(request):
        return _json_response(
            {
                "errors": [
                    {
                        "message": "Cannot return null for non-nullable field",
                        "path": ["users", 0, "id"],
                    }
                ],
                "data": {"users": [None]},
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError, match="Cannot return null"):
            await client.execute("{ users { id } }")


# ─── Multiple errors ──────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_multiple_errors_raises_on_first():
    """Multiple errors in the envelope — client raises; first error message dominates."""

    def handler(request):
        return _json_response(
            {
                "errors": [
                    {"message": "Field 'foo' not found"},
                    {"message": "Field 'bar' not found"},
                ],
                "data": None,
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError):
            await client.execute("{ foo bar }")


@pytest.mark.anyio
async def test_multiple_errors_with_mixed_codes():
    """Multiple errors with different extension codes — typed error for first recognized code."""

    def handler(request):
        return _json_response(
            {
                "errors": [
                    {"message": "Not authenticated", "extensions": {"code": "UNAUTHENTICATED"}},
                    {"message": "Also forbidden", "extensions": {"code": "FORBIDDEN"}},
                ],
                "data": None,
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLAuthError):
            await client.execute("{ secret }")


# ─── Partial success (data + errors) ─────────────────────────────────────────


@pytest.mark.anyio
async def test_partial_success_raises_despite_data():
    """Partial success (data + errors) — client raises even though data is present."""

    def handler(request):
        return _json_response(
            {
                "errors": [{"message": "Could not resolve 'profile'"}],
                "data": {"users": [{"id": "1", "name": "Alice"}]},
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError, match="Could not resolve"):
            await client.execute("{ users { id name } profile { bio } }")


# ─── Error with all optional fields ──────────────────────────────────────────


@pytest.mark.anyio
async def test_full_error_envelope_fields():
    """Full error object with message, locations, path, extensions — all parsed."""

    def handler(request):
        return _json_response(
            {
                "errors": [
                    {
                        "message": "Variable $userId must be a non-null ID",
                        "locations": [{"line": 1, "column": 7}, {"line": 3, "column": 2}],
                        "path": ["user"],
                        "extensions": {
                            "code": "BAD_USER_INPUT",
                            "field": "userId",
                        },
                    }
                ],
                "data": None,
            }
        )

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        with pytest.raises(FraiseQLError, match="userId"):
            await client.execute("query ($userId: ID!) { user(id: $userId) { name } }")


# ─── No-error cases ───────────────────────────────────────────────────────────


@pytest.mark.anyio
async def test_empty_data_without_errors_succeeds():
    """Response with data and no errors returns data without raising."""

    def handler(request):
        return _json_response({"data": {"users": []}})

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        result = await client.execute("{ users { id } }")
    assert result["data"]["users"] == []


@pytest.mark.anyio
async def test_null_errors_field_is_treated_as_success():
    """Response with `"errors": null` is treated as success (not an error)."""

    def handler(request):
        return _json_response({"data": {"ping": "pong"}, "errors": None})

    async with FraiseQLClient("http://test/graphql") as client:
        client._client = httpx.AsyncClient(transport=_mock_transport(handler))
        result = await client.execute("{ ping }")
    assert result["data"]["ping"] == "pong"
