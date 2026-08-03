"""Tests for the OpenAI function-calling integration."""

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
                            }
                        ],
                    },
                    {
                        "kind": "OBJECT",
                        "name": "Mutation",
                        "fields": [
                            {
                                "name": "createUser",
                                "description": "Create a new user",
                                "args": [],
                            }
                        ],
                    },
                ]
            }
        }
    }


def test_function_definitions_have_the_openai_shape(introspection_data):
    from fraiseql.integrations.openai import as_function_definitions

    defs = as_function_definitions(introspection_data)
    assert [d["function"]["name"] for d in defs] == ["users", "createUser"]
    users = defs[0]
    assert users["type"] == "function"
    assert users["function"]["parameters"]["type"] == "object"
    assert users["function"]["parameters"]["properties"]["limit"]["type"] == "integer"
    assert "List all users" in users["function"]["description"]


@pytest.mark.anyio
async def test_call_routes_through_the_client(mock_client, introspection_data):
    from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

    mock_client.execute.return_value = {"data": {"users": [{"id": "1"}]}}
    functions = FraiseQLOpenAIFunctions(mock_client, introspection_data)

    # Arguments as the JSON string the OpenAI API delivers.
    data = await functions.call("users", '{"limit": 5}')
    assert data == {"users": [{"id": "1"}]}
    (document,) = mock_client.execute.call_args.args
    assert document == "query ($limit: Int) { users(limit: $limit) }"
    assert mock_client.execute.call_args.kwargs["variables"] == {"limit": 5}


@pytest.mark.anyio
async def test_unknown_function_never_reaches_the_server(mock_client, introspection_data):
    from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

    functions = FraiseQLOpenAIFunctions(mock_client, introspection_data)
    with pytest.raises(KeyError):
        await functions.call("dropAllTables", "{}")
    mock_client.execute.assert_not_awaited()


@pytest.mark.anyio
async def test_excluded_operation_is_not_defined_and_not_callable(mock_client, introspection_data):
    from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

    functions = FraiseQLOpenAIFunctions(mock_client, introspection_data, exclude=["createUser"])
    assert [d["function"]["name"] for d in functions.definitions()] == ["users"]
    with pytest.raises(KeyError):
        await functions.call("createUser", "{}")
    mock_client.execute.assert_not_awaited()


@pytest.mark.anyio
async def test_invalid_arguments_json_is_a_value_error(mock_client, introspection_data):
    from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

    functions = FraiseQLOpenAIFunctions(mock_client, introspection_data)
    with pytest.raises(ValueError, match="not valid JSON"):
        await functions.call("users", "{not json")
    mock_client.execute.assert_not_awaited()
