"""Integration tests for __typename injection in GraphQL responses.

These tests verify that __typename fields are correctly injected into GraphQL query responses
when using the Rust pipeline with real database queries.
"""

import uuid
from typing import Optional

import pytest
from fastapi.testclient import TestClient

from fraiseql import query
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.types import fraise_type


@fraise_type
class User:
    id: uuid.UUID
    name: str
    email: str


@fraise_type
class Post:
    id: uuid.UUID
    title: str
    content: str


@query
async def user(info, id: uuid.UUID) -> Optional[User]:
    """Get a user by ID using real database query."""
    repo = info.context["db"]

    # Query the database directly
    from psycopg.sql import SQL, Literal

    query = repo._build_find_one_query(
        "v_user", where={"id": str(id)}, field_paths=None, jsonb_column="data"
    )
    results = await repo.run(query)

    if not results:
        return None

    # Convert to typed object
    import json

    user_data = json.loads(results[0]["data"])
    return User(id=uuid.UUID(user_data["id"]), name=user_data["name"], email=user_data["email"])


@query
async def users(info, limit: int = 10) -> list[User]:
    """Get list of users using real database query."""
    repo = info.context["db"]

    # Query the database directly
    query = repo._build_find_query(
        "v_user", where=None, limit=limit, field_paths=None, jsonb_column="data"
    )
    results = await repo.run(query)

    # Convert to typed objects
    import json

    users_list = []
    for row in results:
        user_data = json.loads(row["data"])
        users_list.append(
            User(id=uuid.UUID(user_data["id"]), name=user_data["name"], email=user_data["email"])
        )
    return users_list


@query
async def posts(info, limit: int = 10) -> list[Post]:
    """Get list of posts using real database query."""
    repo = info.context["db"]

    # Query the database directly
    query = repo._build_find_query(
        "v_post", where=None, limit=limit, field_paths=None, jsonb_column="data"
    )
    results = await repo.run(query)

    # Convert to typed objects
    import json

    posts_list = []
    for row in results:
        post_data = json.loads(row["data"])
        posts_list.append(
            Post(
                id=uuid.UUID(post_data["id"]),
                title=post_data["title"],
                content=post_data["content"],
            )
        )
    return posts_list


@pytest.fixture
async def setup_typename_test_data(db_connection):
    """Set up real database with JSONB for typename tests."""
    async with db_connection.cursor() as cur:
        # Create tables with JSONB
        await cur.execute("""
            CREATE TABLE IF NOT EXISTS v_user (
                id UUID PRIMARY KEY,
                data JSONB NOT NULL
            )
        """)

        await cur.execute("""
            CREATE TABLE IF NOT EXISTS v_post (
                id UUID PRIMARY KEY,
                data JSONB NOT NULL
            )
        """)

        # Insert test data
        await cur.execute("""
            INSERT INTO v_user (id, data) VALUES
            (
                '11111111-1111-1111-1111-111111111111',
                '{"id": "11111111-1111-1111-1111-111111111111", "name": "Alice", "email": "alice@example.com"}'
            ),
            (
                '22222222-2222-2222-2222-222222222222',
                '{"id": "22222222-2222-2222-2222-222222222222", "name": "Bob", "email": "bob@example.com"}'
            )
            ON CONFLICT (id) DO NOTHING
        """)

        await cur.execute("""
            INSERT INTO v_post (id, data) VALUES
            (
                '33333333-3333-3333-3333-333333333333',
                '{"id": "33333333-3333-3333-3333-333333333333", "title": "First Post", "content": "Content of first post"}'
            ),
            (
                '44444444-4444-4444-4444-444444444444',
                '{"id": "44444444-4444-4444-4444-444444444444", "title": "Second Post", "content": "Content of second post"}'
            )
            ON CONFLICT (id) DO NOTHING
        """)

        await db_connection.commit()


@pytest.fixture
def graphql_client(db_pool, setup_typename_test_data):
    """Create a GraphQL test client with real database connection."""
    # Inject the test database pool
    from fraiseql.fastapi.dependencies import set_db_pool

    set_db_pool(db_pool)

    app = create_fraiseql_app(
        database_url="postgresql://test/test",  # Dummy URL since we're injecting pool
        types=[User, Post],
        queries=[user, users, posts],
        production=False,
    )
    return TestClient(app)


def test_typename_injected_in_single_object_response(graphql_client):
    """Test that __typename is injected in single object query responses."""
    query = """
    query GetUser {
        user(id: "11111111-1111-1111-1111-111111111111") {
            __typename
            id
            name
            email
        }
    }
    """

    response = graphql_client.post("/graphql", json={"query": query})
    assert response.status_code == 200

    result = response.json()
    assert "data" in result
    assert "user" in result["data"]
    assert result["data"]["user"] is not None
    assert result["data"]["user"]["__typename"] == "User"
    assert result["data"]["user"]["name"] == "Alice"


def test_typename_injected_in_list_response(graphql_client):
    """Test that __typename is injected in list query responses."""
    query = """
    query GetUsers {
        users(limit: 2) {
            __typename
            id
            name
            email
        }
    }
    """

    response = graphql_client.post("/graphql", json={"query": query})
    assert response.status_code == 200

    result = response.json()
    assert "data" in result
    assert "users" in result["data"]
    assert len(result["data"]["users"]) == 2

    for user in result["data"]["users"]:
        assert user["__typename"] == "User"
        assert "id" in user
        assert "name" in user
        assert "email" in user


def test_typename_injected_in_mixed_query_response(graphql_client):
    """Test that __typename is injected correctly in queries with different types."""
    query = """
    query GetMixedData {
        users(limit: 1) {
            __typename
            id
            name
        }
        posts(limit: 1) {
            __typename
            id
            title
        }
    }
    """

    response = graphql_client.post("/graphql", json={"query": query})
    assert response.status_code == 200

    result = response.json()
    assert "data" in result

    # Check users
    assert "users" in result["data"]
    assert len(result["data"]["users"]) == 1
    assert result["data"]["users"][0]["__typename"] == "User"

    # Check posts
    assert "posts" in result["data"]
    assert len(result["data"]["posts"]) == 1
    assert result["data"]["posts"][0]["__typename"] == "Post"


def test_typename_injected_when_query_returns_null(graphql_client):
    """Test that __typename handling works even when query returns null."""
    query = """
    query GetNonExistentUser {
        user(id: "99999999-9999-9999-9999-999999999999") {
            __typename
            id
            name
        }
    }
    """

    response = graphql_client.post("/graphql", json={"query": query})
    assert response.status_code == 200

    result = response.json()
    assert "data" in result
    assert result["data"]["user"] is None  # Should be null, not an object with __typename


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
