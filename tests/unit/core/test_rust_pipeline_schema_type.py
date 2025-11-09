"""Tests for RustResponseBytes schema_type tracking and to_json() method.

Phase 3: TDD Cycle 3.1 & 3.2 - Enhanced Type Safety

This test suite verifies that RustResponseBytes can track the GraphQL schema type
for better debugging and provides a to_json() method for testing purposes.
"""

import json

import pytest

from fraiseql.core.rust_pipeline import RustResponseBytes


class TestRustResponseBytesSchemaType:
    """Test schema_type tracking in RustResponseBytes."""

    def test_rustresponsebytes_tracks_schema_type(self) -> None:
        """Test that RustResponseBytes can track the GraphQL schema type.

        🔴 RED Phase: This test should FAIL initially because RustResponseBytes
        doesn't have a schema_type property yet.

        Expected behavior:
        - RustResponseBytes accepts optional schema_type parameter
        - schema_type is accessible via property
        - Useful for debugging what type the response represents
        """
        # Create RustResponseBytes with schema_type
        response_bytes = b'{"data":{"products":[{"id":"1"}]}}'
        rust_response = RustResponseBytes(response_bytes, schema_type="Product")

        # 🎯 ASSERTION: Should have schema_type property
        assert hasattr(rust_response, "schema_type"), (
            "RustResponseBytes should have schema_type property"
        )
        assert rust_response.schema_type == "Product", (
            f"Expected schema_type='Product', got {rust_response.schema_type}"
        )

    def test_rustresponsebytes_schema_type_optional(self) -> None:
        """Test that schema_type is optional (defaults to None).

        This ensures backwards compatibility - existing code that creates
        RustResponseBytes without schema_type should continue to work.
        """
        # Create RustResponseBytes without schema_type
        response_bytes = b'{"data":{"hello":"world"}}'
        rust_response = RustResponseBytes(response_bytes)

        # 🎯 ASSERTION: Should have schema_type property that defaults to None
        assert hasattr(rust_response, "schema_type"), (
            "RustResponseBytes should have schema_type property"
        )
        assert rust_response.schema_type is None, (
            f"Expected schema_type=None, got {rust_response.schema_type}"
        )

    def test_rustresponsebytes_schema_type_immutable(self) -> None:
        """Test that schema_type is read-only after creation.

        This ensures type information cannot be accidentally changed after
        the RustResponseBytes is created.
        """
        # Create RustResponseBytes with schema_type
        response_bytes = b'{"data":{"user":{"id":"1"}}}'
        rust_response = RustResponseBytes(response_bytes, schema_type="User")

        # 🎯 ASSERTION: Should not be able to modify schema_type
        with pytest.raises(AttributeError):
            rust_response.schema_type = "OtherType"


class TestRustResponseBytesToJson:
    """Test to_json() method for testing purposes."""

    def test_rustresponsebytes_to_json_for_testing(self) -> None:
        """Test that RustResponseBytes has to_json() method for testing.

        🔴 RED Phase (Cycle 3.2): This test should FAIL initially because
        RustResponseBytes doesn't have a to_json() method yet.

        Expected behavior:
        - to_json() parses bytes as JSON and returns dict
        - Useful for testing/debugging but NOT for production
        - Should have clear warning about performance in docstring
        """
        # Create RustResponseBytes with valid JSON
        response_bytes = b'{"data":{"products":[{"id":"1","name":"Test"}]}}'
        rust_response = RustResponseBytes(response_bytes)

        # 🎯 ASSERTION: Should have to_json() method
        assert hasattr(rust_response, "to_json"), "RustResponseBytes should have to_json() method"

        # Call to_json()
        json_data = rust_response.to_json()

        # Verify it returns parsed JSON
        assert isinstance(json_data, dict), f"Expected dict from to_json(), got {type(json_data)}"
        assert "data" in json_data, f"Expected 'data' key: {json_data}"
        assert "products" in json_data["data"], f"Expected 'products' field: {json_data}"

    def test_rustresponsebytes_to_json_with_invalid_json(self) -> None:
        """Test that to_json() handles invalid JSON gracefully.

        This ensures the method provides helpful error messages when
        the bytes don't contain valid JSON.
        """
        # Create RustResponseBytes with invalid JSON
        response_bytes = b'{"invalid": json}'
        rust_response = RustResponseBytes(response_bytes)

        # 🎯 ASSERTION: Should raise JSONDecodeError
        with pytest.raises(json.JSONDecodeError):
            rust_response.to_json()

    def test_rustresponsebytes_to_json_idempotent(self) -> None:
        """Test that calling to_json() multiple times returns same result.

        This ensures the method is side-effect free and can be called
        multiple times safely.
        """
        # Create RustResponseBytes
        response_bytes = b'{"data":{"hello":"world"}}'
        rust_response = RustResponseBytes(response_bytes)

        # Call to_json() twice
        result1 = rust_response.to_json()
        result2 = rust_response.to_json()

        # 🎯 ASSERTION: Should return same result both times
        assert result1 == result2, f"to_json() should be idempotent: {result1} != {result2}"
        assert result1 == {"data": {"hello": "world"}}, f"Expected correct parsed JSON: {result1}"

    def test_rustresponsebytes_to_json_does_not_modify_bytes(self) -> None:
        """Test that to_json() doesn't modify the original bytes.

        This ensures calling to_json() for testing doesn't affect the
        actual response bytes that get sent to the client.
        """
        # Create RustResponseBytes
        original_bytes = b'{"data":{"test":"value"}}'
        rust_response = RustResponseBytes(original_bytes)

        # Call to_json()
        _ = rust_response.to_json()

        # 🎯 ASSERTION: Original bytes should be unchanged
        assert bytes(rust_response) == original_bytes, "to_json() should not modify original bytes"
