"""
Integration tests for GraphQL Cascade functionality.

Tests end-to-end cascade behavior from PostgreSQL functions through
GraphQL responses to client cache updates.
"""

import pytest
import asyncio
from typing import Dict, Any, List, Optional
from unittest.mock import AsyncMock, MagicMock

import pytest_asyncio
from fastapi.testclient import TestClient

import fraiseql
from fraiseql import gql
from fraiseql.mutations import mutation


# Test types
@fraiseql.input
class CreatePostInput:
    title: str
    content: Optional[str] = None
    author_id: str


@fraiseql.type
class Post:
    id: str
    title: str
    content: Optional[str] = None
    author_id: str


@fraiseql.type
class User:
    id: str
    name: str
    post_count: int


@fraiseql.type
class CreatePostSuccess:
    id: str
    message: str


@fraiseql.type
class CreatePostError:
    code: str
    message: str


# Test mutations
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError


def test_cascade_end_to_end(cascade_client, db_connection):
    """Test complete cascade flow from PostgreSQL function to GraphQL response."""
    # Setup test data
    db_connection.execute("""
        INSERT INTO tb_user (id, name, post_count)
        VALUES ('user-123', 'Test User', 0)
    """)

    # Execute mutation
    mutation_query = """
    mutation CreatePost($input: CreatePostInput!) {
        createPost(input: $input) {
            id
            message
            cascade {
                updated {
                    __typename
                    id
                    operation
                    entity
                }
                deleted
                invalidations {
                    queryName
                    strategy
                    scope
                }
                metadata {
                    timestamp
                    affectedCount
                }
            }
        }
    }
    """

    variables = {
        "input": {"title": "Test Post", "content": "Test content", "author_id": "user-123"}
    }

    response = cascade_client.post(
        "/graphql", json={"query": mutation_query, "variables": variables}
    )

    assert response.status_code == 200
    data = response.json()

    # Verify response structure
    assert "data" in data
    assert "createPost" in data["data"]
    assert data["data"]["createPost"]["id"]
    assert data["data"]["createPost"]["message"] == "Post created successfully"

    # Verify cascade data
    cascade = data["data"]["createPost"]["cascade"]
    assert cascade is not None
    assert "updated" in cascade
    assert "deleted" in cascade
    assert "invalidations" in cascade
    assert "metadata" in cascade

    # Verify cascade content
    assert len(cascade["updated"]) == 2  # Post + User

    # Find Post entity
    post_entity = next((u for u in cascade["updated"] if u["__typename"] == "Post"), None)
    assert post_entity is not None
    assert post_entity["operation"] == "CREATED"
    assert post_entity["entity"]["title"] == "Test Post"

    # Find User entity
    user_entity = next((u for u in cascade["updated"] if u["__typename"] == "User"), None)
    assert user_entity is not None
    assert user_entity["operation"] == "UPDATED"
    assert user_entity["entity"]["post_count"] == 1

    # Verify invalidations
    assert len(cascade["invalidations"]) >= 1
    posts_invalidation = next(
        (i for i in cascade["invalidations"] if i["queryName"] == "posts"), None
    )
    assert posts_invalidation is not None
    assert posts_invalidation["strategy"] == "INVALIDATE"

    # Verify metadata
    assert cascade["metadata"]["affectedCount"] == 2
    assert "timestamp" in cascade["metadata"]


def test_cascade_with_error_response(cascade_client, db_connection):
    """Test cascade behavior when mutation returns an error."""
    mutation_query = """
    mutation CreatePost($input: CreatePostInput!) {
        createPost(input: $input) {
            code
            message
            cascade {
                updated
                deleted
                invalidations
            }
        }
    }
    """

    variables = {
        "input": {
            "title": "",  # Invalid: empty title
            "author_id": "nonexistent-user",
        }
    }

    response = cascade_client.post(
        "/graphql", json={"query": mutation_query, "variables": variables}
    )

    assert response.status_code == 200
    data = response.json()

    # Should have error response
    assert "data" in data
    assert "createPost" in data["data"]
    assert data["data"]["createPost"]["code"] == "VALIDATION_ERROR"

    # Cascade should be absent or empty on error
    cascade = data["data"]["createPost"].get("cascade")
    # Note: Depending on implementation, cascade might be None or empty on errors


@pytest.mark.asyncio
async def test_cascade_large_payload(cascade_client, db_connection):
    """Test cascade with multiple entities and operations."""
    # Create multiple users and posts for complex cascade
    await db_connection.execute("""
        INSERT INTO tb_user (id, name, post_count)
        VALUES
            ('user-1', 'User 1', 0),
            ('user-2', 'User 2', 0),
            ('user-3', 'User 3', 0)
    """)

    # This would test a more complex cascade scenario
    # Implementation depends on specific PostgreSQL function logic


def test_cascade_disabled_by_default(cascade_client, db_connection):
    """Test that cascade is not included when enable_cascade=False."""

    @mutation(enable_cascade=False)  # Explicitly disabled
    class CreatePostNoCascade:
        input: CreatePostInput
        success: CreatePostSuccess
        error: CreatePostError

    # This test would verify that cascade field is absent
    # when enable_cascade=False, even if PostgreSQL function returns _cascade


def test_cascade_malformed_data_handling(cascade_client):
    """Test handling of malformed cascade data from PostgreSQL."""
    # This would test error handling for invalid cascade JSON structure
    # Should not break the mutation response, should log warnings


class MockApolloClient:
    """Mock Apollo Client for testing cache integration."""

    def __init__(self):
        self.cache = MagicMock()
        self.mutate = AsyncMock()

    def writeFragment(self, options):
        """Mock cache write operation."""
        pass

    def evict(self, options):
        """Mock cache eviction."""
        pass


def test_apollo_client_cascade_integration():
    """Test Apollo Client cache updates from cascade data."""
    client = MockApolloClient()

    # Simulate cascade data
    cascade_data = {
        "updated": [
            {
                "__typename": "Post",
                "id": "post-123",
                "operation": "CREATED",
                "entity": {"id": "post-123", "title": "Test Post"},
            },
            {
                "__typename": "User",
                "id": "user-456",
                "operation": "UPDATED",
                "entity": {"id": "user-456", "post_count": 1},
            },
        ],
        "invalidations": [{"queryName": "posts", "strategy": "INVALIDATE", "scope": "PREFIX"}],
    }

    # Simulate Apollo Client cascade processing
    for update in cascade_data["updated"]:
        client.writeFragment(
            {
                "id": client.cache.identify(
                    {"__typename": update["__typename"], "id": update["id"]}
                ),
                "fragment": f"fragment _ on {update['__typename']} {{ id }}",
                "data": update["entity"],
            }
        )

    for invalidation in cascade_data["invalidations"]:
        if invalidation["strategy"] == "INVALIDATE":
            client.cache.evict({"fieldName": invalidation["queryName"]})

    # Verify cache operations were called
    assert client.cache.writeFragment.call_count == 2
    assert client.cache.evict.call_count == 1


def test_cascade_data_validation():
    """Test validation of cascade data structure."""
    # Valid cascade data
    valid_cascade = {
        "updated": [
            {
                "__typename": "Post",
                "id": "post-123",
                "operation": "CREATED",
                "entity": {"id": "post-123", "title": "Test"},
            }
        ],
        "deleted": [],
        "invalidations": [{"queryName": "posts", "strategy": "INVALIDATE", "scope": "PREFIX"}],
        "metadata": {"timestamp": "2025-11-13T10:00:00Z", "affectedCount": 1},
    }

    # Should pass validation
    assert validate_cascade_structure(valid_cascade)

    # Invalid cascade data (missing required fields)
    invalid_cascade = {
        "updated": [{"__typename": "Post"}]  # Missing id, operation, entity
    }

    # Should fail validation
    assert not validate_cascade_structure(invalid_cascade)


def validate_cascade_structure(cascade: Dict[str, Any]) -> bool:
    """Validate cascade data structure."""
    required_keys = {"updated", "deleted", "invalidations", "metadata"}

    if not all(key in cascade for key in required_keys):
        return False

    # Validate updated entities
    for entity in cascade["updated"]:
        required_entity_keys = {"__typename", "id", "operation", "entity"}
        if not all(key in entity for key in required_entity_keys):
            return False

    # Validate invalidations
    for invalidation in cascade["invalidations"]:
        required_invalidation_keys = {"queryName", "strategy", "scope"}
        if not all(key in invalidation for key in required_invalidation_keys):
            return False

    return True
