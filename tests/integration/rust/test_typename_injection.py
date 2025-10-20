"""Test fraiseql_rs __typename injection.

Phase 4, TDD Cycle 4.1 - RED: Test __typename injection during JSON transformation
These tests should FAIL initially because the function doesn't exist yet.
"""

import json
import pytest


def test_build_graphql_response_simple():
    """Test simple object with __typename injection using v0.2.0 API.

    Uses build_graphql_response() instead of deprecated transform_json_with_typename()
    """
    import fraiseql_rs

    input_json = '{"user_id": 1, "user_name": "John"}'
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # New API wraps in GraphQL response structure
    assert result["data"]["user"] == {
        "__typename": "User",
        "userId": 1,
        "userName": "John",
    }


def test_build_graphql_response_nested():
    """Test nested object with __typename injection using v0.2.0 API."""
    import fraiseql_rs

    input_json = json.dumps(
        {
            "user_id": 1,
            "user_name": "John",
            "user_profile": {
                "first_name": "John",
                "last_name": "Doe",
            },
        }
    )

    # v0.2.0: Single API call handles nested types automatically
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json],
        field_name="user",
        type_name="User",  # Root type - nested types handled automatically
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # New API wraps in GraphQL response structure
    # Note: v0.2.0 recursively adds __typename to all nested objects
    assert result["data"]["user"] == {
        "__typename": "User",
        "userId": 1,
        "userName": "John",
        "userProfile": {
            "__typename": "User",  # Nested objects also get __typename
            "firstName": "John",
            "lastName": "Doe",
        },
    }


def test_build_graphql_response_array():
    """Test array of objects with __typename injection using v0.2.0 API."""
    import fraiseql_rs

    # For arrays, we need to pass each array element as separate JSON string
    json_strings = [
        '{"post_id": 1, "post_title": "First Post"}',
        '{"post_id": 2, "post_title": "Second Post"}',
    ]

    # Build response for the array
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings,
        field_name="userPosts",
        type_name="Post",  # Type for each array element
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # New API wraps in GraphQL response structure
    assert result["data"]["userPosts"] == [
        {"__typename": "Post", "postId": 1, "postTitle": "First Post"},
        {"__typename": "Post", "postId": 2, "postTitle": "Second Post"},
    ]


def test_build_graphql_response_complex():
    """Test complex nested structure with __typename injection using v0.2.0 API."""
    import fraiseql_rs

    # For complex nested structures, the new API handles it differently
    # We pass the root object as a single JSON string
    input_json = json.dumps(
        {
            "id": 1,
            "name": "James Rodriguez",
            "email": "james.rodriguez@example.com",
            "posts": [
                {
                    "id": 1,
                    "title": "First Post",
                    "comments": [
                        {"id": 1, "text": "Great post!"},
                        {"id": 2, "text": "Thanks!"},
                    ],
                },
                {
                    "id": 2,
                    "title": "Second Post",
                    "comments": [
                        {"id": 3, "text": "Interesting"},
                    ],
                },
            ],
        }
    )

    # v0.2.0: Single API call for the entire structure
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json],
        field_name="user",
        type_name="User",  # Only root gets __typename in basic usage
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # Verify root is wrapped in GraphQL response
    user_data = result["data"]["user"]
    assert user_data["__typename"] == "User"
    assert user_data["id"] == 1
    assert user_data["name"] == "James Rodriguez"

    # Verify posts array (nested objects don't get __typename automatically)
    assert len(user_data["posts"]) == 2
    assert user_data["posts"][0]["id"] == 1
    assert user_data["posts"][0]["title"] == "First Post"

    # Verify nested comments array
    assert len(user_data["posts"][0]["comments"]) == 2
    assert user_data["posts"][0]["comments"][0]["id"] == 1
    assert user_data["posts"][0]["comments"][0]["text"] == "Great post!"


def test_build_graphql_response_no_types():
    """Test that transformation works without typename when type_name is None."""
    import fraiseql_rs

    input_json = '{"user_id": 1, "user_name": "John"}'

    # Pass None for type_name - should work like transform_json (no __typename)
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json],
        field_name="user",
        type_name=None,  # No __typename injection
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # Should still be wrapped in GraphQL structure but no __typename
    assert result["data"]["user"] == {
        "userId": 1,
        "userName": "John",
    }
    assert "__typename" not in result["data"]["user"]


def test_build_graphql_response_empty_object():
    """Test edge case: empty object with typename."""
    import fraiseql_rs

    input_json = "{}"
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="data", type_name="Empty", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["data"] == {"__typename": "Empty"}


def test_build_graphql_response_preserves_existing():
    """Test that existing __typename fields are replaced."""
    import fraiseql_rs

    input_json = '{"__typename": "OldType", "user_id": 1}'
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="data", type_name="NewType", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["data"] == {
        "__typename": "NewType",
        "userId": 1,
    }


def test_build_graphql_response_string_type():
    """Test simple string typename."""
    import fraiseql_rs

    input_json = '{"user_id": 1}'
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"
    assert result["data"]["user"]["userId"] == 1


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
