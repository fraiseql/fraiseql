"""Test auto-population of errors field in mutation error types."""

from typing import Any

import pytest

import fraiseql

# Import commented out as it's not needed - fraiseql.type is used via decorator
from fraiseql.mutations.error_config import MutationErrorConfig
from fraiseql.mutations.parser import parse_mutation_result

# Define a simple Error type for testing


@pytest.mark.unit
@fraiseql.type
class Error:
    message: str
    code: int
    identifier: str
    details: dict[str, Any] | None = None


# Define test mutation types
@fraiseql.type
class MutationTestSuccess:
    message: str
    entity: dict[str, Any] | None = None


@fraiseql.type
class MutationTestError:
    message: str
    status: str
    errors: list[Error] | None = None
    conflicting_entity: dict[str, Any] | None = None


class TestMutationErrorAutoPopulation:
    """Test cases for automatic error field population."""

    def test_error_field_auto_populated_when_none(self):
        """Test that errors field is auto-populated when it's None."""
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Entity already exists",
            "object_data": {"id": "existing-id", "name": "Existing Entity"},
            "extra_metadata": {"conflict_id": "existing-id"},
        }

        # Use a config that treats noop:already_exists as an error
        config = MutationErrorConfig(
            success_keywords={"success", "ok"},
            error_prefixes={"noop:already_exists"},
            error_as_data_prefixes=set(),
        )

        parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

        # Should return TestError instance
        assert isinstance(parsed, MutationTestError)
        assert parsed.message == "Entity already exists"
        assert parsed.status == "noop:already_exists"

        # errors field should be auto-populated
        assert parsed.errors is not None
        assert len(parsed.errors) == 1
        assert parsed.errors[0].message == "Entity already exists"
        assert parsed.errors[0].code == 409  # already_exists gets 409 (conflict)
        assert parsed.errors[0].identifier == "already_exists"
        # Check details - they should be camelCase if config.camel_case_fields is True
        from fraiseql.config.schema_config import SchemaConfig

        config_instance = SchemaConfig.get_instance()
        if config_instance.camel_case_fields:
            assert parsed.errors[0].details == {"conflictId": "existing-id"}
        else:
            assert parsed.errors[0].details == {"conflict_id": "existing-id"}

    def test_error_field_not_overwritten_if_provided(self):
        """Test that explicitly provided errors are not overwritten."""
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "failed:validation",
            "message": "Validation failed",
            "object_data": {},
            "extra_metadata": {
                "errors": [
                    {
                        "message": "Field 'name' is required",
                        "code": 400,
                        "identifier": "missing_field",
                        "details": {"field": "name"},
                    }
                ]
            },
        }

        config = MutationErrorConfig(success_keywords={"success"}, error_prefixes={"failed:"})

        parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

        # Should use the provided errors from metadata
        assert isinstance(parsed, MutationTestError)
        assert parsed.errors is not None
        assert len(parsed.errors) == 1
        assert parsed.errors[0].message == "Field 'name' is required"
        assert parsed.errors[0].identifier == "missing_field"

    def test_conflicting_entity_populated_from_object_data(self):
        """Test that other fields like conflicting_entity are populated from object_data."""
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": [],
            "status": "noop:duplicate_name",
            "message": "A location with this name already exists",
            "object_data": {
                "conflicting_entity": {
                    "id": "existing-location-id",
                    "name": "Main Office",
                    "level": "building",
                }
            },
            "extra_metadata": {},
        }

        config = MutationErrorConfig(
            success_keywords={"success"}, error_prefixes={"noop:duplicate"}
        )

        parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

        assert isinstance(parsed, MutationTestError)
        # Check that conflicting_entity was populated from object_data
        assert parsed.conflicting_entity is not None
        assert parsed.conflicting_entity["id"] == "existing-location-id"
        assert parsed.conflicting_entity["name"] == "Main Office"

        # And errors should still be auto-populated
        assert parsed.errors is not None
        assert len(parsed.errors) == 1
        assert parsed.errors[0].identifier == "duplicate_name"

    def test_various_status_codes_mapped_correctly(self):
        """Test that different status patterns map to appropriate error codes."""
        test_cases = [
            ("noop:not_found", 404),
            ("failed:unauthorized", 401),
            ("failed:forbidden", 403),
            ("noop:conflict", 409),
            ("failed:validation_error", 422),
            ("blocked:children", 422),
            ("failed:timeout", 408),
            ("failed:internal_error", 500),
        ]

        for status, expected_code in test_cases:
            result = {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "updated_fields": [],
                "status": status,
                "message": f"Error: {status}",
                "object_data": {},
                "extra_metadata": {},
            }

            # Config that treats everything as error
            config = MutationErrorConfig(
                success_keywords=set(), error_prefixes={"noop:", "failed:", "blocked:"}
            )

            parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

            assert isinstance(parsed, MutationTestError)
            assert parsed.errors is not None
            assert len(parsed.errors) == 1
            assert parsed.errors[0].code == expected_code, (
                f"Status {status} should map to code {expected_code}"
            )

    def test_success_result_not_affected(self):
        """Test that successful results are not affected by error auto-population."""
        result = {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "updated_fields": ["name", "description"],
            "status": "success",
            "message": "Entity created successfully",
            "object_data": {"id": "new-id", "name": "New Entity"},
            "extra_metadata": {},
        }

        config = MutationErrorConfig()  # Default config,

        parsed = parse_mutation_result(result, MutationTestSuccess, MutationTestError, config)

        # Should return TestSuccess instance
        assert isinstance(parsed, MutationTestSuccess)
        assert parsed.message == "Entity created successfully"
        assert parsed.entity == {"id": "new-id", "name": "New Entity"}


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
