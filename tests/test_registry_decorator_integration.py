"""Integration test for the nested array filtering registry decorator API.

This test specifically validates that the @register_nested_array_filter decorator
works correctly and is properly wired to the schema builder.
"""

import uuid
from typing import Optional

import pytest

from fraiseql.fields import fraise_field
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.nested_array_filters import clear_registry, register_nested_array_filter
from fraiseql.types import fraise_type


# Define the DeviceRegistry type (unique name for this test file)
@fraise_type
class DeviceRegistry:
    id: uuid.UUID
    hostname: str
    ip_address: Optional[str] = None
    status: str = "active"
    priority: int = 1


# Define Network type using the registry decorator (not nested_where_type)
@fraise_type(sql_source="v_network", jsonb_column="data")
class NetworkWithRegistry:
    id: uuid.UUID
    name: str
    # This field uses only supports_where_filtering, no nested_where_type
    # The type will come from the registry
    devices: list[DeviceRegistry] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        description="Network devices with registry-based filtering",
    )


@pytest.mark.integration
class TestRegistryDecoratorIntegration:
    """Test that the registry decorator API is properly wired to schema generation."""

    @pytest.fixture(autouse=True)
    def setup_registry(self) -> None:
        """Clear and setup registry before each test."""
        from fraiseql.gql.builders.registry import SchemaRegistry

        # Clear any existing registry state
        clear_registry()
        SchemaRegistry.get_instance().clear()

        # Register the nested array filter for this test
        register_nested_array_filter(NetworkWithRegistry, "devices", DeviceRegistry)
        yield

        # Cleanup after test
        clear_registry()
        SchemaRegistry.get_instance().clear()

    def test_registry_based_schema_generation(self) -> None:
        """Test that schema generation uses the registry when nested_where_type is not set."""
        from fraiseql import query

        @query
        async def network_with_registry(id: uuid.UUID) -> NetworkWithRegistry:
            """Get network by id."""
            return NetworkWithRegistry(id=id, name="Test Network", devices=[])

        @query
        async def networks_with_registry() -> list[NetworkWithRegistry]:
            """Get all networks."""
            return []

        schema = build_fraiseql_schema(query_types=[network_with_registry, networks_with_registry])
        assert schema is not None

        # Verify the schema includes the NetworkWithRegistry type
        network_type = schema.get_type("NetworkWithRegistry")
        assert network_type is not None

        # Verify the devices field exists
        devices_field = network_type.fields.get("devices")
        assert devices_field is not None

        # Verify the where argument exists on the devices field
        where_arg = devices_field.args.get("where")
        assert where_arg is not None, "Registry-based field should have where argument"

        # The where argument should be a DeviceRegistryWhereInput type
        where_type_name = str(where_arg.type)
        assert "DeviceRegistryWhereInput" in where_type_name, (
            f"Expected DeviceRegistryWhereInput, got {where_type_name}"
        )

    def test_registry_where_input_type_generated(self) -> None:
        """Test that DeviceWhereInput is generated via registry lookup."""
        from fraiseql import query

        @query
        async def network_with_registry(id: uuid.UUID) -> NetworkWithRegistry:
            """Get network by id."""
            return NetworkWithRegistry(id=id, name="Test Network", devices=[])

        schema = build_fraiseql_schema(query_types=[network_with_registry])

        # Check if DeviceRegistryWhereInput type exists in schema
        device_where_input = schema.get_type("DeviceRegistryWhereInput")
        assert device_where_input is not None, (
            "DeviceRegistryWhereInput should be generated from registry"
        )

        # Verify it has the expected fields
        expected_fields = ["id", "hostname", "ipAddress", "status", "priority"]
        for field_name in expected_fields:
            field = device_where_input.fields.get(field_name)
            assert field is not None, f"DeviceRegistryWhereInput should have {field_name} field"

    def test_registry_resolver_functionality(self) -> None:
        """Test that the resolver works with registry-based filtering."""
        import asyncio

        from fraiseql.core.nested_field_resolver import (
            create_nested_array_field_resolver_with_where,
        )
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input

        DeviceRegistryWhereInput = create_graphql_where_input(DeviceRegistry)

        # Create test data
        network = NetworkWithRegistry(
            id=uuid.uuid4(),
            name="Registry Network",
            devices=[
                DeviceRegistry(
                    id=uuid.uuid4(),
                    hostname="active-1",
                    status="active",
                    ip_address="192.168.1.1",
                ),
                DeviceRegistry(
                    id=uuid.uuid4(),
                    hostname="active-2",
                    status="active",
                    ip_address=None,
                ),
                DeviceRegistry(
                    id=uuid.uuid4(),
                    hostname="maintenance-1",
                    status="maintenance",
                    ip_address="192.168.1.2",
                ),
            ],
        )

        # Create resolver
        resolver = create_nested_array_field_resolver_with_where("devices", list[DeviceRegistry])

        # Create where filter
        where_filter = DeviceRegistryWhereInput()
        where_filter.status = {"eq": "active"}
        where_filter.ip_address = {"isnull": False}

        # Test the resolver
        result = asyncio.run(resolver(network, None, where=where_filter))

        # Should return only active devices with IP addresses
        assert len(result) == 1
        assert result[0].hostname == "active-1"
        assert result[0].status == "active"
        assert result[0].ip_address is not None

    def test_registry_priority_system(self) -> None:
        """Test that field attributes have priority over registry."""
        from fraiseql import query
        from fraiseql.fields import fraise_field
        from fraiseql.gql.schema_builder import build_fraiseql_schema
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input
        from fraiseql.types import fraise_type

        # Create a custom where input type
        CustomDeviceRegistryWhereInput = create_graphql_where_input(DeviceRegistry)

        # Define a type with explicit where_input_type (should take priority)
        @fraise_type
        class NetworkWithExplicitWhere:
            id: uuid.UUID
            name: str
            devices: list[DeviceRegistry] = fraise_field(
                default_factory=list,
                supports_where_filtering=True,
                where_input_type=CustomDeviceRegistryWhereInput,
            )

        # Also register it (should be ignored due to field.where_input_type)
        register_nested_array_filter(NetworkWithExplicitWhere, "devices", DeviceRegistry)

        @query
        async def network_explicit(id: uuid.UUID) -> NetworkWithExplicitWhere:
            """Get network by id."""
            return NetworkWithExplicitWhere(id=id, name="Test", devices=[])

        # Should not raise an error - field attribute takes priority
        schema = build_fraiseql_schema(query_types=[network_explicit])
        assert schema is not None

        network_type = schema.get_type("NetworkWithExplicitWhere")
        assert network_type is not None

        devices_field = network_type.fields.get("devices")
        assert devices_field is not None

        where_arg = devices_field.args.get("where")
        assert where_arg is not None
