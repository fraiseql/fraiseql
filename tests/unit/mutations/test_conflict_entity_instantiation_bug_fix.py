"""Test for conflict entity instantiation bug fix - TDD approach.

This test reproduces the bug described in the ticket where DEFAULT_ERROR_CONFIG
is not automatically instantiating conflict entities from errors.details.conflict.conflictObject
into the conflict_* fields of error classes.

Bug Report: FraiseQL v0.7.10 Bug Report: Conflict Entity Instantiation Failure
"""

import uuid
import pytest
import fraiseql
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


@pytest.mark.unit
@fraiseql.type
class Location:
    """Location entity for testing conflict resolution."""
    id: str
    name: str
    identifier: str

    @classmethod
    def from_dict(cls, data: dict) -> "Location":
        """Convert dict data to Location object."""
        return cls(**data)


@fraiseql.success
class CreateLocationSuccess:
    """Success response for location creation."""
    location: Location
    message: str


@fraiseql.failure
class CreateLocationError:
    """Error response for location creation with conflict entity field."""
    message: str
    conflict_location: Location | None = None
    errors: list[dict] | None = None


class TestConflictEntityInstantiationBugFix:
    """Test cases for the conflict entity instantiation bug fix."""

    def test_conflict_entity_instantiation_from_error_details_fails_initially(self):
        """🔴 RED: Test that reproduces the bug - conflict_location should be None initially.

        This test simulates the exact scenario described in the bug ticket where:
        1. PostgreSQL function returns conflict data in errors.details.conflict.conflictObject
        2. The conflict_location field should be populated but currently returns None
        3. This is the failing test that we need to make pass
        """
        # Simulate the exact response structure from the bug ticket
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "A location with this name already exists in this organization",
            "object_data": None,
            "extra_metadata": {},
            # This is where the bug manifests - conflict data is in errors.details but not extracted
            "errors": [{
                "details": {
                    "conflict": {
                        "conflictObject": {
                            "id": "01411222-4111-0000-1000-000000000002",
                            "name": "21411-1 child",
                            "identifier": "test_create_location_deduplication.child"
                        }
                    }
                }
            }]
        }

        # Parse using DEFAULT_ERROR_CONFIG (as mentioned in the bug ticket)
        result = parse_mutation_result(
            mutation_result,
            CreateLocationSuccess,
            CreateLocationError,
            DEFAULT_ERROR_CONFIG
        )

        # Verify it parsed as an error
        assert isinstance(result, CreateLocationError)
        assert result.message == "A location with this name already exists in this organization"

        # 🔴 THIS IS THE FAILING ASSERTION - should be None initially due to the bug
        # After fix, this should contain the instantiated Location object
        print(f"conflict_location value: {result.conflict_location}")
        assert result.conflict_location is None  # This represents the current bug state

        # But the conflict data should be available in errors
        assert result.errors is not None
        # In the current implementation, the conflict data might be in errors details
        if result.errors:
            error_details = result.errors[0].get("details", {})
            conflict_data = error_details.get("conflict", {}).get("conflictObject")
            if conflict_data:
                # The data is available but not instantiated in conflict_location field
                assert conflict_data["id"] == "01411222-4111-0000-1000-000000000002"
                assert conflict_data["name"] == "21411-1 child"


    def test_conflict_entity_instantiation_should_work_when_fixed(self):
        """🟢 GREEN: Test that defines the expected behavior after the fix.

        This test will initially fail but should pass once we implement the fix.
        It tests that conflict_location is properly instantiated from
        errors.details.conflict.conflictObject data.
        """
        # Same mutation result as above
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "A location with this name already exists in this organization",
            "object_data": None,
            "extra_metadata": {},
            "errors": [{
                "details": {
                    "conflict": {
                        "conflictObject": {
                            "id": "01411222-4111-0000-1000-000000000002",
                            "name": "21411-1 child",
                            "identifier": "test_create_location_deduplication.child"
                        }
                    }
                }
            }]
        }

        result = parse_mutation_result(
            mutation_result,
            CreateLocationSuccess,
            CreateLocationError,
            DEFAULT_ERROR_CONFIG
        )

        # After the fix, this should work
        assert isinstance(result, CreateLocationError)
        assert result.message == "A location with this name already exists in this organization"

        # 🟢 THIS IS WHAT WE WANT TO ACHIEVE - properly instantiated conflict entity
        # This will fail initially but should pass after implementing the fix
        assert result.conflict_location is not None
        assert isinstance(result.conflict_location, Location)
        assert result.conflict_location.id == "01411222-4111-0000-1000-000000000002"
        assert result.conflict_location.name == "21411-1 child"
        assert result.conflict_location.identifier == "test_create_location_deduplication.child"

    def test_conflict_entity_manual_instantiation_works(self):
        """✅ Control test: Verify that Location.from_dict works correctly.

        This test ensures that the entity's from_dict method works as expected,
        so we know the problem is in the parser, not in the entity itself.
        """
        # Test data from the bug ticket
        conflict_data = {
            "id": "01411222-4111-0000-1000-000000000002",
            "name": "21411-1 child",
            "identifier": "test_create_location_deduplication.child"
        }

        # This should work fine
        location = Location.from_dict(conflict_data)

        assert location.id == "01411222-4111-0000-1000-000000000002"
        assert location.name == "21411-1 child"
        assert location.identifier == "test_create_location_deduplication.child"

    def test_multiple_conflict_entity_types(self):
        """Test that the fix works for different entity types as mentioned in the bug ticket."""

        @fraiseql.type
        class DnsServer:
            id: str
            name: str
            ip_address: str

            @classmethod
            def from_dict(cls, data: dict) -> "DnsServer":
                return cls(**data)

        @fraiseql.failure
        class CreateDnsServerError:
            message: str
            conflict_dns_server: DnsServer | None = None
            errors: list[dict] | None = None

        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "DNS server already exists",
            "object_data": None,
            "extra_metadata": {},
            "errors": [{
                "details": {
                    "conflict": {
                        "conflictObject": {
                            "id": "dns-server-123",
                            "name": "Primary DNS",
                            "ip_address": "8.8.8.8"
                        }
                    }
                }
            }]
        }

        result = parse_mutation_result(
            mutation_result,
            CreateLocationSuccess,  # Reusing success type for simplicity
            CreateDnsServerError,
            DEFAULT_ERROR_CONFIG
        )

        # After fix: conflict_dns_server should be populated
        assert isinstance(result, CreateDnsServerError)
        # This will initially fail but should pass after the fix
        assert result.conflict_dns_server is not None
        assert result.conflict_dns_server.name == "Primary DNS"
        assert result.conflict_dns_server.ip_address == "8.8.8.8"
