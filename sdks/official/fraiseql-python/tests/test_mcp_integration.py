"""Tests for the raw MCP integration."""

import json
from unittest.mock import AsyncMock

import pytest


@pytest.fixture
def mock_client():
    from fraiseql.client import FraiseQLClient

    return AsyncMock(spec=FraiseQLClient)


@pytest.fixture
def introspection_data():
    return {
        "data": {
            "__schema": {
                "types": [
                    {
                        "kind": "OBJECT",
                        "name": "Query",
                        "fields": [
                            {
                                "name": "users",
                                "description": "List all users",
                                "args": [
                                    {
                                        "name": "limit",
                                        "description": None,
                                        "type": {"kind": "SCALAR", "name": "Int", "ofType": None},
                                        "defaultValue": None,
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": None,
                                    "ofType": {"kind": "OBJECT", "name": "User"},
                                },
                            },
                            {
                                "name": "secrets",
                                "description": "Sensitive — excluded from tools",
                                "args": [],
                                "type": {"kind": "SCALAR", "name": "String", "ofType": None},
                            },
                        ],
                    },
                    {
                        "kind": "OBJECT",
                        "name": "User",
                        "fields": [
                            {
                                "name": "id",
                                "description": None,
                                "args": [],
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": None,
                                    "ofType": {"kind": "SCALAR", "name": "ID"},
                                },
                            },
                            {
                                "name": "name",
                                "description": None,
                                "args": [],
                                "type": {"kind": "SCALAR", "name": "String", "ofType": None},
                            },
                        ],
                    },
                ]
            }
        }
    }


def test_tool_descriptors_have_the_mcp_shape(introspection_data):
    from fraiseql.integrations.mcp import as_tools

    tools = as_tools(introspection_data)
    assert [t["name"] for t in tools] == ["users", "secrets"]
    assert tools[0]["inputSchema"]["type"] == "object"
    assert tools[0]["inputSchema"]["properties"]["limit"]["type"] == "integer"
    assert "List all users" in tools[0]["description"]


@pytest.mark.anyio
async def test_call_tool_routes_through_the_client(mock_client, introspection_data):
    from fraiseql.integrations.mcp import FraiseQLMcpTools

    mock_client.execute.return_value = {"data": {"users": [{"id": "1"}]}}
    tools = FraiseQLMcpTools(mock_client, introspection_data)

    result = await tools.call_tool("users", {"limit": 3})
    assert result["isError"] is False
    assert json.loads(result["content"][0]["text"]) == {"users": [{"id": "1"}]}
    assert mock_client.execute.call_args.kwargs["variables"] == {"limit": 3}

    # The document reaching the server carries a sub-selection. Without one the server
    # answers 200 with one empty object per row and `isError` is still False, so this
    # assertion — not the two above — is what distinguishes success from #1076.
    (document,) = mock_client.execute.call_args.args
    assert document == "query ($limit: Int) { users(limit: $limit) { id name } }"


@pytest.mark.anyio
async def test_a_scalar_returning_tool_sends_no_sub_selection(mock_client, introspection_data):
    from fraiseql.integrations.mcp import FraiseQLMcpTools

    mock_client.execute.return_value = {"data": {"secrets": "redacted"}}
    tools = FraiseQLMcpTools(mock_client, introspection_data)

    await tools.call_tool("secrets")

    (document,) = mock_client.execute.call_args.args
    assert document == "query { secrets }"


@pytest.mark.anyio
async def test_excluded_tool_is_not_listed_and_not_callable(mock_client, introspection_data):
    from fraiseql.integrations.mcp import FraiseQLMcpTools

    tools = FraiseQLMcpTools(mock_client, introspection_data, exclude=["secrets"])
    assert [t["name"] for t in tools.list_tools()] == ["users"]

    # The allowlist holds at the dispatch boundary: no server round-trip.
    result = await tools.call_tool("secrets")
    assert result["isError"] is True
    mock_client.execute.assert_not_awaited()


@pytest.mark.anyio
async def test_client_failure_is_an_in_band_error_result(mock_client, introspection_data):
    from fraiseql.integrations.mcp import FraiseQLMcpTools

    mock_client.execute.side_effect = RuntimeError("boom")
    tools = FraiseQLMcpTools(mock_client, introspection_data)

    result = await tools.call_tool("users", {"limit": 1})
    assert result["isError"] is True
    assert "boom" in result["content"][0]["text"]
