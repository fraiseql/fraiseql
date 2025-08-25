import pytest

"""Test mutation result mapping in production mode."""

from uuid import UUID

import fraiseql
from fraiseql import failure, success
from fraiseql import input as fraise_input
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.unit
@fraiseql.type
class Location:
    id: UUID
    name: str
    identifier: str
    active: bool = True


@fraise_input
class CreateLocationInput:
    name: str
    identifier: str


@success
class CreateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None


@failure
class CreateLocationError:
    status: str = "error"
    message: str = ""


def test_mutation_result_parsing_with_object_data():
    """Test that object_data is properly mapped to entity field."""
    # Simulate PostgreSQL function result
    db_result = {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created"],
        "status": "success",
        "message": "Location successfully created.",
        "object_data": {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Test Warehouse",
            "identifier": "WH-001",
            "active": True,
        },
        "extra_metadata": {"entity": "location", "trigger": "api_create"},
    }

    # Parse the result
    result = parse_mutation_result(
        db_result, CreateLocationSuccess, CreateLocationError, error_config=None
    )

    # Verify result is success type
    assert isinstance(result, CreateLocationSuccess)
    assert result.status == "success"
    assert result.message == "Location successfully created."

    # Verify location is populated from object_data
    assert result.location is not None
    assert str(result.location.id) == "123e4567-e89b-12d3-a456-426614174000"
    assert result.location.name == "Test Warehouse"
    assert result.location.identifier == "WH-001"
    assert result.location.active is True


def test_mutation_result_with_unset_values():
    """Test that UNSET values in object_data are handled correctly."""
    from fraiseql.types.definitions import UNSET

    # Simulate result with UNSET value
    db_result = {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created"],
        "status": "success",
        "message": "Location created.",
        "object_data": {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Test Location",
            "identifier": "LOC-001",
            "active": UNSET,  # This should be cleaned to None
        },
        "extra_metadata": {"entity": "location"},
    }

    # Parse the result
    result = parse_mutation_result(db_result, CreateLocationSuccess, CreateLocationError)

    # Verify result
    assert isinstance(result, CreateLocationSuccess)
    assert result.location is not None
    assert result.location.name == "Test Location"
    # UNSET should be cleaned to None (not the default value)
    assert result.location.active is None
