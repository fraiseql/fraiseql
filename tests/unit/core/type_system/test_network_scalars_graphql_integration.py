"""Tests for network scalars in GraphQL input and output types."""

from uuid import UUID, uuid4

import pytest
from graphql import graphql_sync

import fraiseql
from fraiseql import build_fraiseql_schema
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.types import (
    EmailAddress,
    Hostname,
    IpAddress,
    MacAddress,
    Port,
)
from fraiseql.types.definitions import UNSET


@pytest.fixture(autouse=True)
def clear_registry() -> None:
    """Clear the schema registry before each test."""
    SchemaRegistry.get_instance().clear()
    yield
    SchemaRegistry.get_instance().clear()


def test_email_address_in_input_type() -> None:
    """Test that EmailAddress scalar works in GraphQL input types."""

    @fraiseql.input
    class CreateUserInput:
        name: str
        email: EmailAddress
        secondary_email: EmailAddress | None = UNSET

    @fraiseql.type
    class User:
        id: UUID
        name: str
        email: EmailAddress
        secondary_email: EmailAddress | None = None

    @fraiseql.mutation
    def create_user(info, input: CreateUserInput) -> User:
        return User(
            id=uuid4(),
            name=input.name,
            email=input.email,
            secondary_email=input.secondary_email if input.secondary_email != UNSET else None,
        )

    # Add a dummy query as FraiseQL requires at least one query
    @fraiseql.query
    def health(info) -> str:
        return "ok"

    # This should not raise an error when building the schema
    schema = build_fraiseql_schema()
    assert schema is not None


def test_ip_address_in_mutation_return() -> None:
    """Test that IpAddress scalar properly serializes in mutation returns."""

    @fraiseql.type
    class Router:
        id: UUID
        name: str
        ip_address: IpAddress
        gateway_ip: IpAddress | None = None

    @fraiseql.mutation
    def create_router(info, name: str, ip_address: IpAddress) -> Router:
        return Router(id=uuid4(), name=name, ip_address=ip_address, gateway_ip="192.168.1.1")

    # Add a dummy query as FraiseQL requires at least one query
    @fraiseql.query
    def health(info) -> str:
        return "ok"

    schema = build_fraiseql_schema()

    # Execute the mutation
    query = """
        mutation {
            createRouter(name: "Main Router", ipAddress: "10.0.0.1") {
                id
                name
                ipAddress
                gatewayIp
            }
        }
    """
    result = graphql_sync(schema, query, context_value={})
    assert result.errors is None
    assert result.data is not None

    router_data = result.data["createRouter"]
    assert router_data["name"] == "Main Router"
    assert router_data["ipAddress"] == "10.0.0.1"
    assert router_data["gatewayIp"] == "192.168.1.1"


def test_all_network_scalars_in_input_types() -> None:
    """Test that all network scalars work in GraphQL input types."""

    @fraiseql.input
    class NetworkConfigInput:
        email: EmailAddress
        ip_address: IpAddress
        mac_address: MacAddress
        port: Port
        hostname: Hostname

    @fraiseql.type
    class NetworkConfig:
        id: UUID
        email: EmailAddress
        ip_address: IpAddress
        mac_address: MacAddress
        port: Port
        hostname: Hostname

    @fraiseql.mutation
    def create_network_config(info, input: NetworkConfigInput) -> NetworkConfig:
        return NetworkConfig(
            id=uuid4(),
            email=input.email,
            ip_address=input.ip_address,
            mac_address=input.mac_address,
            port=input.port,
            hostname=input.hostname,
        )

    # Add a dummy query as FraiseQL requires at least one query
    @fraiseql.query
    def health(info) -> str:
        return "ok"

    # This should not raise an error when building the schema
    schema = build_fraiseql_schema()
    assert schema is not None


def test_all_network_scalars_in_output_types() -> None:
    """Test that all network scalars properly serialize in output types."""

    @fraiseql.type
    class NetworkDevice:
        id: UUID
        email: EmailAddress
        ip_address: IpAddress
        mac_address: MacAddress
        port: Port
        hostname: Hostname

    @fraiseql.query
    def get_network_device(info) -> NetworkDevice:
        return NetworkDevice(
            id=uuid4(),
            email="admin@example.com",
            ip_address="192.168.1.100",
            mac_address="00:11:22:33:44:55",
            port=8080,
            hostname="device.local",
        )

    schema = build_fraiseql_schema()

    # Execute the query
    query = """
        query {
            getNetworkDevice {
                id
                email
                ipAddress
                macAddress
                port
                hostname
            }
        }
    """
    result = graphql_sync(schema, query, context_value={})
    assert result.errors is None
    assert result.data is not None

    device_data = result.data["getNetworkDevice"]
    assert device_data["email"] == "admin@example.com"
    assert device_data["ipAddress"] == "192.168.1.100"
    assert device_data["macAddress"] == "00:11:22:33:44:55"
    assert device_data["port"] == 8080
    assert device_data["hostname"] == "device.local"


def test_network_scalars_with_null_values() -> None:
    """Test network scalars handle null values correctly."""

    @fraiseql.input
    class OptionalNetworkInput:
        email: EmailAddress | None = UNSET
        ip_address: IpAddress | None = UNSET
        mac_address: MacAddress | None = UNSET
        port: Port | None = UNSET
        hostname: Hostname | None = UNSET

    @fraiseql.type
    class OptionalNetworkConfig:
        id: UUID
        email: EmailAddress | None = None
        ip_address: IpAddress | None = None
        mac_address: MacAddress | None = None
        port: Port | None = None
        hostname: Hostname | None = None

    @fraiseql.mutation
    def create_optional_config(info, input: OptionalNetworkInput) -> OptionalNetworkConfig:
        return OptionalNetworkConfig(
            id=uuid4(),
            email=input.email if input.email != UNSET else None,
            ip_address=input.ip_address if input.ip_address != UNSET else None,
            mac_address=input.mac_address if input.mac_address != UNSET else None,
            port=input.port if input.port != UNSET else None,
            hostname=input.hostname if input.hostname != UNSET else None,
        )

    # Add a dummy query as FraiseQL requires at least one query
    @fraiseql.query
    def health(info) -> str:
        return "ok"

    schema = build_fraiseql_schema()

    # Test with all nulls
    query = """
        mutation {
            createOptionalConfig(input: {}) {
                id
                email
                ipAddress
                macAddress
                port
                hostname
            }
        }
    """
    result = graphql_sync(schema, query, context_value={})
    assert result.errors is None
    assert result.data is not None

    config_data = result.data["createOptionalConfig"]
    assert config_data["email"] is None
    assert config_data["ipAddress"] is None
    assert config_data["macAddress"] is None
    assert config_data["port"] is None
    assert config_data["hostname"] is None
