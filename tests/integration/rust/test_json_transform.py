"""Test fraiseql_rs JSON parsing and transformation.

Test direct JSON to transformed JSON functionality.
"""

import json

import pytest

pytestmark = pytest.mark.integration


def test_transform_json_simple() -> None:
    """Test simple JSON object transformation.

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing transform_json(), this should pass
    """
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = '{"user_id": 1, "user_name": "John", "email_address": "john@example.com"}'
    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result == {
        "userId": 1,
        "userName": "John",
        "emailAddress": "john@example.com",
    }


def test_transform_json_nested() -> None:
    """Test nested JSON object transformation."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = json.dumps(
        {
            "user_id": 1,
            "user_profile": {
                "first_name": "John",
                "last_name": "Doe",
                "billing_address": {
                    "street_name": "Main St",
                    "postal_code": "12345",
                },
            },
        }
    )

    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result == {
        "userId": 1,
        "userProfile": {
            "firstName": "John",
            "lastName": "Doe",
            "billingAddress": {
                "streetName": "Main St",
                "postalCode": "12345",
            },
        },
    }


def test_transform_json_with_array() -> None:
    """Test JSON with arrays of objects."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = json.dumps(
        {
            "user_id": 1,
            "user_posts": [
                {"post_id": 1, "post_title": "First Post", "created_at": "2025-01-01"},
                {"post_id": 2, "post_title": "Second Post", "created_at": "2025-01-02"},
            ],
        }
    )

    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result == {
        "userId": 1,
        "userPosts": [
            {"postId": 1, "postTitle": "First Post", "createdAt": "2025-01-01"},
            {"postId": 2, "postTitle": "Second Post", "createdAt": "2025-01-02"},
        ],
    }


def test_transform_json_complex() -> None:
    """Test complex nested structure (like FraiseQL User with posts)."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    # Simulate FraiseQL query result from database
    input_json = json.dumps(
        {
            "id": 1,
            "name": "James Rodriguez",
            "email": "james.rodriguez@example.com",
            "created_at": "2025-04-03T09:10:28.71191",
            "posts": [
                {
                    "id": 3361,
                    "user_id": 1,
                    "title": "Python vs Alternatives",
                    "content": "This is a comprehensive guide...",
                    "created_at": "2025-02-02T09:10:29.55859",
                },
                {
                    "id": 4647,
                    "user_id": 1,
                    "title": "React Tutorial for Beginners",
                    "content": "This is a comprehensive guide...",
                    "created_at": "2025-03-11T09:10:29.566722",
                },
            ],
        }
    )

    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    # Verify structure
    assert result["id"] == 1
    assert result["name"] == "James Rodriguez"
    assert result["email"] == "james.rodriguez@example.com"
    assert result["createdAt"] == "2025-04-03T09:10:28.71191"

    # Verify posts array
    assert len(result["posts"]) == 2
    assert result["posts"][0]["id"] == 3361
    assert result["posts"][0]["userId"] == 1
    assert result["posts"][0]["title"] == "Python vs Alternatives"
    assert result["posts"][0]["createdAt"] == "2025-02-02T09:10:29.55859"


def test_transform_json_preserves_types() -> None:
    """Test that JSON types are preserved (int, str, bool, null)."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = json.dumps(
        {
            "user_id": 123,
            "user_name": "John",
            "is_active": True,
            "is_deleted": False,
            "deleted_at": None,
            "post_count": 0,
        }
    )

    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result["userId"] == 123  # int preserved
    assert result["userName"] == "John"  # string preserved
    assert result["isActive"] is True  # bool preserved
    assert result["isDeleted"] is False  # bool preserved
    assert result["deletedAt"] is None  # null preserved
    assert result["postCount"] == 0  # zero preserved


def test_transform_json_empty() -> None:
    """Test edge case: empty object."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = "{}"
    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result == {}


def test_transform_json_invalid() -> None:
    """Test error handling for invalid JSON."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    with pytest.raises((ValueError, Exception)):
        fraiseql_rs.transform_json("not valid json")


def test_transform_json_array_root() -> None:
    """Test transformation when root is an array."""
    import fraiseql._fraiseql_rs as fraiseql_rs

    input_json = json.dumps(
        [
            {"user_id": 1, "user_name": "John"},
            {"user_id": 2, "user_name": "Jane"},
        ]
    )

    result_json = fraiseql_rs.transform_json(input_json)
    result = json.loads(result_json)

    assert result == [
        {"userId": 1, "userName": "John"},
        {"userId": 2, "userName": "Jane"},
    ]


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
