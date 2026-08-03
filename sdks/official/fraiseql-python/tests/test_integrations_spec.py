"""Tests for the shared adapter spec (`fraiseql.integrations._spec`)."""

import pytest

from fraiseql.integrations._spec import operation_specs


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
                                    },
                                    {
                                        "name": "tags",
                                        "description": None,
                                        "type": {
                                            "kind": "LIST",
                                            "name": None,
                                            "ofType": {
                                                "kind": "SCALAR",
                                                "name": "String",
                                                "ofType": None,
                                            },
                                        },
                                        "defaultValue": None,
                                    },
                                ],
                                "type": {"kind": "LIST", "name": None, "ofType": None},
                            },
                            {
                                "name": "__schema",
                                "description": "introspection — must be skipped",
                                "args": [],
                                "type": {"kind": "OBJECT", "name": "__Schema", "ofType": None},
                            },
                        ],
                    },
                    {
                        "kind": "OBJECT",
                        "name": "Mutation",
                        "fields": [
                            {
                                "name": "createUser",
                                "description": "Create a new user",
                                "args": [
                                    {
                                        "name": "id",
                                        "description": None,
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": None,
                                            "ofType": {"kind": "SCALAR", "name": "ID"},
                                        },
                                        "defaultValue": None,
                                    }
                                ],
                                "type": {"kind": "OBJECT", "name": "User", "ofType": None},
                            }
                        ],
                    },
                ],
            }
        }
    }


def test_extracts_operations_and_skips_introspection_fields(introspection_data):
    specs = operation_specs(introspection_data)
    assert [s.name for s in specs] == ["users", "createUser"]
    assert specs[0].kind == "query"
    assert specs[1].kind == "mutation"


def test_documents_carry_true_graphql_types(introspection_data):
    users, create = operation_specs(introspection_data)
    # `limit` is Int (not blanket String), `tags` is a String list.
    assert users.document == (
        "query ($limit: Int, $tags: [String]) { users(limit: $limit, tags: $tags) }"
    )
    assert create.document == "mutation ($id: ID!) { createUser(id: $id) }"


def test_json_schema_marks_only_defaultless_non_null_required(introspection_data):
    users, create = operation_specs(introspection_data)
    users_schema = users.parameters_schema
    assert users_schema["properties"]["limit"] == {
        "type": "integer",
        "description": "Max results",
    }
    assert users_schema["properties"]["tags"] == {
        "type": "array",
        "items": {"type": "string"},
    }
    assert "required" not in users_schema

    create_schema = create.parameters_schema
    assert create_schema["required"] == ["id"]
    assert create_schema["properties"]["id"]["type"] == "string"


def test_include_exclude_filter(introspection_data):
    assert [s.name for s in operation_specs(introspection_data, include=["users"])] == ["users"]
    assert [s.name for s in operation_specs(introspection_data, exclude=["users"])] == [
        "createUser"
    ]
