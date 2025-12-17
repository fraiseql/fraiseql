"""
Tests for fragment spreads and inline fragments in multi-field GraphQL queries.

Tests that fragment spreads (...FragmentName) and inline fragments (... on Type)
are properly expanded at the root level of multi-field queries.
"""

import json

import pytest

from fraiseql.fastapi.routers import execute_multi_field_query


@pytest.mark.asyncio
async def test_fragment_spread_at_root(init_schema_registry_fixture):
    """Test that named fragment spreads are expanded at root level."""
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

    async def resolve_users(root, info):
        return [{"id": 1, "name": "Alice"}]

    async def resolve_posts(root, info):
        return [{"id": 101, "title": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Query with fragment spread
    query = """
        fragment UserData on Query {
            users { id name }
        }

        query {
            ...UserData
            posts { id title }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # Both fields should be present
    assert "users" in result_json["data"]
    assert "posts" in result_json["data"]

    assert len(result_json["data"]["users"]) == 1
    assert len(result_json["data"]["posts"]) == 1


@pytest.mark.asyncio
async def test_inline_fragment_at_root(init_schema_registry_fixture):
    """Test that inline fragments work at root level."""
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

    async def resolve_users(root, info):
        return [{"id": 1, "name": "Alice"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Query with inline fragment
    query = """
        query {
            ... on Query {
                users { id name }
            }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    assert "users" in result_json["data"]
    assert len(result_json["data"]["users"]) == 1


@pytest.mark.asyncio
async def test_fragment_with_directive(init_schema_registry_fixture):
    """Test that directives work on fragment spreads."""
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
        },
    )

    async def resolve_users(root, info):
        return [{"id": 1}]

    async def resolve_posts(root, info):
        return [{"id": 101}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(user_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Fragment spread with @skip directive
    query = """
        fragment UserData on Query {
            users { id }
        }

        query {
            ...UserData @skip(if: true)
            posts { id }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # users should be skipped, only posts
    assert "users" not in result_json["data"]
    assert "posts" in result_json["data"]


@pytest.fixture
def init_schema_registry_fixture():
    """Initialize schema registry for fragment tests."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    # Reset the schema registry to allow re-initialization
    fraiseql_rs.reset_schema_registry_for_testing()

    # Minimal schema IR for testing
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {},
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))
