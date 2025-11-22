"""Test camelCase to snake_case transformation for GraphQL inputs."""

import pytest

import fraiseql
from fraiseql import fraise_input, mutation, query
from fraiseql.config.schema_config import SchemaConfig
from fraiseql.gql.builders import SchemaComposer, SchemaRegistry


@fraise_input
class NetworkInput:
    ip_address: str
    subnet_mask: str
    gateway_address: str | None = None


@fraiseql.type
class Network:
    id: str
    ip_address: str
    subnet_mask: str
    gateway_address: str | None


@query
async def network_query(info, network_config: NetworkInput) -> Network:
    """Test query that accepts camelCase input."""
    return Network(
        id="test-1",
        ip_address=network_config.ip_address,
        subnet_mask=network_config.subnet_mask,
        gateway_address=network_config.gateway_address,
    )


@mutation
async def create_network(info, input: NetworkInput) -> Network:
    """Create network with camelCase input fields."""
    return Network(
        id="new-1",
        ip_address=input.ip_address,
        subnet_mask=input.subnet_mask,
        gateway_address=input.gateway_address,
    )


class TestCamelCaseInputTransformation:
    def setup_method(self) -> None:
        """Clear registry before each test."""
        SchemaRegistry._instance = None
        SchemaConfig._instance = None
        # Also clear the GraphQL type cache
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()

    @pytest.mark.asyncio
    async def test_camelcase_input_fields_are_transformed(self) -> None:
        """Test that camelCase input fields are transformed to snake_case."""
        from graphql import graphql

        # Enable camelCase
        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        # Register types
        registry = SchemaRegistry.get_instance()
        registry.register_type(Network)
        registry.register_type(NetworkInput)
        registry.register_query(network_query)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        # GraphQL query with camelCase input fields
        query_str = """
            query TestNetwork($networkConfig: NetworkInput!) {
                networkQuery(networkConfig: $networkConfig) {
                    id
                    ipAddress
                    subnetMask
                    gatewayAddress
                }
            }
        """
        # Input with camelCase field names
        variables = {
            "networkConfig": {
                "ipAddress": "192.168.1.100",
                "subnetMask": "255.255.255.0",
                "gatewayAddress": "192.168.1.1",
            }
        }

        result = await graphql(
            schema, query_str, variable_values=variables, context_value={"db": None}
        )

        # Should work without errors
        assert result.errors is None or len(result.errors) == 0
        assert result.data is not None
        assert result.data["networkQuery"]["ipAddress"] == "192.168.1.100"
        assert result.data["networkQuery"]["subnetMask"] == "255.255.255.0"
        assert result.data["networkQuery"]["gatewayAddress"] == "192.168.1.1"

    @pytest.mark.asyncio
    async def test_mutation_with_camelcase_input(self) -> None:
        """Test that mutations handle camelCase inputs correctly."""
        from graphql import graphql

        # Enable camelCase
        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        # Register types
        registry = SchemaRegistry.get_instance()
        registry.register_type(Network)
        registry.register_type(NetworkInput)
        registry.register_query(network_query)  # Need at least one query
        registry.register_mutation(create_network)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        # GraphQL mutation with camelCase input fields
        mutation_str = """
            mutation CreateNetwork($input: NetworkInput!) {
                createNetwork(input: $input) {
                    id
                    ipAddress
                    subnetMask
                }
            }
        """
        # Input with camelCase field names
        variables = {
            "input": {
                "ipAddress": "10.0.0.100",
                "subnetMask": "255.255.0.0",
                # gatewayAddress is optional, so we can omit it
            }
        }

        result = await graphql(
            schema, mutation_str, variable_values=variables, context_value={"db": None}
        )

        # Should work without errors
        assert result.errors is None or len(result.errors) == 0
        assert result.data is not None
        assert result.data["createNetwork"]["ipAddress"] == "10.0.0.100"
        assert result.data["createNetwork"]["subnetMask"] == "255.255.0.0"

    def test_introspection_shows_camelcase_fields(self) -> None:
        """Test that GraphQL introspection shows camelCase field names."""
        from graphql import graphql_sync

        # Enable camelCase
        config = SchemaConfig.get_instance()
        config.camel_case_fields = True

        # Register types
        registry = SchemaRegistry.get_instance()
        registry.register_type(Network)
        registry.register_type(NetworkInput)
        registry.register_query(network_query)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        # Introspection query for input type
        introspection_query = """
        {
            __type(name: "NetworkInput") {
                name
                inputFields {
                    name
                    type {
                        name
                    }
                }
            }
        }
        """
        result = graphql_sync(schema, introspection_query)
        assert result.data is not None

        input_type = result.data["__type"]
        field_names = [field["name"] for field in input_type["inputFields"]]

        # GraphQL schema should show camelCase names
        assert "ipAddress" in field_names
        assert "subnetMask" in field_names
        assert "gatewayAddress" in field_names

        # Should NOT show snake_case names
        assert "ip_address" not in field_names
        assert "subnet_mask" not in field_names
        assert "gateway_address" not in field_names

    @pytest.mark.asyncio
    async def test_snake_case_with_camelcase_disabled(self) -> None:
        """Test that snake_case is preserved when camelCase is disabled."""
        from graphql import graphql

        # Disable camelCase (default)
        config = SchemaConfig.get_instance()
        config.camel_case_fields = False

        # Register types
        registry = SchemaRegistry.get_instance()
        registry.register_type(Network)
        registry.register_type(NetworkInput)
        registry.register_query(network_query)

        composer = SchemaComposer(registry)
        schema = composer.compose()

        # GraphQL query with snake_case input fields
        query_str = """
            query TestNetwork($network_config: NetworkInput!) {
                network_query(network_config: $network_config) {
                    id
                    ip_address
                    subnet_mask
                }
            }
        """
        # Input with snake_case field names
        variables = {"network_config": {"ip_address": "172.16.0.100", "subnet_mask": "255.255.0.0"}}

        result = await graphql(
            schema, query_str, variable_values=variables, context_value={"db": None}
        )

        # Should work without errors
        assert result.errors is None or len(result.errors) == 0
        assert result.data is not None
        assert result.data["network_query"]["ip_address"] == "172.16.0.100"
