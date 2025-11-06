"""Test rust_pipeline.py with fraiseql_rs v0.2.0 API."""

import json
import pytest
from fraiseql import _fraiseql_rs as fraiseql_rs
from src.fraiseql.core.rust_pipeline import RustResponseBytes


# Schema registry initialization fixture
@pytest.fixture(scope="module", autouse=True)
def init_schema_registry():
    """Initialize schema registry for tests that need it."""
    # Simple schema IR for testing
    schema_ir = {
        "version": "1.0",
        "features": ["type_resolution"],
        "types": {
            "User": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "user_name": {"type_name": "String", "is_nested_object": False, "is_list": False},
                    "email": {"type_name": "String", "is_nested_object": False, "is_list": False},
                    "equipment": {"type_name": "Equipment", "is_nested_object": True, "is_list": False}
                }
            },
            "Equipment": {
                "fields": {
                    "id": {"type_name": "Int", "is_nested_object": False, "is_list": False},
                    "name": {"type_name": "String", "is_nested_object": False, "is_list": False}
                }
            }
        }
    }

    try:
        result = fraiseql_rs.initialize_schema_registry(json.dumps(schema_ir))
        print(f"Schema registry initialization result: {result}")
    except RuntimeError as e:
        # Registry is already initialized - this is expected in test sessions
        if "already initialized" in str(e):
            print("Schema registry already initialized (expected)")
        else:
            raise
    except Exception as e:
        print(f"Unexpected error initializing schema registry: {e}")
        raise


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


@pytest.mark.skip(reason="Field projection with field_paths not implemented in schema-aware path")
def test_build_graphql_response_with_projection():
    """Test field projection.

    NOTE: This test is skipped because field_paths projection is not yet implemented
    in the schema-aware transformation path. When schema registry is available,
    the schema-aware path is used which doesn't support the old field_paths parameter.

    Use field_selections with aliases instead for both projection and aliasing.
    """
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


def test_build_graphql_response_with_field_selections_and_aliases():
    """Test field selections with aliases - Task 3.4 integration test.

    This tests the new field_selections parameter that supports GraphQL aliases.
    Example GraphQL query: user { userId: id, fullName: user_name }
    """
    json_string = '{"id": 1, "user_name": "Alice", "email": "alice@example.com"}'

    # Field selections with aliases (Rust format with materialized_path)
    field_selections = [
        {
            "materialized_path": "id",
            "alias": "userId"
        },
        {
            "materialized_path": "user_name",
            "alias": "fullName"
        }
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections)
    )

    result = response_bytes.decode("utf-8")

    # Should have aliases applied
    assert '"userId"' in result, "Field 'id' should be aliased to 'userId'"
    assert '"fullName"' in result, "Field 'user_name' should be aliased to 'fullName'"

    # Should NOT have original field names when alias is applied
    assert '"userName"' not in result, "camelCase 'userName' should not appear, only alias 'fullName'"

    # Should have correct values
    assert '1' in result
    assert '"Alice"' in result

    # Note: Field projection (filtering non-selected fields) is not yet implemented
    # The "email" field will still be present in the output


@pytest.mark.skip(reason="Schema registry singleton - only one initialization per process. Test passes individually. Run with: pytest tests/unit/core/test_rust_pipeline_v2.py::test_build_graphql_response_with_nested_object_aliases -v")
def test_build_graphql_response_with_nested_object_aliases():
    """Test field selections with nested object aliases.

    Example GraphQL: user { device: equipment { deviceName: name } }
    """
    json_string = '{"id": 1, "equipment": {"id": 10, "name": "Laptop"}}'

    # Nested field selections with aliases (Rust format with materialized_path)
    field_selections = [
        {
            "materialized_path": "id",
            "alias": "id"
        },
        {
            "materialized_path": "equipment",
            "alias": "device"
        },
        {
            "materialized_path": "equipment.name",
            "alias": "deviceName"
        }
    ]

    response_bytes = fraiseql_rs.build_graphql_response(
        json_strings=[json_string],
        field_name="user",
        type_name="User",
        field_paths=None,
        field_selections=json.dumps(field_selections)
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
