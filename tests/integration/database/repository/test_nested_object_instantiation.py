import pytest

"""Test nested object instantiation from JSONB data."""

import uuid
from typing import Optional

import fraiseql

pytestmark = [pytest.mark.integration, pytest.mark.database]


# Define test types


@pytest.mark.unit
@fraiseql.type
class Gateway:
    """Represents a network gateway device."""

    id: uuid.UUID
    identifier: str
    ip_address: str
    n_total_allocations: Optional[int] = None


@fraiseql.type
class Router:
    """Represents a router device."""

    id: uuid.UUID
    name: str
    firmware_version: str


@fraiseql.type
class NetworkConfiguration:
    """Network configuration with nested objects."""

    id: uuid.UUID
    identifier: str
    ip_address: Optional[str] = None
    subnet_mask: Optional[str] = None
    is_dhcp: Optional[bool] = None

    # Nested objects - should be instantiated from dict
    gateway: Optional[Gateway] = None
    router: Optional[Router] = None


class TestNestedObjectInstantiation:
    """Test that nested objects are properly instantiated from dictionaries."""

    def test_from_dict_with_nested_objects(self) -> None:
        """Test that from_dict properly instantiates nested objects."""
        # Sample data that would come from JSONB
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "identifier": "network-001",
            "ipAddress": "192.168.1.0",
            "subnetMask": "255.255.255.0",
            "isDhcp": True,
            "gateway": {
                "id": "550e8400-e29b-41d4-a716-446655440002",
                "identifier": "gateway-001",
                "ipAddress": "192.168.1.1",
                "nTotalAllocations": 5,
            },
            "router": {
                "id": "550e8400-e29b-41d4-a716-446655440003",
                "name": "Main Router",
                "firmwareVersion": "2.1.0",
            },
        }

        # Create instance using from_dict
        config = NetworkConfiguration.from_dict(data)

        # Debug: verify types
        # config.gateway type and value will be checked by assertions below

        # Basic fields should work (camelCase to snake_case conversion)
        assert str(config.id) == "550e8400-e29b-41d4-a716-446655440001"
        assert config.identifier == "network-001"
        assert config.ip_address == "192.168.1.0"
        assert config.subnet_mask == "255.255.255.0"
        assert config.is_dhcp is True

        # Nested objects should be properly instantiated (not dicts)
        assert isinstance(config.gateway, Gateway)
        assert config.gateway.id == uuid.UUID("550e8400-e29b-41d4-a716-446655440002")
        assert config.gateway.identifier == "gateway-001"
        assert config.gateway.ip_address == "192.168.1.1"
        assert config.gateway.n_total_allocations == 5

        assert isinstance(config.router, Router)
        assert config.router.id == uuid.UUID("550e8400-e29b-41d4-a716-446655440003")
        assert config.router.name == "Main Router"
        assert config.router.firmware_version == "2.1.0"

    def test_from_dict_with_null_nested_objects(self) -> None:
        """Test that from_dict handles null nested objects correctly."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "identifier": "network-002",
            "gateway": None,
            "router": None,
        }

        config = NetworkConfiguration.from_dict(data)

        assert config.gateway is None
        assert config.router is None

    def test_from_dict_with_missing_nested_objects(self) -> None:
        """Test that from_dict handles missing nested objects correctly."""
        data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "identifier": "network-003",
            # gateway and router fields are missing entirely
        }

        config = NetworkConfiguration.from_dict(data)

        # Should use default values (None)
        assert config.gateway is None
        assert config.router is None

    def test_nested_lists_of_objects(self) -> None:
        """Test that lists of nested objects are properly instantiated."""

        @fraiseql.type
        class Device:
            id: uuid.UUID
            name: str

        @fraiseql.type
        class DeviceGroup:
            id: uuid.UUID
            name: str
            devices: list[Device]

        data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "name": "Group A",
            "devices": [
                {"id": "550e8400-e29b-41d4-a716-446655440002", "name": "Device 1"},
                {"id": "550e8400-e29b-41d4-a716-446655440003", "name": "Device 2"},
            ],
        }

        group = DeviceGroup.from_dict(data)

        assert len(group.devices) == 2
        assert all(isinstance(device, Device) for device in group.devices)
        assert group.devices[0].name == "Device 1"
        assert group.devices[1].name == "Device 2"

    def test_deeply_nested_objects(self) -> None:
        """Test multiple levels of nested objects."""

        @fraiseql.type
        class Address:
            street: str
            city: str

        @fraiseql.type
        class Building:
            id: uuid.UUID
            name: str
            address: Address

        @fraiseql.type
        class Campus:
            id: uuid.UUID
            name: str
            main_building: Building

        data = {
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "name": "Tech Campus",
            "mainBuilding": {
                "id": "550e8400-e29b-41d4-a716-446655440002",
                "name": "Engineering Hall",
                "address": {"street": "123 Tech Ave", "city": "Silicon Valley"},
            },
        }

        campus = Campus.from_dict(data)

        assert isinstance(campus.main_building, Building)
        assert isinstance(campus.main_building.address, Address)
        assert campus.main_building.address.street == "123 Tech Ave"
        assert campus.main_building.address.city == "Silicon Valley"
