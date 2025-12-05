"""Test Error type JSON serialization functionality.

This module tests the basic JSON serialization capability of the Error type,
which is essential for GraphQL response serialization.
"""

import json

import pytest

from fraiseql.types.errors import Error


class TestErrorJSONSerialization:
    """Test Error type JSON serialization methods."""

    def test_error_basic_json_serialization_fails_without_fix(self) -> None:
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

    def test_error_list_json_serialization_fails_without_fix(self) -> None:
        """Test that lists of Error objects cannot be JSON serialized (RED phase)."""
        errors = [
            Error(message="Error 1", code=400, identifier="error_1"),
            Error(message="Error 2", code=500, identifier="error_2", details={"key": "value"}),
        ]

        # This should fail without the fix
        with pytest.raises(TypeError, match="Object of type Error is not JSON serializable"):
            json.dumps(errors)

    def test_error_nested_json_serialization_fails_without_fix(self) -> None:
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

    def test_error_should_be_json_serializable_with_fix(self) -> None:
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

    def test_error_to_dict_method(self) -> None:
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

    def test_error_with_none_details_serialization(self) -> None:
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

    def test_error_with_complex_nested_details(self) -> None:
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

    def test_error_list_mixed_with_regular_data(self) -> None:
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

        def error_serializer(obj) -> None:
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

    def test_error_with_deeply_nested_structures(self) -> None:
        """Test Error with very deeply nested structures (edge case)."""
        # Create a deeply nested structure (10 levels deep)
        nested_details = {"level": 1}
        current = nested_details
        for i in range(2, 11):
            current[f"nested_{i}"] = {"level": i, "data": f"value_{i}"}
            current = current[f"nested_{i}"]

        error = Error(
            message="Deep nesting test",
            code=500,
            identifier="deep_nest",
            details=nested_details,
        )

        # Should handle deep nesting
        result = error.to_dict()
        assert result["details"]["level"] == 1
        assert result["details"]["nested_2"]["level"] == 2
        assert result["details"]["nested_2"]["nested_3"]["level"] == 3

        # Should be JSON serializable
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["level"] == 1

    def test_error_with_empty_nested_structures(self) -> None:
        """Test Error with empty nested objects and arrays."""
        error = Error(
            message="Empty structures",
            code=400,
            identifier="empty_structures",
            details={
                "empty_object": {},
                "empty_array": [],
                "nested_empty": {"another_empty": {}, "empty_list": []},
            },
        )

        result = error.to_dict()
        assert result["details"]["empty_object"] == {}
        assert result["details"]["empty_array"] == []
        assert result["details"]["nested_empty"]["another_empty"] == {}

        # Should be JSON serializable
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["empty_object"] == {}
        assert parsed["details"]["empty_array"] == []

    def test_error_with_mixed_data_types_in_nested(self) -> None:
        """Test Error with mixed data types in nested structures."""
        error = Error(
            message="Mixed types",
            code=422,
            identifier="mixed_types",
            details={
                "numbers": [1, 2.5, -3],
                "booleans": [True, False, None],
                "strings": ["hello", "", "café"],
                "nested_mixed": {
                    "int": 42,
                    "float": 3.14,
                    "bool": True,
                    "null": None,
                    "unicode": "naïve",
                    "array": [1, "two", True, None],
                },
            },
        )

        result = error.to_dict()
        assert result["details"]["numbers"] == [1, 2.5, -3]
        assert result["details"]["booleans"] == [True, False, None]
        assert "café" in result["details"]["strings"]
        assert result["details"]["nested_mixed"]["unicode"] == "naïve"

        # Should be JSON serializable
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["nested_mixed"]["array"] == [1, "two", True, None]

    def test_error_with_very_large_nested_structure(self) -> None:
        """Test Error with very large nested structure."""
        # Create a large nested structure
        large_details = {"root": {}}
        current = large_details["root"]

        # Add 100 nested levels with data
        for i in range(100):
            current[f"level_{i}"] = {"index": i, "data": f"value_{i}", "array": list(range(10))}
            current = current[f"level_{i}"]

        error = Error(
            message="Large structure",
            code=413,  # Payload too large
            identifier="large_structure",
            details=large_details,
        )

        # Should handle large structures (though may be slow)
        result = error.to_dict()
        assert result["details"]["root"]["level_0"]["index"] == 0
        assert result["details"]["root"]["level_0"]["array"] == list(range(10))

        # Should be JSON serializable (may be slow)
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["root"]["level_0"]["index"] == 0

    def test_error_with_unicode_in_deeply_nested(self) -> None:
        """Test Error with unicode characters in deeply nested structures."""
        error = Error(
            message="Unicode nesting",
            code=400,
            identifier="unicode_nesting",
            details={
                "level1": {
                    "café": "coffee",
                    "naïve": "innocent",
                    "level2": {
                        "español": "spanish",
                        "русский": "russian",
                        "level3": {
                            "日本語": "japanese",
                            "emoji": "🚀⭐🌟",
                            "mixed": "café + naïve + русский",
                        },
                    },
                }
            },
        )

        result = error.to_dict()
        assert result["details"]["level1"]["café"] == "coffee"
        assert result["details"]["level1"]["level2"]["русский"] == "russian"
        assert "🚀" in result["details"]["level1"]["level2"]["level3"]["emoji"]

        # Should be JSON serializable with unicode preservation
        import json

        json_str = json.dumps(
            error, default=lambda x: x.__json__() if hasattr(x, "__json__") else str(x)
        )
        parsed = json.loads(json_str)
        assert parsed["details"]["level1"]["level2"]["level3"]["日本語"] == "japanese"

    def test_error_nested_array_of_errors(self) -> None:
        """Test nested arrays containing multiple Error objects."""
        errors = [
            Error(message="Outer error 1", code=400, identifier="outer_1"),
            Error(
                message="Outer error 2",
                code=500,
                identifier="outer_2",
                details={
                    "inner_errors": [
                        Error(message="Inner error 1", code=401, identifier="inner_1"),
                        Error(message="Inner error 2", code=403, identifier="inner_2"),
                    ]
                },
            ),
        ]

        complex_structure = {
            "operation": "batch_process",
            "errors": errors,
            "metadata": {"total_errors": 2, "nested_error_count": 2},
        }

        # Should be serializable with custom default handler
        import json

        def error_serializer(obj) -> None:
            if hasattr(obj, "__json__"):
                return obj.__json__()
            raise TypeError(f"Object of type {type(obj).__name__} is not JSON serializable")

        json_str = json.dumps(complex_structure, default=error_serializer)
        parsed = json.loads(json_str)

        # Verify nested error structure
        assert len(parsed["errors"]) == 2
        assert parsed["errors"][0]["message"] == "Outer error 1"
        assert len(parsed["errors"][1]["details"]["inner_errors"]) == 2
        assert parsed["errors"][1]["details"]["inner_errors"][0]["code"] == 401
