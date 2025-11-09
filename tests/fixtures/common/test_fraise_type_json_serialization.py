"""Unit tests for JSON serialization of @fraise_type objects.

This module tests that @fraise_type decorated objects can be properly
serialized to JSON using FraiseQLJSONEncoder and the new GraphQL
execution pipeline cleaning functions.
"""

import json
from dataclasses import dataclass
from enum import Enum
from typing import Any

import pytest

import fraiseql
from fraiseql.fastapi.json_encoder import FraiseQLJSONEncoder
from fraiseql.graphql.execute import _clean_fraise_types


@pytest.mark.unit
class StatusEnum(Enum):
    """Test enum for serialization."""

    ACTIVE = "active"
    INACTIVE = "inactive"


@fraiseql.type
class SimpleError:
    """Simple error type for testing."""

    message: str
    code: int


@fraiseql.type
class ComplexError:
    """Complex error type with optional fields."""

    message: str
    code: int
    identifier: str
    details: dict[str, Any] | None = None
    metadata: list[str] | None = None


@fraiseql.type
class NestedType:
    """Type with nested @fraise_type fields."""

    name: str
    error: SimpleError | None = None
    errors: list[ComplexError] | None = None


@dataclass
class RegularDataclass:
    """Regular dataclass for comparison."""

    name: str
    value: int


class TestFraiseTypeJSONSerialization:
    """Test JSON serialization of @fraise_type objects."""

    def test_simple_fraise_type_with_encoder(self) -> None:
        """Test serialization of simple @fraise_type object with FraiseQLJSONEncoder."""
        error = SimpleError(message="Test error", code=400)

        json_str = json.dumps(error, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        assert parsed == {"message": "Test error", "code": 400}

    def test_complex_fraise_type_with_encoder(self) -> None:
        """Test serialization of complex @fraise_type object with FraiseQLJSONEncoder."""
        error = ComplexError(
            message="Complex error",
            code=422,
            identifier="validation_error",
            details={"field": "email", "value": "invalid"},
            metadata=["tag1", "tag2"],
        )

        json_str = json.dumps(error, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        expected = {
            "message": "Complex error",
            "code": 422,
            "identifier": "validation_error",
            "details": {"field": "email", "value": "invalid"},
            "metadata": ["tag1", "tag2"],
        }
        assert parsed == expected

    def test_fraise_type_with_none_fields(self) -> None:
        """Test serialization with None fields."""
        error = ComplexError(
            message="Simple error", code=400, identifier="simple", details=None, metadata=None
        )

        json_str = json.dumps(error, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        # FraiseQLJSONEncoder only includes non-None fields
        expected = {"message": "Simple error", "code": 400, "identifier": "simple"}
        assert parsed == expected

    def test_nested_fraise_types_with_encoder(self) -> None:
        """Test serialization of nested @fraise_type objects."""
        simple_error = SimpleError(message="Nested error", code=500)
        complex_error = ComplexError(
            message="Complex nested", code=422, identifier="nested", details={"nested": True}
        )

        nested = NestedType(name="Test nested", error=simple_error, errors=[complex_error])

        json_str = json.dumps(nested, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        expected = {
            "name": "Test nested",
            "error": {"message": "Nested error", "code": 500},
            "errors": [
                {
                    "message": "Complex nested",
                    "code": 422,
                    "identifier": "nested",
                    "details": {"nested": True},
                }
            ],
        }
        assert parsed == expected

    def test_list_of_fraise_types_with_encoder(self) -> None:
        """Test serialization of list containing @fraise_type objects."""
        errors = [
            SimpleError(message="Error 1", code=400),
            SimpleError(message="Error 2", code=500),
        ]

        data = {"errors": errors}

        json_str = json.dumps(data, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        expected = {
            "errors": [{"message": "Error 1", "code": 400}, {"message": "Error 2", "code": 500}]
        }
        assert parsed == expected

    def test_mixed_types_with_encoder(self) -> None:
        """Test serialization of mixed @fraise_type and regular objects."""
        error = SimpleError(message="Mixed test", code=400)
        dataclass_obj = RegularDataclass(name="test", value=42)

        data = {
            "error": error,
            "dataclass": dataclass_obj,
            "string": "test",
            "number": 123,
            "list": [1, 2, 3],
            "dict": {"key": "value"},
            "status": StatusEnum.ACTIVE,
        }

        json_str = json.dumps(data, cls=FraiseQLJSONEncoder)
        parsed = json.loads(json_str)

        expected = {
            "error": {"message": "Mixed test", "code": 400},
            "dataclass": {"name": "test", "value": 42},
            "string": "test",
            "number": 123,
            "list": [1, 2, 3],
            "dict": {"key": "value"},
            "status": "active",  # Enum value
        }
        assert parsed == expected

    def test_fraise_type_fails_with_standard_json(self) -> None:
        """Test that @fraise_type objects fail with standard JSON encoder."""
        error = SimpleError(message="Standard JSON test", code=400)

        with pytest.raises(TypeError, match="Object of type SimpleError is not JSON serializable"):
            json.dumps(error)

        # Same for complex structures
        data = {"error": error}
        with pytest.raises(TypeError, match="Object of type SimpleError is not JSON serializable"):
            json.dumps(data)

    def test_clean_fraise_types_function(self) -> None:
        """Test the _clean_fraise_types function directly."""
        error = ComplexError(
            message="Clean test", code=422, identifier="clean", details={"test": True}
        )

        cleaned = _clean_fraise_types(error)

        # Should be converted to dict
        assert isinstance(cleaned, dict)
        assert cleaned["message"] == "Clean test"
        assert cleaned["code"] == 422
        assert cleaned["identifier"] == "clean"
        assert cleaned["details"] == {"test": True}

    def test_clean_fraise_types_preserves_structure(self) -> None:
        """Test that _clean_fraise_types preserves complex data structures."""
        error1 = SimpleError(message="Error 1", code=400)
        error2 = SimpleError(message="Error 2", code=500)

        data = {
            "level1": {
                "level2": [{"error": error1, "id": 1}, {"error": error2, "id": 2}],
                "metadata": {"count": 2},
            },
            "simple": "value",
        }

        cleaned = _clean_fraise_types(data)

        # Structure should be preserved
        assert cleaned["simple"] == "value"
        assert cleaned["level1"]["metadata"] == {"count": 2}
        assert len(cleaned["level1"]["level2"]) == 2

        # @fraise_type objects should be cleaned
        assert isinstance(cleaned["level1"]["level2"][0]["error"], dict)
        assert cleaned["level1"]["level2"][0]["error"]["message"] == "Error 1"
        assert cleaned["level1"]["level2"][1]["error"]["message"] == "Error 2"

        # Non-@fraise_type data preserved
        assert cleaned["level1"]["level2"][0]["id"] == 1

    def test_clean_fraise_types_handles_empty_structures(self) -> None:
        """Test that _clean_fraise_types handles empty/None structures."""
        assert _clean_fraise_types(None) is None
        assert _clean_fraise_types([]) == []
        assert _clean_fraise_types({}) == {}
        assert _clean_fraise_types("string") == "string"
        assert _clean_fraise_types(42) == 42
        assert _clean_fraise_types(True) is True

    def test_clean_fraise_types_with_circular_references(self) -> None:
        """Test that _clean_fraise_types handles structures safely."""
        # Create a structure that could be problematic
        error = SimpleError(message="Circular test", code=400)
        data = {"error": error}

        # Create a less problematic circular structure
        nested = {"parent": data}
        data["nested"] = nested

        # Should not cause infinite recursion
        # Note: Our implementation should handle this gracefully
        try:
            cleaned = _clean_fraise_types(data)
            # The error should be cleaned
            assert isinstance(cleaned["error"], dict)
            assert cleaned["error"]["message"] == "Circular test"
            # The circular reference should be handled
            assert "nested" in cleaned
        except RecursionError:
            pytest.fail("_clean_fraise_types should handle circular references")

    def test_performance_with_many_objects(self) -> None:
        """Test performance with many @fraise_type objects."""
        import time

        # Create many error objects
        errors = [SimpleError(message=f"Error {i}", code=400 + (i % 100)) for i in range(1000)]

        data = {"errors": errors}

        # Time the cleaning
        start_time = time.time()
        cleaned = _clean_fraise_types(data)
        end_time = time.time()

        # Should complete quickly (< 5.0 seconds for 1000 objects in CI environment)
        # Increased threshold to account for slower CI machines
        duration = end_time - start_time
        assert duration < 5.0, f"Cleaning 1000 objects took {duration} seconds"

        # Verify correctness
        assert len(cleaned["errors"]) == 1000
        assert all(isinstance(e, dict) for e in cleaned["errors"])
        assert cleaned["errors"][0]["message"] == "Error 0"
        assert cleaned["errors"][999]["message"] == "Error 999"

    def test_json_serializable_after_cleaning(self) -> None:
        """Test that cleaned objects are always JSON serializable."""
        # Create complex nested structure
        errors = [
            ComplexError(
                message=f"Batch error {i}",
                code=400 + i,
                identifier=f"batch_{i}",
                details={f"field_{j}": f"value_{j}" for j in range(5)},
                metadata=[f"tag_{i}_{j}" for j in range(3)],
            )
            for i in range(10)
        ]

        nested = NestedType(
            name="Complex nested test",
            error=SimpleError(message="Main error", code=500),
            errors=errors,
        )

        data = {
            "result": nested,
            "metadata": {"timestamp": "2024-01-01T00:00:00Z", "count": len(errors)},
            "status": StatusEnum.ACTIVE,
        }

        # Should fail with standard JSON
        with pytest.raises(TypeError):
            json.dumps(data)

        # Should work after cleaning
        cleaned = _clean_fraise_types(data)
        json_str = json.dumps(cleaned)

        # Verify we can parse it back
        parsed = json.loads(json_str)
        assert parsed["result"]["name"] == "Complex nested test"
        assert parsed["result"]["error"]["message"] == "Main error"
        assert len(parsed["result"]["errors"]) == 10
        assert parsed["result"]["errors"][0]["message"] == "Batch error 0"
        assert parsed["metadata"]["count"] == 10
        assert parsed["status"] == "active"

    def test_encoder_handles_fraiseql_definition_attribute(self) -> None:
        """Test that the encoder correctly identifies @fraise_type objects."""
        error = SimpleError(message="Attribute test", code=400)

        # Should have the FraiseQL definition attribute
        assert hasattr(error, "__fraiseql_definition__")

        # Encoder should recognize it
        encoder = FraiseQLJSONEncoder()
        result = encoder.default(error)

        assert isinstance(result, dict)
        assert result["message"] == "Attribute test"
        assert result["code"] == 400

    def test_complex_graphql_response_structure(self) -> None:
        """Test cleaning of a structure that mirrors a GraphQL response."""
        # Simulate a GraphQL mutation response with auto-populated errors
        error = ComplexError(
            message="Validation failed",
            code=422,
            identifier="validation_error",
            details={"field": "email", "constraint": "format"},
        )

        graphql_response = {
            "data": {
                "createUser": {
                    "__typename": "CreateUserError",
                    "message": "User creation failed",
                    "errors": [error],
                    "conflictingUser": None,
                }
            },
            "extensions": {"tracing": {"duration": 150}},
        }

        # Should fail with standard JSON due to the Error object
        with pytest.raises(TypeError):
            json.dumps(graphql_response)

        # Should work after cleaning
        cleaned = _clean_fraise_types(graphql_response)
        json_str = json.dumps(cleaned)

        # Verify structure is preserved
        parsed = json.loads(json_str)
        assert parsed["data"]["createUser"]["__typename"] == "CreateUserError"
        assert parsed["data"]["createUser"]["message"] == "User creation failed"
        assert len(parsed["data"]["createUser"]["errors"]) == 1

        error_data = parsed["data"]["createUser"]["errors"][0]
        assert error_data["message"] == "Validation failed"
        assert error_data["code"] == 422
        assert error_data["identifier"] == "validation_error"
        assert error_data["details"]["field"] == "email"

        assert parsed["extensions"]["tracing"]["duration"] == 150
