"""Phase 1: RED - Failing tests that document conflict auto-population issues."""

import pytest
import fraiseql
from fraiseql.mutations.parser import parse_mutation_result, _populate_conflict_fields
from fraiseql.mutations.types import MutationResult
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG


@fraiseql.type
class Location:
    """Test location entity for conflict testing."""
    id: str
    name: str

    @classmethod
    def from_dict(cls, data: dict) -> "Location":
        return cls(**data)


@fraiseql.success
class CreateLocationSuccess:
    """Success type for location creation."""
    location: Location
    message: str = "Location created successfully"


@fraiseql.failure
class CreateLocationError:
    """Error type with conflict_location field."""
    message: str
    code: str
    conflict_location: Location | None = None


class TestConflictAutoPopulationFailures:
    """Tests documenting the current failures in conflict auto-population."""

    def test_conflict_location_is_none_with_snake_case_format(self):
        """RED TEST: Shows that conflict_location = None with current snake_case data format.

        This test demonstrates the issue described in the plan where the
        conflict auto-population feature exists but doesn't work with the
        snake_case format used internally.
        """
        # PostgreSQL function returns snake_case format in extra_metadata.conflict.conflict_object
        result_data = {
            "status": "conflict",
            "message": "Location already exists",
            "object_data": None,
            "extra_metadata": {
                "conflict": {
                    "conflict_object": {  # snake_case format
                        "id": "loc-123",
                        "name": "Existing Location"
                    }
                }
            }
        }

        # Parse using DEFAULT_ERROR_CONFIG (which is what doesn't work)
        parsed_result = parse_mutation_result(
            result_data,
            CreateLocationSuccess,
            CreateLocationError,
            DEFAULT_ERROR_CONFIG
        )

        # This should fail - conflict_location should be None
        assert isinstance(parsed_result, CreateLocationError)
        assert parsed_result.conflict_location is None  # BUG: This should be populated!

    def test_typeerror_missing_message_with_errors_array_format(self):
        """RED TEST: Shows TypeError: missing message with errors array format.

        This test demonstrates the second issue where the errors array format
        causes instantiation failures due to missing required fields.
        """
        # PostgreSQL function returns errors array with camelCase conflictObject
        result_data = {
            "status": "conflict",
            "message": "Location already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [{
                    "details": {
                        "conflict": {
                            "conflictObject": {  # camelCase format
                                "id": "loc-456",
                                "name": "Another Existing Location"
                            }
                        }
                    }
                    # Note: Missing "message" field that might be required for Error objects
                }]
            }
        }

        # This should fail with TypeError about missing message
        with pytest.raises((TypeError, AttributeError)):
            parse_mutation_result(
                result_data,
                CreateLocationSuccess,
                CreateLocationError,
                DEFAULT_ERROR_CONFIG
            )

    def test_integration_parse_error_populate_conflict_does_not_work(self):
        """RED TEST: Shows that _parse_error calls _populate_conflict_fields but it fails.

        This test demonstrates the integration issue where the data structure
        expected by _populate_conflict_fields doesn't match what _parse_error provides.
        """
        # Test the exact data structure that _parse_error would pass to _populate_conflict_fields
        mutation_result = MutationResult(
            status="conflict",
            message="Location already exists",
            object_data=None,
            extra_metadata={
                "conflict": {
                    "conflict_object": {  # snake_case - won't work with current implementation
                        "id": "loc-789",
                        "name": "Snake Case Location"
                    }
                }
            }
        )

        annotations = {
            "message": str,
            "code": str,
            "conflict_location": Location | None,
        }

        fields = {
            "message": "Location already exists",
            "code": "conflict"
        }

        # Call _populate_conflict_fields directly
        _populate_conflict_fields(mutation_result, annotations, fields)

        # This should fail - conflict_location should not be populated
        # because _populate_conflict_fields looks for errors.details.conflict.conflictObject
        # but we have conflict.conflict_object
        assert "conflict_location" not in fields or fields["conflict_location"] is None

    def test_both_formats_need_support_for_backward_compatibility(self):
        """RED TEST: Shows that we need to support both snake_case and camelCase formats.

        This test documents that real applications use both formats and we need
        backward compatibility.
        """
        # Test snake_case format (internal)
        snake_case_result = MutationResult(
            status="conflict",
            extra_metadata={
                "conflict": {
                    "conflict_object": {
                        "id": "snake-123",
                        "name": "Snake Case Entity"
                    }
                }
            }
        )

        # Test camelCase format (API/frontend)
        camel_case_result = MutationResult(
            status="conflict",
            extra_metadata={
                "errors": [{
                    "details": {
                        "conflict": {
                            "conflictObject": {
                                "id": "camel-456",
                                "name": "Camel Case Entity"
                            }
                        }
                    }
                }]
            }
        )

        annotations = {"conflict_location": Location | None}

        # Neither format currently works
        snake_fields = {}
        _populate_conflict_fields(snake_case_result, annotations, snake_fields)
        assert "conflict_location" not in snake_fields  # SHOULD work but doesn't

        camel_fields = {}
        _populate_conflict_fields(camel_case_result, annotations, camel_fields)
        assert "conflict_location" in camel_fields  # This works (current implementation)

    def test_default_error_config_integration_failure(self):
        """RED TEST: Shows that DEFAULT_ERROR_CONFIG doesn't work without configuration.

        This demonstrates that the PrintOptim backend needs to remove conditional tests
        because the framework should handle conflict auto-population automatically.
        """
        # This is the exact scenario that should work out of the box
        result_data = {
            "status": "conflict",
            "message": "Entity already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [{
                    "details": {
                        "conflict": {
                            "conflictObject": {
                                "id": "default-config-test",
                                "name": "Default Config Location"
                            }
                        }
                    }
                }]
            }
        }

        # Using DEFAULT_ERROR_CONFIG should just work
        result = parse_mutation_result(
            result_data,
            CreateLocationSuccess,
            CreateLocationError,
            DEFAULT_ERROR_CONFIG  # This is the configuration that should work automatically
        )

        # This currently fails because of missing message or other fields
        assert isinstance(result, CreateLocationError)
        # BUG: Either conflict_location is None or we get TypeError during instantiation
        if result.conflict_location is not None:
            assert result.conflict_location.id == "default-config-test"
        else:
            pytest.fail("conflict_location should be auto-populated but is None")
