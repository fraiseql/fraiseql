"""Test rust_pipeline.py with fraiseql_rs v0.2.0 API."""

import pytest
from fraiseql import _fraiseql_rs as fraiseql_rs
from src.fraiseql.core.rust_pipeline import RustResponseBytes


def test_build_graphql_response_list():
    """Test list response with new API."""
    json_strings = ['{"id": 1, "user_name": "Alice"}', '{"id": 2, "user_name": "Bob"}']

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=json_strings, field_name="users", type_name="User", field_paths=None
    )

    result = response_bytes.decode("utf-8")

    # Should have GraphQL wrapper
    assert '"data"' in result
    assert '"users"' in result

    # Should be an array
    assert "[" in result

    # Should have camelCase
    assert '"userName"' in result

    # Should have __typename
    assert '"__typename":"User"' in result


def test_build_graphql_response_empty_list():
    """Test empty list response."""
    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[],  # Empty list
        field_name="users",
        type_name=None,
        field_paths=None,
    )

    result = response_bytes.decode("utf-8")

    # Should have empty array
    assert '"users":[]' in result


def test_build_graphql_response_single_object():
    """Test single object response."""
    json_string = '{"id": 1, "user_name": "Alice"}'

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],  # Single item in list
        field_name="user",
        type_name="User",
        field_paths=None,
    )

    result = response_bytes.decode("utf-8")

    # Single object (not array)
    assert '"user":{' in result
    assert '"userName":"Alice"' in result


def test_build_graphql_response_with_projection():
    """Test field projection."""
    json_string = '{"id": 1, "user_name": "Alice", "email": "alice@example.com", "age": 30}'

    field_paths = [["id"], ["user_name"]]  # Only request id and user_name

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string], field_name="user", type_name="User", field_paths=field_paths
    )

    result = response_bytes.decode("utf-8")

    # Should have projected fields
    assert '"id"' in result
    assert '"userName"' in result

    # Should NOT have non-projected fields
    assert '"email"' not in result
    assert '"age"' not in result


def test_rust_response_bytes_wrapper():
    """Test RustResponseBytes wrapper class."""
    data = b'{"test": "data"}'
    wrapper = RustResponseBytes(data)

    assert wrapper.bytes == data
    assert wrapper.content_type == "application/json"
    assert bytes(wrapper) == data
