"""Tests for @skip and @include directive support in multi-field queries.

Tests GraphQL directives (@skip, @include) on both root fields and sub-fields
in multi-field query execution.
"""

import json

import pytest

from fraiseql.fastapi.routers import execute_multi_field_query


@pytest.mark.asyncio
async def test_skip_directive_on_root_field_true(init_schema_registry_fixture):
    """Test @skip(if: true) on root field - field should be excluded."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Query with @skip(if: true) on users field
    query = """
        query {
            users @skip(if: true) { id name }
            posts { id title }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # users should be skipped, only posts should be present
    assert "data" in result_json
    assert "users" not in result_json["data"]
    assert "posts" in result_json["data"]
    assert len(result_json["data"]["posts"]) == 1


@pytest.mark.asyncio
async def test_skip_directive_on_root_field_false(init_schema_registry_fixture):
    """Test @skip(if: false) on root field - field should be included."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query {
            users @skip(if: false) { id name }
            posts { id title }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # Both fields should be present (skip is false)
    assert "data" in result_json
    assert "users" in result_json["data"]
    assert "posts" in result_json["data"]
    assert len(result_json["data"]["users"]) == 1
    assert len(result_json["data"]["posts"]) == 1


@pytest.mark.asyncio
async def test_skip_directive_with_variable(init_schema_registry_fixture):
    """Test @skip(if: $var) with variable."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query TestSkip($skipUsers: Boolean!) {
            users @skip(if: $skipUsers) { id name }
            posts { id title }
        }
    """

    # Test with skipUsers = true
    variables = {"skipUsers": True}
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    assert "users" not in result_json["data"]
    assert "posts" in result_json["data"]

    # Test with skipUsers = false
    variables = {"skipUsers": False}
    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    assert "users" in result_json["data"]
    assert "posts" in result_json["data"]


@pytest.mark.asyncio
async def test_include_directive_on_root_field_true(init_schema_registry_fixture):
    """Test @include(if: true) on root field - field should be included."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query {
            users @include(if: true) { id name }
            posts { id title }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # Both fields should be present
    assert "data" in result_json
    assert "users" in result_json["data"]
    assert "posts" in result_json["data"]


@pytest.mark.asyncio
async def test_include_directive_on_root_field_false(init_schema_registry_fixture):
    """Test @include(if: false) on root field - field should be excluded."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query {
            users @include(if: false) { id name }
            posts { id title }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # users should be excluded, only posts should be present
    assert "data" in result_json
    assert "users" not in result_json["data"]
    assert "posts" in result_json["data"]


@pytest.mark.asyncio
async def test_skip_directive_on_sub_fields(init_schema_registry_fixture):
    """Test @skip directive on sub-fields."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
            "email": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice", "email": "alice@example.com"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query {
            users {
                id
                name @skip(if: true)
                email
            }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # name field should be skipped
    user = result_json["data"]["users"][0]
    assert "id" in user
    assert "name" not in user  # Skipped
    assert "email" in user


@pytest.mark.asyncio
async def test_include_directive_on_sub_fields(init_schema_registry_fixture):
    """Test @include directive on sub-fields."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
            "email": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice", "email": "alice@example.com"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        query {
            users {
                id
                name @include(if: false)
                email
            }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # name field should be excluded
    user = result_json["data"]["users"][0]
    assert "id" in user
    assert "name" not in user  # Not included
    assert "email" in user


@pytest.mark.asyncio
async def test_skip_takes_precedence_over_include(init_schema_registry_fixture):
    """Test that @skip takes precedence when both directives are present."""
    from graphql import (
        GraphQLField,
        GraphQLInt,
        GraphQLList,
        GraphQLObjectType,
        GraphQLSchema,
        GraphQLString,
    )

    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    post_type = GraphQLObjectType(
        "Post",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "title": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Both @skip(if: true) and @include(if: true) present on users field
    # According to GraphQL spec, @skip takes precedence
    # Include posts field to ensure we have at least one field to execute
    query = """
        query {
            users @skip(if: true) @include(if: true) {
                id
                name
            }
            posts {
                id
                title
            }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # users should be skipped (skip takes precedence), posts should be present
    assert "data" in result_json
    assert "users" not in result_json["data"]
    assert "posts" in result_json["data"]


@pytest.fixture(scope="module", autouse=True)
def init_schema_registry_fixture():
    """Initialize schema registry for directive tests."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    # Reset the schema registry
    fraiseql_rs.reset_schema_registry_for_testing()

    # Schema IR with types for testing
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "User": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "name": {"type_name": "String", "is_nested_object": False, "is_list": False},
                    "email": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
            "Post": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "title": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
        },
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))

    yield

    fraiseql_rs.reset_schema_registry_for_testing()
