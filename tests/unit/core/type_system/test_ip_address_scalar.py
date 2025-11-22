"""Tests for IpAddress scalar type validation."""

import pytest
from graphql import GraphQLError
from graphql.language import IntValueNode, StringValueNode

from fraiseql.types.scalars.ip_address import (
    IpAddressField,
    parse_ip_address_literal,
    parse_ip_address_value,
    serialize_ip_address_string,
)


@pytest.mark.unit
class TestIpAddressSerialization:
    """Test IP address serialization."""

    def test_serialize_valid_ipv4(self) -> None:
        """Test serializing valid IPv4 addresses."""
        assert serialize_ip_address_string("192.168.1.1") == "192.168.1.1"
        assert serialize_ip_address_string("10.0.0.1") == "10.0.0.1"
        assert serialize_ip_address_string("172.16.0.1") == "172.16.0.1"
        assert serialize_ip_address_string("8.8.8.8") == "8.8.8.8"
        assert serialize_ip_address_string("255.255.255.255") == "255.255.255.255"
        assert serialize_ip_address_string("0.0.0.0") == "0.0.0.0"

    def test_serialize_valid_ipv6(self) -> None:
        """Test serializing valid IPv6 addresses."""
        assert serialize_ip_address_string("2001:db8::1") == "2001:db8::1"
        assert serialize_ip_address_string("::1") == "::1"
        assert serialize_ip_address_string("fe80::1") == "fe80::1"
        assert (
            serialize_ip_address_string("2001:db8:85a3::8a2e:370:7334")
            == "2001:db8:85a3::8a2e:370:7334"
        )

    def test_serialize_invalid_ip(self) -> None:
        """Test serializing invalid IP addresses raises error."""
        with pytest.raises(GraphQLError, match="cannot represent non-IP address"):
            serialize_ip_address_string("999.999.999.999")

        with pytest.raises(GraphQLError, match="cannot represent non-IP address"):
            serialize_ip_address_string("192.168.1")

        with pytest.raises(GraphQLError, match="cannot represent non-IP address"):
            serialize_ip_address_string("192.168.1.1.1")

        with pytest.raises(GraphQLError, match="cannot represent non-IP address"):
            serialize_ip_address_string("invalid")

        with pytest.raises(GraphQLError, match="cannot represent non-IP address"):
            serialize_ip_address_string("localhost")


class TestIpAddressParsing:
    """Test IP address parsing from variables."""

    def test_parse_valid_ipv4(self) -> None:
        """Test parsing valid IPv4 addresses."""
        result = parse_ip_address_value("192.168.1.1")
        assert str(result) == "192.168.1.1"

        result = parse_ip_address_value("10.0.0.1")
        assert str(result) == "10.0.0.1"

    def test_parse_valid_ipv6(self) -> None:
        """Test parsing valid IPv6 addresses."""
        result = parse_ip_address_value("2001:db8::1")
        assert str(result) == "2001:db8::1"

        result = parse_ip_address_value("::1")
        assert str(result) == "::1"

    def test_parse_ipv4_with_cidr_notation(self) -> None:
        """Test parsing IPv4 addresses with CIDR notation (extracts IP only)."""
        result = parse_ip_address_value("192.168.1.1/24")
        assert str(result) == "192.168.1.1"

        result = parse_ip_address_value("10.0.0.1/8")
        assert str(result) == "10.0.0.1"

        result = parse_ip_address_value("172.16.0.1/16")
        assert str(result) == "172.16.0.1"

    def test_parse_ipv6_with_cidr_notation(self) -> None:
        """Test parsing IPv6 addresses with CIDR notation (extracts IP only)."""
        result = parse_ip_address_value("2001:db8::1/64")
        assert str(result) == "2001:db8::1"

        result = parse_ip_address_value("fe80::1/10")
        assert str(result) == "fe80::1"

    def test_parse_invalid_ip(self) -> None:
        """Test parsing invalid IP addresses raises error."""
        with pytest.raises(GraphQLError, match="Invalid IP address string"):
            parse_ip_address_value("999.999.999.999")

        with pytest.raises(GraphQLError, match="Invalid IP address string"):
            parse_ip_address_value("invalid")

    def test_parse_invalid_type(self) -> None:
        """Test parsing non-string types raises error."""
        with pytest.raises(GraphQLError, match="cannot represent non-string value"):
            parse_ip_address_value(123)

        with pytest.raises(GraphQLError, match="cannot represent non-string value"):
            parse_ip_address_value(None)


class TestIpAddressField:
    """Test IpAddressField class."""

    def test_create_valid_ipv4_field(self) -> None:
        """Test creating IpAddressField with valid IPv4 values."""
        # IpAddressField is just a string marker, no validation in constructor
        ip = IpAddressField("192.168.1.1")
        assert ip == "192.168.1.1"
        assert isinstance(ip, str)

    def test_create_valid_ipv6_field(self) -> None:
        """Test creating IpAddressField with valid IPv6 values."""
        ip = IpAddressField("2001:db8::1")
        assert ip == "2001:db8::1"
        assert isinstance(ip, str)


class TestIpAddressLiteralParsing:
    """Test parsing IP address from GraphQL literals."""

    def test_parse_valid_literal(self) -> None:
        """Test parsing valid IP address literals."""
        result = parse_ip_address_literal(StringValueNode(value="192.168.1.1"))
        assert str(result) == "192.168.1.1"

        result = parse_ip_address_literal(StringValueNode(value="2001:db8::1"))
        assert str(result) == "2001:db8::1"

    def test_parse_literal_with_cidr_notation(self) -> None:
        """Test parsing IP address literals with CIDR notation."""
        result = parse_ip_address_literal(StringValueNode(value="192.168.1.1/24"))
        assert str(result) == "192.168.1.1"

        result = parse_ip_address_literal(StringValueNode(value="2001:db8::1/64"))
        assert str(result) == "2001:db8::1"

    def test_parse_invalid_literal_format(self) -> None:
        """Test parsing invalid IP address format literals."""
        with pytest.raises(GraphQLError, match="Invalid IP address string"):
            parse_ip_address_literal(StringValueNode(value="999.999.999.999"))

    def test_parse_non_string_literal(self) -> None:
        """Test parsing non-string literals."""
        with pytest.raises(GraphQLError, match="cannot represent non-string literal"):
            parse_ip_address_literal(IntValueNode(value="192"))


class TestIpAddressInInputTypes:
    """Test IpAddress scalar can be used in GraphQL input types.

    Fixes bug from FRAISEQL_IPADDRESS_SCALAR_BUG.md
    """

    def test_ipaddress_in_input_type(self) -> None:
        """Test that IpAddress scalar can be used in GraphQL input types."""
        from fraiseql import UNSET
        from fraiseql.types import IpAddress, Port
        from fraiseql.types.fraise_input import fraise_input

        # This should not raise TypeError anymore
        @fraise_input
        class CreateSmtpServerInput:
            hostname: str
            ip_address: IpAddress | None = UNSET
            port: Port | None = UNSET

        # Verify the input type was created successfully
        assert CreateSmtpServerInput.__name__ == "CreateSmtpServerInput"
        assert hasattr(CreateSmtpServerInput, "__gql_typename__")

        # Test creating an instance
        input_obj = CreateSmtpServerInput(
            hostname="smtp.example.com", ip_address="192.168.1.1", port=587
        )
        assert input_obj.hostname == "smtp.example.com"
        assert input_obj.ip_address == "192.168.1.1"
        assert input_obj.port == 587

    def test_network_device_input_with_all_scalars(self) -> None:
        """Test using multiple network scalars in an input type."""
        from fraiseql import UNSET
        from fraiseql.types import CIDR, Hostname, IpAddress, MacAddress, Port
        from fraiseql.types.fraise_input import fraise_input

        @fraise_input
        class NetworkDeviceInput:
            hostname: Hostname
            ip_address: IpAddress
            mac_address: MacAddress | None = UNSET
            subnet: CIDR | None = UNSET
            ssh_port: Port = 22

        # Verify all fields work
        device = NetworkDeviceInput(
            hostname="router.local",
            ip_address="10.0.0.1",
            mac_address="00:11:22:33:44:55",
            subnet="10.0.0.0/24",
        )

        assert device.hostname == "router.local"
        assert device.ip_address == "10.0.0.1"
        assert device.mac_address == "00:11:22:33:44:55"
        assert device.subnet == "10.0.0.0/24"
        assert device.ssh_port == 22

    def test_ipaddress_required_field(self) -> None:
        """Test IpAddress as a required field in input type."""
        from fraiseql import UNSET
        from fraiseql.types import IpAddress
        from fraiseql.types.fraise_input import fraise_input

        @fraise_input
        class DnsServerInput:
            ip_address: IpAddress  # Required
            name: str | None = UNSET

        # Should work with valid IP
        dns = DnsServerInput(ip_address="8.8.8.8")
        assert dns.ip_address == "8.8.8.8"

        # IPv6 should also work
        dns_v6 = DnsServerInput(ip_address="2001:db8::1")
        assert dns_v6.ip_address == "2001:db8::1"
