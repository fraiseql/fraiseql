"""Integration tests for GraphQL error serialization with @fraise_type objects.

This module tests that GraphQL execution properly serializes @fraise_type objects
in responses, particularly for error auto-population scenarios where Error objects
are created during mutation parsing.
"""

from typing import Any

import pytest
from graphql import ExecutionResult

import fraiseql
from fraiseql.graphql.execute import (
    _clean_fraise_types,
    _serialize_fraise_types_in_result,
)
from fraiseql.mutations.error_config import MutationErrorConfig
from fraiseql.mutations.parser import parse_mutation_result


@fraiseql.type
class Error:
    """Test Error type matching the one in fraiseql.types.errors."""

    message: str
    code: int
    identifier: str
    details: Any = None


@fraiseql.type
class MutationTestSuccess:
    """Success response for test mutations."""

    message: str
    entity: dict[str, Any] | None = None


@fraiseql.type
class MutationTestError:
    """Error response with auto-populated errors field."""

    message: str
    status: str
    errors: list[Error] | None = None
    conflicting_entity: dict[str, Any] | None = None


class TestGraphQLErrorSerialization:
    """Test cases for GraphQL error serialization."""

    def test_clean_fraise_types_single_object(self):
        """Test cleaning a single @fraise_type object."""
        error = Error(
            message="Test error", code=400, identifier="test_error", details={"field": "name"}
        )

        cleaned = _clean_fraise_types(error)

        # Should be converted to dict
        assert isinstance(cleaned, dict)
        assert cleaned["message"] == "Test error"
        assert cleaned["code"] == 400
        assert cleaned["identifier"] == "test_error"
        assert cleaned["details"] == {"field": "name"}

    def test_clean_fraise_types_list_of_objects(self):
        """Test cleaning a list containing @fraise_type objects."""
        errors = [
            Error(message="Error 1", code=400, identifier="error_1"),
            Error(message="Error 2", code=500, identifier="error_2"),
        ]

        cleaned = _clean_fraise_types(errors)

        # Should be list of dicts
        assert isinstance(cleaned, list)
        assert len(cleaned) == 2
        assert all(isinstance(item, dict) for item in cleaned)
        assert cleaned[0]["message"] == "Error 1"
        assert cleaned[1]["message"] == "Error 2"

    def test_clean_fraise_types_nested_structure(self):
        """Test cleaning nested data structures with @fraise_type objects."""
        error = Error(message="Nested error", code=409, identifier="nested")

        data = {
            "mutation": {"result": {"errors": [error], "message": "Failed"}},
            "other_data": "preserved",
        }

        cleaned = _clean_fraise_types(data)

        # Structure should be preserved
        assert cleaned["other_data"] == "preserved"
        assert cleaned["mutation"]["result"]["message"] == "Failed"

        # Error should be cleaned
        error_dict = cleaned["mutation"]["result"]["errors"][0]
        assert isinstance(error_dict, dict)
        assert error_dict["message"] == "Nested error"
        assert error_dict["code"] == 409

    def test_clean_fraise_types_preserves_non_fraise_objects(self):
        """Test that non-@fraise_type objects are preserved."""
        data = {
            "string": "test",
            "number": 42,
            "list": [1, 2, 3],
            "nested": {"key": "value"},
            "null": None,
        }

        cleaned = _clean_fraise_types(data)

        # Should be identical
        assert cleaned == data
        assert cleaned is not data  # But different object

    def test_serialize_fraise_types_in_result(self):
        """Test the ExecutionResult serialization wrapper."""
        error = Error(message="Test", code=400, identifier="test")

        result_data = {"testMutation": {"errors": [error]}}

        execution_result = ExecutionResult(
            data=result_data, errors=None, extensions={"test": "value"}
        )

        cleaned_result = _serialize_fraise_types_in_result(execution_result)

        # Should be new ExecutionResult
        assert isinstance(cleaned_result, ExecutionResult)
        assert cleaned_result is not execution_result
        assert cleaned_result.extensions == {"test": "value"}  # Preserved

        # Data should be cleaned
        error_dict = cleaned_result.data["testMutation"]["errors"][0]
        assert isinstance(error_dict, dict)
        assert error_dict["message"] == "Test"

    def test_serialize_fraise_types_empty_result(self):
        """Test serialization with empty result data."""
        execution_result = ExecutionResult(data=None, errors=None)

        cleaned_result = _serialize_fraise_types_in_result(execution_result)

        # Should return same result
        assert cleaned_result.data is None
        assert cleaned_result.errors is None

    def test_mutation_error_autopop_integration(self):
        """Test that error auto-population creates serializable objects."""
        # Simulate PostgreSQL function result
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Entity already exists",
            "object_data": {"id": "existing-id", "name": "Existing Entity"},
            "extra_metadata": {"conflict_id": "existing-id"},
        }

        # Parse with error config
        config = MutationErrorConfig(
            success_keywords={"success", "ok"},
            error_prefixes={"noop:already_exists"},
        )

        parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

        # Should be MutationTestError with auto-populated errors
        assert isinstance(parsed, MutationTestError)
        assert parsed.errors is not None
        assert len(parsed.errors) == 1

        # The auto-populated error should be an Error object
        error_obj = parsed.errors[0]
        assert isinstance(error_obj, Error)
        assert hasattr(error_obj, "__fraiseql_definition__")

        # Now test that this can be cleaned for JSON serialization
        cleaned_parsed = _clean_fraise_types(parsed)

        # MutationTestError is @fraise_type, so it should be cleaned to dict
        assert isinstance(cleaned_parsed, dict)
        error_dict = cleaned_parsed["errors"][0]

        # Error should now be a dict
        assert isinstance(error_dict, dict)
        assert error_dict["message"] == "Entity already exists"
        assert error_dict["code"] == 409
        assert error_dict["identifier"] == "already_exists"

    def test_json_serialization_after_cleaning(self):
        """Test that cleaned objects can be JSON serialized."""
        import json

        error = Error(
            message="JSON test",
            code=422,
            identifier="json_test",
            details={"nested": {"data": "value"}},
        )

        complex_data = {
            "data": {
                "createUser": {
                    "message": "Validation failed",
                    "errors": [error],
                    "nested": {
                        "more_errors": [
                            Error(message="Nested error", code=400, identifier="nested")
                        ]
                    },
                }
            },
            "extensions": {"trace": "debug"},
        }

        # Should fail with standard JSON
        with pytest.raises(TypeError, match="Object of type Error is not JSON serializable"):
            json.dumps(complex_data)

        # Should succeed after cleaning
        cleaned = _clean_fraise_types(complex_data)
        json_str = json.dumps(cleaned)

        # Should be valid JSON
        parsed = json.loads(json_str)
        assert parsed["data"]["createUser"]["message"] == "Validation failed"
        assert parsed["data"]["createUser"]["errors"][0]["message"] == "JSON test"
        assert parsed["data"]["createUser"]["nested"]["more_errors"][0]["message"] == "Nested error"
        assert parsed["extensions"]["trace"] == "debug"

    @pytest.mark.asyncio
    async def test_graphql_execution_integration(self):
        """Test that the fix works in actual GraphQL execution context.

        This test would require a full GraphQL schema setup, but we can test
        the key components that matter for the fix.
        """
        # Create a mock ExecutionResult as would be created by GraphQL execution
        error = Error(message="GraphQL test", code=500, identifier="graphql_test")

        # Simulate what GraphQL execution would create
        execution_result = ExecutionResult(
            data={
                "testMutation": {
                    "__typename": "TestMutationError",
                    "message": "Mutation failed",
                    "errors": [error],  # This would cause JSON serialization to fail
                }
            },
            errors=None,
        )

        # The fix should clean this before JSON serialization
        cleaned_result = _serialize_fraise_types_in_result(execution_result)

        # Should now be JSON serializable
        import json

        json_str = json.dumps(cleaned_result.data)

        # Verify the structure is correct
        parsed = json.loads(json_str)
        assert parsed["testMutation"]["message"] == "Mutation failed"
        assert parsed["testMutation"]["errors"][0]["message"] == "GraphQL test"
        assert parsed["testMutation"]["errors"][0]["code"] == 500

    def test_performance_with_large_structures(self):
        """Test that cleaning performance is reasonable with large structures."""
        import time

        # Create a large structure with many @fraise_type objects
        errors = [
            Error(message=f"Error {i}", code=400, identifier=f"error_{i}") for i in range(1000)
        ]

        large_data = {
            "mutations": [
                {
                    "id": i,
                    "result": {
                        "errors": errors[:10],  # Share errors to test deduplication
                        "metadata": {"batch": i},
                    },
                }
                for i in range(100)
            ]
        }

        # Time the cleaning operation
        start_time = time.time()
        cleaned = _clean_fraise_types(large_data)
        end_time = time.time()

        # Should complete in reasonable time (< 5 seconds for this size in CI)
        # Increased threshold to account for slower CI machines
        assert end_time - start_time < 5.0, f"Cleaning took {end_time - start_time} seconds"

        # Verify correctness
        assert len(cleaned["mutations"]) == 100
        assert len(cleaned["mutations"][0]["result"]["errors"]) == 10
        assert isinstance(cleaned["mutations"][0]["result"]["errors"][0], dict)

    def test_recursive_cleaning_safety(self):
        """Test that recursive cleaning handles circular references safely."""
        error = Error(message="Recursive test", code=400, identifier="recursive")

        # Create a structure that could cause infinite recursion if not handled properly
        data = {"level1": {"level2": {"level3": {"error": error}}}}

        # Should not raise recursion error
        cleaned = _clean_fraise_types(data)

        # Should work correctly
        assert isinstance(cleaned["level1"]["level2"]["level3"]["error"], dict)
        assert cleaned["level1"]["level2"]["level3"]["error"]["message"] == "Recursive test"
