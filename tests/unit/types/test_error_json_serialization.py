"""Test Error type JSON serialization functionality.

This module tests the basic JSON serialization capability of the Error type,
which is essential for GraphQL response serialization.
"""

import json
import pytest

from fraiseql.types.errors import Error


class TestErrorJSONSerialization:
    """Test Error type JSON serialization methods."""

    def test_error_basic_json_serialization_fails_without_fix(self):
        """Test that Error objects cannot be JSON serialized by default (RED phase)."""
        error = Error(
            message="Test error",
            code=400,
            identifier="test_error",
            details={"field": "name", "reason": "invalid"},
        )

        # This should fail without the fix
        with pytest.raises(TypeError, match="Object of type Error is not JSON serializable"):
            json.dumps(error)

    def test_error_list_json_serialization_fails_without_fix(self):
        """Test that lists of Error objects cannot be JSON serialized (RED phase)."""
        errors = [
            Error(message="Error 1", code=400, identifier="error_1"),
            Error(message="Error 2", code=500, identifier="error_2", details={"key": "value"}),
        ]

        # This should fail without the fix
        with pytest.raises(TypeError, match="Object of type Error is not JSON serializable"):
            json.dumps(errors)

    def test_error_nested_json_serialization_fails_without_fix(self):
        """Test that nested structures with Error objects cannot be JSON serialized (RED phase)."""
        error = Error(message="Nested error", code=409, identifier="conflict")

        complex_structure = {
            "data": {
                "createUser": {"message": "Validation failed", "errors": [error], "success": False}
            }
        }

        # This should fail without the fix
        with pytest.raises(TypeError, match="Object of type Error is not JSON serializable"):
            json.dumps(complex_structure)

    def test_error_should_be_json_serializable_with_fix(self):
        """Test that Error objects can be JSON serialized after implementing __json__ method (GREEN phase goal)."""
        error = Error(
            message="Test error",
            code=400,
            identifier="test_error",
            details={"field": "name", "reason": "invalid"},
        )

        # Should be able to serialize directly with custom JSONEncoder
        json_result = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )

        # Parse back to verify structure
        parsed = json.loads(json_result)
        assert parsed["message"] == "Test error"
        assert parsed["code"] == 400
        assert parsed["identifier"] == "test_error"
        assert parsed["details"] == {"field": "name", "reason": "invalid"}

    def test_error_to_dict_method(self):
        """Test that Error objects have a to_dict method for conversion (GREEN phase goal)."""
        error = Error(
            message="Dict test",
            code=422,
            identifier="dict_test",
            details={"nested": {"key": "value"}},
        )

        # Should have a to_dict method
        assert hasattr(error, "to_dict"), "Error should have a to_dict method"

        # Should return the correct dictionary
        result_dict = error.to_dict()
        expected = {
            "message": "Dict test",
            "code": 422,
            "identifier": "dict_test",
            "details": {"nested": {"key": "value"}},
        }
        assert result_dict == expected

    def test_error_with_none_details_serialization(self):
        """Test Error serialization when details is None (edge case)."""
        error = Error(
            message="No details error",
            code=500,
            identifier="no_details",
            # details defaults to None
        )

        # Should serialize without issues
        result = error.to_dict()
        expected = {
            "message": "No details error",
            "code": 500,
            "identifier": "no_details",
            "details": None,
        }
        assert result == expected

        # Should be JSON serializable
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed == expected

    def test_error_with_complex_nested_details(self):
        """Test Error with complex nested details structure (edge case)."""
        complex_details = {
            "validation": {
                "fields": [
                    {"field": "email", "errors": ["invalid format"]},
                    {"field": "age", "errors": ["must be positive"]},
                ]
            },
            "metadata": {
                "request_id": "123e4567-e89b-12d3",
                "timestamp": "2025-01-01T00:00:00Z",
                "user_context": {"role": "admin", "permissions": ["read", "write"]},
            },
        }

        error = Error(
            message="Complex validation failed",
            code=422,
            identifier="validation_failed",
            details=complex_details,
        )

        # Should handle complex nested structures
        result = error.to_dict()
        assert result["details"] == complex_details

        # Should be JSON serializable
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["validation"]["fields"][0]["field"] == "email"
        assert parsed["details"]["metadata"]["user_context"]["role"] == "admin"

    def test_error_list_mixed_with_regular_data(self):
        """Test list of Errors mixed with other data types (integration edge case)."""
        errors = [
            Error(message="Error 1", code=400, identifier="error_1"),
            Error(message="Error 2", code=500, identifier="error_2", details={"info": "extra"}),
        ]

        mixed_data = {
            "success": False,
            "errors": errors,
            "metadata": {"timestamp": "2025-01-01T00:00:00Z", "request_id": "test-123"},
            "counts": [1, 2, 3],
        }

        # Should be serializable with custom default handler
        import json

        def error_serializer(obj):
            if hasattr(obj, "__json__"):
                return obj.__json__()
            raise TypeError(f"Object of type {type(obj).__name__} is not JSON serializable")

        json_str = json.dumps(mixed_data, default=error_serializer)
        parsed = json.loads(json_str)

        # Verify structure is preserved
        assert parsed["success"] is False
        assert len(parsed["errors"]) == 2
        assert parsed["errors"][0]["message"] == "Error 1"
        assert parsed["errors"][1]["details"]["info"] == "extra"
        assert parsed["metadata"]["request_id"] == "test-123"
        assert parsed["counts"] == [1, 2, 3]
