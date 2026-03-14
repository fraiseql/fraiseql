"""Tests for LangChain integration."""

import json
from unittest.mock import AsyncMock

import pytest

pytest.importorskip("langchain_core", reason="langchain-core not installed; skip langchain tests")


@pytest.fixture
def mock_client():
    from fraiseql.client import FraiseQLClient

    client = AsyncMock(spec=FraiseQLClient)
    return client


@pytest.fixture
def introspection_data():
    return {
        "data": {
            "__schema": {
                "queryType": {"name": "Query"},
                "mutationType": {"name": "Mutation"},
                "types": [
                    {
                        "kind": "OBJECT",
                        "name": "Query",
                        "description": None,
                        "fields": [
                            {
                                "name": "users",
                                "description": "List all users",
                                "args": [
                                    {
                                        "name": "limit",
                                        "description": "Max results",
                                        "type": {"kind": "SCALAR", "name": "Int", "ofType": None},
                                        "defaultValue": "10",
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": None,
                                    "ofType": {"kind": "OBJECT", "name": "User", "ofType": None},
                                },
                            },
                            {
                                "name": "user",
                                "description": "Get a single user",
                                "args": [
                                    {
                                        "name": "id",
                                        "description": "User ID",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": None,
                                            "ofType": {"kind": "SCALAR", "name": "ID"},
                                        },
                                        "defaultValue": None,
                                    }
                                ],
                                "type": {
                                    "kind": "OBJECT",
                                    "name": "User",
                                    "ofType": None,
                                },
                            },
                        ],
                    },
                    {
                        "kind": "OBJECT",
                        "name": "Mutation",
                        "description": None,
                        "fields": [
                            {
                                "name": "createUser",
                                "description": "Create a new user",
                                "args": [
                                    {
                                        "name": "name",
                                        "description": None,
                                        "type": {
                                            "kind": "SCALAR",
                                            "name": "String",
                                            "ofType": None,
                                        },
                                        "defaultValue": None,
                                    }
                                ],
                                "type": {
                                    "kind": "OBJECT",
                                    "name": "User",
                                    "ofType": None,
                                },
                            }
                        ],
                    },
                ],
            }
        }
    }


def test_toolkit_generates_tools(mock_client, introspection_data):
    from fraiseql.integrations.langchain import FraiseQLToolkit

    toolkit = FraiseQLToolkit(client=mock_client, schema_data=introspection_data)
    tools = toolkit.get_tools()

    assert len(tools) == 3
    names = {t.name for t in tools}
    assert names == {"users", "user", "createUser"}


def test_toolkit_include_filter(mock_client, introspection_data):
    from fraiseql.integrations.langchain import FraiseQLToolkit

    toolkit = FraiseQLToolkit(client=mock_client, schema_data=introspection_data)
    tools = toolkit.get_tools(include=["users"])

    assert len(tools) == 1
    assert tools[0].name == "users"


def test_toolkit_exclude_filter(mock_client, introspection_data):
    from fraiseql.integrations.langchain import FraiseQLToolkit

    toolkit = FraiseQLToolkit(client=mock_client, schema_data=introspection_data)
    tools = toolkit.get_tools(exclude=["createUser"])

    assert len(tools) == 2
    names = {t.name for t in tools}
    assert "createUser" not in names


@pytest.mark.anyio
async def test_tool_execution_returns_json(mock_client, introspection_data):
    from fraiseql.integrations.langchain import FraiseQLToolkit

    mock_client.execute.return_value = {"data": {"users": [{"id": "1", "name": "Alice"}]}}
    toolkit = FraiseQLToolkit(client=mock_client, schema_data=introspection_data)
    tools = toolkit.get_tools(include=["users"])
    tool = tools[0]

    result = await tool._arun('{"limit": 5}')
    parsed = json.loads(result)
    assert parsed["users"][0]["name"] == "Alice"


@pytest.mark.anyio
async def test_retriever_returns_documents(mock_client):
    from fraiseql.integrations.langchain import FraiseQLRetriever

    mock_client.execute.return_value = {
        "data": {"users": [{"id": "1", "name": "Alice"}, {"id": "2", "name": "Bob"}]}
    }

    retriever = FraiseQLRetriever(
        client=mock_client,
        query="{ users { id name } }",
        text_key="name",
    )

    docs = await retriever._aget_relevant_documents("")
    assert len(docs) == 2
    assert docs[0].page_content == "Alice"
    assert docs[1].page_content == "Bob"
    assert docs[0].metadata["source"] == "users"
