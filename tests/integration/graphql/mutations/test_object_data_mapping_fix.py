import pytest

"""Test fix for object_data mapping issue in v0.1.0b6."""

import uuid

import fraiseql
from fraiseql import success
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.unit
@fraiseql.type
class Location:
    """Test location type."""

    id: uuid.UUID
    name: str
    identifier: str


@success
class CreateLocationSuccess:
    """Success response with both message and status fields."""

    message: str
    status: str  # This field was causing the mapping to fail
    location: Location | None = None


@success
class UpdateLocationSuccess:
    """Success response with only message field."""

    message: str
    location: Location | None = None


@success
class LocationSuccessWithEntity:
    """Success response using entity metadata hint."""

    message: str
    status: str
    loc: Location | None = None  # Different field name


class TestObjectDataMapping:
    """Test object_data mapping from mutation results."""

    def test_success_with_status_field(self) -> None:
        """Test that object_data is mapped even when status field is present."""
        # Simulate a mutation result from PostgreSQL
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "updated_fields": ["created"],
            "status": "new",
            "message": "Location successfully created.",
            "object_data": {
                "id": "550e8400-e29b-41d4-a716-446655440001",
                "name": "Main Office",
                "identifier": "main-office",
            },
            "extra_metadata": {"entity": "location"},
        }

        # Parse the result
        parsed = parse_mutation_result(
            result_data,
            CreateLocationSuccess,
            Exception,  # Error class not used in this test
        )

        # Verify the result
        assert isinstance(parsed, CreateLocationSuccess)
        assert parsed.message == "Location successfully created."
        assert parsed.status == "new"
        assert parsed.location is not None
        assert str(parsed.location.id) == "550e8400-e29b-41d4-a716-446655440001"
        assert parsed.location.name == "Main Office"
        assert parsed.location.identifier == "main-office"

    def test_success_without_status_field(self) -> None:
        """Test that object_data mapping still works without status field."""
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440002",
            "updated_fields": ["updated"],
            "status": "success",
            "message": "Location successfully updated.",
            "object_data": {
                "id": "550e8400-e29b-41d4-a716-446655440002",
                "name": "Branch Office",
                "identifier": "branch-office",
            },
            "extra_metadata": {"entity": "location"},
        }

        parsed = parse_mutation_result(result_data, UpdateLocationSuccess, Exception)

        assert isinstance(parsed, UpdateLocationSuccess)
        assert parsed.message == "Location successfully updated."
        assert parsed.location is not None
        assert parsed.location.name == "Branch Office"

    def test_entity_metadata_hint(self) -> None:
        """Test that entity metadata hint works for non-standard field names."""
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440003",
            "updated_fields": ["created"],
            "status": "new",
            "message": "Location created.",
            "object_data": {
                "id": "550e8400-e29b-41d4-a716-446655440003",
                "name": "Remote Office",
                "identifier": "remote-office",
            },
            "extra_metadata": {"entity": "loc"},  # Hints to use 'loc' field
        }

        parsed = parse_mutation_result(result_data, LocationSuccessWithEntity, Exception)

        assert isinstance(parsed, LocationSuccessWithEntity)
        assert parsed.message == "Location created."
        assert parsed.status == "new"
        assert parsed.loc is not None
        assert parsed.loc.name == "Remote Office"

    def test_direct_field_in_object_data(self) -> None:
        """Test direct field mapping from object_data."""
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440004",
            "updated_fields": ["created"],
            "status": "new",
            "message": "Location created.",
            "object_data": {
                "location": {  # Direct field name match
                    "id": "550e8400-e29b-41d4-a716-446655440004",
                    "name": "Warehouse",
                    "identifier": "warehouse",
                }
            },
            "extra_metadata": {},
        }

        parsed = parse_mutation_result(result_data, CreateLocationSuccess, Exception)

        assert isinstance(parsed, CreateLocationSuccess)
        assert parsed.location is not None
        assert parsed.location.name == "Warehouse"

    def test_null_object_data(self) -> None:
        """Test handling of null object_data."""
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440005",
            "updated_fields": [],
            "status": "success",
            "message": "Operation completed.",
            "object_data": None,
            "extra_metadata": {},
        }

        parsed = parse_mutation_result(result_data, CreateLocationSuccess, Exception)

        assert isinstance(parsed, CreateLocationSuccess)
        assert parsed.message == "Operation completed."
        assert parsed.status == "success"
        assert parsed.location is None

    def test_complex_object_data(self) -> None:
        """Test with nested object_data structure."""
        result_data = {
            "id": "550e8400-e29b-41d4-a716-446655440006",
            "updated_fields": ["created"],
            "status": "new",
            "message": "Created successfully.",
            "object_data": {
                "id": "550e8400-e29b-41d4-a716-446655440006",
                "name": "Complex Location",
                "identifier": "complex-location",
                # Note: Extra fields would cause instantiation errors
                # so we don't include them in this test
            },
            "extra_metadata": {"entity": "location"},
        }

        parsed = parse_mutation_result(result_data, CreateLocationSuccess, Exception)

        assert isinstance(parsed, CreateLocationSuccess)
        assert parsed.location is not None
        assert str(parsed.location.id) == "550e8400-e29b-41d4-a716-446655440006"
        assert parsed.location.name == "Complex Location"
