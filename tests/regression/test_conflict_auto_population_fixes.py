"""Phase 2: GREEN - Tests that verify conflict auto-population fixes work correctly."""

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


class TestConflictAutoPopulationFixes:
    """Tests verifying that conflict auto-population fixes work correctly."""

    def test_conflict_location_populated_with_snake_case_format(self):
        """GREEN TEST: Verifies that conflict_location is populated with snake_case format.

        This test verifies that the fix for snake_case format works:
        extra_metadata.conflict.conflict_object -> conflict_location field
        """
        # PostgreSQL function returns snake_case format in extra_metadata.conflict.conflict_object
        result_data = {
            "status": "conflict",
            "message": "Location already exists",
            "object_data": None,
            "extra_metadata": {
                "conflict": {
                    "conflict_object": {  # snake_case format now works!
                        "id": "loc-123",
                        "name": "Existing Location",
                    }
                }
            },
        }

        # Parse using DEFAULT_ERROR_CONFIG (now works!)
        parsed_result = parse_mutation_result(
            result_data, CreateLocationSuccess, CreateLocationError, DEFAULT_ERROR_CONFIG
        )

        # Verify the fix works
        assert isinstance(parsed_result, CreateLocationError)
        assert parsed_result.conflict_location is not None  # FIXED!
        assert parsed_result.conflict_location.id == "loc-123"
        assert parsed_result.conflict_location.name == "Existing Location"

    def test_no_typeerror_with_errors_array_format(self):
        """GREEN TEST: Verifies that errors array format no longer causes TypeError.

        This test verifies that the Error object instantiation fix works by
        providing default values for missing required fields.
        """
        # PostgreSQL function returns errors array with camelCase conflictObject
        result_data = {
            "status": "conflict",
            "message": "Location already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {  # camelCase format
                                    "id": "loc-456",
                                    "name": "Another Existing Location",
                                }
                            }
                        }
                        # Note: Missing "message" field is now handled with defaults
                    }
                ]
            },
        }

        # This should no longer fail with TypeError
        parsed_result = parse_mutation_result(
            result_data, CreateLocationSuccess, CreateLocationError, DEFAULT_ERROR_CONFIG
        )

        # Verify no exception and conflict_location is populated
        assert isinstance(parsed_result, CreateLocationError)
        assert parsed_result.conflict_location is not None
        assert parsed_result.conflict_location.id == "loc-456"
        assert parsed_result.conflict_location.name == "Another Existing Location"

        # Verify errors field is also populated with defaults
        assert parsed_result.errors is not None
        assert len(parsed_result.errors) > 0

    def test_integration_parse_error_populate_conflict_works(self):
        """GREEN TEST: Verifies that _parse_error + _populate_conflict_fields integration works.

        This test verifies that the integration between _parse_error and _populate_conflict_fields
        now works with both data formats.
        """
        # Test the exact data structure that _parse_error would pass to _populate_conflict_fields
        mutation_result = MutationResult(
            status="conflict",
            message="Location already exists",
            object_data=None,
            extra_metadata={
                "conflict": {
                    "conflict_object": {  # snake_case - now works!
                        "id": "loc-789",
                        "name": "Snake Case Location",
                    }
                }
            },
        )

        annotations = {
            "message": str,
            "code": str,
            "conflict_location": Location | None,
        }

        fields = {"message": "Location already exists", "code": "conflict"}

        # Call _populate_conflict_fields directly
        _populate_conflict_fields(mutation_result, annotations, fields)

        # Verify the fix works - conflict_location should now be populated
        assert "conflict_location" in fields
        assert fields["conflict_location"] is not None
        assert fields["conflict_location"].id == "loc-789"
        assert fields["conflict_location"].name == "Snake Case Location"

    def test_both_formats_supported_for_backward_compatibility(self):
        """GREEN TEST: Verifies that both snake_case and camelCase formats work.

        This test verifies that we now support both formats for backward compatibility.
        """
        # Test snake_case format (internal)
        snake_case_result = MutationResult(
            status="conflict",
            extra_metadata={
                "conflict": {"conflict_object": {"id": "snake-123", "name": "Snake Case Entity"}}
            },
        )

        # Test camelCase format (API/frontend)
        camel_case_result = MutationResult(
            status="conflict",
            extra_metadata={
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {"id": "camel-456", "name": "Camel Case Entity"}
                            }
                        }
                    }
                ]
            },
        )

        annotations = {"conflict_location": Location | None}

        # Both formats now work
        snake_fields = {}
        _populate_conflict_fields(snake_case_result, annotations, snake_fields)
        assert "conflict_location" in snake_fields  # NOW works!
        assert snake_fields["conflict_location"].id == "snake-123"

        camel_fields = {}
        _populate_conflict_fields(camel_case_result, annotations, camel_fields)
        assert "conflict_location" in camel_fields  # Still works
        assert camel_fields["conflict_location"].id == "camel-456"

    def test_default_error_config_works_out_of_the_box(self):
        """GREEN TEST: Verifies that DEFAULT_ERROR_CONFIG works without any configuration.

        This test verifies that the PrintOptim backend can now remove conditional tests
        because the framework handles conflict auto-population automatically.
        """
        # This is the exact scenario that should work out of the box
        result_data = {
            "status": "conflict",
            "message": "Entity already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "default-config-test",
                                    "name": "Default Config Location",
                                }
                            }
                        }
                    }
                ]
            },
        }

        # Using DEFAULT_ERROR_CONFIG now just works
        result = parse_mutation_result(
            result_data,
            CreateLocationSuccess,
            CreateLocationError,
            DEFAULT_ERROR_CONFIG,  # This configuration now works automatically
        )

        # Verify everything works perfectly
        assert isinstance(result, CreateLocationError)
        assert result.conflict_location is not None  # Auto-populated!
        assert result.conflict_location.id == "default-config-test"
        assert result.conflict_location.name == "Default Config Location"
        assert result.message == "Entity already exists"
        assert result.code == "conflict"

    def test_multiple_conflict_fields_populated(self):
        """GREEN TEST: Verifies that multiple conflict_* fields can be populated."""
        result_data = {
            "status": "conflict",
            "message": "Multiple conflicts detected",
            "extra_metadata": {
                "conflict": {
                    "conflict_object": {"id": "multi-conflict", "name": "Multi Conflict Location"}
                }
            },
        }

        @fraiseql.failure
        class MultiConflictError:
            message: str
            code: str
            conflict_location: Location | None = None
            conflict_primary: Location | None = None

        result = parse_mutation_result(
            result_data, CreateLocationSuccess, MultiConflictError, DEFAULT_ERROR_CONFIG
        )

        # Both conflict fields should be populated
        assert isinstance(result, MultiConflictError)
        assert result.conflict_location is not None
        assert result.conflict_primary is not None
        assert result.conflict_location.id == "multi-conflict"
        assert result.conflict_primary.id == "multi-conflict"

    def test_graceful_handling_of_malformed_data(self):
        """GREEN TEST: Verifies graceful handling of malformed conflict data."""
        result_data = {
            "status": "conflict",
            "message": "Malformed test",
            "extra_metadata": {"conflict": {"conflict_object": "not-a-dict"}},  # Invalid structure
        }

        # Should not crash, just not populate conflict fields
        result = parse_mutation_result(
            result_data, CreateLocationSuccess, CreateLocationError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, CreateLocationError)
        assert result.conflict_location is None  # Not populated due to malformed data
        assert result.message == "Malformed test"  # Basic fields still work
