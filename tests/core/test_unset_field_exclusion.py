"""Test that UNSET fields are excluded from mutation input dictionaries."""

import pytest

import fraiseql
from fraiseql import fraise_input
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.gql.builders import SchemaRegistry
from fraiseql.mutations.mutation_decorator import _to_dict
from fraiseql.types.definitions import UNSET


@fraise_input
class UpdateRouterInput:
    """Input for updating a router with optional fields."""

    id: str
    hostname: str | None = UNSET
    ip_address: str | None = UNSET
    mac_address: str | None = UNSET
    location: str | None = UNSET


@fraiseql.type
class Router:
    """Router type."""

    id: str
    hostname: str
    ip_address: str
    mac_address: str
    location: str | None


class TestUnsetFieldExclusion:
    """Test that UNSET fields are properly excluded from mutation input."""

    def setup_method(self):
        """Clear registry before each test."""
        SchemaRegistry._instance = None
        SchemaConfig._instance = None

    def test_unset_fields_are_excluded(self):
        """Test that fields with UNSET values are not included in the dictionary."""
        # Create input with only id and ip_address provided
        input_obj = UpdateRouterInput(
            id="router-123",
            ip_address="192.168.1.100",
            # hostname, mac_address, and location default to UNSET
        )

        # Convert to dictionary
        result = _to_dict(input_obj)

        # Only id and ip_address should be present
        assert result == {"id": "router-123", "ip_address": "192.168.1.100"}

        # UNSET fields should NOT be in the dictionary
        assert "hostname" not in result
        assert "mac_address" not in result
        assert "location" not in result

    def test_explicit_none_is_included(self):
        """Test that explicitly setting a field to None includes it in the dictionary."""
        # Create input with explicit None values
        input_obj = UpdateRouterInput(
            id="router-123",
            hostname=None,  # Explicitly set to None
            ip_address="192.168.1.100",
            # mac_address and location default to UNSET
        )

        # Convert to dictionary
        result = _to_dict(input_obj)

        # id, hostname (as None), and ip_address should be present
        assert result == {
            "id": "router-123",
            "hostname": None,  # Explicitly set to None, so it's included
            "ip_address": "192.168.1.100",
        }

        # UNSET fields should NOT be in the dictionary
        assert "mac_address" not in result
        assert "location" not in result

    def test_all_fields_provided(self):
        """Test when all fields are explicitly provided."""
        # Create input with all fields
        input_obj = UpdateRouterInput(
            id="router-123",
            hostname="router.local",
            ip_address="192.168.1.100",
            mac_address="AA:BB:CC:DD:EE:FF",
            location="Server Room A",
        )

        # Convert to dictionary
        result = _to_dict(input_obj)

        # All fields should be present
        assert result == {
            "id": "router-123",
            "hostname": "router.local",
            "ip_address": "192.168.1.100",
            "mac_address": "AA:BB:CC:DD:EE:FF",
            "location": "Server Room A",
        }

    def test_partial_update_scenario(self):
        """Test a realistic partial update scenario."""
        # Simulate updating only the IP address
        input_obj = UpdateRouterInput(
            id="router-123",
            ip_address="10.0.0.100",
            # All other fields remain UNSET
        )

        # Convert to dictionary
        result = _to_dict(input_obj)

        # Only id and ip_address should be in the JSONB
        assert result == {"id": "router-123", "ip_address": "10.0.0.100"}

        # This dictionary can now be used in PostgreSQL functions
        # to perform partial updates without setting other fields to NULL

    def test_clearing_optional_field(self):
        """Test clearing an optional field by setting it to None."""
        # Clear the location field by explicitly setting to None
        input_obj = UpdateRouterInput(
            id="router-123",
            location=None,  # Explicitly clear the location
            # All other fields remain UNSET
        )

        # Convert to dictionary
        result = _to_dict(input_obj)

        # Only id and location (as None) should be present
        assert result == {"id": "router-123", "location": None}

        # Other fields are not included
        assert "hostname" not in result
        assert "ip_address" not in result
        assert "mac_address" not in result


@pytest.mark.asyncio
async def test_mutation_with_unset_fields():
    """Test that mutations correctly handle UNSET fields in practice."""
    from fraiseql.mutations.parser import parse_mutation_result

    @fraiseql.type
    class UpdateRouterSuccess:
        router: Router
        message: str = "Router updated successfully"

    @fraiseql.type
    class UpdateRouterError:
        message: str
        code: str

    # Simulate a mutation that only updates IP address
    input_obj = UpdateRouterInput(id="router-123", ip_address="172.16.0.100")

    # Convert to dict as the mutation decorator would
    input_dict = _to_dict(input_obj)

    # Verify only provided fields are in the dict
    assert set(input_dict.keys()) == {"id", "ip_address"}

    # This ensures PostgreSQL functions can check:
    # IF input_payload ? 'hostname' -- will be FALSE
    # IF input_payload ? 'ip_address' -- will be TRUE

    # Simulate successful mutation result
    db_result = {
        "id": "router-123",
        "status": "success",
        "message": "Router updated successfully",
        "object_data": {
            "id": "router-123",
            "hostname": "existing-hostname",  # Preserved
            "ip_address": "172.16.0.100",  # Updated
            "mac_address": "AA:BB:CC:DD:EE:FF",  # Preserved
            "location": "Server Room A",  # Preserved
        },
    }

    # Parse the result
    result = parse_mutation_result(db_result, UpdateRouterSuccess, UpdateRouterError)

    # Verify it's a success with the updated router
    assert isinstance(result, UpdateRouterSuccess)
    assert result.router.ip_address == "172.16.0.100"
    assert result.router.hostname == "existing-hostname"  # Not changed!
