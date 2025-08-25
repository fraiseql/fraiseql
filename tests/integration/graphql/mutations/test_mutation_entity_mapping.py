import pytest

"""Test mutation entity field mapping from object_data.

This test suite verifies that FraiseQL properly maps object_data
from PostgreSQL mutation results to entity fields in GraphQL responses.
"""

from uuid import UUID

import fraiseql
from fraiseql import failure, success
from fraiseql.mutations.parser import parse_mutation_result

# Define test entities


@pytest.mark.unit
@fraiseql.type
class Location:
    id: UUID
    name: str
    identifier: str
    active: bool = True


@fraiseql.type
class Machine:
    id: UUID
    name: str
    location_id: UUID | None = None


@fraiseql.type
class User:
    id: UUID
    email: str
    name: str


# Test case 1: Single entity mapping
@success
class CreateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None


@failure
class CreateLocationError:
    status: str = "error"
    message: str = ""


def test_single_entity_mapping_from_object_data():
    """Test mapping entire object_data to a single entity field."""
    # Simulate PostgreSQL function result
    db_result = {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created"],
        "status": "success",
        "message": "Location successfully created.",
        "object_data": {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "name": "Main Warehouse",
            "identifier": "WH-001",
            "active": True,
        },
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, CreateLocationSuccess, CreateLocationError)

    # Verify the location field is populated from object_data
    assert isinstance(result, CreateLocationSuccess)
    assert result.location is not None
    assert str(result.location.id) == "123e4567-e89b-12d3-a456-426614174000"
    assert result.location.name == "Main Warehouse"
    assert result.location.identifier == "WH-001"
    assert result.location.active is True


# Test case 2: Multiple entity mapping
@success
class UpdateLocationSuccess:
    status: str = "success"
    message: str = ""
    location: Location | None = None
    affected_machines: list[Machine] | None = None


def test_multiple_entity_mapping_from_object_data():
    """Test mapping object_data with multiple named entities."""
    db_result = {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["name", "active"],
        "status": "success",
        "message": "Location updated successfully.",
        "object_data": {
            "location": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "name": "Updated Warehouse",
                "identifier": "WH-001",
                "active": False,
            },
            "affected_machines": [
                {
                    "id": "456e4567-e89b-12d3-a456-426614174000",
                    "name": "Machine 1",
                    "location_id": "123e4567-e89b-12d3-a456-426614174000",
                },
                {
                    "id": "789e4567-e89b-12d3-a456-426614174000",
                    "name": "Machine 2",
                    "location_id": "123e4567-e89b-12d3-a456-426614174000",
                },
            ],
        },
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, UpdateLocationSuccess, CreateLocationError)

    # Verify both fields are populated
    assert isinstance(result, UpdateLocationSuccess)
    assert result.location is not None
    assert result.location.name == "Updated Warehouse"
    assert result.location.active is False

    assert result.affected_machines is not None
    assert len(result.affected_machines) == 2
    assert result.affected_machines[0].name == "Machine 1"
    assert result.affected_machines[1].name == "Machine 2"


# Test case 3: Entity hint in metadata
@success
class CreateMachineSuccess:
    status: str = "success"
    message: str = ""
    machine: Machine | None = None


def test_entity_mapping_with_metadata_hint():
    """Test mapping with entity hint in extra_metadata."""
    db_result = {
        "id": "789e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created"],
        "status": "success",
        "message": "Machine created successfully.",
        "object_data": {
            "id": "789e4567-e89b-12d3-a456-426614174000",
            "name": "Printer 1",
            "location_id": "123e4567-e89b-12d3-a456-426614174000",
        },
        "extra_metadata": {"entity": "machine"},  # Hint for field mapping
    }

    result = parse_mutation_result(db_result, CreateMachineSuccess, CreateLocationError)

    assert isinstance(result, CreateMachineSuccess)
    assert result.machine is not None
    assert result.machine.name == "Printer 1"


# Test case 4: Complex success type with multiple optional entities
@success
class ComplexMutationSuccess:
    status: str = "success"
    message: str = ""
    created_user: User | None = None
    updated_location: Location | None = None
    deleted_machines: list[Machine] | None = None


def test_complex_mutation_with_selective_population():
    """Test that only relevant fields are populated from object_data."""
    db_result = {
        "id": "999e4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created_user", "updated_location"],
        "status": "success",
        "message": "Complex operation completed.",
        "object_data": {
            "created_user": {
                "id": "111e4567-e89b-12d3-a456-426614174000",
                "email": "user@example.com",
                "name": "Test User",
            },
            "updated_location": {
                "id": "222e4567-e89b-12d3-a456-426614174000",
                "name": "Updated Location",
                "identifier": "LOC-002",
                "active": True,
            },
            # Note: deleted_machines is not in object_data, should remain None
        },
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, ComplexMutationSuccess, CreateLocationError)

    assert isinstance(result, ComplexMutationSuccess)
    assert result.created_user is not None
    assert result.created_user.email == "user@example.com"

    assert result.updated_location is not None
    assert result.updated_location.name == "Updated Location"

    assert result.deleted_machines is None  # Not populated


# Test case 5: UNSET value handling
def test_entity_mapping_with_unset_values():
    """Test that UNSET values are properly cleaned during mapping."""
    from fraiseql.types.definitions import UNSET

    db_result = {
        "id": "aaa4567-e89b-12d3-a456-426614174000",
        "updated_fields": ["created"],
        "status": "success",
        "message": "Location created.",
        "object_data": {
            "id": "aaa4567-e89b-12d3-a456-426614174000",
            "name": "Test Location",
            "identifier": "LOC-003",
            "active": UNSET,  # Should be cleaned to None
        },
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, CreateLocationSuccess, CreateLocationError)

    assert isinstance(result, CreateLocationSuccess)
    assert result.location is not None
    assert result.location.name == "Test Location"
    assert result.location.active is None  # UNSET cleaned to None


# Test case 6: Empty object_data handling
def test_empty_object_data():
    """Test that empty or null object_data is handled gracefully."""
    db_result = {
        "id": None,
        "updated_fields": [],
        "status": "noop",
        "message": "No operation performed.",
        "object_data": None,
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, CreateLocationSuccess, CreateLocationError)

    # Should still parse successfully, just with None entity
    assert result.location is None
    assert result.message == "No operation performed."


# Test case 7: List entity field
@success
class BulkCreateSuccess:
    status: str = "success"
    message: str = ""
    locations: list[Location] | None = None


def test_list_entity_mapping():
    """Test mapping a list of entities from object_data."""
    db_result = {
        "id": None,
        "updated_fields": ["created"],
        "status": "success",
        "message": "Bulk create completed.",
        "object_data": {
            "locations": [
                {
                    "id": "bbb4567-e89b-12d3-a456-426614174000",
                    "name": "Location 1",
                    "identifier": "LOC-001",
                    "active": True,
                },
                {
                    "id": "ccc4567-e89b-12d3-a456-426614174000",
                    "name": "Location 2",
                    "identifier": "LOC-002",
                    "active": True,
                },
            ]
        },
        "extra_metadata": {},
    }

    result = parse_mutation_result(db_result, BulkCreateSuccess, CreateLocationError)

    assert isinstance(result, BulkCreateSuccess)
    assert result.locations is not None
    assert len(result.locations) == 2
    assert result.locations[0].name == "Location 1"
    assert result.locations[1].name == "Location 2"
