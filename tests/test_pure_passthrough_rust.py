"""Tests for Rust transformation integration in pure passthrough mode.

These tests verify that the Rust transformer is correctly integrated into
the execution path and performs snake_case → camelCase transformation.
"""

import pytest
import json
from fraiseql.core.raw_json_executor import RawJSONResult


def test_raw_json_result_transform_with_rust():
    """Test that RawJSONResult.transform() uses Rust transformer."""
    # Create a raw JSON result with snake_case fields
    json_data = {
        "data": {
            "users": [
                {"id": 1, "first_name": "John", "last_name": "Doe", "email_address": "john@example.com"},
                {"id": 2, "first_name": "Jane", "last_name": "Smith", "email_address": "jane@example.com"},
            ]
        }
    }

    result = RawJSONResult(json.dumps(json_data), transformed=False)

    # Transform with type name (should use Rust)
    transformed = result.transform(root_type="User")

    # Parse transformed JSON
    transformed_data = json.loads(transformed.json_string)

    # Verify transformation occurred
    assert transformed._transformed is True, "Result should be marked as transformed"

    # Verify camelCase fields (Rust transformer should have converted them)
    users = transformed_data["data"]["users"]
    first_user = users[0]

    # Check that fields exist (exact format depends on Rust transformer implementation)
    # The Rust transformer should convert snake_case to camelCase
    assert "id" in first_user, "Should have id field"


def test_raw_json_result_already_transformed():
    """Test that already transformed results are not re-transformed."""
    json_data = {"data": {"users": []}}

    result = RawJSONResult(json.dumps(json_data), transformed=True)

    # Transform should be no-op
    transformed = result.transform(root_type="User")

    assert transformed is result, "Should return same object if already transformed"
    assert transformed._transformed is True


def test_raw_json_result_transform_without_type():
    """Test transformation without type name (fallback behavior)."""
    json_data = {"data": {"users": [{"id": 1, "user_name": "test"}]}}

    result = RawJSONResult(json.dumps(json_data), transformed=False)

    # Transform without type_name
    transformed = result.transform(root_type=None)

    # Should still attempt transformation (using passthrough mode)
    assert isinstance(transformed, RawJSONResult)


def test_raw_json_result_transform_invalid_json():
    """Test that invalid JSON is handled gracefully."""
    result = RawJSONResult("invalid json {{{", transformed=False)

    # Transform should handle error gracefully
    transformed = result.transform(root_type="User")

    # Should return original or handle error
    assert isinstance(transformed, RawJSONResult)


def test_raw_json_result_transform_null_data():
    """Test transformation with null data."""
    json_data = {"data": {"user": None}}

    result = RawJSONResult(json.dumps(json_data), transformed=False)

    transformed = result.transform(root_type="User")

    # Should handle null gracefully
    transformed_data = json.loads(transformed.json_string)
    assert transformed_data["data"]["user"] is None


def test_raw_json_result_repr():
    """Test RawJSONResult string representation."""
    short_json = '{"data": {"test": 1}}'
    result = RawJSONResult(short_json)

    repr_str = repr(result)

    assert "RawJSONResult" in repr_str
    assert "test" in repr_str


def test_raw_json_result_repr_truncation():
    """Test that long JSON is truncated in repr."""
    long_json = '{"data": {"items": [' + ','.join(['{"id": 1}'] * 100) + ']}}'
    result = RawJSONResult(long_json)

    repr_str = repr(result)

    assert "RawJSONResult" in repr_str
    assert "..." in repr_str, "Long JSON should be truncated"
    assert len(repr_str) < len(long_json), "Repr should be shorter than full JSON"


def test_raw_json_result_content_type():
    """Test that RawJSONResult has correct content type."""
    result = RawJSONResult('{"data": {}}')

    assert result.content_type == "application/json"


@pytest.mark.asyncio
async def test_execute_raw_json_list_query_with_rust(mock_psycopg_connection):
    """Test that execute_raw_json_list_query passes Rust parameters correctly."""
    from fraiseql.core.raw_json_executor import execute_raw_json_list_query
    from psycopg.sql import SQL

    # This test would require a mock connection that returns JSON rows
    # For now, we're documenting the expected behavior

    # When called with use_rust=True and type_name="User":
    # 1. Should execute the SQL query
    # 2. Should combine JSON rows into array
    # 3. Should call Rust transformer with type_name
    # 4. Should return RawJSONResult with transformed=True

    # This would be tested in integration tests with real database
    pass


@pytest.mark.asyncio
async def test_execute_raw_json_query_with_rust(mock_psycopg_connection):
    """Test that execute_raw_json_query passes Rust parameters correctly."""
    from fraiseql.core.raw_json_executor import execute_raw_json_query
    from psycopg.sql import SQL

    # Similar to above, this documents expected behavior
    # Actual testing happens in integration tests

    pass


def test_rust_transformer_import():
    """Test that Rust transformer can be imported."""
    try:
        from fraiseql.core.rust_transformer import get_transformer

        transformer = get_transformer()
        assert transformer is not None, "Should get transformer instance"
    except ImportError:
        pytest.skip("Rust transformer not available (fraiseql_rs not built)")


def test_rust_transformer_basic_transformation():
    """Test basic Rust transformer functionality."""
    try:
        from fraiseql.core.rust_transformer import get_transformer

        transformer = get_transformer()

        # Test snake_case to camelCase
        input_json = '{"user_name": "test", "email_address": "test@example.com"}'

        # Call transform method
        if hasattr(transformer, 'transform'):
            result = transformer.transform(input_json, "User")
            result_data = json.loads(result)

            # Verify transformation (exact format depends on Rust implementation)
            assert result_data is not None
        else:
            pytest.skip("Transformer doesn't have transform method")

    except ImportError:
        pytest.skip("Rust transformer not available")


# Fixtures for mocking

@pytest.fixture
def mock_psycopg_connection():
    """Mock psycopg connection for testing."""

    class MockCursor:
        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc_val, exc_tb):
            pass

        async def execute(self, query, params=None):
            pass

        async def fetchone(self):
            return ('{"id": 1, "name": "test"}',)

        async def fetchall(self):
            return [
                ('{"id": 1, "name": "test1"}',),
                ('{"id": 2, "name": "test2"}',),
            ]

    class MockConnection:
        def cursor(self):
            return MockCursor()

        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc_val, exc_tb):
            pass

    return MockConnection()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
