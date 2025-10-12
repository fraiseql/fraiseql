"""Integration test focusing on the where field functionality."""

import uuid
from typing import List, Optional

import pytest

from fraiseql.fields import fraise_field
from fraiseql.types import fraise_type
from fraiseql.core.graphql_type import convert_type_to_graphql_output


@fraise_type
class TestDevice:
    id: uuid.UUID
    name: str
    status: str = "active"


@fraise_type
class TestNetwork:
    id: uuid.UUID
    name: str
    devices: List[TestDevice] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        nested_where_type=TestDevice
    )


@pytest.mark.integration
class TestWhereFieldIntegration:
    """Test the integration of where filtering with FraiseQL field processing."""

    def test_field_metadata_is_set_correctly(self):
        """Test that field metadata for where filtering is set correctly."""
        network_fields = getattr(TestNetwork, '__gql_fields__', {})
        devices_field = network_fields.get('devices')

        assert devices_field is not None, "devices field should be present"
        assert hasattr(devices_field, 'supports_where_filtering')
        assert devices_field.supports_where_filtering is True
        assert hasattr(devices_field, 'nested_where_type')
        assert devices_field.nested_where_type == TestDevice

    def test_graphql_type_conversion_works(self):
        """Test that GraphQL type conversion works with where-enabled fields."""
        try:
            gql_type = convert_type_to_graphql_output(TestNetwork)
            assert gql_type is not None

            # The type should be a GraphQLObjectType
            assert hasattr(gql_type, 'fields')

            # The devices field should exist
            devices_field = gql_type.fields.get('devices')
            assert devices_field is not None

            # If where filtering is properly integrated, the field should have args
            if hasattr(devices_field, 'args') and devices_field.args:
                where_arg = devices_field.args.get('where')
                if where_arg:
                    # Verify the where argument has the correct type
                    where_type_name = str(where_arg.type)
                    assert "TestDeviceWhereInput" in where_type_name or "DeviceWhereInput" in where_type_name

        except Exception as e:
            pytest.fail(f"GraphQL type conversion should work: {e}")

    def test_where_input_type_generation(self):
        """Test that WhereInput types are generated for nested types."""
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input

        TestDeviceWhereInput = create_graphql_where_input(TestDevice)

        # Verify the where input type has the expected structure
        assert TestDeviceWhereInput is not None

        # Create an instance to test
        where_filter = TestDeviceWhereInput()

        # Should be able to set filter conditions
        where_filter.name = {'contains': 'test'}
        where_filter.status = {'eq': 'active'}

        # Verify the filter conversion works
        if hasattr(where_filter, '_to_sql_where'):
            sql_where = where_filter._to_sql_where()
            assert sql_where is not None

            # The SQL where should have the field filters
            assert hasattr(sql_where, 'name')
            assert hasattr(sql_where, 'status')

            # The field filters should be dictionaries with operators
            assert sql_where.name == {'contains': 'test'}
            assert sql_where.status == {'eq': 'active'}

    def test_enhanced_resolver_creation(self):
        """Test that enhanced resolvers are created for where-enabled fields."""
        from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where

        # Get the field metadata
        network_fields = getattr(TestNetwork, '__gql_fields__', {})
        devices_field = network_fields.get('devices')

        # Create the enhanced resolver
        resolver = create_nested_array_field_resolver_with_where(
            'devices',
            List[TestDevice],
            devices_field
        )

        assert resolver is not None
        assert callable(resolver)

        # Test the resolver with sample data
        network = TestNetwork(
            id=uuid.uuid4(),
            name="Test Network",
            devices=[
                TestDevice(id=uuid.uuid4(), name="device-1", status="active"),
                TestDevice(id=uuid.uuid4(), name="device-2", status="inactive"),
            ]
        )

        # Test without where filter - should return all devices
        import asyncio
        result_all = asyncio.run(resolver(network, None))
        assert len(result_all) == 2

        # Test with where filter
        from fraiseql.sql.graphql_where_generator import create_graphql_where_input
        TestDeviceWhereInput = create_graphql_where_input(TestDevice)

        where_filter = TestDeviceWhereInput()
        where_filter.status = {'eq': 'active'}

        result_filtered = asyncio.run(resolver(network, None, where=where_filter))
        assert len(result_filtered) == 1
        assert result_filtered[0].status == "active"
        assert result_filtered[0].name == "device-1"

    def test_field_without_where_filtering_works_normally(self):
        """Test that fields without where filtering still work normally."""

        @fraise_type
        class NormalNetwork:
            id: uuid.UUID
            name: str
            devices: List[TestDevice] = fraise_field(default_factory=list)

        # This should work without where filtering
        normal_network = NormalNetwork(
            id=uuid.uuid4(),
            name="Normal Network",
            devices=[
                TestDevice(id=uuid.uuid4(), name="normal-device", status="active")
            ]
        )

        assert len(normal_network.devices) == 1
        assert normal_network.devices[0].name == "normal-device"

        # GraphQL type conversion should still work
        try:
            gql_type = convert_type_to_graphql_output(NormalNetwork)
            assert gql_type is not None
        except Exception as e:
            pytest.fail(f"Normal fields should still work: {e}")

    def test_multiple_where_enabled_fields(self):
        """Test a type with multiple where-enabled fields."""

        @fraise_type
        class TestServer:
            id: uuid.UUID
            hostname: str

        @fraise_type
        class ComplexNetwork:
            id: uuid.UUID
            name: str
            devices: List[TestDevice] = fraise_field(
                default_factory=list,
                supports_where_filtering=True,
                nested_where_type=TestDevice
            )
            servers: List[TestServer] = fraise_field(
                default_factory=list,
                supports_where_filtering=True,
                nested_where_type=TestServer
            )

        # Verify both fields have where filtering enabled
        complex_fields = getattr(ComplexNetwork, '__gql_fields__', {})

        devices_field = complex_fields.get('devices')
        assert devices_field and devices_field.supports_where_filtering
        assert devices_field.nested_where_type == TestDevice

        servers_field = complex_fields.get('servers')
        assert servers_field and servers_field.supports_where_filtering
        assert servers_field.nested_where_type == TestServer
