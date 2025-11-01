"""Test fraiseql_rs camelCase conversion.

Phase 2, TDD Cycle 2.1 - RED: Test basic snake_case → camelCase conversion
These tests should FAIL initially because the function doesn't exist yet.
"""

import pytest


def test_transform_keys():
    """Test batch transformation of dictionary keys.

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing transform_keys(), this should pass
    """
    from fraiseql import _fraiseql_rs as fraiseql_rs

    input_dict = {
        "user_id": 1,
        "user_name": "John",
        "email_address": "john@example.com",
        "created_at": "2025-01-01",
    }

    expected = {
        "userId": 1,
        "userName": "John",
        "emailAddress": "john@example.com",
        "createdAt": "2025-01-01",
    }

    result = fraiseql_rs.transform_keys(input_dict)
    assert result == expected


def test_transform_keys_nested():
    """Test transformation of nested dictionaries."""
    from fraiseql import _fraiseql_rs as fraiseql_rs

    input_dict = {
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

    expected = {
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

    result = fraiseql_rs.transform_keys(input_dict, recursive=True)
    assert result == expected


def test_transform_keys_with_lists():
    """Test transformation with lists of dictionaries."""
    from fraiseql import _fraiseql_rs as fraiseql_rs

    input_dict = {
        "user_id": 1,
        "user_posts": [
            {"post_id": 1, "post_title": "First Post"},
            {"post_id": 2, "post_title": "Second Post"},
        ],
    }

    expected = {
        "userId": 1,
        "userPosts": [
            {"postId": 1, "postTitle": "First Post"},
            {"postId": 2, "postTitle": "Second Post"},
        ],
    }

    result = fraiseql_rs.transform_keys(input_dict, recursive=True)
    assert result == expected


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
