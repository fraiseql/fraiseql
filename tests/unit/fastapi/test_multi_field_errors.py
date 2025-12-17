"""Tests for error handling in multi-field queries with partial results.

Tests that when one field fails, other fields continue to execute and return data,
with errors collected in the GraphQL spec format.
"""

import json

import pytest
from graphql import (
    GraphQLField,
    GraphQLInt,
    GraphQLList,
    GraphQLObjectType,
    GraphQLSchema,
    GraphQLString,
)

from fraiseql.fastapi.routers import execute_multi_field_query


@pytest.mark.asyncio
async def test_single_field_error_others_succeed(init_schema_registry_fixture):
    """Test that when one field fails, others still return data."""
    # Create types
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

    # Create resolvers
    async def resolve_users(info):
        """Resolver that succeeds."""
        return [
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"},
        ]

    async def resolve_posts(info):
        """Resolver that fails."""
        raise RuntimeError("Database connection failed")

    async def resolve_comments(info):
        """Resolver that succeeds."""
        return [
            {"id": 101, "text": "Great post!"},
        ]

    comment_type = GraphQLObjectType(
        "Comment",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "text": GraphQLField(GraphQLString),
        },
    )

    # Create schema
    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(post_type), resolve=resolve_posts),
            "comments": GraphQLField(GraphQLList(comment_type), resolve=resolve_comments),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Execute query with one failing field
    query = """
        {
            users { id name }
            posts { id title }
            comments { id text }
        }
    """

    variables = None
    context = {}

    result = await execute_multi_field_query(schema, query, variables, context)
    result_json = json.loads(bytes(result))

    # Verify structure
    assert "data" in result_json
    assert "errors" in result_json

    # Verify successful fields have data
    assert "users" in result_json["data"]
    assert len(result_json["data"]["users"]) == 2
    assert result_json["data"]["users"][0]["name"] == "Alice"

    assert "comments" in result_json["data"]
    assert len(result_json["data"]["comments"]) == 1
    assert result_json["data"]["comments"][0]["text"] == "Great post!"

    # Verify failed field has null/empty data
    assert "posts" in result_json["data"]
    assert result_json["data"]["posts"] == []

    # Verify error is present
    assert len(result_json["errors"]) == 1
    error = result_json["errors"][0]
    assert "message" in error
    assert "Database connection failed" in error["message"]
    assert "path" in error
    assert error["path"] == ["posts"]


@pytest.mark.asyncio
async def test_multiple_field_errors(init_schema_registry_fixture):
    """Test that multiple field failures are collected."""
    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    # All resolvers fail with different errors
    async def resolve_users(info):
        raise ValueError("Invalid user query")

    async def resolve_posts(info):
        raise RuntimeError("Database timeout")

    async def resolve_comments(info):
        raise PermissionError("Access denied")

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(user_type), resolve=resolve_posts),
            "comments": GraphQLField(GraphQLList(user_type), resolve=resolve_comments),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        {
            users { id name }
            posts { id name }
            comments { id name }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # Verify all fields have empty data
    assert result_json["data"]["users"] == []
    assert result_json["data"]["posts"] == []
    assert result_json["data"]["comments"] == []

    # Verify all errors are collected
    assert len(result_json["errors"]) == 3

    # Check error messages
    error_messages = [e["message"] for e in result_json["errors"]]
    assert "Invalid user query" in error_messages
    assert "Database timeout" in error_messages
    assert "Access denied" in error_messages

    # Check error paths
    error_paths = [e["path"] for e in result_json["errors"]]
    assert ["users"] in error_paths
    assert ["posts"] in error_paths
    assert ["comments"] in error_paths


@pytest.mark.asyncio
async def test_error_with_alias(init_schema_registry_fixture):
    """Test that error paths use the alias (response key) when present."""
    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    async def resolve_users(info):
        raise ValueError("User resolver failed")

    async def resolve_posts(info):
        return [{"id": 1, "name": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(user_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    # Query with alias on failing field
    query = """
        {
            allUsers: users { id name }
            allPosts: posts { id name }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # Verify error path uses alias
    assert len(result_json["errors"]) == 1
    error = result_json["errors"][0]
    assert error["path"] == ["allUsers"]

    # Verify data keys use aliases
    assert "allUsers" in result_json["data"]
    assert "allPosts" in result_json["data"]
    assert result_json["data"]["allUsers"] == []
    assert len(result_json["data"]["allPosts"]) == 1


@pytest.mark.asyncio
async def test_all_fields_succeed_no_errors_key(init_schema_registry_fixture):
    """Test that when all fields succeed, no 'errors' key is present."""
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
        return [{"id": 101, "name": "Post 1"}]

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "users": GraphQLField(GraphQLList(user_type), resolve=resolve_users),
            "posts": GraphQLField(GraphQLList(user_type), resolve=resolve_posts),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        {
            users { id name }
            posts { id name }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # Verify no errors key when all succeed
    assert "errors" not in result_json
    assert "data" in result_json
    assert len(result_json["data"]["users"]) == 1
    assert len(result_json["data"]["posts"]) == 1


@pytest.mark.asyncio
async def test_error_in_single_object_field(init_schema_registry_fixture):
    """Test error handling for non-list fields (single objects)."""
    user_type = GraphQLObjectType(
        "User",
        lambda: {
            "id": GraphQLField(GraphQLInt),
            "name": GraphQLField(GraphQLString),
        },
    )

    async def resolve_current_user(info):
        raise RuntimeError("Session expired")

    async def resolve_admin_user(info):
        return {"id": 999, "name": "Admin"}

    query_type = GraphQLObjectType(
        "Query",
        lambda: {
            "currentUser": GraphQLField(user_type, resolve=resolve_current_user),
            "adminUser": GraphQLField(user_type, resolve=resolve_admin_user),
        },
    )

    schema = GraphQLSchema(query=query_type)

    query = """
        {
            currentUser { id name }
            adminUser { id name }
        }
    """

    result = await execute_multi_field_query(schema, query, None, {})
    result_json = json.loads(bytes(result))

    # Verify failed field is null (not array, as it's a single object field)
    assert result_json["data"]["currentUser"] is None

    # Verify successful field has data
    assert result_json["data"]["adminUser"]["id"] == 999

    # Verify error
    assert len(result_json["errors"]) == 1
    assert "Session expired" in result_json["errors"][0]["message"]
    assert result_json["errors"][0]["path"] == ["currentUser"]


@pytest.fixture(scope="module", autouse=True)
def init_schema_registry_fixture():
    """Initialize schema registry for error handling tests."""
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
                }
            },
            "Post": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "title": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
            "Comment": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "text": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
        },
    }

    fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))

    yield

    fraiseql_rs.reset_schema_registry_for_testing()
