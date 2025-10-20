"""Test fraiseql_rs schema-aware nested array resolution.

Phase 5, TDD Cycle 5.1 - RED: Test schema-based automatic type resolution
These tests should FAIL initially because the function doesn't exist yet.

This phase builds on Phase 4's typename injection by adding:
- Schema registry for automatic type detection
- Array field detection
- Polymorphic array support (union types)
- Cleaner API with schema awareness
"""

import json
import pytest


def test_schema_based_transformation_simple():
    """Test transformation with v0.2.0 API (schema-based transformation removed).

    v0.2.0: Schema-based automatic type detection removed.
    Now uses unified build_graphql_response() API.
    """
    import fraiseql_rs

    input_json = '{"id": 1, "name": "John", "email": "john@example.com"}'

    # v0.2.0: Use build_graphql_response for transformation
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json],
        field_name="user",
        type_name="User",  # Only root level __typename
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # New API wraps in GraphQL response structure
    assert result["data"]["user"] == {
        "__typename": "User",
        "id": 1,
        "name": "John",
        "email": "john@example.com",
    }


def test_schema_based_transformation_with_array():
    """Test array handling with v0.2.0 API (nested __typename injection removed)."""
    import fraiseql_rs

    # v0.2.0: No automatic schema-based type detection for nested objects
    # Arrays are handled by passing each element as separate JSON string

    # For array of posts, pass each post as separate JSON string
    post_jsons = [
        '{"id": 1, "title": "First Post"}',
        '{"id": 2, "title": "Second Post"}',
    ]

    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=post_jsons,
        field_name="posts",
        type_name="Post",  # All array elements get this type
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # New API creates array with __typename on each element
    assert result["data"]["posts"][0]["__typename"] == "Post"
    assert result["data"]["posts"][0]["id"] == 1
    assert result["data"]["posts"][1]["__typename"] == "Post"


def test_schema_based_nested_arrays():
    """Test nested structures with v0.2.0 API (simplified - no automatic nested __typename)."""
    import fraiseql_rs

    # v0.2.0: Complex nested structures need to be handled differently
    # The API doesn't automatically inject __typename into nested objects
    # For this test, we'll test a simpler case - just the root level

    input_json = json.dumps(
        {
            "id": 1,
            "name": "John",
            "posts": [
                {
                    "id": 1,
                    "title": "First",
                    "comments": [
                        {"id": 1, "text": "Great!"},
                        {"id": 2, "text": "Thanks!"},
                    ],
                }
            ],
        }
    )

    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json],
        field_name="user",
        type_name="User",  # Only root gets __typename
        field_paths=None,
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    # Only root level gets __typename in v0.2.0
    user_data = result["data"]["user"]
    assert user_data["__typename"] == "User"
    assert user_data["id"] == 1
    assert user_data["name"] == "John"

    # Nested objects don't get __typename automatically
    assert user_data["posts"][0]["id"] == 1
    assert user_data["posts"][0]["title"] == "First"
    assert user_data["posts"][0]["comments"][0]["text"] == "Great!"


def test_schema_based_nullable_fields():
    """Test handling of nullable fields with v0.2.0 API."""
    import fraiseql_rs

    # Test with null profile
    input_json = json.dumps({"id": 1, "name": "John", "profile": None})
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"
    assert result["data"]["user"]["profile"] is None

    # Test with actual profile (nested objects don't get __typename)
    input_json = json.dumps({"id": 1, "name": "John", "profile": {"bio": "Developer"}})
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"
    # Nested profile doesn't get __typename in v0.2.0
    assert result["data"]["user"]["profile"]["bio"] == "Developer"


def test_schema_based_empty_arrays():
    """Test handling of empty arrays with v0.2.0 API."""
    import fraiseql_rs

    input_json = json.dumps({"id": 1, "posts": []})
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"
    assert result["data"]["user"]["posts"] == []


def test_schema_based_mixed_fields():
    """Test object with mix of scalars, objects, and arrays with v0.2.0 API."""
    import fraiseql_rs

    input_json = json.dumps(
        {
            "id": 1,
            "name": "John",
            "is_active": True,
            "profile": {"bio": "Developer"},
            "posts": [{"id": 1, "title": "First"}],
        }
    )

    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    user_data = result["data"]["user"]
    assert user_data["__typename"] == "User"
    assert user_data["id"] == 1
    assert user_data["name"] == "John"
    assert user_data["isActive"] is True
    # Nested objects don't get __typename in v0.2.0
    assert user_data["profile"]["bio"] == "Developer"
    assert user_data["posts"][0]["id"] == 1


def test_schema_registry():
    """Test that SchemaRegistry is removed in v0.2.0."""
    import fraiseql_rs
    import pytest

    # SchemaRegistry removed in v0.2.0 - should not exist
    with pytest.raises(AttributeError):
        registry = fraiseql_rs.SchemaRegistry()

    # Use new API instead
    input_json = json.dumps(
        {
            "id": 1,
            "name": "John",
            "posts": [{"id": 1, "title": "First"}],
        }
    )

    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"


def test_backward_compatibility_with_phase4():
    """Test that old Phase 4 APIs are removed in v0.2.0."""
    import fraiseql_rs
    import pytest

    # Old Phase 4 API removed in v0.2.0
    type_map = {"$": "User", "posts": "Post"}
    input_json = json.dumps({"id": 1, "posts": [{"id": 1}]})

    with pytest.raises(AttributeError):
        result_json = fraiseql_rs.transform_json_with_typename(input_json, type_map)

    # Use new API instead
    result_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[input_json], field_name="user", type_name="User", field_paths=None
    )
    result_json = result_bytes.decode("utf-8")
    result = json.loads(result_json)

    assert result["data"]["user"]["__typename"] == "User"


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
