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
                                # `[User]!` as the server publishes it: NON_NULL wrapping
                                # LIST wrapping the named object. Three levels, which is
                                # exactly what `client.introspect` asks for.
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": None,
                                    "ofType": {
                                        "kind": "LIST",
                                        "name": None,
                                        "ofType": {"kind": "OBJECT", "name": "User"},
                                    },
                                },
                            },
                            {
                                "name": "userCount",
                                "description": "A scalar root field",
                                "args": [],
                                "type": {"kind": "SCALAR", "name": "Int", "ofType": None},
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
                                "name": "status",
                                "description": None,
                                "args": [],
                                "type": {"kind": "ENUM", "name": "UserStatus", "ofType": None},
                            },
                            {
                                "name": "orders",
                                "description": "A composite field — not a leaf",
                                "args": [],
                                "type": {
                                    "kind": "LIST",
                                    "name": None,
                                    "ofType": {"kind": "OBJECT", "name": "Order"},
                                },
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
    assert [s.name for s in specs] == ["users", "userCount", "createUser"]
    assert specs[0].kind == "query"
    assert specs[1].kind == "query"
    assert specs[2].kind == "mutation"


def test_documents_carry_true_graphql_types(introspection_data):
    users, _count, create = operation_specs(introspection_data)
    # `limit` is Int (not blanket String), `tags` is a String list.
    #
    # The constants here used to end `... $tags) }` — no sub-selection — and this test
    # was one of two pinning that shape. It could not have failed on #1076: it asserts
    # the document the SDK builds against the document the SDK builds, and nothing in
    # the suite ever sent one to a server.
    assert users.document == (
        "query ($limit: Int, $tags: [String]) { users(limit: $limit, tags: $tags) { id status } }"
    )
    assert create.document == "mutation ($id: ID!) { createUser(id: $id) { id status } }"


def test_json_schema_marks_only_defaultless_non_null_required(introspection_data):
    users, _count, create = operation_specs(introspection_data)
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
        "userCount",
        "createUser",
    ]


# --- #1076: a composite root field needs a sub-selection ------------------------------
#
# Without one the server answers HTTP 200 with no `errors` and one **empty object** per
# row: `validate_selection_set` never reaches §5.3.3 for a root field with no nested
# fields, `extract_projection_fields` returns `[]`, and `ResultProjector` iterates zero
# fields. So every adapter tool reported success and handed the model no data.
#
# The mutation half fails the other way: `project_entity` documents "an empty slice means
# no field filtering" and returns the stored entity unchanged — the whole raw blob,
# including fields the caller never asked for. One selection set closes both.


def test_composite_query_carries_a_leaf_selection_set(introspection_data):
    users, _count, _create = operation_specs(introspection_data)

    assert users.document == (
        "query ($limit: Int, $tags: [String]) { users(limit: $limit, tags: $tags) { id status } }"
    )


def test_composite_mutation_carries_a_leaf_selection_set(introspection_data):
    _users, _count, create = operation_specs(introspection_data)

    assert create.document == "mutation ($id: ID!) { createUser(id: $id) { id status } }"


def test_a_scalar_root_field_gets_no_selection_set(introspection_data):
    """`query { userCount }` is valid GraphQL; a sub-selection there would be an error."""
    _users, count, _create = operation_specs(introspection_data)

    assert count.document == "query { userCount }"


def test_the_selection_set_stops_at_leaves(introspection_data):
    """`User.orders` is a list of objects, so selecting it bare would be the same defect.

    A leaf-only walk is what keeps the generated document valid without the adapter
    having to decide how deep to recurse or how to break a cycle.
    """
    users, _count, _create = operation_specs(introspection_data)

    assert users.selection == ("id", "status")
    assert "orders" not in users.selection


def test_a_return_type_with_no_leaf_fields_selects_typename():
    """`__typename` is the only field guaranteed to exist and to be a leaf.

    Emitting `{ }` would not parse, and emitting nothing puts us back at the defect.
    """
    data = {
        "data": {
            "__schema": {
                "types": [
                    {
                        "kind": "OBJECT",
                        "name": "Query",
                        "fields": [
                            {
                                "name": "wrapper",
                                "description": None,
                                "args": [],
                                "type": {"kind": "OBJECT", "name": "Wrapper", "ofType": None},
                            }
                        ],
                    },
                    {
                        "kind": "OBJECT",
                        "name": "Wrapper",
                        "fields": [
                            {
                                "name": "inner",
                                "description": None,
                                "args": [],
                                "type": {"kind": "OBJECT", "name": "Wrapper", "ofType": None},
                            }
                        ],
                    },
                ]
            }
        }
    }

    (wrapper,) = operation_specs(data)

    assert wrapper.document == "query { wrapper { __typename } }"


def test_an_unresolvable_return_type_emits_no_selection():
    """A named type absent from `types` is not guessed at.

    The introspection payload carries three levels of type wrapping, which is exactly
    what the server publishes today. A deeper chain would arrive truncated, and inventing
    a selection for a type we cannot see would produce a document the server rejects
    outright — worse than the one it accepts.
    """
    data = {
        "data": {
            "__schema": {
                "types": [
                    {
                        "kind": "OBJECT",
                        "name": "Query",
                        "fields": [
                            {
                                "name": "mystery",
                                "description": None,
                                "args": [],
                                "type": {"kind": "OBJECT", "name": "Undeclared", "ofType": None},
                            }
                        ],
                    }
                ]
            }
        }
    }

    (mystery,) = operation_specs(data)

    assert mystery.selection == ()
    assert mystery.document == "query { mystery }"
