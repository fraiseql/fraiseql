"""Test rust_pipeline.py with fraiseql_rs v0.2.0 API."""

import json

import fraiseql._fraiseql_rs as fraiseql_rs
import pytest
from src.fraiseql.core.rust_pipeline import RustResponseBytes


# Schema registry initialization fixture
@pytest.fixture(scope="module", autouse=True)
def init_schema_registry() -> None:
    """Initialize schema registry for tests that need it.

    Uses reset_schema_registry_for_testing() to ensure a clean state,
    allowing re-initialization with the schema these tests need.
    """
    # Reset the schema registry to allow re-initialization
    # This is safe for tests - in production, the registry is never reset
    fraiseql_rs.reset_schema_registry_for_testing()

    # Simple schema IR for testing (includes Equipment for nested object tests)
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "User": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "user_name": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "email": {"type_name": "String", "is_nested_object": False, "is_list": False},
                    "password_hash": {
                        "type_name": "String",
                        "is_nested_object": False,
                        "is_list": False,
                    },
                    "equipment": {
                        "type_name": "Equipment",
                        "is_nested_object": True,
                        "is_list": False,
                    },
                }
            },
            "Equipment": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "name": {"type_name": "String", "is_nested_object": False, "is_list": False},
                }
            },
        },
    }

    result = fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))
    print(f"Schema registry initialization result: {result}")


def test_build_graphql_response_list() -> None:
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


def test_build_graphql_response_empty_list() -> None:
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


def test_build_graphql_response_single_object() -> None:
    """Test single object response (non-list query)."""
    json_string = '{"id": 1, "user_name": "Alice"}'

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],  # Single item in list
        field_name="user",
        type_name="User",
        field_paths=None,
        is_list=False,  # Explicitly request single object (not array)
    )

    result = response_bytes.decode("utf-8")

    # Single object (not array) when is_list=False
    assert '"user":{' in result
    assert '"userName":"Alice"' in result


def test_rust_response_bytes_wrapper() -> None:
    """Test RustResponseBytes wrapper class."""
    data = b'{"test": "data"}'
    wrapper = RustResponseBytes(data)

    assert wrapper.bytes == data
    assert wrapper.content_type == "application/json"
    assert bytes(wrapper) == data


def test_build_graphql_response_with_field_selections_and_aliases() -> None:
    """Test field selections with aliases - Task 3.4 integration test.

    This tests the new field_selections parameter that supports GraphQL aliases.
    Example GraphQL query: user { userId: id, fullName: user_name }
    """
    json_string = '{"id": 1, "user_name": "Alice", "email": "alice@example.com"}'

    # Field selections with aliases (Rust format with materialized_path)
    field_selections = [
        {"materialized_path": "id", "alias": "userId"},
        {"materialized_path": "user_name", "alias": "fullName"},
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections),
    )

    result = response_bytes.decode("utf-8")

    # Should have aliases applied
    assert '"userId"' in result, "Field 'id' should be aliased to 'userId'"
    assert '"fullName"' in result, "Field 'user_name' should be aliased to 'fullName'"

    # Should NOT have original field names when alias is applied
    assert '"userName"' not in result, (
        "camelCase 'userName' should not appear, only alias 'fullName'"
    )

    # Should have correct values
    assert "1" in result
    assert '"Alice"' in result

    # Note: Field projection (filtering non-selected fields) is not yet implemented
    # The "email" field will still be present in the output


def test_build_graphql_response_field_projection_filters_unselected_fields() -> None:
    """RED PHASE: Test that field projection filters out non-selected fields.

    When field_selections are provided, fields NOT in the selections should be excluded
    from the response. This includes sensitive data like passwords.

    Currently: This test FAILS because field projection is not implemented.
    After implementation: This test should PASS.
    """
    json_string = '{"id": 1, "user_name": "Alice", "email": "alice@example.com", "password_hash": "secret123"}'

    # Only select id and user_name (exclude email and password_hash)
    field_selections = [
        {"materialized_path": "id", "alias": "id"},
        {"materialized_path": "user_name", "alias": "userName"},
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections),
        is_list=False,  # Single object response
    )

    result = json.loads(response_bytes.decode("utf-8"))
    user = result["data"]["user"]

    # Should have selected fields
    assert user["__typename"] == "User"
    assert user["id"] == 1
    assert user["userName"] == "Alice"

    # Field projection: these should NOT be present
    assert "email" not in user, "email should be filtered out by field projection"
    assert "passwordHash" not in user, "password_hash should be filtered out by field projection"


def test_build_graphql_response_field_projection_always_includes_typename() -> None:
    """RED PHASE: Test that __typename is always included even when not in selections.

    The __typename field is automatically injected by the GraphQL spec and should
    always be present regardless of field selections.

    Currently: This test PASSES because __typename is already injected.
    """
    json_string = '{"id": 1, "user_name": "Alice"}'

    # Only select id (no explicit __typename)
    field_selections = [
        {"materialized_path": "id", "alias": "id"},
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections),
        is_list=False,  # Single object response
    )

    result = json.loads(response_bytes.decode("utf-8"))
    user = result["data"]["user"]

    # __typename should ALWAYS be present
    assert user["__typename"] == "User"
    assert user["id"] == 1

    # user_name should NOT be present (not selected) - tests field projection
    assert "userName" not in user, "user_name should be filtered out by field projection"


def test_build_graphql_response_with_nested_object_aliases() -> None:
    """Test field selections with nested object aliases.

    Example GraphQL: user { device: equipment { deviceName: name } }
    """
    json_string = '{"id": 1, "equipment": {"id": 10, "name": "Laptop"}}'

    # Nested field selections with aliases (Rust format with materialized_path)
    field_selections = [
        {"materialized_path": "id", "alias": "id"},
        {"materialized_path": "equipment", "alias": "device"},
        {"materialized_path": "equipment.name", "alias": "deviceName"},
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections),
    )

    result = response_bytes.decode("utf-8")

    # Should have top-level alias
    assert '"device"' in result, "Field 'equipment' should be aliased to 'device'"
    assert '"equipment"' not in result, "Original 'equipment' should not appear"

    # Should have nested alias
    assert '"deviceName"' in result, "Nested field 'name' should be aliased to 'deviceName'"
    assert '"name"' not in result, "Original 'name' should not appear"

    # Should have correct value
    assert '"Laptop"' in result
