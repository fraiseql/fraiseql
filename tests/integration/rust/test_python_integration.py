"""Integration tests for Rust transformer Python bindings.

Tests the complete integration between fraiseql_rs (Rust) and Python code.
Verifies that the Rust module builds, imports, and functions correctly.
"""

import json

import pytest


def test_rust_module_can_be_imported():
    """Test that fraiseql_rs module can be imported (RED phase - will fail initially)."""
    try:
        import fraiseql_rs
        assert fraiseql_rs is not None
    except ImportError as e:
        pytest.fail(f"Failed to import fraiseql_rs: {e}")


def test_rust_transformer_wrapper_exists():
    """Test that get_transformer() function exists and returns a transformer."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()
    assert transformer is not None


def test_basic_camel_case_transformation():
    """Test basic snake_case to camelCase transformation."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    input_json = '{"user_id": 1, "user_name": "John"}'
    result = transformer.transform_json_passthrough(input_json)

    expected = {"userId": 1, "userName": "John"}
    assert json.loads(result) == expected


def test_typename_injection():
    """Test __typename injection for GraphQL responses."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    input_json = '{"user_id": 1, "user_name": "John"}'
    result = transformer.transform(input_json, "User")

    data = json.loads(result)
    assert data["__typename"] == "User"
    assert data["userId"] == 1
    assert data["userName"] == "John"


def test_nested_object_transformation():
    """Test transformation of nested objects."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    input_json = """{
        "user_id": 1,
        "user_profile": {
            "first_name": "John",
            "last_name": "Doe"
        }
    }"""

    result = transformer.transform(input_json, "User")
    data = json.loads(result)

    assert data["__typename"] == "User"
    assert data["userId"] == 1
    assert "userProfile" in data
    assert data["userProfile"]["firstName"] == "John"
    assert data["userProfile"]["lastName"] == "Doe"


def test_array_transformation():
    """Test transformation of arrays."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    input_json = """{
        "user_posts": [
            {"post_id": 1, "post_title": "First Post"},
            {"post_id": 2, "post_title": "Second Post"}
        ]
    }"""

    result = transformer.transform_json_passthrough(input_json)
    data = json.loads(result)

    assert "userPosts" in data
    assert len(data["userPosts"]) == 2
    assert data["userPosts"][0]["postId"] == 1
    assert data["userPosts"][0]["postTitle"] == "First Post"


def test_raw_json_result_transform_integration():
    """Test that RawJSONResult.transform() works with Rust transformer."""
    from fraiseql.core.raw_json_executor import RawJSONResult

    # Create a GraphQL response with snake_case
    graphql_response = json.dumps({
        "data": {
            "users": [
                {"user_id": 1, "user_name": "John"},
                {"user_id": 2, "user_name": "Jane"}
            ]
        }
    })

    result = RawJSONResult(graphql_response)
    transformed = result.transform("User")

    data = json.loads(transformed.json_string)
    users = data["data"]["users"]

    # Should be transformed to camelCase with __typename
    assert users[0]["__typename"] == "User"
    assert users[0]["userId"] == 1
    assert users[0]["userName"] == "John"


def test_performance_baseline():
    """Test that Rust transformation is reasonably fast."""
    import time

    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    # Generate a moderately complex JSON structure
    input_data = {
        "user_id": 1,
        "user_name": "John Doe",
        "user_posts": [
            {
                "post_id": i,
                "post_title": f"Post {i}",
                "post_comments": [
                    {"comment_id": j, "comment_text": f"Comment {j}"}
                    for j in range(5)
                ]
            }
            for i in range(10)
        ]
    }
    input_json = json.dumps(input_data)

    # Measure time for 100 transformations
    start = time.perf_counter()
    for _ in range(100):
        _ = transformer.transform_json_passthrough(input_json)
    elapsed = time.perf_counter() - start

    # Should complete 100 transformations in under 1 second
    assert elapsed < 1.0, f"Performance too slow: {elapsed:.3f}s for 100 transforms"


def test_error_handling_invalid_json():
    """Test that invalid JSON is handled gracefully."""
    from fraiseql.core.rust_transformer import get_transformer

    transformer = get_transformer()

    invalid_json = '{"user_id": 1, invalid'

    # Should raise an exception or return error, not crash
    # Using ValueError as Rust JSON parser raises specific parse errors
    with pytest.raises((ValueError, RuntimeError)):
        transformer.transform_json_passthrough(invalid_json)


def test_rust_module_has_expected_functions():
    """Test that fraiseql_rs module exports expected functions."""
    import fraiseql_rs

    # Check for expected functions
    assert hasattr(fraiseql_rs, "to_camel_case")
    assert callable(fraiseql_rs.to_camel_case)

    # Test to_camel_case
    result = fraiseql_rs.to_camel_case("user_name")
    assert result == "userName"

    result = fraiseql_rs.to_camel_case("user_profile_picture")
    assert result == "userProfilePicture"
