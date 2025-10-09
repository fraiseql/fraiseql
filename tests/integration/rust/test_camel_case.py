"""Test fraiseql_rs camelCase conversion.

Phase 2, TDD Cycle 2.1 - RED: Test basic snake_case → camelCase conversion
These tests should FAIL initially because the function doesn't exist yet.
"""
import pytest


def test_to_camel_case_basic():
    """Test basic snake_case to camelCase conversion.

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing to_camel_case(), this should pass
    """
    import fraiseql_rs

    # Basic conversions
    assert fraiseql_rs.to_camel_case("user_name") == "userName"
    assert fraiseql_rs.to_camel_case("first_name") == "firstName"
    assert fraiseql_rs.to_camel_case("email_address") == "emailAddress"


def test_to_camel_case_single_word():
    """Test that single words remain unchanged."""
    import fraiseql_rs

    assert fraiseql_rs.to_camel_case("user") == "user"
    assert fraiseql_rs.to_camel_case("email") == "email"
    assert fraiseql_rs.to_camel_case("id") == "id"


def test_to_camel_case_multiple_underscores():
    """Test conversion with multiple underscores."""
    import fraiseql_rs

    assert fraiseql_rs.to_camel_case("user_full_name") == "userFullName"
    assert fraiseql_rs.to_camel_case("billing_address_line_1") == "billingAddressLine1"
    assert fraiseql_rs.to_camel_case("very_long_field_name_example") == "veryLongFieldNameExample"


def test_to_camel_case_edge_cases():
    """Test edge cases."""
    import fraiseql_rs

    # Empty string
    assert fraiseql_rs.to_camel_case("") == ""

    # Already camelCase (no underscores)
    assert fraiseql_rs.to_camel_case("userName") == "userName"

    # Leading underscore (private field - preserve it)
    assert fraiseql_rs.to_camel_case("_private") == "_private"
    assert fraiseql_rs.to_camel_case("_user_name") == "_userName"

    # Trailing underscore
    assert fraiseql_rs.to_camel_case("user_name_") == "userName"

    # Multiple consecutive underscores
    assert fraiseql_rs.to_camel_case("user__name") == "userName"


def test_to_camel_case_with_numbers():
    """Test conversion with numbers in field names."""
    import fraiseql_rs

    assert fraiseql_rs.to_camel_case("address_line_1") == "addressLine1"
    assert fraiseql_rs.to_camel_case("ipv4_address") == "ipv4Address"
    assert fraiseql_rs.to_camel_case("user_123_id") == "user123Id"


def test_transform_keys():
    """Test batch transformation of dictionary keys.

    RED: This should fail with AttributeError (function doesn't exist)
    GREEN: After implementing transform_keys(), this should pass
    """
    import fraiseql_rs

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
    import fraiseql_rs

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
    import fraiseql_rs

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
