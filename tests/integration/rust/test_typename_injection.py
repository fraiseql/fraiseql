"""Test fraiseql_rs __typename injection.

Phase 4, TDD Cycle 4.1 - RED: Test __typename injection during JSON transformation
These tests should FAIL initially because the function doesn't exist yet.
"""
import json
import pytest


def test_transform_json_with_typename_simple():
    """Test simple object with __typename injection.

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing transform_json_with_typename(), this should pass
    """
    import fraiseql_rs

    input_json = '{"user_id": 1, "user_name": "John"}'
    result_json = fraiseql_rs.transform_json_with_typename(input_json, "User")
    result = json.loads(result_json)

    assert result == {
        "__typename": "User",
        "userId": 1,
        "userName": "John",
    }


def test_transform_json_with_typename_nested():
    """Test nested object with __typename injection."""
    import fraiseql_rs

    input_json = json.dumps({
        "user_id": 1,
        "user_name": "John",
        "user_profile": {
            "first_name": "John",
            "last_name": "Doe",
        },
    })

    # Type map: root is User, user_profile is Profile
    type_map = {
        "$": "User",
        "user_profile": "Profile",
    }

    result_json = fraiseql_rs.transform_json_with_typename(input_json, type_map)
    result = json.loads(result_json)

    assert result == {
        "__typename": "User",
        "userId": 1,
        "userName": "John",
        "userProfile": {
            "__typename": "Profile",
            "firstName": "John",
            "lastName": "Doe",
        },
    }


def test_transform_json_with_typename_array():
    """Test array of objects with __typename injection."""
    import fraiseql_rs

    input_json = json.dumps({
        "user_id": 1,
        "user_posts": [
            {"post_id": 1, "post_title": "First Post"},
            {"post_id": 2, "post_title": "Second Post"},
        ],
    })

    # Type map: root is User, each post is Post
    type_map = {
        "$": "User",
        "user_posts": "Post",  # Type for array elements
    }

    result_json = fraiseql_rs.transform_json_with_typename(input_json, type_map)
    result = json.loads(result_json)

    assert result == {
        "__typename": "User",
        "userId": 1,
        "userPosts": [
            {"__typename": "Post", "postId": 1, "postTitle": "First Post"},
            {"__typename": "Post", "postId": 2, "postTitle": "Second Post"},
        ],
    }


def test_transform_json_with_typename_complex():
    """Test complex nested structure with multiple __typename injections."""
    import fraiseql_rs

    input_json = json.dumps({
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
    })

    # Type map with nested types
    type_map = {
        "$": "User",
        "posts": "Post",
        "posts.comments": "Comment",
    }

    result_json = fraiseql_rs.transform_json_with_typename(input_json, type_map)
    result = json.loads(result_json)

    # Verify root
    assert result["__typename"] == "User"
    assert result["id"] == 1
    assert result["name"] == "James Rodriguez"

    # Verify posts array
    assert len(result["posts"]) == 2
    assert result["posts"][0]["__typename"] == "Post"
    assert result["posts"][0]["id"] == 1
    assert result["posts"][0]["title"] == "First Post"

    # Verify nested comments array
    assert len(result["posts"][0]["comments"]) == 2
    assert result["posts"][0]["comments"][0]["__typename"] == "Comment"
    assert result["posts"][0]["comments"][0]["id"] == 1
    assert result["posts"][0]["comments"][0]["text"] == "Great post!"


def test_transform_json_with_typename_no_types():
    """Test that transformation works without typename when no type map provided."""
    import fraiseql_rs

    input_json = '{"user_id": 1, "user_name": "John"}'

    # Pass None or empty dict - should work like transform_json
    result_json = fraiseql_rs.transform_json_with_typename(input_json, None)
    result = json.loads(result_json)

    assert result == {
        "userId": 1,
        "userName": "John",
    }
    assert "__typename" not in result


def test_transform_json_with_typename_empty_object():
    """Test edge case: empty object with typename."""
    import fraiseql_rs

    input_json = "{}"
    result_json = fraiseql_rs.transform_json_with_typename(input_json, "Empty")
    result = json.loads(result_json)

    assert result == {"__typename": "Empty"}


def test_transform_json_with_typename_preserves_existing():
    """Test that existing __typename fields are replaced."""
    import fraiseql_rs

    input_json = '{"__typename": "OldType", "user_id": 1}'
    result_json = fraiseql_rs.transform_json_with_typename(input_json, "NewType")
    result = json.loads(result_json)

    assert result == {
        "__typename": "NewType",
        "userId": 1,
    }


def test_transform_json_with_typename_string_type():
    """Test simple string typename (not dict)."""
    import fraiseql_rs

    input_json = '{"user_id": 1}'
    result_json = fraiseql_rs.transform_json_with_typename(input_json, "User")
    result = json.loads(result_json)

    assert result["__typename"] == "User"
    assert result["userId"] == 1


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
